//! PCEP Desk: painel e API compatível com o RustDesk.

pub mod auth;
pub mod clock;
pub mod config;
pub mod db;
pub mod error;
pub mod state;

use axum::{Router, routing::get};

use crate::state::AppState;

/// Monta o router completo da aplicação.
pub fn build_router(state: AppState) -> Router {
    Router::new().route("/saude", get(saude)).with_state(state)
}

async fn saude() -> &'static str {
    "ok"
}
