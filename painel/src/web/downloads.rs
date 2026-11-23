//! Página pública de download do Pc&P Desk (cliente e equipe).
//!
//! Fica fora do login de propósito: o colaborador manda o link ao cliente.
//! Sem login, mostra só o app do cliente; o app da equipe aparece apenas para
//! quem está logado no painel. Os arquivos vêm da última release do app.

use askama::Template;
use askama_web::WebTemplate;
use axum::{Router, extract::State, http::HeaderMap, routing::get};

use crate::{
    auth::tokens::{TokenKind, resolve_token},
    state::AppState,
    web::session::session_token,
};

/// Base dos links: sempre a última release publicada.
const BASE_URL: &str = "https://github.com/DevPCEP/pcep-desk-app/releases/latest/download/";

/// Código-fonte do app (AGPL).
const SOURCE_URL: &str = "https://github.com/DevPCEP/pcep-desk-app";

const WINDOWS_HELP: &str = "Se aparecer \"O Windows protegeu o computador\", clique em Mais informações → Executar assim mesmo.";
const MACOS_HELP: &str = "Na primeira vez, clique com o botão direito no aplicativo → Abrir. Macs com processador Intel ainda não são suportados.";
const LINUX_HELP: &str = "Dê permissão de execução (botão direito → Propriedades → Permitir executar como programa) e abra.";
const DEB_HELP: &str = "Ubuntu/Debian: baixe e rode sudo apt install ./PcepDesk-Linux-x86_64.deb (instala e inicia junto com o computador). Para usar sem instalar, prefira o AppImage.";
const DEB_EQUIPE_HELP: &str = "Ubuntu/Debian: baixe e rode sudo apt install ./PcepDeskEquipe-Linux-x86_64.deb (instala e inicia junto com o computador). Para usar sem instalar, prefira o AppImage.";

/// Um entregável: rótulo da plataforma, nome exato do arquivo na release e instrução.
struct Entrega {
    rotulo: &'static str,
    arquivo: &'static str,
    ajuda: &'static str,
}

/// Entregáveis do Pc&P Desk (cliente). Nomes iguais aos do workflow do app.
/// macOS Intel fora dos builds desde 2026-11-04 (runners escassos).
const CLIENTE: [Entrega; 4] = [
    Entrega { rotulo: "Windows", arquivo: "PcepDesk-Windows-x64.exe", ajuda: WINDOWS_HELP },
    Entrega {
        rotulo: "macOS (Apple Silicon — M1/M2/M3/M4)",
        arquivo: "PcepDesk-macOS-arm64.dmg",
        ajuda: MACOS_HELP,
    },
    Entrega { rotulo: "Linux (AppImage)", arquivo: "PcepDesk-Linux-x86_64.AppImage", ajuda: LINUX_HELP },
    Entrega { rotulo: "Linux (.deb)", arquivo: "PcepDesk-Linux-x86_64.deb", ajuda: DEB_HELP },
];

/// Entregáveis do Pc&P Desk Equipe (colaborador).
const EQUIPE: [Entrega; 4] = [
    Entrega { rotulo: "Windows", arquivo: "PcepDeskEquipe-Windows-x64.exe", ajuda: WINDOWS_HELP },
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
    Entrega {
        rotulo: "Linux (.deb)",
        arquivo: "PcepDeskEquipe-Linux-x86_64.deb",
        ajuda: DEB_EQUIPE_HELP,
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
    /// Vazia sem login: a seção da equipe não aparece.
    equipe: Vec<Link>,
    source_url: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/baixar", get(download_page))
}

async fn download_page(State(st): State<AppState>, headers: HeaderMap) -> DownloadPage {
    // Sessão opcional: sem cookie, com cookie inválido ou com erro ao conferir,
    // a página continua pública (só o app do cliente), sem redirecionar.
    let logged = match session_token(&headers) {
        Some(token) => match resolve_token(&st.pool, token, TokenKind::Web).await {
            Ok(user) => user.is_some(),
            Err(e) => {
                tracing::error!("erro ao resolver sessão na página de download: {e}");
                false
            }
        },
        None => false,
    };
    DownloadPage {
        cliente: links(&CLIENTE),
        equipe: if logged { links(&EQUIPE) } else { Vec::new() },
        source_url: SOURCE_URL,
    }
}
