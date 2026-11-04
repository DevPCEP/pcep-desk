//! Rotas `/api/*` consumidas pelo app RustDesk.

pub mod account;
pub mod extract;
pub mod groups;

use axum::Router;

use crate::state::AppState;

/// Router das rotas `/api` (o chamador aplica o limite de corpo e o prefixo).
pub fn router() -> Router<AppState> {
    Router::new().merge(account::router()).merge(groups::router())
}
