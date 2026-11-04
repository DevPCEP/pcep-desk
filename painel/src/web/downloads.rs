//! Página pública de download do Pc&P Desk (cliente e equipe).
//!
//! Fica fora do login de propósito: o colaborador manda o link ao cliente.
//! Os arquivos vêm da última release do repositório do app.

use askama::Template;
use askama_web::WebTemplate;
use axum::{Router, routing::get};

use crate::state::AppState;

/// Base dos links: sempre a última release publicada.
const BASE_URL: &str = "https://github.com/DevPCEP/pcep-desk-app/releases/latest/download/";

/// Código-fonte do app (AGPL).
const SOURCE_URL: &str = "https://github.com/DevPCEP/pcep-desk-app";

const WINDOWS_HELP: &str = "Se aparecer \"O Windows protegeu o computador\", clique em Mais informações → Executar assim mesmo.";
const MACOS_HELP: &str = "Na primeira vez, clique com o botão direito no aplicativo → Abrir.";
const LINUX_HELP: &str = "Dê permissão de execução (botão direito → Propriedades → Permitir executar como programa) e abra.";

/// Um entregável: rótulo da plataforma, nome exato do arquivo na release e instrução.
struct Entrega {
    rotulo: &'static str,
    arquivo: &'static str,
    ajuda: &'static str,
}

/// Entregáveis do Pc&P Desk (cliente). Nomes iguais aos do workflow do app.
const CLIENTE: [Entrega; 4] = [
    Entrega { rotulo: "Windows", arquivo: "PcepDesk-Windows-x64.exe", ajuda: WINDOWS_HELP },
    Entrega { rotulo: "macOS (Intel)", arquivo: "PcepDesk-macOS-x64.dmg", ajuda: MACOS_HELP },
    Entrega {
        rotulo: "macOS (Apple Silicon — M1/M2/M3/M4)",
        arquivo: "PcepDesk-macOS-arm64.dmg",
        ajuda: MACOS_HELP,
    },
    Entrega { rotulo: "Linux (AppImage)", arquivo: "PcepDesk-Linux-x86_64.AppImage", ajuda: LINUX_HELP },
];

/// Entregáveis do Pc&P Desk Equipe (colaborador).
const EQUIPE: [Entrega; 4] = [
    Entrega { rotulo: "Windows", arquivo: "PcepDeskEquipe-Windows-x64.exe", ajuda: WINDOWS_HELP },
    Entrega { rotulo: "macOS (Intel)", arquivo: "PcepDeskEquipe-macOS-x64.dmg", ajuda: MACOS_HELP },
    Entrega {
        rotulo: "macOS (Apple Silicon — M1/M2/M3/M4)",
        arquivo: "PcepDeskEquipe-macOS-arm64.dmg",
        ajuda: MACOS_HELP,
    },
    Entrega {
        rotulo: "Linux (AppImage)",
        arquivo: "PcepDeskEquipe-Linux-x86_64.AppImage",
        ajuda: LINUX_HELP,
    },
];

/// Link já montado, pronto para o template.
struct Link {
    rotulo: &'static str,
    url: String,
    ajuda: &'static str,
}

fn links(entregas: &[Entrega]) -> Vec<Link> {
    entregas
        .iter()
        .map(|e| Link {
            rotulo: e.rotulo,
            url: format!("{BASE_URL}{}", e.arquivo),
            ajuda: e.ajuda,
        })
        .collect()
}

#[derive(Template, WebTemplate)]
#[template(path = "baixar.html")]
struct DownloadPage {
    cliente: Vec<Link>,
    equipe: Vec<Link>,
    source_url: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/baixar", get(download_page))
}

async fn download_page() -> DownloadPage {
    DownloadPage {
        cliente: links(&CLIENTE),
        equipe: links(&EQUIPE),
        source_url: SOURCE_URL,
    }
}
