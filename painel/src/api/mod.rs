//! Rotas `/api/*` consumidas pelo app RustDesk.

pub mod ab;
pub mod account;
pub mod audit;
pub mod devices;
pub mod extract;
pub mod groups;

use axum::Router;

use crate::state::AppState;

/// Limite de corpo (4 KiB) das rotas sem autenticação (`heartbeat`, `sysinfo*`,
/// `audit/*`): os eventos reais do app têm poucas centenas de bytes.
pub const UNAUTH_BODY_LIMIT: usize = 4096;

/// Router das rotas `/api` (o chamador aplica o limite de corpo e o prefixo).
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(account::router())
        .merge(groups::router())
        .merge(devices::router())
        .merge(audit::router())
        .merge(ab::router())
}
