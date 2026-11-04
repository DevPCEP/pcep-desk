#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use painel::{
    auth::limiter::LoginLimiter, build_router, config::Config, db, state::AppState,
};
use sqlx::SqlitePool;
use tower::ServiceExt;

pub struct TestApp {
    pub router: Router,
    pub pool: SqlitePool,
}

/// Cria um app de teste com banco SQLite em memória (uma única conexão,
/// para que todas as consultas enxerguem o mesmo banco).
pub async fn test_app() -> TestApp {
    let pool = db::connect("sqlite::memory:").await.expect("banco em memória");
    let config = Config {
        db_url: "sqlite::memory:".to_string(),
        bind: "127.0.0.1:0".to_string(),
        public_url: "http://localhost".to_string(),
        cookie_secure: false,
    };
    let state = AppState {
        pool: pool.clone(),
        config: Arc::new(config),
        limiter: Arc::new(LoginLimiter::new()),
    };
    TestApp {
        router: build_router(state),
        pool,
    }
}

/// Envia a requisição ao router e devolve (status, cabeçalhos, corpo como texto).
pub async fn send(app: &TestApp, req: Request<Body>) -> (StatusCode, HeaderMap, String) {
    let resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("resposta do router");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("corpo da resposta")
        .to_bytes();
    (status, headers, String::from_utf8_lossy(&bytes).into_owned())
}

pub fn json_req(
    method: &str,
    uri: &str,
    body: serde_json::Value,
    bearer: Option<&str>,
) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(t) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::from(body.to_string())).unwrap()
}

/// Codifica pares chave/valor como `application/x-www-form-urlencoded`.
pub fn form_encode(pairs: &[(&str, &str)]) -> String {
    fn enc(s: &str) -> String {
        let mut out = String::new();
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char)
                }
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Requisição de formulário web; `cookie` é o valor do cookie `pcep_sessao`.
pub fn form_req(
    method: &str,
    uri: &str,
    pairs: &[(&str, &str)],
    cookie: Option<&str>,
) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, format!("pcep_sessao={c}"));
    }
    b.body(Body::from(form_encode(pairs))).unwrap()
}

/// GET de página web, opcionalmente com o cookie de sessão.
pub fn get_req(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method("GET").uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, format!("pcep_sessao={c}"));
    }
    b.body(Body::empty()).unwrap()
}

/// Valor do cookie `pcep_sessao` num `Set-Cookie`, se houver.
pub fn session_cookie_value(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|v| v.strip_prefix("pcep_sessao="))
        .map(|rest| rest.split(';').next().unwrap_or("").to_string())
}

/// Faz login web e devolve o valor do cookie de sessão.
pub async fn web_login(app: &TestApp, username: &str, password: &str) -> String {
    let (status, headers, body) = send(
        app,
        form_req(
            "POST",
            "/entrar",
            &[("usuario", username), ("senha", password)],
            None,
        ),
    )
    .await;
    assert_eq!(status, 303, "login web falhou: {body}");
    session_cookie_value(&headers).expect("cookie de sessão")
}
