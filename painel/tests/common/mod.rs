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
