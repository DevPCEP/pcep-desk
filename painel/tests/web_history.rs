mod common;

use common::{TestApp, get_req, send, test_app, web_login};
use painel::{
    clients, clock,
    connections::{self, ConnEvent, ConnEventKind},
    users,
};
use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

/// Segundos Unix de uma data/hora no fuso UTC−03:00.
fn brt(year: i32, month: Month, day: u8, hour: u8, min: u8) -> i64 {
    PrimitiveDateTime::new(
        Date::from_calendar_date(year, month, day).unwrap(),
        Time::from_hms(hour, min, 0).unwrap(),
    )
    .assume_offset(UtcOffset::from_hms(-3, 0, 0).unwrap())
    .unix_timestamp()
}

struct Env {
    app: TestApp,
    maria: i64,
    joao: i64,
    padaria: i64,
    mercado: i64,
}

/// Banco com admin, Maria e João; dois clientes com um computador cada;
/// os apps de Maria (111111111) e João (222222222) vinculados aos usuários.
async fn setup() -> Env {
    let app = test_app().await;
    users::create(&app.pool, "admin", "Administrador", "senha-admin1", true)
        .await
        .unwrap();
    let maria = users::create(&app.pool, "maria", "Maria – PCEP", "senha12345", false)
        .await
        .unwrap();
    let joao = users::create(&app.pool, "joao", "João – PCEP", "senha12345", false)
        .await
        .unwrap();
    for (peer, user) in [("111111111", maria), ("222222222", joao)] {
        sqlx::query(
            "INSERT INTO user_devices (peer_id, user_id, uuid, last_login_at) VALUES (?, ?, '', 0)",
        )
        .bind(peer)
        .bind(user)
        .execute(&app.pool)
        .await
        .unwrap();
    }
    let padaria = clients::create(&app.pool, "Padaria X", "", "").await.unwrap();
    let mercado = clients::create(&app.pool, "Mercado Y", "", "").await.unwrap();
    clients::add_computer(&app.pool, padaria, "333333333", "Caixa", "").await.unwrap();
    clients::add_computer(&app.pool, mercado, "444444444", "Balcão", "").await.unwrap();
    Env {
        app,
        maria,
        joao,
        padaria,
        mercado,
    }
}

/// Conexão autorizada em `at` e encerrada `secs` depois (se houver).
async fn connect(
    app: &TestApp,
    target: &str,
    conn_id: i64,
    operator: &str,
    operator_name: &str,
    at: i64,
    ended_after: Option<i64>,
) {
    let ev = |kind| ConnEvent {
        peer_id: target.into(),
        conn_id,
        session_id: "s".into(),
        kind,
    };
    connections::apply_conn_event(
        &app.pool,
        &ev(ConnEventKind::Authorized {
            operator_peer_id: operator.into(),
            operator_name: operator_name.into(),
            conn_type: Some(0),
        }),
        at,
    )
    .await
    .unwrap();
    if let Some(secs) = ended_after {
        connections::apply_conn_event(&app.pool, &ev(ConnEventKind::Close), at + secs)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn collaborator_sees_only_own() {
    let e = setup().await;
    let t = clock::now() - 3600;
    connect(&e.app, "333333333", 1, "111111111", "Maria – PCEP", t, Some(60)).await;
    connect(&e.app, "444444444", 2, "222222222", "João – PCEP", t + 100, Some(60)).await;
    let cookie = web_login(&e.app, "maria", "senha12345").await;
    let (status, _, body) = send(&e.app, get_req("/historico", Some(&cookie))).await;
    assert_eq!(status, 200);
    assert!(body.contains("Padaria X – Caixa"), "{body}");
    assert!(!body.contains("Mercado Y – Balcão"), "{body}");
    // Tentar filtrar por outro colaborador não funciona, e o select não aparece.
    let (_, _, body) = send(
        &e.app,
        get_req(&format!("/historico?colaborador={}", e.joao), Some(&cookie)),
    )
    .await;
    assert!(body.contains("Padaria X – Caixa"));
    assert!(!body.contains("Mercado Y – Balcão"));
    assert!(!body.contains("name=\"colaborador\""));
}

#[tokio::test]
async fn admin_filters_by_user_and_client() {
    let e = setup().await;
    let t = clock::now() - 3600;
    connect(&e.app, "333333333", 1, "111111111", "Maria – PCEP", t, Some(60)).await;
    connect(&e.app, "444444444", 2, "222222222", "João – PCEP", t + 100, Some(60)).await;
    connect(&e.app, "444444444", 3, "111111111", "Maria – PCEP", t + 200, Some(60)).await;
    let cookie = web_login(&e.app, "admin", "senha-admin1").await;

    let (_, _, body) = send(&e.app, get_req("/historico", Some(&cookie))).await;
    assert!(body.contains("name=\"colaborador\""));
    assert!(body.contains("Padaria X – Caixa") && body.contains("Mercado Y – Balcão"));

    let uri = format!("/historico?colaborador={}", e.joao);
    let (_, _, body) = send(&e.app, get_req(&uri, Some(&cookie))).await;
    assert!(body.contains("Mercado Y – Balcão"));
    assert!(!body.contains("Padaria X – Caixa"));
    assert!(!body.contains("Maria – PCEP</td>"));

    let uri = format!("/historico?cliente={}", e.padaria);
    let (_, _, body) = send(&e.app, get_req(&uri, Some(&cookie))).await;
    assert!(body.contains("Padaria X – Caixa"));
    assert!(!body.contains("Mercado Y – Balcão"));

    let uri = format!("/historico?colaborador={}&cliente={}", e.maria, e.mercado);
    let (_, _, body) = send(&e.app, get_req(&uri, Some(&cookie))).await;
    assert!(body.contains("Mercado Y – Balcão"));
    assert!(!body.contains("Padaria X – Caixa"));
    assert!(!body.contains("João – PCEP</td>"));
    // Valores escolhidos continuam selecionados.
    assert!(body.contains(&format!("value=\"{}\" selected", e.maria)), "{body}");
    assert!(body.contains(&format!("value=\"{}\" selected", e.mercado)), "{body}");
}

#[tokio::test]
async fn period_filter_inclusive_end() {
    let e = setup().await;
    // 23:30 BRT do dia 10/03/2026 (já é dia 11 em UTC).
    connect(&e.app, "333333333", 1, "111111111", "Maria – PCEP", brt(2026, Month::March, 10, 23, 30), Some(60)).await;
    // Dia 9 às 12:00 e dia 11 às 00:00 ficam fora de 10..10.
    connect(&e.app, "444444444", 2, "111111111", "Maria – PCEP", brt(2026, Month::March, 9, 12, 0), Some(60)).await;
    connect(&e.app, "444444444", 3, "111111111", "Maria – PCEP", brt(2026, Month::March, 11, 0, 0), Some(60)).await;
    let cookie = web_login(&e.app, "admin", "senha-admin1").await;
    let (status, _, body) = send(
        &e.app,
        get_req("/historico?de=2026-03-10&ate=2026-03-10", Some(&cookie)),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("10/03/2026 23:30"), "{body}");
    assert!(!body.contains("09/03/2026"));
    assert!(!body.contains("11/03/2026"));
    assert!(body.contains("value=\"2026-03-10\""));
}

#[tokio::test]
async fn invalid_date_shows_message_and_ignores_filter() {
    let e = setup().await;
    connect(&e.app, "333333333", 1, "111111111", "Maria – PCEP", clock::now() - 3600, Some(60)).await;
    let cookie = web_login(&e.app, "admin", "senha-admin1").await;
    let (status, _, body) = send(
        &e.app,
        get_req("/historico?de=31/02/2026&ate=", Some(&cookie)),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body.contains("Data inválida"));
    assert!(body.contains("Padaria X – Caixa"));
}

#[tokio::test]
async fn duration_and_status_render() {
    let e = setup().await;
    let t = clock::now() - 7200;
    connect(&e.app, "333333333", 1, "111111111", "Maria – PCEP", t, Some(3725)).await;
    let cookie = web_login(&e.app, "admin", "senha-admin1").await;
    let (_, _, body) = send(&e.app, get_req("/historico", Some(&cookie))).await;
    assert!(body.contains("01:02:05"), "{body}");
    assert!(body.contains("Conectado"));
    assert!(body.contains("Acesso remoto"));

    // new + close sem aceite: "Não concluída", sem duração.
    let ev = |kind| ConnEvent {
        peer_id: "444444444".into(),
        conn_id: 9,
        session_id: "s".into(),
        kind,
    };
    connections::apply_conn_event(&e.app.pool, &ev(ConnEventKind::New { ip: "10.0.0.5".into() }), t + 10)
        .await
        .unwrap();
    connections::apply_conn_event(&e.app.pool, &ev(ConnEventKind::Close), t + 20)
        .await
        .unwrap();
    let (_, _, body) = send(&e.app, get_req("/historico", Some(&cookie))).await;
    assert!(body.contains("Não concluída"), "{body}");
    assert!(body.contains("10.0.0.5"));
}

#[tokio::test]
async fn unlinked_names_and_unregistered_code() {
    let e = setup().await;
    let t = clock::now() - 3600;
    // Operador sem vínculo e computador não cadastrado.
    connect(&e.app, "555666777", 1, "999888777", "Suporte Externo", t, Some(60)).await;
    let cookie = web_login(&e.app, "admin", "senha-admin1").await;
    let (_, _, body) = send(&e.app, get_req("/historico", Some(&cookie))).await;
    assert!(body.contains("Suporte Externo (não vinculado)"), "{body}");
    assert!(body.contains("555 666 777"), "{body}");
}

#[tokio::test]
async fn limit_warning_when_over_500() {
    let e = setup().await;
    let t = clock::now() - 100_000;
    for i in 0..501 {
        connections::apply_conn_event(
            &e.app.pool,
            &ConnEvent {
                peer_id: "333333333".into(),
                conn_id: i,
                session_id: "s".into(),
                kind: ConnEventKind::New { ip: "1.1.1.1".into() },
            },
            t + i,
        )
        .await
        .unwrap();
    }
    let cookie = web_login(&e.app, "admin", "senha-admin1").await;
    let (_, _, body) = send(&e.app, get_req("/historico", Some(&cookie))).await;
    assert!(body.contains("Mostrando as 500 conexões mais recentes"));
    assert_eq!(body.matches("<tr>").count(), 501, "cabeçalho + 500 linhas");
}

#[tokio::test]
async fn requires_login() {
    let e = setup().await;
    let (status, _, _) = send(&e.app, get_req("/historico", None)).await;
    assert!(status == 303 || status == 401, "{status}");
}

#[tokio::test]
async fn client_page_links_to_full_history() {
    let e = setup().await;
    let cookie = web_login(&e.app, "maria", "senha12345").await;
    let (_, _, body) = send(&e.app, get_req(&format!("/clientes/{}", e.padaria), Some(&cookie))).await;
    assert!(body.contains(&format!("href=\"/historico?cliente={}\"", e.padaria)));
    assert!(body.contains("Ver histórico completo"));
}
