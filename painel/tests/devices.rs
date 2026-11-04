mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use common::{json_req, send, test_app};
use painel::{devices, users};
use serde_json::json;

fn raw_req(uri: &str, body: String) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn heartbeat_creates_and_touches_device() {
    let app = test_app().await;
    let (status, _, body) = send(
        &app,
        json_req(
            "POST",
            "/api/heartbeat",
            json!({"id": "111222333", "uuid": "dXU=", "ver": "1004005"}),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "{}");
    let d = devices::get(&app.pool, "111222333").await.unwrap().unwrap();
    assert_eq!(d.version, "1004005");
    assert_eq!(d.uuid, "dXU=");
}

#[tokio::test]
async fn heartbeat_accepts_missing_content_type_and_trims_id() {
    let app = test_app().await;
    let (status, _, body) = send(
        &app,
        raw_req("/api/heartbeat", r#"{"id":"  555  ","uuid":" u ","ver":"1"}"#.into()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "{}");
    let d = devices::get(&app.pool, "555").await.unwrap().unwrap();
    assert_eq!(d.uuid, "u");
}

#[tokio::test]
async fn heartbeat_rejects_blank_or_long_id() {
    let app = test_app().await;
    for id in ["".to_string(), "   ".to_string(), "9".repeat(65)] {
        let (status, _, body) = send(
            &app,
            json_req("POST", "/api/heartbeat", json!({"id": id}), None),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, r#"{"error":"Requisição inválida"}"#);
    }
    let (status, _, _) = send(&app, json_req("POST", "/api/heartbeat", json!({}), None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM devices")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn sysinfo_updates_fields() {
    let app = test_app().await;
    let (status, headers, body) = send(
        &app,
        json_req(
            "POST",
            "/api/sysinfo",
            json!({
                "id": "111222333", "uuid": "dXU=", "hostname": "PC-CAIXA",
                "os": "Windows / 11 Pro", "username": "caixa", "cpu": "i5",
                "memory": "8GB", "version": "1.4.5",
                "preset-address-book-password": "x"
            }),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "SYSINFO_UPDATED");
    assert!(
        headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    let d = devices::get(&app.pool, "111222333").await.unwrap().unwrap();
    assert_eq!(d.hostname, "PC-CAIXA");
    assert_eq!(d.os, "Windows / 11 Pro");
    assert_eq!(d.username, "caixa");
    assert_eq!(d.cpu, "i5");
    assert_eq!(d.memory, "8GB");

    // Nenhuma coluna guarda o valor da senha do catálogo.
    let rows: Vec<(String, String, String, String, String, String, String)> =
        sqlx::query_as("SELECT peer_id, uuid, version, hostname, os, username, cpu || memory FROM devices")
            .fetch_all(&app.pool)
            .await
            .unwrap();
    for r in rows {
        for col in [r.0, r.1, r.2, r.3, r.4, r.5, r.6] {
            assert_ne!(col, "x");
        }
    }
}

#[tokio::test]
async fn sysinfo_without_id() {
    let app = test_app().await;
    for payload in [json!({"hostname": "PC"}), json!({"id": "  "}), json!({"id": "9".repeat(65)})] {
        let (status, _, body) = send(&app, json_req("POST", "/api/sysinfo", payload, None)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "ID_NOT_FOUND");
    }
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM devices")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn sysinfo_truncates_long_fields() {
    let app = test_app().await;
    let long = "é".repeat(400);
    send(
        &app,
        json_req(
            "POST",
            "/api/sysinfo",
            json!({"id": "1", "hostname": long}),
            None,
        ),
    )
    .await;
    let d = devices::get(&app.pool, "1").await.unwrap().unwrap();
    assert_eq!(d.hostname.chars().count(), 255);
}

#[tokio::test]
async fn heartbeat_keeps_sysinfo_data() {
    let app = test_app().await;
    send(
        &app,
        json_req(
            "POST",
            "/api/sysinfo",
            json!({"id": "7", "uuid": "u", "hostname": "PC-7"}),
            None,
        ),
    )
    .await;
    let first = devices::get(&app.pool, "7").await.unwrap().unwrap();
    devices::touch(&app.pool, "7", "u", "1004005", "", first.last_seen_at + 100)
        .await
        .unwrap();
    let d = devices::get(&app.pool, "7").await.unwrap().unwrap();
    assert_eq!(d.hostname, "PC-7");
    assert_eq!(d.version, "1004005");
    assert_eq!(d.first_seen_at, first.first_seen_at);
    assert_eq!(d.last_seen_at, first.last_seen_at + 100);
}

#[tokio::test]
async fn sysinfo_ver_returns_empty_text() {
    let app = test_app().await;
    let (status, _, body) = send(&app, raw_req("/api/sysinfo_ver", String::new())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "");
}

#[tokio::test]
async fn heartbeat_rejects_garbage_and_oversize() {
    let app = test_app().await;
    let (status, _, body) = send(&app, raw_req("/api/heartbeat", "não é json".into())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, r#"{"error":"Requisição inválida"}"#);

    let big = json!({"id": "1", "pad": "a".repeat(70_000)}).to_string();
    assert!(big.len() > 65536);
    let (status, _, _) = send(&app, raw_req("/api/heartbeat", big.clone())).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    let (status, _, _) = send(&app, raw_req("/api/sysinfo", big)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    let (status, _, _) = send(&app, raw_req("/api/sysinfo", "lixo".into())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM devices")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn unlinked_excludes_known() {
    let app = test_app().await;
    for (i, peer) in ["100", "200", "300"].iter().enumerate() {
        devices::touch(&app.pool, peer, "", "1", "", 1000 + i as i64)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO clients (id, name, created_at) VALUES (1, 'Cliente', 0)")
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO computers (client_id, peer_id, label, created_at) VALUES (1, '100', 'Caixa', 0)",
    )
    .execute(&app.pool)
    .await
    .unwrap();
    let user_id = users::create(&app.pool, "ana", "Ana", "senha12345", false)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_devices (peer_id, user_id, last_login_at) VALUES ('200', ?, 0)")
        .bind(user_id)
        .execute(&app.pool)
        .await
        .unwrap();

    let list = devices::list_unlinked(&app.pool).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].peer_id, "300");
}

#[test]
fn is_online_window() {
    assert!(devices::is_online(1000, 1060));
    assert!(!devices::is_online(1000, 1061));
}

#[tokio::test]
async fn heartbeat_records_origin_ip() {
    let app = test_app().await;
    // Com X-Forwarded-For vale o ÚLTIMO endereço (o acrescentado pelo Caddy).
    common::heartbeat_from(&app, "111222333", "6.6.6.6, 203.0.113.7").await;
    let ip: String = sqlx::query_scalar("SELECT last_ip FROM devices WHERE peer_id = '111222333'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(ip, "203.0.113.7");

    // Um novo heartbeat de outro IP substitui o anterior.
    common::heartbeat_from(&app, "111222333", "198.51.100.9").await;
    let ip: String = sqlx::query_scalar("SELECT last_ip FROM devices WHERE peer_id = '111222333'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(ip, "198.51.100.9");

    // Sysinfo não mexe no IP.
    let (status, _, _) = send(
        &app,
        common::json_req_from(
            "POST",
            "/api/sysinfo",
            json!({"id": "111222333", "hostname": "PC"}),
            "7.7.7.7",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ip: String = sqlx::query_scalar("SELECT last_ip FROM devices WHERE peer_id = '111222333'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(ip, "198.51.100.9");
}
