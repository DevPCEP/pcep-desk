mod common;

use axum::{
    body::Body,
    http::{Request, header},
};
use common::{form_req, get_req, send, session_cookie_value, test_app, web_login, TestApp};
use painel::{
    auth::tokens::{TokenKind, issue_token},
    users,
};

async fn setup() -> TestApp {
    let app = test_app().await;
    users::create(&app.pool, "maria", "Maria – PCEP", "senha12345", false)
        .await
        .unwrap();
    app
}

fn location(headers: &axum::http::HeaderMap) -> &str {
    headers.get(header::LOCATION).unwrap().to_str().unwrap()
}

#[tokio::test]
async fn pages_require_login() {
    let app = setup().await;
    let (status, headers, _) = send(&app, get_req("/clientes", None)).await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), "/entrar");
}

#[tokio::test]
async fn root_redirects_to_clientes() {
    let app = setup().await;
    let (status, headers, _) = send(&app, get_req("/", None)).await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), "/clientes");
}

#[tokio::test]
async fn login_page_renders_form() {
    let app = setup().await;
    let (status, _, body) = send(&app, get_req("/entrar", None)).await;
    assert_eq!(status, 200);
    assert!(body.contains("name=\"usuario\""));
    assert!(body.contains("name=\"senha\""));
}

#[tokio::test]
async fn web_login_sets_cookie() {
    let app = setup().await;
    let (status, headers, _) = send(
        &app,
        form_req(
            "POST",
            "/entrar",
            &[("usuario", "maria"), ("senha", "senha12345")],
            None,
        ),
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), "/clientes");
    let cookie = headers
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("pcep_sessao="), "{cookie}");
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("Path=/"));
    assert!(cookie.contains("Max-Age=43200"));
    // test_app usa cookie_secure = false.
    assert!(!cookie.contains("Secure"));

    // O cookie dá acesso à página.
    let value = session_cookie_value(&headers).unwrap();
    let (status, _, body) = send(&app, get_req("/clientes", Some(&value))).await;
    assert_eq!(status, 200);
    assert!(body.contains("Maria – PCEP"));
    assert!(!body.contains("Colaboradores"), "menu admin vazou");
}

#[tokio::test]
async fn web_login_wrong_password_shows_error() {
    let app = setup().await;
    let (status, headers, body) = send(
        &app,
        form_req(
            "POST",
            "/entrar",
            &[("usuario", "maria"), ("senha", "errada123")],
            None,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Usuário ou senha incorretos"));
    assert!(session_cookie_value(&headers).is_none());
}

#[tokio::test]
async fn web_login_unknown_user_same_message() {
    let app = setup().await;
    let (status, _, body) = send(
        &app,
        form_req(
            "POST",
            "/entrar",
            &[("usuario", "ninguem"), ("senha", "qualquer123")],
            None,
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Usuário ou senha incorretos"));
}

#[tokio::test]
async fn web_login_disabled_user_message() {
    let app = setup().await;
    users::set_active(&app.pool, 1, false).await.unwrap();
    let (_, _, wrong) = send(
        &app,
        form_req("POST", "/entrar", &[("usuario", "maria"), ("senha", "errada123")], None),
    )
    .await;
    assert!(!wrong.contains("Usuário desativado"));
    let (status, _, body) = send(
        &app,
        form_req("POST", "/entrar", &[("usuario", "maria"), ("senha", "senha12345")], None),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Usuário desativado"));
}

#[tokio::test]
async fn web_login_lockout_shared_with_app() {
    let app = setup().await;
    // 5 falhas pelo web (chave sem diferenciar maiúsculas) bloqueiam inclusive a API.
    for _ in 0..5 {
        let (_, _, body) = send(
            &app,
            form_req("POST", "/entrar", &[("usuario", " MARIA "), ("senha", "errada123")], None),
        )
        .await;
        assert!(body.contains("Usuário ou senha incorretos"));
    }
    let (status, headers, body) = send(
        &app,
        form_req("POST", "/entrar", &[("usuario", "maria"), ("senha", "senha12345")], None),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Muitas tentativas. Aguarde 15 minutos."));
    assert!(session_cookie_value(&headers).is_none());
    let (_, _, api) = send(
        &app,
        common::json_req(
            "POST",
            "/api/login",
            serde_json::json!({"username": "maria", "password": "senha12345"}),
            None,
        ),
    )
    .await;
    assert!(api.contains("Muitas tentativas"));
}

#[tokio::test]
async fn app_token_not_valid_as_web_cookie() {
    let app = setup().await;
    let token = issue_token(&app.pool, 1, TokenKind::App).await.unwrap();
    let (status, headers, _) = send(&app, get_req("/clientes", Some(&token))).await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), "/entrar");
}

#[tokio::test]
async fn logout_revokes_web_token_and_clears_cookie() {
    let app = setup().await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, headers, _) = send(&app, form_req("POST", "/sair", &[], Some(&cookie))).await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), "/entrar");
    let set = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(set.starts_with("pcep_sessao=;"), "{set}");
    assert!(set.contains("Max-Age=0"));
    let (status, headers, _) = send(&app, get_req("/clientes", Some(&cookie))).await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), "/entrar");
}

#[tokio::test]
async fn cross_origin_post_forbidden() {
    let app = setup().await;
    let req = Request::builder()
        .method("POST")
        .uri("/sair")
        .header(header::ORIGIN, "https://malicioso.example")
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = send(&app, req).await;
    assert_eq!(status, 403);

    // Também vale para o login.
    let req = Request::builder()
        .method("POST")
        .uri("/entrar")
        .header(header::ORIGIN, "https://malicioso.example")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("usuario=maria&senha=senha12345"))
        .unwrap();
    let (status, _, _) = send(&app, req).await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn same_origin_post_allowed() {
    let app = setup().await;
    // public_url do test_app é http://localhost.
    for origin in ["http://localhost", "http://LOCALHOST:80"] {
        let req = Request::builder()
            .method("POST")
            .uri("/sair")
            .header(header::ORIGIN, origin)
            .body(Body::empty())
            .unwrap();
        let (status, _, _) = send(&app, req).await;
        assert_eq!(status, 303, "origin {origin}");
    }
    // Origin "null" (ex.: iframe sandbox) é recusado.
    let req = Request::builder()
        .method("POST")
        .uri("/sair")
        .header(header::ORIGIN, "null")
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = send(&app, req).await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn origin_check_does_not_apply_to_api() {
    let app = setup().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/logout")
        .header(header::ORIGIN, "https://qualquer.example")
        .body(Body::empty())
        .unwrap();
    let (status, _, _) = send(&app, req).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn static_assets_are_served() {
    let app = setup().await;
    let (status, _, body) = send(&app, get_req("/static/painel.css", None)).await;
    assert_eq!(status, 200);
    assert!(!body.is_empty());
    let (status, _, body) = send(&app, get_req("/static/htmx.min.js", None)).await;
    assert_eq!(status, 200);
    assert!(body.contains("htmx"));
}
