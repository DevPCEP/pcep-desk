//! Painel web: HTML renderizado no servidor (Askama) com htmx.

pub mod login;
pub mod session;
pub mod users;

use askama::Template;
use askama_web::WebTemplate;
use axum::{Router, extract::DefaultBodyLimit, middleware, routing::get};

use crate::{state::AppState, users::User, web::session::WebUser};

/// Rotas do painel (fora de `/api` e de `/static`), já com a checagem de
/// origem e o limite de corpo de 64 KiB.
pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(|| async { axum::response::Redirect::to("/clientes") }))
        .route("/clientes", get(clientes))
        .merge(login::router())
        .merge(users::router())
        .layer(DefaultBodyLimit::max(65536))
        .layer(middleware::from_fn_with_state(
            state,
            session::require_same_origin,
        ))
}

#[derive(Template, WebTemplate)]
#[template(path = "em_construcao.html")]
struct UnderConstruction {
    user: User,
    active: &'static str,
}

/// Provisório: destino pós-login até a página de clientes existir.
async fn clientes(WebUser(user): WebUser) -> UnderConstruction {
    UnderConstruction {
        user,
        active: "clientes",
    }
}
