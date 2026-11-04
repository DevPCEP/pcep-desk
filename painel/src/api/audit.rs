//! Eventos de auditoria enviados pelo app RustDesk do computador que RECEBE
//! a conexão: `audit/conn`, `audit/file` e `audit/alarm`. Sem autenticação.
//!
//! O app trata qualquer corpo de resposta não vazio como falha e reenvia (até
//! 3 vezes, com o mesmo `nonce`), então o sucesso é sempre 200 com corpo vazio.

use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State, rejection::BytesRejection},
    http::StatusCode,
    routing::post,
};
use serde_json::Value;
use sqlx::{Sqlite, Transaction};

use super::devices::{clean_peer_id, invalid_request};
use crate::{
    clock,
    connections::{self, ConnEvent, ConnEventKind},
    error::ApiError,
    state::AppState,
};

/// Tamanho máximo do `ip`, do `session_id` e do código do operador.
const MAX_SHORT_LEN: usize = 64;
const MAX_OPERATOR_NAME_LEN: usize = 120;
const MAX_NONCE_LEN: usize = 128;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/audit/conn", post(audit_conn))
        .route("/audit/file", post(audit_file))
        .route("/audit/alarm", post(audit_alarm))
        // Mais restrito que os 64 KiB das demais rotas `/api` (a camada interna vence).
        .layer(DefaultBodyLimit::max(super::UNAUTH_BODY_LIMIT))
}

#[derive(Clone, Copy)]
enum Kind {
    Conn,
    File,
    Alarm,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Conn => "conn",
            Kind::File => "file",
            Kind::Alarm => "alarm",
        }
    }
}

async fn audit_conn(
    State(st): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<StatusCode, ApiError> {
    record(&st, body, Kind::Conn).await
}

async fn audit_file(
    State(st): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<StatusCode, ApiError> {
    record(&st, body, Kind::File).await
}

async fn audit_alarm(
    State(st): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<StatusCode, ApiError> {
    record(&st, body, Kind::Alarm).await
}

/// Grava o evento bruto (com dedup por `nonce`) e, para `conn`, aplica ao
/// histórico, tudo na mesma transação: se aplicar falhar, o `nonce` não fica
/// marcado e o reenvio do app ainda funciona.
async fn record(
    st: &AppState,
    body: Result<Bytes, BytesRejection>,
    kind: Kind,
) -> Result<StatusCode, ApiError> {
    let raw = body.map_err(|rej| ApiError::new(rej.status(), "Requisição grande demais"))?;
    let value: Value = serde_json::from_slice(&raw).map_err(|_| invalid_request())?;
    let obj = value.as_object().ok_or_else(invalid_request)?;
    let peer_id = obj
        .get("id")
        .and_then(text_of)
        .and_then(|id| clean_peer_id(&id).map(str::to_owned))
        .ok_or_else(invalid_request)?;
    let nonce = obj
        .get("nonce")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| n.chars().take(MAX_NONCE_LEN).collect::<String>());
    let now = clock::now();

    let mut tx = st.pool.begin().await?;
    if !insert_raw(&mut tx, nonce.as_deref(), kind, &peer_id, &raw, now).await? {
        // Reenvio de um evento já recebido: nada a reaplicar.
        return Ok(StatusCode::OK);
    }
    if matches!(kind, Kind::Conn)
        && let Some(ev) = parse_conn_event(obj, peer_id)
    {
        connections::apply_conn_event(&mut *tx, &ev, now).await?;
    }
    tx.commit().await?;
    Ok(StatusCode::OK)
}

/// Insere em `audit_raw`; `false` se o `nonce` já existia.
async fn insert_raw(
    tx: &mut Transaction<'_, Sqlite>,
    nonce: Option<&str>,
    kind: Kind,
    peer_id: &str,
    raw: &Bytes,
    now: i64,
) -> sqlx::Result<bool> {
    let inserted = sqlx::query(
        "INSERT INTO audit_raw (nonce, kind, peer_id, body, received_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(nonce) DO NOTHING",
    )
    .bind(nonce)
    .bind(kind.as_str())
    .bind(peer_id)
    .bind(String::from_utf8_lossy(raw).into_owned())
    .bind(now)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(inserted > 0)
}

/// Texto de um valor JSON que seja string ou número.
fn text_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

/// Interpreta o evento de conexão. `None` para formatos desconhecidos (ficam
/// só em `audit_raw`, sem quebrar o app com versões futuras).
fn parse_conn_event(obj: &serde_json::Map<String, Value>, peer_id: String) -> Option<ConnEvent> {
    let conn_id = obj.get("conn_id")?.as_i64()?;
    // `session_id` é u64 e pode passar de 2^53: o `Value` preserva o número
    // exato e guardamos o texto.
    let session_id = obj.get("session_id").and_then(text_of).unwrap_or_default();
    let kind = match obj.get("action") {
        Some(Value::String(a)) if a == "new" => ConnEventKind::New {
            ip: truncate(obj.get("ip")?.as_str()?, MAX_SHORT_LEN),
        },
        Some(Value::String(a)) if a == "close" => ConnEventKind::Close,
        None | Some(Value::Null) => {
            let peer = obj.get("peer")?.as_array()?;
            ConnEventKind::Authorized {
                operator_peer_id: truncate(&text_of(peer.first()?)?, MAX_SHORT_LEN),
                operator_name: truncate(peer.get(1)?.as_str()?, MAX_OPERATOR_NAME_LEN),
                conn_type: obj.get("type").and_then(Value::as_i64),
            }
        }
        _ => return None,
    };
    Some(ConnEvent {
        peer_id,
        conn_id,
        session_id: truncate(&session_id, MAX_SHORT_LEN),
        kind,
    })
}
