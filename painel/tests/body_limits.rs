mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::{send, test_app};
use serde_json::json;

fn post_raw(uri: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

/// JSON válido com `id` e preenchimento até passar de 4 KiB (mas longe de 64 KiB).
fn body_over_4k() -> String {
    let body = json!({"id": "111222333", "nonce": "n1", "pad": "a".repeat(5000)}).to_string();
    assert!(body.len() > 4096 && body.len() < 65536);
    body
}

#[tokio::test]
async fn unauthenticated_routes_limit_body_to_4k() {
    let app = test_app().await;
    for uri in [
        "/api/heartbeat",
        "/api/sysinfo",
        "/api/sysinfo_ver",
        "/api/audit/conn",
        "/api/audit/file",
        "/api/audit/alarm",
    ] {
        let (status, _, _) = send(&app, post_raw(uri, body_over_4k())).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{uri}");
    }
    for table in [
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM devices"),
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_raw"),
    ] {
        assert_eq!(table.fetch_one(&app.pool).await.unwrap(), 0);
    }
}

#[tokio::test]
async fn unauthenticated_routes_accept_body_just_under_4k() {
    let app = test_app().await;
    let pad = "a".repeat(4096 - r#"{"id":"111222333","pad":""}"#.len());
    let body = json!({"id": "111222333", "pad": pad}).to_string();
    assert_eq!(body.len(), 4096);
    let (status, _, _) = send(&app, post_raw("/api/heartbeat", body)).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn other_api_routes_keep_64k_limit() {
    let app = test_app().await;
    // Corpo de ~5 KiB em /api/login: não é barrado pelo limite (falha por outro motivo).
    let (status, _, _) = send(&app, post_raw("/api/login", body_over_4k())).await;
    assert_ne!(status, StatusCode::PAYLOAD_TOO_LARGE);
    let big = json!({"id": "1", "pad": "a".repeat(70_000)}).to_string();
    let (status, _, _) = send(&app, post_raw("/api/login", big)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}
