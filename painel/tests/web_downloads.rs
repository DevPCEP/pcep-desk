mod common;

use common::{get_req, send, test_app, web_login};
use painel::users;

const BASE: &str = "https://github.com/DevPCEP/pcep-desk-app/releases/latest/download/";

#[tokio::test]
async fn download_page_public() {
    let app = test_app().await;
    let (status, _, body) = send(&app, get_req("/baixar", None)).await;
    assert_eq!(status, 200);
    assert!(body.contains("Para clientes"));
    assert!(body.contains("Para a equipe"));
    // Página pública: sem o menu de usuário logado.
    assert!(!body.contains("Sair"));
    assert!(!body.contains("href=\"/historico\""));
}

#[tokio::test]
async fn download_links_point_to_latest_release() {
    let app = test_app().await;
    let (_, _, body) = send(&app, get_req("/baixar", None)).await;
    for nome in [
        "PcepDesk-Windows-x64.exe",
        "PcepDeskEquipe-Windows-x64.exe",
        "PcepDesk-Linux-x86_64.AppImage",
        "PcepDeskEquipe-Linux-x86_64.AppImage",
        "PcepDesk-macOS-arm64.dmg",
        "PcepDeskEquipe-macOS-arm64.dmg",
    ] {
        assert!(
            body.contains(&format!("{BASE}{nome}")),
            "link ausente: {nome}"
        );
    }
}

#[tokio::test]
async fn download_page_has_source_link() {
    let app = test_app().await;
    let (_, _, body) = send(&app, get_req("/baixar", None)).await;
    assert!(body.contains("href=\"https://github.com/DevPCEP/pcep-desk-app\""));
    assert!(body.contains("Código-fonte"));
}

#[tokio::test]
async fn download_page_public_has_security_headers() {
    let app = test_app().await;
    let (_, headers, _) = send(&app, get_req("/baixar", None)).await;
    assert_eq!(headers.get("cache-control").unwrap(), "no-store");
    assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
    assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
}

#[tokio::test]
async fn download_page_shows_instructions_and_brand() {
    let app = test_app().await;
    let (_, _, body) = send(&app, get_req("/baixar", None)).await;
    assert!(body.contains("Pc&amp;P Desk"));
    assert!(!body.contains("PCEP Desk"));
    // macOS Intel fora dos builds (runners escassos) — sem link quebrado na página
    assert!(!body.contains("macOS-x64.dmg"));
    assert!(body.contains("Macs com processador Intel"));
    assert!(body.contains("Linux (AppImage)"));
    assert!(body.contains("Permitir executar como programa"));
}

#[tokio::test]
async fn menu_has_download_link_for_logged_user() {
    let app = test_app().await;
    users::create(&app.pool, "maria", "Maria", "senha12345", false)
        .await
        .unwrap();
    let cookie = web_login(&app, "maria", "senha12345").await;
    let (status, _, body) = send(&app, get_req("/clientes", Some(&cookie))).await;
    assert_eq!(status, 200);
    assert!(body.contains("href=\"/baixar\""));
    assert!(body.contains("Pc&amp;P Desk"));
}
