mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::{TestApp, json_req, send, test_app};
use painel::connections::{self, ConnEvent, ConnEventKind, HistoryFilter, HistoryRow};
use painel::users;
use serde_json::{Value, json};
use sqlx::Row;

async fn post(app: &TestApp, uri: &str, body: Value) -> (StatusCode, String) {
    let (status, _, text) = send(app, json_req("POST", uri, body, None)).await;
    (status, text)
}

/// Envia o evento e confirma 200 com corpo vazio (contrato do app).
async fn post_ok(app: &TestApp, uri: &str, body: Value) {
    let (status, text) = post(app, uri, body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(text, "");
}

async fn count(app: &TestApp, table: &str) -> i64 {
    // Nomes de tabela fixos nos testes; o sqlx só aceita SQL literal.
    let sql = match table {
        "connections" => "SELECT COUNT(*) FROM connections",
        "audit_raw" => "SELECT COUNT(*) FROM audit_raw",
        _ => unreachable!(),
    };
    sqlx::query_scalar(sql).fetch_one(&app.pool).await.unwrap()
}

fn new_ev(id: &str, conn_id: i64) -> Value {
    json!({"id": id, "uuid": "dXU=", "conn_id": conn_id, "session_id": 1, "action": "new", "ip": "200.1.2.3"})
}

fn authorized_ev(id: &str, conn_id: i64, operator: &str) -> Value {
    json!({"id": id, "uuid": "dXU=", "conn_id": conn_id, "session_id": 1,
           "peer": [operator, "Maria – PCEP"], "type": 0})
}

fn close_ev(id: &str, conn_id: i64) -> Value {
    json!({"id": id, "uuid": "dXU=", "conn_id": conn_id, "session_id": 1, "action": "close"})
}

fn filter() -> HistoryFilter {
    HistoryFilter {
        user_id: None,
        client_id: None,
        from: None,
        to: None,
        limit: 100,
    }
}

async fn all_rows(app: &TestApp) -> Vec<HistoryRow> {
    connections::query(&app.pool, &filter()).await.unwrap()
}

#[tokio::test]
async fn full_lifecycle() {
    let app = test_app().await;
    post_ok(&app, "/api/audit/conn", new_ev("111222333", 7)).await;
    post_ok(&app, "/api/audit/conn", authorized_ev("111222333", 7, "987654321")).await;
    post_ok(&app, "/api/audit/conn", close_ev("111222333", 7)).await;

    assert_eq!(count(&app, "connections").await, 1);
    let rows = all_rows(&app).await;
    let r = &rows[0];
    assert_eq!(r.peer_id, "111222333");
    assert_eq!(r.ip, "200.1.2.3");
    assert_eq!(r.operator_name, "Maria – PCEP");
    assert_eq!(r.conn_type, Some(0));
    assert!(r.authorized_at.is_some());
    assert!(r.ended_at.is_some());
    assert_eq!(connections::status_label(r), "Conectado");
    assert_eq!(connections::type_label(r.conn_type), "Acesso remoto");
}

#[tokio::test]
async fn user_linked_by_operator_device() {
    let app = test_app().await;
    let maria = users::create(&app.pool, "maria", "Maria Silva", "senha-forte-123", false)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_devices (peer_id, user_id, uuid, last_login_at) VALUES ('987654321', ?, '', 1)")
        .bind(maria)
        .execute(&app.pool)
        .await
        .unwrap();
    post_ok(&app, "/api/audit/conn", new_ev("111222333", 7)).await;
    post_ok(&app, "/api/audit/conn", authorized_ev("111222333", 7, "987654321")).await;

    let rows = all_rows(&app).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].user_display_name.as_deref(), Some("Maria Silva"));
    let uid: Option<i64> = sqlx::query_scalar("SELECT user_id FROM connections")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(uid, Some(maria));
}

#[tokio::test]
async fn duplicate_nonce_ignored() {
    let app = test_app().await;
    let mut ev = new_ev("111222333", 7);
    ev["nonce"] = json!("abc");
    post_ok(&app, "/api/audit/conn", ev.clone()).await;
    post_ok(&app, "/api/audit/conn", ev).await;
    assert_eq!(count(&app, "connections").await, 1);
    assert_eq!(count(&app, "audit_raw").await, 1);
}

#[tokio::test]
async fn failed_apply_does_not_burn_nonce() {
    let app = test_app().await;
    // Sem a tabela, aplicar o evento falha (500); o nonce não pode ficar marcado.
    sqlx::query("ALTER TABLE connections RENAME TO connections_off")
        .execute(&app.pool)
        .await
        .unwrap();
    let mut ev = new_ev("111222333", 7);
    ev["nonce"] = json!("retry-1");
    let (status, _) = post(&app, "/api/audit/conn", ev.clone()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(count(&app, "audit_raw").await, 0);

    sqlx::query("ALTER TABLE connections_off RENAME TO connections")
        .execute(&app.pool)
        .await
        .unwrap();
    post_ok(&app, "/api/audit/conn", ev).await;
    assert_eq!(count(&app, "connections").await, 1);
    assert_eq!(count(&app, "audit_raw").await, 1);
}

#[tokio::test]
async fn authorized_before_new_creates_single_record() {
    let app = test_app().await;
    post_ok(&app, "/api/audit/conn", authorized_ev("111222333", 7, "987654321")).await;
    assert_eq!(count(&app, "connections").await, 1);
    let rows = all_rows(&app).await;
    assert!(rows[0].authorized_at.is_some());
    assert!(rows[0].ended_at.is_none());

    post_ok(&app, "/api/audit/conn", close_ev("111222333", 7)).await;
    assert_eq!(count(&app, "connections").await, 1);
    let rows = all_rows(&app).await;
    assert!(rows[0].ended_at.is_some());
}

#[tokio::test]
async fn close_without_open_is_ignored() {
    let app = test_app().await;
    post_ok(&app, "/api/audit/conn", close_ev("111222333", 7)).await;
    assert_eq!(count(&app, "connections").await, 0);
    assert_eq!(count(&app, "audit_raw").await, 1);
}

#[tokio::test]
async fn refused_attempt_status() {
    let app = test_app().await;
    post_ok(&app, "/api/audit/conn", new_ev("111222333", 7)).await;
    let rows = all_rows(&app).await;
    assert_eq!(connections::status_label(&rows[0]), "Aguardando aceite");
    post_ok(&app, "/api/audit/conn", close_ev("111222333", 7)).await;
    let rows = all_rows(&app).await;
    assert_eq!(connections::status_label(&rows[0]), "Não concluída");
}

#[tokio::test]
async fn type_labels() {
    assert_eq!(connections::type_label(Some(1)), "Transferência de arquivos");
    assert_eq!(connections::type_label(Some(2)), "Redirecionamento de porta");
    assert_eq!(connections::type_label(Some(3)), "Câmera");
    assert_eq!(connections::type_label(Some(4)), "Terminal");
    assert_eq!(connections::type_label(Some(9)), "—");
    assert_eq!(connections::type_label(None), "—");
}

#[tokio::test]
async fn session_id_large_number() {
    let app = test_app().await;
    let ev = new_ev("111222333", 7);
    // Escrito como texto bruto para garantir o valor exato no fio.
    let raw = ev.to_string().replace("\"session_id\":1", "\"session_id\":18446744073709551615");
    let req = Request::builder()
        .method("POST")
        .uri("/api/audit/conn")
        .body(Body::from(raw))
        .unwrap();
    let (status, _, text) = send(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(text, "");
    let sid: String = sqlx::query_scalar("SELECT session_id FROM connections")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(sid, "18446744073709551615");
}

#[tokio::test]
async fn audit_rejects_garbage() {
    let app = test_app().await;
    for uri in ["/api/audit/conn", "/api/audit/file", "/api/audit/alarm"] {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .body(Body::from("{"))
            .unwrap();
        let (status, _, text) = send(&app, req).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(text, r#"{"error":"Requisição inválida"}"#);
    }
    assert_eq!(count(&app, "audit_raw").await, 0);
}

#[tokio::test]
async fn audit_rejects_missing_or_long_id_and_non_object() {
    let app = test_app().await;
    for body in [
        json!({"conn_id": 1, "action": "close"}),
        json!({"id": "", "conn_id": 1, "action": "close"}),
        json!({"id": "x".repeat(65), "conn_id": 1, "action": "close"}),
        json!([1, 2]),
    ] {
        let (status, _) = post(&app, "/api/audit/conn", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _) = post(&app, "/api/audit/file", json!({"type": 1})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(count(&app, "audit_raw").await, 0);
}

#[tokio::test]
async fn audit_rejects_oversize_body() {
    let app = test_app().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/audit/alarm")
        .body(Body::from(format!(r#"{{"id":"1","info":"{}"}}"#, "a".repeat(70_000))))
        .unwrap();
    let (status, _, _) = send(&app, req).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(count(&app, "audit_raw").await, 0);
}

#[tokio::test]
async fn file_and_alarm_stored_raw() {
    let app = test_app().await;
    post_ok(
        &app,
        "/api/audit/file",
        json!({"id": "111222333", "peer_id": "987654321", "type": 0, "path": "C:\\x.txt",
               "is_file": true, "info": "{\"ip\":\"1.2.3.4\"}", "nonce": "f1"}),
    )
    .await;
    post_ok(
        &app,
        "/api/audit/alarm",
        json!({"id": "111222333", "typ": 0, "info": "{}", "conn_id": 7, "nonce": "a1"}),
    )
    .await;
    // Reenvio com o mesmo nonce não duplica.
    post_ok(
        &app,
        "/api/audit/alarm",
        json!({"id": "111222333", "typ": 0, "info": "{}", "conn_id": 7, "nonce": "a1"}),
    )
    .await;

    let rows = sqlx::query("SELECT kind, peer_id, body FROM audit_raw ORDER BY id")
        .fetch_all(&app.pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<String, _>("kind"), "file");
    assert_eq!(rows[1].get::<String, _>("kind"), "alarm");
    assert_eq!(rows[0].get::<String, _>("peer_id"), "111222333");
    assert!(rows[0].get::<String, _>("body").contains("C:\\\\x.txt"));
    assert_eq!(count(&app, "connections").await, 0);
}

#[tokio::test]
async fn unknown_conn_format_stored_raw() {
    let app = test_app().await;
    post_ok(
        &app,
        "/api/audit/conn",
        json!({"id": "111222333", "conn_id": 7, "action": "futuro"}),
    )
    .await;
    assert_eq!(count(&app, "connections").await, 0);
    assert_eq!(count(&app, "audit_raw").await, 1);
}

#[tokio::test]
async fn operator_name_and_ip_truncated() {
    let app = test_app().await;
    let mut ev = new_ev("111222333", 7);
    ev["ip"] = json!("9".repeat(100));
    post_ok(&app, "/api/audit/conn", ev).await;
    let mut ev = authorized_ev("111222333", 7, "987654321");
    ev["peer"] = json!(["987654321", "é".repeat(200)]);
    post_ok(&app, "/api/audit/conn", ev).await;
    let rows = all_rows(&app).await;
    assert_eq!(rows[0].ip.chars().count(), 64);
    assert_eq!(rows[0].operator_name.chars().count(), 120);
}

#[tokio::test]
async fn query_filters() {
    let app = test_app().await;
    let ana = users::create(&app.pool, "ana", "Ana", "senha-forte-123", false)
        .await
        .unwrap();
    let bia = users::create(&app.pool, "bia", "Bia", "senha-forte-123", false)
        .await
        .unwrap();
    for (peer, user) in [("1001", ana), ("1002", bia)] {
        sqlx::query("INSERT INTO user_devices (peer_id, user_id, uuid, last_login_at) VALUES (?, ?, '', 1)")
            .bind(peer)
            .bind(user)
            .execute(&app.pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO clients (id, name, created_at) VALUES (1, 'Padaria', 1), (2, 'Oficina', 1)")
        .execute(&app.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO computers (client_id, peer_id, label, created_at) VALUES (1, '555', 'Caixa', 1), (2, '666', 'Balcão', 1)")
        .execute(&app.pool)
        .await
        .unwrap();
    post_ok(&app, "/api/audit/conn", authorized_ev("555", 1, "1001")).await;
    post_ok(&app, "/api/audit/conn", authorized_ev("666", 2, "1002")).await;

    assert_eq!(all_rows(&app).await.len(), 2);

    let only_ana = connections::query(&app.pool, &HistoryFilter { user_id: Some(ana), ..filter() })
        .await
        .unwrap();
    assert_eq!(only_ana.len(), 1);
    assert_eq!(only_ana[0].peer_id, "555");
    assert_eq!(only_ana[0].computer_label.as_deref(), Some("Caixa"));
    assert_eq!(only_ana[0].client_name.as_deref(), Some("Padaria"));

    let only_oficina = connections::query(&app.pool, &HistoryFilter { client_id: Some(2), ..filter() })
        .await
        .unwrap();
    assert_eq!(only_oficina.len(), 1);
    assert_eq!(only_oficina[0].peer_id, "666");
    assert_eq!(only_oficina[0].client_id, Some(2));

    // Intervalo: `from` inclusivo, `to` exclusivo; limite respeitado.
    let started: i64 = sqlx::query_scalar("SELECT MIN(started_at) FROM connections")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    let none = connections::query(&app.pool, &HistoryFilter { to: Some(started), ..filter() })
        .await
        .unwrap();
    assert!(none.is_empty());
    let all = connections::query(&app.pool, &HistoryFilter { from: Some(started), ..filter() })
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
    let one = connections::query(&app.pool, &HistoryFilter { limit: 1, ..filter() })
        .await
        .unwrap();
    assert_eq!(one.len(), 1);
}

#[tokio::test]
async fn authorized_then_new_then_close_single_record() {
    let app = test_app().await;
    post_ok(&app, "/api/audit/conn", authorized_ev("111222333", 7, "987654321")).await;
    post_ok(&app, "/api/audit/conn", new_ev("111222333", 7)).await;
    post_ok(&app, "/api/audit/conn", close_ev("111222333", 7)).await;

    assert_eq!(count(&app, "connections").await, 1);
    let rows = all_rows(&app).await;
    assert_eq!(rows[0].ip, "200.1.2.3");
    assert!(rows[0].authorized_at.is_some());
    assert!(rows[0].ended_at.is_some());
    assert_eq!(connections::status_label(&rows[0]), "Conectado");

    // Uma nova conexão com o mesmo conn_id depois do close é outro registro.
    post_ok(&app, "/api/audit/conn", new_ev("111222333", 7)).await;
    assert_eq!(count(&app, "connections").await, 2);
}

#[tokio::test]
async fn authorized_repeated_keeps_first_time_and_user() {
    let app = test_app().await;
    let maria = users::create(&app.pool, "maria", "Maria Silva", "senha-forte-123", false)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_devices (peer_id, user_id, uuid, last_login_at) VALUES ('987654321', ?, '', 1)")
        .bind(maria)
        .execute(&app.pool)
        .await
        .unwrap();
    let ev = |operator: &str| ConnEvent {
        peer_id: "111222333".into(),
        conn_id: 7,
        session_id: "1".into(),
        kind: ConnEventKind::Authorized {
            operator_peer_id: operator.into(),
            operator_name: "Maria".into(),
            conn_type: Some(0),
        },
    };
    connections::apply_conn_event(&app.pool, &ev("987654321"), 100).await.unwrap();
    // Reenvio sem nonce, com operador desconhecido e horário posterior.
    connections::apply_conn_event(&app.pool, &ev("000000000"), 200).await.unwrap();

    assert_eq!(count(&app, "connections").await, 1);
    let (authorized_at, user_id): (i64, Option<i64>) =
        sqlx::query_as("SELECT authorized_at, user_id FROM connections")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(authorized_at, 100);
    assert_eq!(user_id, Some(maria));
}
