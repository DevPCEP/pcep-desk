//! Rotas sem autenticação chamadas periodicamente pelo app RustDesk:
//! `heartbeat`, `sysinfo` e `sysinfo_ver`.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{State, rejection::BytesRejection},
    http::StatusCode,
    routing::post,
};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use crate::{
    clock,
    devices::{self, SysInfo},
    error::ApiError,
    state::AppState,
};

/// Tamanho máximo de um código de conexão (peer id).
const MAX_PEER_ID_LEN: usize = 64;
/// Tamanho máximo dos campos textuais gravados.
const MAX_FIELD_LEN: usize = 255;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/heartbeat", post(heartbeat))
        .route("/sysinfo", post(sysinfo))
        .route("/sysinfo_ver", post(sysinfo_ver))
}

/// Aceita string, número ou null (vira vazio); qualquer outro tipo vira vazio.
fn lenient_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    })
}

#[derive(Deserialize)]
struct HeartbeatBody {
    #[serde(default, deserialize_with = "lenient_string")]
    id: String,
    #[serde(default, deserialize_with = "lenient_string")]
    uuid: String,
    #[serde(default, deserialize_with = "lenient_string")]
    ver: String,
}

#[derive(Deserialize)]
struct SysInfoBody {
    #[serde(default, deserialize_with = "lenient_string")]
    id: String,
    #[serde(default, deserialize_with = "lenient_string")]
    uuid: String,
    #[serde(default, deserialize_with = "lenient_string")]
    hostname: String,
    #[serde(default, deserialize_with = "lenient_string")]
    os: String,
    #[serde(default, deserialize_with = "lenient_string")]
    username: String,
    #[serde(default, deserialize_with = "lenient_string")]
    cpu: String,
    #[serde(default, deserialize_with = "lenient_string")]
    memory: String,
    #[serde(default, deserialize_with = "lenient_string")]
    version: String,
}

/// Lê o corpo como bytes (ignora o `Content-Type`) e faz o parse do JSON.
/// Corpo acima do limite → 413; JSON inválido → 400.
fn parse_body<T: serde::de::DeserializeOwned>(
    body: Result<Bytes, BytesRejection>,
) -> Result<T, ApiError> {
    let bytes = body.map_err(|rej| ApiError::new(rej.status(), "Requisição grande demais"))?;
    serde_json::from_slice(&bytes).map_err(|_| invalid_request())
}

pub(super) fn invalid_request() -> ApiError {
    ApiError::bad_request("Requisição inválida")
}

/// Remove espaços e limita a quantidade de caracteres.
fn clean(s: &str) -> String {
    s.trim().chars().take(MAX_FIELD_LEN).collect()
}

/// Id normalizado, ou `None` se vazio ou longo demais.
pub(super) fn clean_peer_id(id: &str) -> Option<&str> {
    let id = id.trim();
    (!id.is_empty() && id.chars().count() <= MAX_PEER_ID_LEN).then_some(id)
}

async fn heartbeat(
    State(st): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<Value>, ApiError> {
    let req: HeartbeatBody = parse_body(body)?;
    let peer_id = clean_peer_id(&req.id).ok_or_else(invalid_request)?;
    let (uuid, ver) = (clean(&req.uuid), clean(&req.ver));
    devices::touch(&st.pool, peer_id, &uuid, &ver, clock::now()).await?;
    Ok(Json(json!({})))
}

async fn sysinfo(
    State(st): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<&'static str, ApiError> {
    let req: SysInfoBody = parse_body(body)?;
    let Some(peer_id) = clean_peer_id(&req.id) else {
        return Ok("ID_NOT_FOUND");
    };
    // Só os campos conhecidos são lidos: chaves extras (como a senha do
    // catálogo pré-configurada) nunca chegam ao banco.
    let info = SysInfo {
        hostname: clean(&req.hostname),
        os: clean(&req.os),
        username: clean(&req.username),
        cpu: clean(&req.cpu),
        memory: clean(&req.memory),
        version: clean(&req.version),
    };
    devices::upsert_sysinfo(&st.pool, peer_id, &clean(&req.uuid), &info, clock::now()).await?;
    Ok("SYSINFO_UPDATED")
}

/// O app só compara com a versão guardada; resposta vazia = "sem novidade".
async fn sysinfo_ver() -> StatusCode {
    StatusCode::OK
}
