//! Entrar e sair do painel.

use askama::Template;
use askama_web::WebTemplate;
use axum::{
    Form, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::{
    auth::{
        login::{LoginFailure, authenticate},
        tokens::{TokenKind, issue_token, revoke_token},
    },
    state::AppState,
    web::session::{clear_cookie, session_cookie, session_token},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/entrar", get(login_form).post(login_submit))
        .route("/sair", post(logout))
}

#[derive(Template, WebTemplate)]
#[template(path = "entrar.html")]
struct LoginPage {
    usuario: String,
    error: Option<String>,
}

async fn login_form() -> LoginPage {
    LoginPage {
        usuario: String::new(),
        error: None,
    }
}

#[derive(Deserialize)]
struct LoginForm {
    usuario: String,
    senha: String,
}

fn set_cookie(response: &mut Response, value: &str) {
    // O valor é montado só com base64url e constantes: sempre válido como cabeçalho.
    if let Ok(v) = HeaderValue::from_str(value) {
        response.headers_mut().append(header::SET_COOKIE, v);
    }
}

async fn login_submit(State(st): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    let show_error = |msg: &str| {
        LoginPage {
            usuario: form.usuario.clone(),
            error: Some(msg.to_string()),
        }
        .into_response()
    };
    let user = match authenticate(&st, &form.usuario, form.senha.clone()).await {
        Ok(u) => u,
        Err(LoginFailure::Internal(msg)) => {
            tracing::error!("erro interno no login web: {msg}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Erro interno").into_response();
        }
        Err(f) => return show_error(f.message()),
    };
    let token = match issue_token(&st.pool, user.id, TokenKind::Web).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("erro ao emitir sessão web: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Erro interno").into_response();
        }
    };
    let mut response = Redirect::to("/clientes").into_response();
    set_cookie(&mut response, &session_cookie(&token, st.config.cookie_secure));
    response
}

/// Revoga a sessão (se houver) e apaga o cookie. Não exige sessão válida.
async fn logout(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token(&headers)
        && let Err(e) = revoke_token(&st.pool, token).await
    {
        tracing::error!("falha ao revogar sessão web: {e}");
    }
    let mut response = Redirect::to("/entrar").into_response();
    set_cookie(&mut response, &clear_cookie(st.config.cookie_secure));
    response
}
