mod common;

use axum::{body::Body, http::Request};
use common::{TestApp, form_req, get_req, send, test_app, web_login};
use painel::{
    clients, clock,
    connections::{self, ConnEvent, ConnEventKind},
    devices::{self, SysInfo},
    users,
};

/// Banco com o admin `admin` e a colaboradora `maria`.
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

async fn client(app: &TestApp, name: &str) -> i64 {
    clients::create(&app.pool, name, "", "").await.unwrap()
}

fn location(headers: &axum::http::HeaderMap) -> String {
    headers.get("location").unwrap().to_str().unwrap().to_string()
}

#[tokio::test]
async fn search_fragment_filters() {
    let app = setup().await;
    client(&app, "Padaria X").await;
    client(&app, "Mercado Y").await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, _, body) = send(&app, get_req("/clientes/lista?q=pada", Some(&cookie))).await;
    assert_eq!(status, 200);
    assert!(body.contains("Padaria X"));
    assert!(!body.contains("Mercado Y"));
    // Fragmento: sem o layout da página.
    assert!(!body.contains("<html"));
    // A página inteira traz a busca htmx e a lista completa.
    let (_, _, page) = send(&app, get_req("/clientes", Some(&cookie))).await;
    assert!(page.contains("hx-get=\"/clientes/lista\""));
    assert!(page.contains("delay:300ms"));
    assert!(page.contains("Padaria X") && page.contains("Mercado Y"));
}

#[tokio::test]
async fn search_without_results_says_so() {
    let app = setup().await;
    client(&app, "Padaria X").await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (_, _, body) = send(&app, get_req("/clientes/lista?q=zzz", Some(&cookie))).await;
    assert!(body.contains("Nenhum cliente encontrado"));
}

#[tokio::test]
async fn list_shows_computer_and_online_counts() {
    let app = setup().await;
    let id = client(&app, "Padaria X").await;
    clients::add_computer(&app.pool, id, "111222333", "Caixa", "").await.unwrap();
    clients::add_computer(&app.pool, id, "444555666", "Balcão", "").await.unwrap();
    devices::touch(&app.pool, "111222333", "", "1", clock::now()).await.unwrap();
    devices::touch(&app.pool, "444555666", "", "1", clock::now() - 3600).await.unwrap();
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (_, _, body) = send(&app, get_req("/clientes/lista", Some(&cookie))).await;
    assert!(body.contains("<td class=\"num\">2</td>"), "{body}");
    assert!(body.contains("<td class=\"num\">1</td>"), "{body}");
}

#[tokio::test]
async fn add_computer_normalizes_and_shows() {
    let app = setup().await;
    let id = client(&app, "Padaria X").await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, headers, _) = send(
        &app,
        form_req(
            "POST",
            &format!("/clientes/{id}/computadores"),
            &[("codigo", "123 456 789"), ("rotulo", "Caixa"), ("observacao", "")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), format!("/clientes/{id}"));
    let (status, _, body) = send(&app, get_req(&format!("/clientes/{id}"), Some(&cookie))).await;
    assert_eq!(status, 200);
    // Copiado sem espaços; exibido agrupado de 3 em 3.
    assert!(body.contains("data-copiar=\"123456789\""), "{body}");
    assert!(body.contains("123 456 789"));
    assert!(body.contains("Caixa"));
    assert!(body.contains("Nunca conectou ao servidor"));
}

#[tokio::test]
async fn duplicate_peer_id_message() {
    let app = setup().await;
    let a = client(&app, "Padaria X").await;
    let b = client(&app, "Mercado Y").await;
    clients::add_computer(&app.pool, a, "123456789", "Caixa", "").await.unwrap();
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, _, body) = send(
        &app,
        form_req(
            "POST",
            &format!("/clientes/{b}/computadores"),
            &[("codigo", "123 456 789"), ("rotulo", "Outro"), ("observacao", "")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Esse código já está cadastrado no cliente Padaria X"));
    // O formulário mantém o que foi digitado.
    assert!(body.contains("value=\"Outro\""));
}

#[tokio::test]
async fn invalid_peer_id_message() {
    let app = setup().await;
    let id = client(&app, "Padaria X").await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, _, body) = send(
        &app,
        form_req(
            "POST",
            &format!("/clientes/{id}/computadores"),
            &[("codigo", "12"), ("rotulo", "Caixa"), ("observacao", "")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Código de conexão inválido"));
}

#[tokio::test]
async fn online_status_shown() {
    let app = setup().await;
    let id = client(&app, "Padaria X").await;
    clients::add_computer(&app.pool, id, "111222333", "Caixa", "").await.unwrap();
    let cookie = web_login(&app, "maria", "senha12345").await;
    let uri = format!("/clientes/{id}");

    devices::touch(&app.pool, "111222333", "", "1", clock::now()).await.unwrap();
    let (_, _, body) = send(&app, get_req(&uri, Some(&cookie))).await;
    assert!(body.contains("Online"));
    assert!(!body.contains("Offline"));

    devices::touch(&app.pool, "111222333", "", "1", clock::now() - 120).await.unwrap();
    let (_, _, body) = send(&app, get_req(&uri, Some(&cookie))).await;
    assert!(body.contains("Offline"));
    assert!(!body.contains("Online"));
    assert!(!body.contains("Nunca conectou"));
}

#[tokio::test]
async fn client_page_shows_device_info_and_history() {
    let app = setup().await;
    let id = client(&app, "Padaria X").await;
    clients::add_computer(&app.pool, id, "111222333", "Caixa", "").await.unwrap();
    let info = SysInfo {
        hostname: "PC-CAIXA".into(),
        os: "windows".into(),
        username: "joana".into(),
        cpu: String::new(),
        memory: String::new(),
        version: "1.4".into(),
    };
    devices::upsert_sysinfo(&app.pool, "111222333", "", &info, 1_700_000_000)
        .await
        .unwrap();
    // Conexão de 1 h 02 min 05 s.
    let ev = |kind| ConnEvent {
        peer_id: "111222333".into(),
        conn_id: 7,
        session_id: "s".into(),
        kind,
    };
    connections::apply_conn_event(
        &app.pool,
        &ev(ConnEventKind::Authorized {
            operator_peer_id: "999888777".into(),
            operator_name: "Suporte Joao".into(),
            conn_type: Some(0),
        }),
        1_700_000_100,
    )
    .await
    .unwrap();
    connections::apply_conn_event(&app.pool, &ev(ConnEventKind::Close), 1_700_000_100 + 3725)
        .await
        .unwrap();
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (_, _, body) = send(&app, get_req(&format!("/clientes/{id}"), Some(&cookie))).await;
    assert!(body.contains("PC-CAIXA"));
    assert!(body.contains("windows"));
    assert!(body.contains("joana"));
    assert!(body.contains("Suporte Joao"));
    assert!(body.contains("01:02:05"), "{body}");
    assert!(body.contains(&clock::format_ts(1_700_000_100)));
    assert!(body.contains("/static/painel.js"));
}

#[tokio::test]
async fn unlinked_page_and_link() {
    let app = setup().await;
    let id = client(&app, "Padaria X").await;
    let info = SysInfo {
        hostname: "PC-NOVO".into(),
        os: "windows".into(),
        username: "x".into(),
        cpu: String::new(),
        memory: String::new(),
        version: "1.4".into(),
    };
    devices::upsert_sysinfo(&app.pool, "777888999", "", &info, clock::now())
        .await
        .unwrap();
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, _, body) = send(&app, get_req("/nao-vinculados", Some(&cookie))).await;
    assert_eq!(status, 200);
    assert!(body.contains("777 888 999"));
    assert!(body.contains("PC-NOVO"));
    assert!(body.contains("/nao-vinculados/777888999/vincular"));

    // Formulário: código preenchido e seleção de cliente.
    let (status, _, form) = send(
        &app,
        get_req("/nao-vinculados/777888999/vincular", Some(&cookie)),
    )
    .await;
    assert_eq!(status, 200);
    assert!(form.contains("value=\"777888999\""));
    assert!(form.contains(&format!("<option value=\"{id}\">Padaria X</option>")));
    assert!(form.contains("value=\"PC-NOVO\""), "rótulo sugerido: {form}");

    let (status, headers, _) = send(
        &app,
        form_req(
            "POST",
            "/nao-vinculados/vincular",
            &[
                ("cliente_id", &id.to_string()),
                ("codigo", "777888999"),
                ("rotulo", "Recepção"),
                ("observacao", ""),
            ],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), format!("/clientes/{id}"));
    let (_, _, body) = send(&app, get_req("/nao-vinculados", Some(&cookie))).await;
    assert!(!body.contains("PC-NOVO"));
    assert!(body.contains("Nenhum"));
    let (_, _, page) = send(&app, get_req(&format!("/clientes/{id}"), Some(&cookie))).await;
    assert!(page.contains("Recepção"));
    assert!(page.contains("PC-NOVO"));
}

#[tokio::test]
async fn link_with_unknown_client_shows_error() {
    let app = setup().await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, _, body) = send(
        &app,
        form_req(
            "POST",
            "/nao-vinculados/vincular",
            &[("cliente_id", "999"), ("codigo", "777888999"), ("rotulo", "X"), ("observacao", "")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Cliente não encontrado"));
}

#[tokio::test]
async fn collaborator_cannot_delete_client() {
    let app = setup().await;
    let id = client(&app, "Padaria X").await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, _, _) = send(
        &app,
        form_req("POST", &format!("/clientes/{id}/excluir"), &[], Some(&cookie)),
    )
    .await;
    assert_eq!(status, 403);
    assert!(clients::get(&app.pool, id).await.unwrap().is_some());
    // O botão de excluir nem aparece para colaborador.
    let (_, _, page) = send(&app, get_req(&format!("/clientes/{id}"), Some(&cookie))).await;
    assert!(!page.contains("/excluir"));
}

#[tokio::test]
async fn admin_deletes_client() {
    let app = setup().await;
    let id = client(&app, "Padaria X").await;
    clients::add_computer(&app.pool, id, "111222333", "Caixa", "").await.unwrap();
    let cookie = web_login(&app, "admin", "senha-admin1").await;
    let (_, _, page) = send(&app, get_req(&format!("/clientes/{id}"), Some(&cookie))).await;
    assert!(page.contains("hx-confirm"));
    assert!(page.contains(&format!("/clientes/{id}/excluir")));
    let (status, headers, _) = send(
        &app,
        form_req("POST", &format!("/clientes/{id}/excluir"), &[], Some(&cookie)),
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), "/clientes");
    assert!(clients::get(&app.pool, id).await.unwrap().is_none());
    assert!(clients::list_computers(&app.pool, id).await.unwrap().is_empty());
}

#[tokio::test]
async fn collaborator_creates_and_edits_client() {
    let app = setup().await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, headers, _) = send(
        &app,
        form_req(
            "POST",
            "/clientes",
            &[("nome", "Padaria X"), ("documento", "12.345.678/0001-90"), ("observacao", "Fala com Joana")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 303);
    let loc = location(&headers);
    let (_, _, page) = send(&app, get_req(&loc, Some(&cookie))).await;
    assert!(page.contains("Padaria X"));
    assert!(page.contains("12.345.678/0001-90"));
    assert!(page.contains("Fala com Joana"));

    // Edição com nome repetido reexibe o formulário com o erro.
    client(&app, "Mercado Y").await;
    let (status, _, body) = send(
        &app,
        form_req("POST", &loc, &[("nome", "Mercado Y"), ("documento", ""), ("observacao", "")], Some(&cookie)),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Já existe um cliente com esse nome"));
    let (status, _, _) = send(
        &app,
        form_req("POST", &loc, &[("nome", "Padaria Z"), ("documento", ""), ("observacao", "")], Some(&cookie)),
    )
    .await;
    assert_eq!(status, 303);
    let (_, _, form) = send(&app, get_req(&format!("{loc}/editar"), Some(&cookie))).await;
    assert!(form.contains("value=\"Padaria Z\""));
}

#[tokio::test]
async fn new_client_with_short_name_shows_error() {
    let app = setup().await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, _, body) = send(
        &app,
        form_req("POST", "/clientes", &[("nome", "A"), ("documento", ""), ("observacao", "")], Some(&cookie)),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("entre 2 e 120 caracteres"));
}

#[tokio::test]
async fn edit_and_remove_computer() {
    let app = setup().await;
    let id = client(&app, "Padaria X").await;
    let comp = clients::add_computer(&app.pool, id, "111222333", "Caixa", "").await.unwrap();
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, _, form) = send(&app, get_req(&format!("/computadores/{comp}/editar"), Some(&cookie))).await;
    assert_eq!(status, 200);
    assert!(form.contains("value=\"Caixa\""));
    assert!(form.contains("111 222 333"));
    let (status, headers, _) = send(
        &app,
        form_req(
            "POST",
            &format!("/computadores/{comp}"),
            &[("rotulo", "Caixa 2"), ("observacao", "perto da porta")],
            Some(&cookie),
        ),
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), format!("/clientes/{id}"));
    let (_, _, page) = send(&app, get_req(&format!("/clientes/{id}"), Some(&cookie))).await;
    assert!(page.contains("Caixa 2") && page.contains("perto da porta"));
    assert!(page.contains("hx-confirm"));

    // Rótulo vazio: erro no formulário.
    let (status, _, body) = send(
        &app,
        form_req("POST", &format!("/computadores/{comp}"), &[("rotulo", " "), ("observacao", "")], Some(&cookie)),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Rótulo do computador"));

    let (status, headers, _) = send(
        &app,
        form_req("POST", &format!("/computadores/{comp}/remover"), &[], Some(&cookie)),
    )
    .await;
    assert_eq!(status, 303);
    assert_eq!(location(&headers), format!("/clientes/{id}"));
    assert!(clients::list_computers(&app.pool, id).await.unwrap().is_empty());
}

#[tokio::test]
async fn unknown_ids_give_404() {
    let app = setup().await;
    let cookie = web_login(&app, "maria", "senha12345").await;
    for uri in [
        "/clientes/999",
        "/clientes/999/editar",
        "/clientes/999/computadores/novo",
        "/computadores/999/editar",
        "/nao-vinculados/000000000/vincular",
    ] {
        let (status, _, _) = send(&app, get_req(uri, Some(&cookie))).await;
        assert_eq!(status, 404, "{uri}");
    }
}

#[tokio::test]
async fn pages_require_login() {
    let app = setup().await;
    for uri in ["/clientes", "/clientes/lista", "/clientes/novo", "/clientes/1", "/nao-vinculados"] {
        let (status, headers, _) = send(&app, get_req(uri, None)).await;
        assert_eq!(status, 303, "{uri}");
        assert_eq!(location(&headers), "/entrar");
    }
}

#[tokio::test]
async fn htmx_request_without_session_gets_hx_redirect() {
    let app = setup().await;
    let req = Request::builder()
        .method("GET")
        .uri("/clientes/lista?q=a")
        .header("HX-Request", "true")
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = send(&app, req).await;
    assert_eq!(status, 200);
    assert_eq!(headers.get("hx-redirect").unwrap(), "/entrar");
    assert!(!body.contains("Entrar"));
}
