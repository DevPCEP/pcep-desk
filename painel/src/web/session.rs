//! Sessão do painel web: cookie, extratores de usuário e checagem de origem.

use axum::{
    extract::{FromRequestParts, Request, State},
    http::{HeaderMap, Method, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use askama::Template;
use askama_web::WebTemplate;

use crate::{
    auth::tokens::{TokenKind, resolve_token},
    state::AppState,
    users::User,
};

/// Nome do cookie de sessão.
pub const COOKIE_NAME: &str = "pcep_sessao";
/// Duração do cookie (igual à validade do token web: 12 horas).
const COOKIE_MAX_AGE: u32 = 12 * 3600;

/// Valor do cookie de sessão na requisição, se houver.
pub fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE_NAME)
        .map(|(_, value)| value.trim())
        .filter(|v| !v.is_empty())
}

/// `Set-Cookie` que inicia a sessão.
pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "{COOKIE_NAME}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={COOKIE_MAX_AGE}{}",
        if secure { "; Secure" } else { "" }
    )
}

/// `Set-Cookie` que apaga a sessão.
pub fn clear_cookie(secure: bool) -> String {
    format!(
        "{COOKIE_NAME}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

/// Usuário autenticado pelo cookie de sessão web. Sem sessão válida,
/// redireciona (303) para `/entrar`; em requisição htmx (`HX-Request`)
/// responde 200 com `HX-Redirect`, para a página de login não ser injetada
/// no trecho da página que o htmx atualiza.
pub struct WebUser(pub User);

impl FromRequestParts<AppState> for WebUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let htmx = parts.headers.contains_key("hx-request");
        let to_login = || {
            if htmx {
                (StatusCode::OK, [("hx-redirect", "/entrar")]).into_response()
            } else {
                Redirect::to("/entrar").into_response()
            }
        };
        let token = session_token(&parts.headers).ok_or_else(to_login)?;
        match resolve_token(&state.pool, token, TokenKind::Web).await {
            Ok(Some(user)) => Ok(WebUser(user)),
            Ok(None) => Err(to_login()),
            Err(e) => {
                tracing::error!("erro ao resolver sessão web: {e}");
                Err(error_page_anonymous(StatusCode::INTERNAL_SERVER_ERROR))
            }
        }
    }
}

/// Como `WebUser`, mas exige administrador; senão responde 403.
pub struct AdminUser(pub User);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let WebUser(user) = WebUser::from_request_parts(parts, state).await?;
        if user.is_admin {
            Ok(AdminUser(user))
        } else {
            Err(error_page(
                StatusCode::FORBIDDEN,
                user,
                "Acesso restrito ao administrador",
                "Esta área é só para quem administra o painel.",
            ))
        }
    }
}

#[derive(Template, WebTemplate)]
#[template(path = "erro.html")]
struct ErrorPage {
    user: User,
    active: &'static str,
    title: &'static str,
    message: &'static str,
}

/// Página de erro dentro do layout do painel, com o status indicado.
pub fn error_page(
    status: StatusCode,
    user: User,
    title: &'static str,
    message: &'static str,
) -> Response {
    let page = ErrorPage {
        user,
        active: "",
        title,
        message,
    };
    (status, page).into_response()
}

fn error_page_anonymous(status: StatusCode) -> Response {
    (status, "Erro interno").into_response()
}

/// Origem (`esquema://host[:porta]`, minúsculas, sem porta padrão) de uma URL.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let authority = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let default_port = match scheme.as_str() {
        "http" => ":80",
        "https" => ":443",
        _ => "",
    };
    let authority = authority
        .strip_suffix(default_port)
        .filter(|_| !default_port.is_empty())
        .unwrap_or(&authority);
    Some(format!("{scheme}://{authority}"))
}

/// Recusa (403) requisições que alteram estado vindas de outra origem.
/// Sem cabeçalho `Origin`, deixa passar (o cookie `SameSite=Strict` cobre).
pub async fn require_same_origin(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    if !matches!(*req.method(), Method::GET | Method::HEAD)
        && let Some(origin) = req.headers().get(header::ORIGIN)
    {
        let expected = origin_of(&state.config.public_url);
        let received = origin.to_str().ok().and_then(origin_of);
        if expected.is_none() || expected != received {
            return (StatusCode::FORBIDDEN, "Origem não permitida").into_response();
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_normalization() {
        assert_eq!(origin_of("https://Desk.PCEP.com.br/"), Some("https://desk.pcep.com.br".into()));
        assert_eq!(origin_of("https://x.com:443"), Some("https://x.com".into()));
        assert_eq!(origin_of("http://x.com:80/a/b"), Some("http://x.com".into()));
        assert_eq!(origin_of("http://localhost:8080"), Some("http://localhost:8080".into()));
        assert_ne!(origin_of("http://x.com"), origin_of("https://x.com"));
        assert_eq!(origin_of("null"), None);
        assert_eq!(origin_of("http://"), None);
    }

    #[test]
    fn cookie_parsing() {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, "a=1; pcep_sessao=abc123; b=2".parse().unwrap());
        assert_eq!(session_token(&h), Some("abc123"));
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, "pcep_sessao=".parse().unwrap());
        assert_eq!(session_token(&h), None);
        assert_eq!(session_token(&HeaderMap::new()), None);
    }

    #[test]
    fn cookie_flags() {
        let c = session_cookie("tok", true);
        assert!(c.contains("HttpOnly") && c.contains("Secure") && c.contains("Max-Age=43200"));
        assert!(!session_cookie("tok", false).contains("Secure"));
        assert!(clear_cookie(true).contains("Max-Age=0"));
    }
}
