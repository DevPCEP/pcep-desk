mod common;

use axum::{body::Body, http::Request, http::StatusCode};
use common::{send, test_app};
use painel::{clock, db, users};

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

#[tokio::test]
async fn backup_creates_file() {
    // VACUUM INTO não faz nada em banco em memória, então o teste usa um arquivo real.
    let dir = std::env::temp_dir().join(format!(
        "painel-backup-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let origem = db::connect(&format!("sqlite://{}", dir.join("origem.db").display()))
        .await
        .unwrap();
    users::create(&origem, "maria", "Maria", "senha-forte-123", false)
        .await
        .unwrap();

    let dest = dir.join("copia.db");
    db::backup(&origem, &dest).await.unwrap();
    assert!(dest.exists());

    // Abre a cópia como um banco independente e confere o conteúdo.
    let copia = db::connect(&format!("sqlite://{}", dest.display()))
        .await
        .unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM users WHERE username = 'maria'")
        .fetch_one(&copia)
        .await
        .unwrap();
    assert_eq!(n, 1);
    copia.close().await;

    // Destino já existente é recusado e não é sobrescrito.
    let err = db::backup(&origem, &dest).await.unwrap_err();
    assert!(err.to_string().contains("já existe"), "{err}");

    origem.close().await;
    std::fs::remove_dir_all(&dir).unwrap();
}
