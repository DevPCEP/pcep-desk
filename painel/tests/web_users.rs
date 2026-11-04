mod common;

use common::{form_req, get_req, send, test_app, web_login, TestApp};
use painel::{
    auth::tokens::{TokenKind, issue_token},
    users,
};
use serde_json::json;

/// Banco com o admin `admin` (id 1) e a colaboradora `maria` (id 2).
async fn setup() -> TestApp {
    let app = test_app().await;
    users::create(&app.pool, "admin", "Administrador", "senha-admin1", true)
        .await
        .unwrap();
    users::create(&app.pool, "maria", "Maria – PCEP", "senha12345", false)
        .await
        .unwrap();
    app
}

#[tokio::test]
async fn collaborator_cannot_manage_users() {
    let app = setup().await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, _, body) = send(&app, get_req("/colaboradores", Some(&cookie))).await;
    assert_eq!(status, 403);
    assert!(body.contains("Acesso restrito ao administrador"));
    for (method, uri) in [
        ("GET", "/colaboradores/novo"),
        ("GET", "/colaboradores/1/editar"),
        ("POST", "/colaboradores/1/desativar"),
        ("POST", "/colaboradores/1/senha"),
    ] {
        let (status, _, _) =
            send(&app, form_req(method, uri, &[("senha", "novasenha123")], Some(&cookie))).await;
        assert_eq!(status, 403, "{method} {uri}");
    }
    let (status, _, _) = send(
        &app,
        form_req(
            "POST",
            "/colaboradores",
            &[("usuario", "joao"), ("nome", "João"), ("senha", "senha12345")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 403);
    assert!(users::find_by_username(&app.pool, "joao").await.unwrap().is_none());
}

#[tokio::test]
async fn colaboradores_requires_login() {
    let app = setup().await;
    let (status, headers, _) = send(&app, get_req("/colaboradores", None)).await;
    assert_eq!(status, 303);
    assert_eq!(headers.get("location").unwrap(), "/entrar");
}

#[tokio::test]
async fn admin_sees_menu_and_list() {
    let app = setup().await;
    let cookie = web_login(&app, "admin", "senha-admin1").await;
    let (status, _, body) = send(&app, get_req("/colaboradores", Some(&cookie))).await;
    assert_eq!(status, 200);
    assert!(body.contains("Maria – PCEP"));
    assert!(body.contains("maria"));
    assert!(body.contains("Administrador"));
    assert!(body.contains("Novo colaborador"));
    let (_, _, clientes) = send(&app, get_req("/clientes", Some(&cookie))).await;
    assert!(clientes.contains("href=\"/colaboradores\""));
}

#[tokio::test]
async fn admin_creates_collaborator() {
    let app = setup().await;
    let cookie = web_login(&app, "admin", "senha-admin1").await;
    let (status, _, form) = send(&app, get_req("/colaboradores/novo", Some(&cookie))).await;
    assert_eq!(status, 200);
    assert!(form.contains("Maria – PCEP"), "ajuda do nome de exibição");

    let (status, headers, _) = send(
        &app,
        form_req(
            "POST",
            "/colaboradores",
            &[
                ("usuario", "Joao.Silva"),
                ("nome", "  João – PCEP "),
                ("email", "joao@pcep.example"),
                ("senha", "senha12345"),
            ],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(headers.get("location").unwrap(), "/colaboradores");
    let (_, _, list) = send(&app, get_req("/colaboradores", Some(&cookie))).await;
    assert!(list.contains("João – PCEP"));
    assert!(list.contains("joao.silva"));
    let (u, _) = users::find_by_username(&app.pool, "joao.silva").await.unwrap().unwrap();
    assert_eq!(u.email, "joao@pcep.example");
    assert!(!u.is_admin);

    // O novo colaborador consegue entrar.
    web_login(&app, "joao.silva", "senha12345").await;
}

#[tokio::test]
async fn create_can_make_admin() {
    let app = setup().await;
    let cookie = web_login(&app, "admin", "senha-admin1").await;
    let (status, _, _) = send(
        &app,
        form_req(
            "POST",
            "/colaboradores",
            &[("usuario", "chefe"), ("nome", "Chefe"), ("senha", "senha12345"), ("admin", "on")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 303);
    let (u, _) = users::find_by_username(&app.pool, "chefe").await.unwrap().unwrap();
    assert!(u.is_admin);
}

#[tokio::test]
async fn create_errors_are_shown_in_form() {
    let app = setup().await;
    let cookie = web_login(&app, "admin", "senha-admin1").await;
    // Usuário duplicado.
    let (status, _, body) = send(
        &app,
        form_req(
            "POST",
            "/colaboradores",
            &[("usuario", "maria"), ("nome", "Outra Maria"), ("senha", "senha12345")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("usuário já existe"));
    assert!(body.contains("Outra Maria"), "valores preservados");
    assert!(!body.contains("senha12345"), "senha não é reexibida");
    // Senha curta.
    let (status, _, body) = send(
        &app,
        form_req(
            "POST",
            "/colaboradores",
            &[("usuario", "novo"), ("nome", "Novo"), ("senha", "curta")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("A senha deve ter pelo menos 8 caracteres"));
    // E-mail grande demais.
    let longo = "a".repeat(121);
    let (status, _, body) = send(
        &app,
        form_req(
            "POST",
            "/colaboradores",
            &[("usuario", "novo"), ("nome", "Novo"), ("email", &longo), ("senha", "senha12345")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("E-mail"));
    assert!(users::find_by_username(&app.pool, "novo").await.unwrap().is_none());
}

#[tokio::test]
async fn disable_revokes_web_and_app() {
    let app = setup().await;
    let admin = web_login(&app, "admin", "senha-admin1").await;
    let maria_web = web_login(&app, "maria", "senha12345").await;
    let maria_app = issue_token(&app.pool, 2, TokenKind::App).await.unwrap();
    let (status, _, _) = send(
        &app,
        common::json_req("POST", "/api/currentUser", json!({}), Some(&maria_app)),
    )
    .await;
    assert_eq!(status, 200);

    let (status, headers, _) =
        send(&app, form_req("POST", "/colaboradores/2/desativar", &[], Some(&admin))).await;
    assert_eq!(status, 303);
    assert_eq!(headers.get("location").unwrap(), "/colaboradores");

    let (status, _, _) = send(
        &app,
        common::json_req("POST", "/api/currentUser", json!({}), Some(&maria_app)),
    )
    .await;
    assert_eq!(status, 401);
    let (status, headers, _) = send(&app, get_req("/clientes", Some(&maria_web))).await;
    assert_eq!(status, 303);
    assert_eq!(headers.get("location").unwrap(), "/entrar");

    let (_, _, list) = send(&app, get_req("/colaboradores", Some(&admin))).await;
    assert!(list.contains("Desativado"));
    assert!(list.contains("Reativar"));
}

#[tokio::test]
async fn reactivate_allows_login_again() {
    let app = setup().await;
    let admin = web_login(&app, "admin", "senha-admin1").await;
    send(&app, form_req("POST", "/colaboradores/2/desativar", &[], Some(&admin))).await;
    let (status, _, _) =
        send(&app, form_req("POST", "/colaboradores/2/reativar", &[], Some(&admin))).await;
    assert_eq!(status, 303);
    web_login(&app, "maria", "senha12345").await;
}

#[tokio::test]
async fn admin_cannot_disable_self() {
    let app = setup().await;
    let admin = web_login(&app, "admin", "senha-admin1").await;
    let (status, _, body) =
        send(&app, form_req("POST", "/colaboradores/1/desativar", &[], Some(&admin))).await;
    assert_eq!(status, 200);
    assert!(body.contains("Você não pode desativar a própria conta"));
    assert!(users::get(&app.pool, 1).await.unwrap().unwrap().active);
    // Sessão continua válida.
    let (status, _, _) = send(&app, get_req("/colaboradores", Some(&admin))).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn admin_cannot_remove_own_admin_role() {
    let app = setup().await;
    let admin = web_login(&app, "admin", "senha-admin1").await;
    // Sem o checkbox "admin" marcado.
    let (status, _, body) = send(
        &app,
        form_req(
            "POST",
            "/colaboradores/1",
            &[("nome", "Administrador"), ("email", "")],
            Some(&admin),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Você não pode remover o próprio papel de administrador"));
    assert!(users::get(&app.pool, 1).await.unwrap().unwrap().is_admin);
}

#[tokio::test]
async fn edit_updates_profile_and_role() {
    let app = setup().await;
    let admin = web_login(&app, "admin", "senha-admin1").await;
    let (status, _, form) = send(&app, get_req("/colaboradores/2/editar", Some(&admin))).await;
    assert_eq!(status, 200);
    assert!(form.contains("Maria – PCEP"));
    let (status, headers, _) = send(
        &app,
        form_req(
            "POST",
            "/colaboradores/2",
            &[("nome", " Maria Souza – PCEP "), ("email", "maria@pcep.example"), ("admin", "on")],
            Some(&admin),
        ),
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(headers.get("location").unwrap(), "/colaboradores");
    let u = users::get(&app.pool, 2).await.unwrap().unwrap();
    assert_eq!(u.display_name, "Maria Souza – PCEP");
    assert_eq!(u.email, "maria@pcep.example");
    assert!(u.is_admin);
}

#[tokio::test]
async fn edit_validates_name_and_email() {
    let app = setup().await;
    let admin = web_login(&app, "admin", "senha-admin1").await;
    for (nome, email) in [("A", ""), (&"x".repeat(61)[..], ""), ("Maria", &"e".repeat(121)[..])] {
        let (status, _, body) = send(
            &app,
            form_req(
                "POST",
                "/colaboradores/2",
                &[("nome", nome), ("email", email)],
                Some(&admin),
            ),
        )
        .await;
        assert_eq!(status, 200, "nome={nome:?}");
        assert!(body.contains("class=\"erro\""), "nome={nome:?}");
    }
    let u = users::get(&app.pool, 2).await.unwrap().unwrap();
    assert_eq!(u.display_name, "Maria – PCEP");
    assert_eq!(u.email, "");
}

#[tokio::test]
async fn reset_password_revokes_tokens() {
    let app = setup().await;
    let admin = web_login(&app, "admin", "senha-admin1").await;
    let maria_web = web_login(&app, "maria", "senha12345").await;
    let maria_app = issue_token(&app.pool, 2, TokenKind::App).await.unwrap();

    // Senha curta: erro, nada muda.
    let (status, _, body) = send(
        &app,
        form_req("POST", "/colaboradores/2/senha", &[("senha", "curta")], Some(&admin)),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("A senha deve ter pelo menos 8 caracteres"));
    let (status, _, _) = send(&app, get_req("/clientes", Some(&maria_web))).await;
    assert_eq!(status, 200);

    let (status, headers, _) = send(
        &app,
        form_req("POST", "/colaboradores/2/senha", &[("senha", "nova-senha-99")], Some(&admin)),
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(headers.get("location").unwrap(), "/colaboradores");

    let (status, _, _) = send(&app, get_req("/clientes", Some(&maria_web))).await;
    assert_eq!(status, 303);
    let (status, _, _) = send(
        &app,
        common::json_req("POST", "/api/currentUser", json!({}), Some(&maria_app)),
    )
    .await;
    assert_eq!(status, 401);
    // Admin continua logado e a senha nova funciona.
    let (status, _, _) = send(&app, get_req("/colaboradores", Some(&admin))).await;
    assert_eq!(status, 200);
    web_login(&app, "maria", "nova-senha-99").await;
}

#[tokio::test]
async fn unknown_collaborator_is_404() {
    let app = setup().await;
    let admin = web_login(&app, "admin", "senha-admin1").await;
    for (method, uri) in [
        ("GET", "/colaboradores/99/editar"),
        ("GET", "/colaboradores/99/senha"),
        ("POST", "/colaboradores/99/desativar"),
        ("POST", "/colaboradores/99/reativar"),
    ] {
        let (status, _, _) = send(&app, form_req(method, uri, &[], Some(&admin))).await;
        assert_eq!(status, 404, "{method} {uri}");
    }
}
