//! Autenticação por usuário e senha, compartilhada entre o login do app
//! (`/api/login`) e o do painel web (`/entrar`): mesmas regras e mensagens.

use std::sync::OnceLock;

use crate::{
    auth::password::{hash_password, run_limited, verify_password},
    clock,
    state::AppState,
    users::{self, User},
};

/// Tamanho máximo do nome de usuário aceito no login.
const MAX_USERNAME_LEN: usize = 128;

/// Motivo da recusa do login.
#[derive(Debug)]
pub enum LoginFailure {
    /// Muitas tentativas: bloqueado pelo limitador.
    Blocked,
    /// Usuário inexistente, senha errada ou nome inválido (mesma mensagem).
    BadCredentials,
    /// Senha correta, mas o usuário está desativado.
    Inactive,
    /// Falha interna (banco, tarefa); a mensagem é só para o log.
    Internal(String),
}

impl LoginFailure {
    /// Mensagem para mostrar ao usuário.
    pub fn message(&self) -> &'static str {
        match self {
            LoginFailure::Blocked => "Muitas tentativas. Aguarde 15 minutos.",
            LoginFailure::BadCredentials => "Usuário ou senha incorretos",
            LoginFailure::Inactive => "Usuário desativado",
            LoginFailure::Internal(_) => "Erro interno",
        }
    }
}

/// Hash fictício (calculado uma vez) para gastar o mesmo tempo de CPU
/// quando o usuário não existe.
fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password("senha-fictícia-para-igualar-o-tempo"))
}

async fn verify_blocking(password: String, hash: Option<String>) -> Result<bool, LoginFailure> {
    run_limited(move || match hash {
        Some(h) => verify_password(&password, &h),
        None => {
            // Resultado descartado: só iguala o tempo de resposta.
            verify_password(&password, dummy_hash());
            false
        }
    })
    .await
    .map_err(|e| LoginFailure::Internal(e.to_string()))
}

/// Confere usuário e senha respeitando o limitador de tentativas.
///
/// A tentativa é contada antes de verificar a senha (atômico), então
/// requisições paralelas não furam o bloqueio. O contador só é zerado em
/// caso de sucesso.
pub async fn authenticate(
    state: &AppState,
    username: &str,
    password: String,
) -> Result<User, LoginFailure> {
    let key = username.trim().to_lowercase();
    // Nomes enormes nem chegam ao limiter nem ao banco (evita chaves gigantes na memória).
    if key.chars().count() > MAX_USERNAME_LEN {
        return Err(LoginFailure::BadCredentials);
    }
    if !state.limiter.try_acquire(&key, clock::now()) {
        return Err(LoginFailure::Blocked);
    }

    let found = users::find_by_username(&state.pool, username)
        .await
        .map_err(|e| LoginFailure::Internal(e.to_string()))?;
    let (user, hash) = match found {
        Some((u, h)) => (Some(u), Some(h)),
        None => (None, None),
    };
    let password_ok = verify_blocking(password, hash).await?;
    let user = match user {
        Some(u) if password_ok => u,
        _ => return Err(LoginFailure::BadCredentials),
    };
    if !user.active {
        return Err(LoginFailure::Inactive);
    }

    state.limiter.reset(&key);
    Ok(user)
}
