use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::{clock, users::User};

/// Tipo de token: app (180 dias) ou sessão web (12 horas).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    App,
    Web,
}

impl TokenKind {
    fn as_str(self) -> &'static str {
        match self {
            TokenKind::App => "app",
            TokenKind::Web => "web",
        }
    }

    fn ttl_secs(self) -> i64 {
        match self {
            TokenKind::App => 180 * 24 * 3600,
            TokenKind::Web => 12 * 3600,
        }
    }
}

/// SHA-256 em hex: é o que fica no banco.
fn hash_token(raw: &str) -> String {
    Sha256::digest(raw.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Emite um token novo e devolve o valor cru (só o hash é gravado).
pub async fn issue_token(
    pool: &SqlitePool,
    user_id: i64,
    kind: TokenKind,
) -> sqlx::Result<String> {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let raw = URL_SAFE_NO_PAD.encode(bytes);
    let now = clock::now();
    sqlx::query(
        "INSERT INTO tokens (user_id, token_hash, kind, created_at, expires_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(hash_token(&raw))
    .bind(kind.as_str())
    .bind(now)
    .bind(now + kind.ttl_secs())
    .execute(pool)
    .await?;
    Ok(raw)
}

/// Usuário dono do token; `None` se inexistente, expirado, de outro tipo ou usuário inativo.
pub async fn resolve_token(
    pool: &SqlitePool,
    raw: &str,
    kind: TokenKind,
) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>(
        "SELECT u.id, u.username, u.display_name, u.email, u.is_admin, u.active
         FROM tokens t JOIN users u ON u.id = t.user_id
         WHERE t.token_hash = ? AND t.kind = ? AND t.expires_at > ? AND u.active = 1",
    )
    .bind(hash_token(raw))
    .bind(kind.as_str())
    .bind(clock::now())
    .fetch_optional(pool)
    .await
}

pub async fn revoke_token(pool: &SqlitePool, raw: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM tokens WHERE token_hash = ?")
        .bind(hash_token(raw))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn revoke_user_tokens(pool: &SqlitePool, user_id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM tokens WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
