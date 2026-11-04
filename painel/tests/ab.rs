mod common;

use axum::{
    body::Body,
    http::{Request, header},
};
use common::{TestApp, send, test_app};
use painel::{
    addressbook::{self, SHARED_AB_GUID, platform_of, tag_color},
    auth::tokens::{TokenKind, issue_token},
    clients,
    devices::{self, SysInfo},
    users,
};
use serde_json::{Value, json};

/// Leitura como o app faz: POST sem corpo e sem Content-Type.
fn read(uri: &str, token: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().method("POST").uri(uri);
    if let Some(t) = token {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.header(header::CONTENT_LENGTH, "0").body(Body::empty()).unwrap()
}

fn write(method: &str, uri: &str, body: Value, token: &str) -> Request<Body> {
    common::json_req(method, uri, body, Some(token))
}

async fn user_token(app: &TestApp, username: &str) -> String {
    let id = users::create(&app.pool, username, "Colaborador", "senha12345", false)
        .await
        .unwrap();
    issue_token(&app.pool, id, TokenKind::App).await.unwrap()
}

async fn personal(app: &TestApp, token: &str) -> String {
    let (status, _, body) = send(app, read("/api/ab/personal", Some(token))).await;
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    v["guid"].as_str().unwrap().to_string()
}

async fn peers(app: &TestApp, token: &str, guid: &str) -> Value {
    let (status, _, body) = send(
        app,
        read(&format!("/api/ab/peers?ab={guid}&current=1&pageSize=100"), Some(token)),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

/// Escrita bem-sucedida: 200 com corpo vazio.
async fn ok_write(app: &TestApp, method: &str, uri: &str, body: Value, token: &str) {
    let (status, _, text) = send(app, write(method, uri, body, token)).await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(text, "");
}

#[tokio::test]
async fn personal_guid_stable() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let a = personal(&app, &token).await;
    let b = personal(&app, &token).await;
    assert_eq!(a, b);
    assert_eq!(a.len(), 36);
    assert_eq!(a.as_bytes()[14], b'4', "UUID versão 4: {a}");
    let other = user_token(&app, "maria").await;
    assert_ne!(personal(&app, &other).await, a);
}

#[tokio::test]
async fn settings_ok() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let (status, _, body) = send(&app, read("/api/ab/settings", Some(&token))).await;
    assert_eq!(status, 200);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), json!({"max_peer_one_ab": 0}));
}

#[tokio::test]
async fn shared_profile_listed_read_only() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let (status, _, body) = send(
        &app,
        read("/api/ab/shared/profiles?current=1&pageSize=20", Some(&token)),
    )
    .await;
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["total"], 1);
    assert_eq!(v["data"][0]["guid"], "clientes-pcep");
    assert_eq!(v["data"][0]["name"], "Clientes PCEP");
    assert_eq!(v["data"][0]["owner"], "PCEP");
    assert_eq!(v["data"][0]["rule"], 1);

    let (_, _, body) = send(
        &app,
        read("/api/ab/shared/profiles?current=2&pageSize=20", Some(&token)),
    )
    .await;
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["total"], 1);
    assert_eq!(v["data"], json!([]));
}

async fn seed_client(app: &TestApp) {
    let c = clients::create(&app.pool, "Padaria X", "", "").await.unwrap();
    clients::add_computer(&app.pool, c, "123456789", "Caixa", "Balcão")
        .await
        .unwrap();
    // Cliente sem computador: não aparece em peers nem em tags.
    clients::create(&app.pool, "Mercado Y", "", "").await.unwrap();
    let info = SysInfo {
        hostname: "CAIXA-01".into(),
        os: "Windows / 11".into(),
        username: "operador".into(),
        ..Default::default()
    };
    devices::upsert_sysinfo(&app.pool, "123456789", "", &info, 1_700_000_000)
        .await
        .unwrap();
}

#[tokio::test]
async fn shared_peers_from_clients() {
    let app = test_app().await;
    seed_client(&app).await;
    let token = user_token(&app, "joao").await;
    let v = peers(&app, &token, SHARED_AB_GUID).await;
    assert_eq!(v["total"], 1);
    let p = &v["data"][0];
    assert_eq!(p["id"], "123456789");
    assert_eq!(p["alias"], "Padaria X – Caixa");
    assert_eq!(p["tags"], json!(["Padaria X"]));
    assert_eq!(p["hostname"], "CAIXA-01");
    assert_eq!(p["username"], "operador");
    assert_eq!(p["platform"], "Windows");
    assert_eq!(p["note"], "Balcão");

    // Paginação: página 2 vazia, total preservado.
    let (_, _, body) = send(
        &app,
        read("/api/ab/peers?ab=clientes-pcep&current=2&pageSize=1", Some(&token)),
    )
    .await;
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["total"], 1);
    assert_eq!(v["data"], json!([]));
}

#[tokio::test]
async fn shared_peer_without_device_has_empty_fields() {
    let app = test_app().await;
    let c = clients::create(&app.pool, "Padaria X", "", "").await.unwrap();
    clients::add_computer(&app.pool, c, "123456789", "Caixa", "").await.unwrap();
    let token = user_token(&app, "joao").await;
    let v = peers(&app, &token, SHARED_AB_GUID).await;
    assert_eq!(v["data"][0]["platform"], "");
    assert_eq!(v["data"][0]["hostname"], "");
}

#[tokio::test]
async fn shared_tags_list() {
    let app = test_app().await;
    seed_client(&app).await;
    let token = user_token(&app, "joao").await;
    let (status, _, body) = send(&app, read("/api/ab/tags/clientes-pcep", Some(&token))).await;
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert!(v.is_array());
    assert_eq!(v, json!([{"name": "Padaria X", "color": tag_color("Padaria X")}]));
}

#[tokio::test]
async fn shared_write_forbidden() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let cases = [
        ("POST", "/api/ab/peer/add/clientes-pcep", json!({"id": "1"})),
        ("PUT", "/api/ab/peer/update/clientes-pcep", json!({"id": "1"})),
        ("DELETE", "/api/ab/peer/clientes-pcep", json!(["1"])),
        ("POST", "/api/ab/tag/add/clientes-pcep", json!({"name": "a", "color": 1})),
        ("PUT", "/api/ab/tag/rename/clientes-pcep", json!({"old": "a", "new": "b"})),
        ("PUT", "/api/ab/tag/update/clientes-pcep", json!({"name": "a", "color": 2})),
        ("DELETE", "/api/ab/tag/clientes-pcep", json!(["a"])),
    ];
    for (method, uri, body) in cases {
        let (status, _, text) = send(&app, write(method, uri, body, &token)).await;
        assert_eq!(status, 403, "{method} {uri}");
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["error"], "Catálogo somente leitura");
    }
}

#[tokio::test]
async fn personal_crud() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    assert_eq!(peers(&app, &token, &guid).await["total"], 0);

    ok_write(
        &app,
        "POST",
        &format!("/api/ab/peer/add/{guid}"),
        json!({"id": "555666777", "alias": "Meu PC", "tags": ["casa"]}),
        &token,
    )
    .await;
    let v = peers(&app, &token, &guid).await;
    assert_eq!(v["total"], 1);
    assert_eq!(v["data"][0]["id"], "555666777");
    assert_eq!(v["data"][0]["alias"], "Meu PC");
    assert_eq!(v["data"][0]["tags"], json!(["casa"]));

    ok_write(
        &app,
        "PUT",
        &format!("/api/ab/peer/update/{guid}"),
        json!({"id": "555666777", "alias": "Casa"}),
        &token,
    )
    .await;
    let v = peers(&app, &token, &guid).await;
    assert_eq!(v["data"][0]["alias"], "Casa");
    assert_eq!(v["data"][0]["tags"], json!(["casa"]));

    ok_write(&app, "DELETE", &format!("/api/ab/peer/{guid}"), json!(["555666777"]), &token).await;
    let v = peers(&app, &token, &guid).await;
    assert_eq!(v["total"], 0);
    assert_eq!(v["data"], json!([]));
}

#[tokio::test]
async fn peer_add_existing_updates_only_present_fields() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    let add = format!("/api/ab/peer/add/{guid}");
    ok_write(
        &app,
        "POST",
        &add,
        json!({"id": "1", "alias": "A", "tags": ["x"], "hash": "h1", "note": "n"}),
        &token,
    )
    .await;
    ok_write(&app, "POST", &add, json!({"id": "1", "alias": "B"}), &token).await;
    let v = peers(&app, &token, &guid).await;
    assert_eq!(v["total"], 1);
    let p = &v["data"][0];
    assert_eq!((p["alias"].as_str(), p["hash"].as_str(), p["note"].as_str()), (Some("B"), Some("h1"), Some("n")));
    assert_eq!(p["tags"], json!(["x"]));
}

#[tokio::test]
async fn peer_add_ignores_extra_fields_and_never_stores_password() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    ok_write(
        &app,
        "POST",
        &format!("/api/ab/peer/add/{guid}"),
        json!({
            "id": " 999888777 ", "password": "segredo-da-maquina", "forceAlwaysRelay": false,
            "rdpPort": "", "rdpUsername": "", "loginName": "x", "device_group_name": "",
            "same_server": false, "username": "u", "hostname": "h", "platform": "Windows"
        }),
        &token,
    )
    .await;
    let v = peers(&app, &token, &guid).await;
    let p = &v["data"][0];
    assert_eq!(p["id"], "999888777");
    assert_eq!(p["username"], "u");
    assert_eq!(p["platform"], "Windows");
    assert!(!v.to_string().contains("segredo-da-maquina"));
    let (stored,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM ab_peers WHERE hash LIKE '%segredo%' OR note LIKE '%segredo%' OR alias LIKE '%segredo%'",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(stored, 0);

    // Também no update, `password` é ignorado.
    ok_write(
        &app,
        "PUT",
        &format!("/api/ab/peer/update/{guid}"),
        json!({"id": "999888777", "password": "outra", "note": "ok"}),
        &token,
    )
    .await;
    let v = peers(&app, &token, &guid).await;
    assert_eq!(v["data"][0]["note"], "ok");
    assert_eq!(v["data"][0]["hash"], "");
}

#[tokio::test]
async fn peer_validation_rejects_bad_input() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    let add = format!("/api/ab/peer/add/{guid}");
    let many_tags: Vec<String> = (0..51).map(|i| format!("t{i}")).collect();
    let bad = [
        json!({}),
        json!({"id": "   "}),
        json!({"id": "x".repeat(65)}),
        json!({"id": "1", "alias": "a".repeat(121)}),
        json!({"id": "1", "note": "n".repeat(1001)}),
        json!({"id": "1", "hash": "h".repeat(257)}),
        json!({"id": "1", "tags": ["t".repeat(61)]}),
        json!({"id": "1", "tags": many_tags}),
        json!({"id": "1", "tags": "casa"}),
        json!("texto"),
    ];
    for body in bad {
        let (status, _, text) = send(&app, write("POST", &add, body.clone(), &token)).await;
        assert_eq!(status, 400, "{body}");
        assert!(serde_json::from_str::<Value>(&text).unwrap()["error"].is_string());
    }
    let (_, _, text) = send(&app, write("POST", &add, json!({"id": ""}), &token)).await;
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        json!({"error": "Requisição inválida"})
    );
    // Limites exatos são aceitos.
    ok_write(
        &app,
        "POST",
        &add,
        json!({"id": "x".repeat(64), "alias": "a".repeat(120), "note": "n".repeat(1000),
               "hash": "h".repeat(256), "tags": ["t".repeat(60)]}),
        &token,
    )
    .await;
    // Corpo que não é JSON.
    let req = Request::builder()
        .method("POST")
        .uri(&add)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from("não é json"))
        .unwrap();
    let (status, _, _) = send(&app, req).await;
    assert_eq!(status, 400);
    assert_eq!(peers(&app, &token, &guid).await["total"], 1);
}

#[tokio::test]
async fn peer_update_missing_is_404() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    let (status, _, text) = send(
        &app,
        write("PUT", &format!("/api/ab/peer/update/{guid}"), json!({"id": "nada", "alias": "x"}), &token),
    )
    .await;
    assert_eq!(status, 404);
    assert!(serde_json::from_str::<Value>(&text).unwrap()["error"].is_string());
    assert_eq!(peers(&app, &token, &guid).await["total"], 0);
}

#[tokio::test]
async fn peers_pagination_personal() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    for id in ["1", "2", "3"] {
        ok_write(&app, "POST", &format!("/api/ab/peer/add/{guid}"), json!({"id": id}), &token).await;
    }
    let (_, _, body) = send(
        &app,
        read(&format!("/api/ab/peers?ab={guid}&current=2&pageSize=2"), Some(&token)),
    )
    .await;
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["total"], 3);
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert_eq!(v["data"][0]["id"], "3");
}

#[tokio::test]
async fn tag_crud_personal() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    let list = || async {
        let (status, _, body) = send(&app, read(&format!("/api/ab/tags/{guid}"), Some(&token))).await;
        assert_eq!(status, 200);
        serde_json::from_str::<Value>(&body).unwrap()
    };
    assert_eq!(list().await, json!([]));
    ok_write(&app, "POST", &format!("/api/ab/tag/add/{guid}"), json!({"name": "casa", "color": 4278190335u64}), &token).await;
    ok_write(&app, "POST", &format!("/api/ab/tag/add/{guid}"), json!({"name": "ab", "color": 5}), &token).await;
    assert_eq!(
        list().await,
        json!([{"name": "ab", "color": 5}, {"name": "casa", "color": 4278190335u64}])
    );
    ok_write(&app, "PUT", &format!("/api/ab/tag/update/{guid}"), json!({"name": "ab", "color": 9}), &token).await;
    assert_eq!(list().await[0], json!({"name": "ab", "color": 9}));
    let (status, _, _) = send(
        &app,
        write("PUT", &format!("/api/ab/tag/update/{guid}"), json!({"name": "nao", "color": 1}), &token),
    )
    .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn tag_rename_updates_peers() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    ok_write(&app, "POST", &format!("/api/ab/tag/add/{guid}"), json!({"name": "casa", "color": 7}), &token).await;
    ok_write(
        &app,
        "POST",
        &format!("/api/ab/peer/add/{guid}"),
        json!({"id": "1", "tags": ["casa", "outra"]}),
        &token,
    )
    .await;
    ok_write(&app, "POST", &format!("/api/ab/peer/add/{guid}"), json!({"id": "2", "tags": ["x"]}), &token).await;
    ok_write(&app, "PUT", &format!("/api/ab/tag/rename/{guid}"), json!({"old": "casa", "new": "lar"}), &token).await;
    let v = peers(&app, &token, &guid).await;
    assert_eq!(v["data"][0]["tags"], json!(["lar", "outra"]));
    assert_eq!(v["data"][1]["tags"], json!(["x"]));
    let (_, _, body) = send(&app, read(&format!("/api/ab/tags/{guid}"), Some(&token))).await;
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), json!([{"name": "lar", "color": 7}]));
}

#[tokio::test]
async fn tag_rename_to_existing_merges_without_duplicates() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    for (n, c) in [("a", 1), ("b", 2)] {
        ok_write(&app, "POST", &format!("/api/ab/tag/add/{guid}"), json!({"name": n, "color": c}), &token).await;
    }
    ok_write(&app, "POST", &format!("/api/ab/peer/add/{guid}"), json!({"id": "1", "tags": ["a", "b"]}), &token).await;
    ok_write(&app, "PUT", &format!("/api/ab/tag/rename/{guid}"), json!({"old": "a", "new": "b"}), &token).await;
    let v = peers(&app, &token, &guid).await;
    assert_eq!(v["data"][0]["tags"], json!(["b"]));
    let (_, _, body) = send(&app, read(&format!("/api/ab/tags/{guid}"), Some(&token))).await;
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), json!([{"name": "b", "color": 2}]));
}

#[tokio::test]
async fn tag_delete_removes_from_peers() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    ok_write(&app, "POST", &format!("/api/ab/tag/add/{guid}"), json!({"name": "casa", "color": 1}), &token).await;
    ok_write(&app, "POST", &format!("/api/ab/tag/add/{guid}"), json!({"name": "obra", "color": 2}), &token).await;
    ok_write(&app, "POST", &format!("/api/ab/peer/add/{guid}"), json!({"id": "1", "tags": ["casa", "obra"]}), &token).await;
    ok_write(&app, "DELETE", &format!("/api/ab/tag/{guid}"), json!(["casa", "inexistente"]), &token).await;
    let v = peers(&app, &token, &guid).await;
    assert_eq!(v["data"][0]["tags"], json!(["obra"]));
    let (_, _, body) = send(&app, read(&format!("/api/ab/tags/{guid}"), Some(&token))).await;
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), json!([{"name": "obra", "color": 2}]));
}

#[tokio::test]
async fn delete_bodies_must_be_arrays() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let guid = personal(&app, &token).await;
    for uri in [format!("/api/ab/peer/{guid}"), format!("/api/ab/tag/{guid}")] {
        let (status, _, _) = send(&app, write("DELETE", &uri, json!({"id": "1"}), &token)).await;
        assert_eq!(status, 400, "{uri}");
    }
}

#[tokio::test]
async fn other_users_personal_404() {
    let app = test_app().await;
    let joao = user_token(&app, "joao").await;
    let maria = user_token(&app, "maria").await;
    let guid = personal(&app, &joao).await;
    ok_write(&app, "POST", &format!("/api/ab/peer/add/{guid}"), json!({"id": "1"}), &joao).await;

    let reads = [
        format!("/api/ab/peers?ab={guid}&current=1&pageSize=20"),
        format!("/api/ab/tags/{guid}"),
    ];
    for uri in reads {
        let (status, _, body) = send(&app, read(&uri, Some(&maria))).await;
        assert_eq!(status, 404, "{uri}");
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap(),
            json!({"error": "Catálogo não encontrado"})
        );
    }
    let writes = [
        ("POST", format!("/api/ab/peer/add/{guid}"), json!({"id": "2"})),
        ("PUT", format!("/api/ab/peer/update/{guid}"), json!({"id": "1", "alias": "x"})),
        ("DELETE", format!("/api/ab/peer/{guid}"), json!(["1"])),
        ("POST", format!("/api/ab/tag/add/{guid}"), json!({"name": "a", "color": 1})),
        ("PUT", format!("/api/ab/tag/rename/{guid}"), json!({"old": "a", "new": "b"})),
        ("PUT", format!("/api/ab/tag/update/{guid}"), json!({"name": "a", "color": 1})),
        ("DELETE", format!("/api/ab/tag/{guid}"), json!(["a"])),
    ];
    for (method, uri, body) in writes {
        let (status, _, _) = send(&app, write(method, &uri, body, &maria)).await;
        assert_eq!(status, 404, "{method} {uri}");
    }
    // Guid inexistente também é 404, e nada do João mudou.
    let (status, _, _) = send(&app, read("/api/ab/peers?ab=nao-existe&current=1&pageSize=20", Some(&maria))).await;
    assert_eq!(status, 404);
    assert_eq!(peers(&app, &joao, &guid).await["total"], 1);
}

#[tokio::test]
async fn ab_requires_token() {
    let app = test_app().await;
    let uris = [
        "/api/ab/settings",
        "/api/ab/personal",
        "/api/ab/shared/profiles",
        "/api/ab/peers?ab=clientes-pcep",
        "/api/ab/tags/clientes-pcep",
    ];
    for uri in uris {
        let (status, _, _) = send(&app, read(uri, None)).await;
        assert_eq!(status, 401, "{uri}");
    }
    let (status, _, _) = send(
        &app,
        common::json_req("POST", "/api/ab/peer/add/x", json!({"id": "1"}), None),
    )
    .await;
    assert_eq!(status, 401);
    let (status, _, _) = send(&app, read("/api/ab/personal", Some("token-invalido"))).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn disabled_user_loses_catalog_access() {
    let app = test_app().await;
    let token = user_token(&app, "joao").await;
    let (u, _) = users::find_by_username(&app.pool, "joao").await.unwrap().unwrap();
    users::set_active(&app.pool, u.id, false).await.unwrap();
    let (status, _, _) = send(&app, read("/api/ab/personal", Some(&token))).await;
    assert_eq!(status, 401);
}

#[test]
fn platform_conversion() {
    assert_eq!(platform_of("Windows / 11"), "Windows");
    assert_eq!(platform_of("windows / Server 2019"), "Windows");
    assert_eq!(platform_of("Linux / Ubuntu 22.04"), "Linux");
    assert_eq!(platform_of("macOS / 14.2"), "Mac OS");
    assert_eq!(platform_of("Android / 13"), "Android");
    assert_eq!(platform_of("Ubuntu / linux mint"), "Linux");
    assert_eq!(platform_of("Plan9 / 4"), "");
    assert_eq!(platform_of(""), "");
}

#[test]
fn tag_color_is_deterministic_fnv1a() {
    // Vetores de teste conhecidos do FNV-1a de 32 bits.
    assert_eq!(addressbook::fnv1a32(""), 0x811C_9DC5);
    assert_eq!(addressbook::fnv1a32("a"), 0xE40C_292C);
    assert_eq!(addressbook::fnv1a32("foobar"), 0xBF9C_F968);
    assert_eq!(tag_color("a"), 0xFF0C_292C);
    assert_eq!(tag_color("Padaria X"), tag_color("Padaria X"));
    assert!(tag_color("Padaria X") >= 0xFF00_0000);
}
