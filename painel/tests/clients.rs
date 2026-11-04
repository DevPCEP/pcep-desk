mod common;

use common::test_app;
use painel::clients::{self, ClientError, normalize_peer_id};

#[test]
fn peer_id_normalized() {
    assert_eq!(normalize_peer_id("123 456 789"), Some("123456789".to_string()));
    assert_eq!(normalize_peer_id("123-456.789"), Some("123456789".to_string()));
    assert_eq!(normalize_peer_id("1-2.3"), None);
    assert_eq!(normalize_peer_id("abc"), None);
    assert_eq!(
        normalize_peer_id("pcep_caixa1"),
        Some("pcep_caixa1".to_string())
    );
    assert_eq!(
        normalize_peer_id("  pcep_caixa1 "),
        Some("pcep_caixa1".to_string())
    );
    // Dígitos demais (13) não cabem no formato numérico, mas valem como id personalizado.
    assert_eq!(
        normalize_peer_id("1234567890123"),
        Some("1234567890123".to_string())
    );
    assert_eq!(normalize_peer_id("abc def ghi"), None);
    assert_eq!(normalize_peer_id(""), None);
}

#[tokio::test]
async fn duplicate_peer_id_rejected() {
    let app = test_app().await;
    let a = clients::create(&app.pool, "Padaria X", "", "").await.unwrap();
    let b = clients::create(&app.pool, "Mercado Y", "", "").await.unwrap();
    clients::add_computer(&app.pool, a, "123 456 789", "Caixa", "")
        .await
        .unwrap();
    let err = clients::add_computer(&app.pool, b, "123456789", "Balcão", "")
        .await
        .unwrap_err();
    match &err {
        ClientError::PeerIdTaken { client_name } => assert_eq!(client_name, "Padaria X"),
        other => panic!("esperava PeerIdTaken, veio {other:?}"),
    }
    assert_eq!(
        err.to_string(),
        "Esse código já está cadastrado no cliente Padaria X"
    );
}

#[tokio::test]
async fn invalid_peer_id_rejected() {
    let app = test_app().await;
    let a = clients::create(&app.pool, "Padaria X", "", "").await.unwrap();
    let err = clients::add_computer(&app.pool, a, "1-2.3", "Caixa", "")
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::InvalidPeerId));
    assert_eq!(err.to_string(), "Código de conexão inválido");
}

#[tokio::test]
async fn search_matches_computer_label_and_id() {
    let app = test_app().await;
    let a = clients::create(&app.pool, "Padaria X", "12.345.678/0001-90", "")
        .await
        .unwrap();
    clients::create(&app.pool, "Mercado Y", "", "").await.unwrap();
    clients::add_computer(&app.pool, a, "123456789", "Caixa", "")
        .await
        .unwrap();
    clients::add_computer(&app.pool, a, "987654321", "Caixa 2", "")
        .await
        .unwrap();

    for q in ["caixa", "4567", "padaria", "0001-90"] {
        let found = clients::search(&app.pool, q, 50).await.unwrap();
        // DISTINCT: dois computadores casando não duplicam o cliente.
        assert_eq!(found.len(), 1, "busca {q:?}");
        assert_eq!(found[0].name, "Padaria X");
    }
    assert!(clients::search(&app.pool, "inexistente", 50)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn search_empty_lists_all_by_name_with_limit() {
    let app = test_app().await;
    for n in ["Zeta", "alfa", "Beta"] {
        clients::create(&app.pool, n, "", "").await.unwrap();
    }
    let all = clients::search(&app.pool, "", 50).await.unwrap();
    let names: Vec<_> = all.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["alfa", "Beta", "Zeta"]);
    assert_eq!(clients::search(&app.pool, "  ", 2).await.unwrap().len(), 2);
}

#[tokio::test]
async fn search_escapes_like_wildcards() {
    let app = test_app().await;
    clients::create(&app.pool, "Padaria X", "", "").await.unwrap();
    clients::create(&app.pool, "100% Pão", "", "").await.unwrap();
    let found = clients::search(&app.pool, "%", 50).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "100% Pão");
    assert!(clients::search(&app.pool, "_", 50).await.unwrap().is_empty());
}

#[tokio::test]
async fn client_name_unique_case_insensitive() {
    let app = test_app().await;
    clients::create(&app.pool, "Padaria X", "", "").await.unwrap();
    let err = clients::create(&app.pool, "padaria x", "", "")
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::NameTaken));
    assert_eq!(err.to_string(), "Já existe um cliente com esse nome");

    // Renomear para o nome de outro também falha; manter o próprio nome não.
    let b = clients::create(&app.pool, "Mercado Y", "", "").await.unwrap();
    let err = clients::update(&app.pool, b, "PADARIA X", "", "")
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::NameTaken));
    clients::update(&app.pool, b, "Mercado Y", "123", "nota")
        .await
        .unwrap();
    let c = clients::get(&app.pool, b).await.unwrap().unwrap();
    assert_eq!((c.document.as_str(), c.note.as_str()), ("123", "nota"));
}

#[tokio::test]
async fn validation_limits() {
    let app = test_app().await;
    let err = clients::create(&app.pool, " a ", "", "").await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "Nome do cliente deve ter entre 2 e 120 caracteres"
    );
    assert!(clients::create(&app.pool, &"a".repeat(121), "", "")
        .await
        .is_err());
    assert!(clients::create(&app.pool, "Ok", &"1".repeat(21), "")
        .await
        .is_err());
    assert!(clients::create(&app.pool, "Ok", "", &"n".repeat(1001))
        .await
        .is_err());
    let id = clients::create(&app.pool, "  Ok  ", "", "").await.unwrap();
    assert_eq!(clients::get(&app.pool, id).await.unwrap().unwrap().name, "Ok");

    assert!(clients::add_computer(&app.pool, id, "123456789", "  ", "")
        .await
        .is_err());
    assert!(
        clients::add_computer(&app.pool, id, "123456789", &"l".repeat(61), "")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn computers_crud_and_catalog() {
    let app = test_app().await;
    let a = clients::create(&app.pool, "Padaria X", "", "").await.unwrap();
    let c1 = clients::add_computer(&app.pool, a, "123456789", "Caixa", "")
        .await
        .unwrap();
    clients::update_computer(&app.pool, c1, "Caixa 1", "nota")
        .await
        .unwrap();
    let list = clients::list_computers(&app.pool, a).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].label, "Caixa 1");
    assert_eq!(list[0].note, "nota");
    assert_eq!(list[0].peer_id, "123456789");

    let all = clients::all_computers_with_client(&app.pool).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].0.peer_id, "123456789");
    assert_eq!(all[0].1.name, "Padaria X");
    assert_eq!(all[0].1.id, a);
    assert_eq!(all[0].0.id, c1);
    assert_eq!(all[0].0.client_id, a);

    clients::remove_computer(&app.pool, c1).await.unwrap();
    assert!(clients::list_computers(&app.pool, a).await.unwrap().is_empty());
}

#[tokio::test]
async fn add_computer_to_missing_client_fails() {
    let app = test_app().await;
    let err = clients::add_computer(&app.pool, 999, "123456789", "Caixa", "")
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Invalid(_)), "{err:?}");
}

#[tokio::test]
async fn delete_client_cascades_computers() {
    let app = test_app().await;
    let a = clients::create(&app.pool, "Padaria X", "", "").await.unwrap();
    clients::add_computer(&app.pool, a, "123456789", "Caixa", "")
        .await
        .unwrap();
    clients::delete(&app.pool, a).await.unwrap();
    assert!(clients::get(&app.pool, a).await.unwrap().is_none());
    assert!(clients::list_computers(&app.pool, a).await.unwrap().is_empty());
    assert!(clients::all_computers_with_client(&app.pool)
        .await
        .unwrap()
        .is_empty());
    // O código volta a ficar livre.
    let b = clients::create(&app.pool, "Outro", "", "").await.unwrap();
    clients::add_computer(&app.pool, b, "123456789", "Caixa", "")
        .await
        .unwrap();
}

#[tokio::test]
async fn search_finds_code_typed_with_spaces() {
    let app = test_app().await;
    let a = clients::create(&app.pool, "Padaria X", "", "").await.unwrap();
    clients::create(&app.pool, "Mercado Y", "", "").await.unwrap();
    clients::add_computer(&app.pool, a, "123456789", "Caixa", "")
        .await
        .unwrap();
    for q in ["123 456 789", " 123-456-789 ", "123.456.789", "123456789"] {
        let found = clients::search(&app.pool, q, 50).await.unwrap();
        assert_eq!(found.len(), 1, "busca {q:?}");
        assert_eq!(found[0].name, "Padaria X");
    }
    assert!(clients::search(&app.pool, "123 456 780", 50).await.unwrap().is_empty());
}
