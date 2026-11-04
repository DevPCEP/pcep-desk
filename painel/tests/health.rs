mod common;

use axum::{body::Body, http::Request, http::StatusCode};
use common::{send, test_app};
use painel::clock;

#[tokio::test]
async fn health_ok() {
    let app = test_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/saude")
        .body(Body::empty())
        .unwrap();
    let (status, _headers, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn migrations_create_tables() {
    let app = test_app().await;
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN \
         ('users','tokens','user_devices','devices','clients','computers','ab_personal','ab_peers','ab_tags','connections','audit_raw')",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(n, 11);
}

#[test]
fn format_ts_brt() {
    // 12:00 UTC == 09:00 em UTC-03:00
    assert_eq!(clock::format_ts(1762257600), "04/11/2025 09:00");
}
