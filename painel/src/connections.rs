//! Histórico de atendimentos: conexões recebidas pelos computadores dos
//! clientes, alimentadas pelos eventos `audit/conn` do app RustDesk.

use sqlx::{Acquire, FromRow, Sqlite, SqlitePool};

/// Evento de conexão enviado pelo computador que RECEBE o acesso.
#[derive(Debug, Clone)]
pub struct ConnEvent {
    /// Código do computador que recebeu a conexão.
    pub peer_id: String,
    pub conn_id: i64,
    pub session_id: String,
    pub kind: ConnEventKind,
}

#[derive(Debug, Clone)]
pub enum ConnEventKind {
    /// Pedido de conexão recebido (ainda sem aceite).
    New { ip: String },
    /// Conexão aceita; traz quem está acessando.
    Authorized {
        operator_peer_id: String,
        operator_name: String,
        conn_type: Option<i64>,
    },
    /// Conexão encerrada (ou pedido recusado).
    Close,
}

/// Aplica um evento ao histórico. Roda em transação própria (ou aninhada,
/// se receber uma transação) e é idempotente para eventos fora de ordem:
/// `Authorized` sem registro aberto cria um já autorizado, e `Close` sem
/// registro aberto é ignorado (limitação aceita: um `close` que chega antes de
/// qualquer outro evento da conexão não deixa rastro no histórico).
pub async fn apply_conn_event<'a, A>(db: A, ev: &ConnEvent, now: i64) -> sqlx::Result<()>
where
    A: Acquire<'a, Database = Sqlite>,
{
    let mut tx = db.begin().await?;
    match &ev.kind {
        ConnEventKind::New { ip } => {
            // `new` atrasado: se um `authorized` já criou o registro aberto
            // (sem ip), completa esse registro em vez de duplicar.
            let merged = sqlx::query(
                "UPDATE connections SET
                   ip = ?4,
                   session_id = CASE WHEN session_id = '' THEN ?3 ELSE session_id END
                 WHERE id = (
                   SELECT id FROM connections
                   WHERE peer_id = ?1 AND conn_id = ?2 AND ended_at IS NULL
                     AND ip = '' AND authorized_at IS NOT NULL
                   ORDER BY started_at DESC, id DESC LIMIT 1)",
            )
            .bind(&ev.peer_id)
            .bind(ev.conn_id)
            .bind(&ev.session_id)
            .bind(ip)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if merged == 0 {
                sqlx::query(
                    "INSERT INTO connections (peer_id, conn_id, session_id, ip, started_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .bind(&ev.peer_id)
                .bind(ev.conn_id)
                .bind(&ev.session_id)
                .bind(ip)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
        }
        ConnEventKind::Authorized {
            operator_peer_id,
            operator_name,
            conn_type,
        } => {
            let updated = sqlx::query(
                "UPDATE connections SET
                   session_id = CASE WHEN ?3 = '' THEN session_id ELSE ?3 END,
                   authorized_at = COALESCE(authorized_at, ?4), operator_peer_id = ?5, operator_name = ?6,
                   conn_type = ?7,
                   user_id = COALESCE((SELECT user_id FROM user_devices WHERE peer_id = ?5), user_id)
                 WHERE id = (
                   SELECT id FROM connections
                   WHERE peer_id = ?1 AND conn_id = ?2 AND ended_at IS NULL
                   ORDER BY started_at DESC, id DESC LIMIT 1)",
            )
            .bind(&ev.peer_id)
            .bind(ev.conn_id)
            .bind(&ev.session_id)
            .bind(now)
            .bind(operator_peer_id)
            .bind(operator_name)
            .bind(conn_type)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated == 0 {
                sqlx::query(
                    "INSERT INTO connections
                       (peer_id, conn_id, session_id, operator_peer_id, operator_name,
                        user_id, conn_type, started_at, authorized_at)
                     VALUES (?1, ?2, ?3, ?4, ?5,
                       (SELECT user_id FROM user_devices WHERE peer_id = ?4), ?6, ?7, ?7)",
                )
                .bind(&ev.peer_id)
                .bind(ev.conn_id)
                .bind(&ev.session_id)
                .bind(operator_peer_id)
                .bind(operator_name)
                .bind(conn_type)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
        }
        ConnEventKind::Close => {
            sqlx::query(
                "UPDATE connections SET ended_at = ?3
                 WHERE id = (
                   SELECT id FROM connections
                   WHERE peer_id = ?1 AND conn_id = ?2 AND ended_at IS NULL
                   ORDER BY started_at DESC, id DESC LIMIT 1)",
            )
            .bind(&ev.peer_id)
            .bind(ev.conn_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await
}

/// Filtros da consulta de histórico.
#[derive(Debug, Clone)]
pub struct HistoryFilter {
    pub user_id: Option<i64>,
    pub client_id: Option<i64>,
    /// Início do intervalo (inclusivo), em segundos Unix.
    pub from: Option<i64>,
    /// Fim do intervalo (exclusivo), em segundos Unix.
    pub to: Option<i64>,
    pub limit: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct HistoryRow {
    pub id: i64,
    pub started_at: i64,
    pub authorized_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub peer_id: String,
    pub computer_label: Option<String>,
    pub client_id: Option<i64>,
    pub client_name: Option<String>,
    pub operator_name: String,
    pub user_display_name: Option<String>,
    pub ip: String,
    pub conn_type: Option<i64>,
}

/// Conexões mais recentes primeiro, com computador, cliente e colaborador.
pub async fn query(pool: &SqlitePool, f: &HistoryFilter) -> sqlx::Result<Vec<HistoryRow>> {
    sqlx::query_as(
        "SELECT c.id, c.started_at, c.authorized_at, c.ended_at, c.peer_id,
                comp.label AS computer_label, cl.id AS client_id, cl.name AS client_name,
                c.operator_name, u.display_name AS user_display_name, c.ip, c.conn_type
         FROM connections c
         LEFT JOIN computers comp ON comp.peer_id = c.peer_id
         LEFT JOIN clients cl ON cl.id = comp.client_id
         LEFT JOIN users u ON u.id = c.user_id
         WHERE (?1 IS NULL OR c.user_id = ?1)
           AND (?2 IS NULL OR cl.id = ?2)
           AND (?3 IS NULL OR c.started_at >= ?3)
           AND (?4 IS NULL OR c.started_at < ?4)
         ORDER BY c.started_at DESC, c.id DESC
         LIMIT ?5",
    )
    .bind(f.user_id)
    .bind(f.client_id)
    .bind(f.from)
    .bind(f.to)
    .bind(f.limit)
    .fetch_all(pool)
    .await
}

/// Situação da conexão para exibição.
pub fn status_label(r: &HistoryRow) -> &'static str {
    if r.authorized_at.is_some() {
        "Conectado"
    } else if r.ended_at.is_some() {
        "Não concluída"
    } else {
        "Aguardando aceite"
    }
}

/// Tipo da conexão (valor `type` do evento `authorized`).
pub fn type_label(t: Option<i64>) -> &'static str {
    match t {
        Some(0) => "Acesso remoto",
        Some(1) => "Transferência de arquivos",
        Some(2) => "Redirecionamento de porta",
        Some(3) => "Câmera",
        Some(4) => "Terminal",
        _ => "—",
    }
}
