//! Painel web: HTML renderizado no servidor (Askama) com htmx.

pub mod clients;
pub mod downloads;
pub mod format;
pub mod history;
pub mod login;
pub mod session;
pub mod users;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, header},
    middleware,
    routing::get,
};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::state::AppState;

/// Cabeçalho de segurança fixo, que substitui qualquer valor já definido.
fn fixed_header(name: HeaderName, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(name, HeaderValue::from_static(value))
}

/// Rotas do painel (fora de `/api` e de `/static`), já com a checagem de
/// origem, o limite de corpo de 64 KiB e os cabeçalhos de segurança
/// (sem cache, sem iframe, sem sniffing, referrer só na mesma origem).
pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(|| async { axum::response::Redirect::to("/clientes") }))
        .merge(clients::router())
        .merge(downloads::router())
        .merge(history::router())
        .merge(login::router())
        .merge(users::router())
        .layer(DefaultBodyLimit::max(65536))
        .layer(middleware::from_fn_with_state(
            state,
            session::require_same_origin,
        ))
        // Mais externas: valem também para redirecionamentos e erros do middleware.
        .layer(fixed_header(header::CACHE_CONTROL, "no-store"))
        .layer(fixed_header(header::X_FRAME_OPTIONS, "DENY"))
        .layer(fixed_header(header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .layer(fixed_header(header::REFERRER_POLICY, "same-origin"))
}
