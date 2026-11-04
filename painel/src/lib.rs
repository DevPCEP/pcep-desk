//! PCEP Desk: painel e API compatível com o RustDesk.

pub mod api;
pub mod auth;
pub mod clock;
pub mod config;
pub mod db;
pub mod error;
pub mod state;
pub mod users;

use axum::{Router, extract::DefaultBodyLimit, routing::get};

use crate::state::AppState;

/// Monta o router completo da aplicação.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/saude", get(saude))
        // Limite de corpo de 64 KiB em todas as rotas `/api/*`.
        .nest("/api", api::router().layer(DefaultBodyLimit::max(65536)))
        .with_state(state)
}

async fn saude() -> &'static str {
    "ok"
}
