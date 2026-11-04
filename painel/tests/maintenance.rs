mod common;

use common::test_app;
use painel::maintenance;

const NOW: i64 = 1_000_000_000;
const DAY: i64 = 86_400;

async fn count(pool: &sqlx::SqlitePool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn run_deletes_only_old_data() {
    let app = test_app().await;
    let pool = &app.pool;

    // audit_raw: um com 91 dias, outro com 89.
    for (nonce, age) in [("velho", 91 * DAY), ("novo", 89 * DAY)] {
        sqlx::query(
            "INSERT INTO audit_raw (nonce, kind, peer_id, body, received_at) VALUES (?, 'conn', '1', '', ?)",
        )
        .bind(nonce)
        .bind(NOW - age)
        .execute(pool)
        .await
        .unwrap();
    }

    // devices: antigo solto (apaga), recente solto (fica), antigo vinculado a
    // computador (fica) e antigo vinculado a colaborador (fica).
    for (peer, age) in [
        ("100000", 31 * DAY),
        ("200000", 29 * DAY),
        ("300000", 400 * DAY),
        ("400000", 400 * DAY),
    ] {
        sqlx::query("INSERT INTO devices (peer_id, first_seen_at, last_seen_at) VALUES (?, 1, ?)")
            .bind(peer)
            .bind(NOW - age)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO clients (id, name, created_at) VALUES (1, 'Padaria', 1)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO computers (client_id, peer_id, label, created_at) VALUES (1, '300000', 'Caixa', 1)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, username, display_name, password_hash, created_at) VALUES (1, 'ana', 'Ana', 'x', 1)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_devices (peer_id, user_id, last_login_at) VALUES ('400000', 1, 1)")
        .execute(pool)
        .await
        .unwrap();

    // tokens: expirado (apaga) e válido (fica).
    for (hash, expires) in [("expirado", NOW - 1), ("valido", NOW + 1)] {
        sqlx::query("INSERT INTO tokens (user_id, token_hash, kind, created_at, expires_at) VALUES (1, ?, 'web', 1, ?)")
            .bind(hash)
            .bind(expires)
            .execute(pool)
            .await
            .unwrap();
    }

    let report = maintenance::run(pool, NOW).await.unwrap();
    assert_eq!(
        (report.audit_raw, report.devices, report.tokens),
        (1, 1, 1)
    );

    let nonces: Vec<String> = sqlx::query_scalar("SELECT nonce FROM audit_raw")
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(nonces, vec!["novo".to_string()]);
    let mut peers: Vec<String> = sqlx::query_scalar("SELECT peer_id FROM devices")
        .fetch_all(pool)
        .await
        .unwrap();
    peers.sort();
    assert_eq!(peers, vec!["200000", "300000", "400000"]);
    let tokens: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM tokens")
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(tokens, vec!["valido".to_string()]);
}

#[tokio::test]
async fn run_on_empty_database_is_noop() {
    let app = test_app().await;
    let report = maintenance::run(&app.pool, NOW).await.unwrap();
    assert_eq!((report.audit_raw, report.devices, report.tokens), (0, 0, 0));
    assert_eq!(count(&app.pool, "SELECT COUNT(*) FROM devices").await, 0);
}
