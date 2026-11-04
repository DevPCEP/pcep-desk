mod common;

use common::{json_req, send, test_app, TestApp};
use painel::users;
use serde_json::{Value, json};

async fn setup() -> TestApp {
    let app = test_app().await;
    users::create(&app.pool, "maria", "Maria – PCEP", "senha12345", false)
        .await
        .unwrap();
    app
}

async fn login(app: &TestApp, username: &str, password: &str) -> Value {
    let (status, _, body) = send(
        app,
        json_req(
            "POST",
            "/api/login",
            json!({"username": username, "password": password}),
            None,
        ),
    )
    .await;
    assert_eq!(status, 200, "corpo: {body}");
    serde_json::from_str(&body).unwrap()
}

#[tokio::test]
async fn login_success_returns_token_and_user() {
    let app = setup().await;
    let (status, _, body) = send(
        &app,
        json_req(
            "POST",
            "/api/login",
            json!({"username": "maria", "password": "senha12345", "id": "123456789", "uuid": "u-1", "type": "account"}),
            None,
        ),
    )
    .await;
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["type"], "access_token");
    assert!(!v["access_token"].as_str().unwrap().is_empty());
    assert_eq!(v["user"]["name"], "maria");
    assert_eq!(v["user"]["display_name"], "Maria – PCEP");
    assert_eq!(v["user"]["is_admin"], false);
    assert_eq!(v["user"]["status"], 1);

    let row: (String, String) =
        sqlx::query_as("SELECT uuid, (SELECT username FROM users WHERE id = user_id) FROM user_devices WHERE peer_id = '123456789'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(row, ("u-1".to_string(), "maria".to_string()));
}

#[tokio::test]
async fn login_without_id_does_not_record_device() {
    let app = setup().await;
    let v = login(&app, "maria", "senha12345").await;
    assert_eq!(v["type"], "access_token");
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_devices")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn login_device_moves_to_other_user() {
    let app = setup().await;
    users::create(&app.pool, "joao", "João", "senha12345", false)
        .await
        .unwrap();
    for user in ["maria", "joao"] {
        send(
            &app,
            json_req(
                "POST",
                "/api/login",
                json!({"username": user, "password": "senha12345", "id": "555"}),
                None,
            ),
        )
        .await;
    }
    let owner: String = sqlx::query_scalar(
        "SELECT u.username FROM user_devices d JOIN users u ON u.id = d.user_id WHERE d.peer_id = '555'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(owner, "joao");
}

#[tokio::test]
async fn login_wrong_password_error() {
    let app = setup().await;
    let v = login(&app, "maria", "errada123").await;
    assert_eq!(v, json!({"error": "Usuário ou senha incorretos"}));
    let v = login(&app, "fantasma", "senha12345").await;
    assert_eq!(v, json!({"error": "Usuário ou senha incorretos"}));
}

#[tokio::test]
async fn login_unsupported_type() {
    let app = setup().await;
    let (_, _, body) = send(
        &app,
        json_req(
            "POST",
            "/api/login",
            json!({"username": "maria", "password": "senha12345", "type": "email_code"}),
            None,
        ),
    )
    .await;
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v, json!({"error": "Método de login não suportado"}));
}

#[tokio::test]
async fn login_case_insensitive_username() {
    let app = setup().await;
    let v = login(&app, "MARIA", "senha12345").await;
    assert_eq!(v["type"], "access_token");
    assert_eq!(v["user"]["name"], "maria");
}

#[tokio::test]
async fn login_lockout_after_five_failures() {
    let app = setup().await;
    for _ in 0..5 {
        let v = login(&app, "MARIA", "errada123").await;
        assert_eq!(v, json!({"error": "Usuário ou senha incorretos"}));
    }
    let v = login(&app, "maria", "senha12345").await;
    assert_eq!(v, json!({"error": "Muitas tentativas. Aguarde 15 minutos."}));
}

#[tokio::test]
async fn current_user_ok_and_401() {
    let app = setup().await;
    let token = login(&app, "maria", "senha12345").await["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, _, body) = send(
        &app,
        json_req("POST", "/api/currentUser", json!({"id": "777", "uuid": "u-2"}), Some(&token)),
    )
    .await;
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["name"], "maria");
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_devices WHERE peer_id = '777'")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);

    // corpo vazio com token válido continua valendo a sessão
    let (status, _, body) = send(
        &app,
        axum::http::Request::builder()
            .method("POST")
            .uri("/api/currentUser")
            .header("authorization", format!("Bearer {token}"))
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("maria"));

    let (status, _, _) = send(&app, json_req("POST", "/api/currentUser", json!({}), None)).await;
    assert_eq!(status, 401);
    let (status, _, _) = send(
        &app,
        json_req("POST", "/api/currentUser", json!({}), Some("lixo")),
    )
    .await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn disabled_user_token_rejected() {
    let app = setup().await;
    let token = login(&app, "maria", "senha12345").await["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let id = users::find_by_username(&app.pool, "maria")
        .await
        .unwrap()
        .unwrap()
        .0
        .id;
    users::set_active(&app.pool, id, false).await.unwrap();
    let (status, _, _) = send(
        &app,
        json_req("POST", "/api/currentUser", json!({}), Some(&token)),
    )
    .await;
    assert_eq!(status, 401);
    let v = login(&app, "maria", "senha12345").await;
    assert_eq!(v, json!({"error": "Usuário desativado"}));
    // senha errada não revela que o usuário está desativado
    let v = login(&app, "maria", "errada123").await;
    assert_eq!(v, json!({"error": "Usuário ou senha incorretos"}));
}

#[tokio::test]
async fn logout_revokes() {
    let app = setup().await;
    let token = login(&app, "maria", "senha12345").await["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, _, body) = send(&app, json_req("POST", "/api/logout", json!({}), Some(&token))).await;
    assert_eq!(status, 200);
    assert_eq!(body, "");
    let (status, _, _) = send(
        &app,
        json_req("POST", "/api/currentUser", json!({}), Some(&token)),
    )
    .await;
    assert_eq!(status, 401);
    // sem token ou com token inválido continua 200 vazio
    for bearer in [None, Some("lixo")] {
        let (status, _, body) = send(&app, json_req("POST", "/api/logout", json!({}), bearer)).await;
        assert_eq!(status, 200);
        assert_eq!(body, "");
    }
}

#[tokio::test]
async fn login_options_empty_array() {
    let app = test_app().await;
    let (status, _, body) = send(
        &app,
        axum::http::Request::builder()
            .uri("/api/login-options")
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body, "[]");
}

#[tokio::test]
async fn oversize_body_rejected() {
    let app = setup().await;
    let big = "x".repeat(70_000);
    let (status, _, _) = send(
        &app,
        json_req("POST", "/api/login", json!({"username": big, "password": "x"}), None),
    )
    .await;
    assert_eq!(status, 413);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn login_lockout_concurrent() {
    let app = std::sync::Arc::new(setup().await);
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let app = app.clone();
        set.spawn(async move { login(&app, "maria", "errada123").await });
    }
    let mut wrong = 0;
    let mut blocked = 0;
    while let Some(res) = set.join_next().await {
        let v = res.unwrap();
        match v["error"].as_str().unwrap() {
            "Usuário ou senha incorretos" => wrong += 1,
            "Muitas tentativas. Aguarde 15 minutos." => blocked += 1,
            outro => panic!("erro inesperado: {outro}"),
        }
    }
    assert!(wrong <= 5, "mais de 5 tentativas passaram: {wrong}");
    assert_eq!(wrong + blocked, 20);
    // bloqueado também para a senha certa
    let v = login(&app, "maria", "senha12345").await;
    assert_eq!(v, json!({"error": "Muitas tentativas. Aguarde 15 minutos."}));
}

#[tokio::test]
async fn login_username_too_long_is_rejected_early() {
    let app = setup().await;
    let v = login(&app, &"a".repeat(129), "senha12345").await;
    assert_eq!(v, json!({"error": "Usuário ou senha incorretos"}));
}
