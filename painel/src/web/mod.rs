//! Painel web: HTML renderizado no servidor (Askama) com htmx.

pub mod clients;
pub mod format;
pub mod history;
pub mod login;
pub mod session;
pub mod users;

use axum::{Router, extract::DefaultBodyLimit, middleware, routing::get};

use crate::state::AppState;

/// Rotas do painel (fora de `/api` e de `/static`), já com a checagem de
/// origem e o limite de corpo de 64 KiB.
pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(|| async { axum::response::Redirect::to("/clientes") }))
        .merge(clients::router())
        .merge(history::router())
        .merge(login::router())
        .merge(users::router())
        .layer(DefaultBodyLimit::max(65536))
        .layer(middleware::from_fn_with_state(
            state,
            session::require_same_origin,
        ))
}
