use sqlx::SqlitePool;

use crate::{
    auth::{password::hash_password, tokens::revoke_user_tokens},
    clock,
};

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub is_admin: bool,
    pub active: bool,
}

#[derive(Debug)]
pub enum UserError {
    UsernameTaken,
    Invalid(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserError::UsernameTaken => write!(f, "usuário já existe"),
            UserError::Invalid(msg) => write!(f, "{msg}"),
            UserError::Db(e) => write!(f, "erro de banco de dados: {e}"),
        }
    }
}

impl std::error::Error for UserError {}

impl From<sqlx::Error> for UserError {
    fn from(e: sqlx::Error) -> Self {
        UserError::Db(e)
    }
}

#[derive(sqlx::FromRow)]
struct UserWithHash {
    #[sqlx(flatten)]
    user: User,
    password_hash: String,
}

/// Valida o nome de usuário (`^[a-z0-9._-]{3,32}$`, já em minúsculas).
fn valid_username(u: &str) -> bool {
    (3..=32).contains(&u.chars().count())
        && u.chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '.' | '_' | '-'))
}

/// Hash de Argon2 é pesado: roda fora do executor assíncrono.
async fn hash_blocking(password: &str) -> Result<String, UserError> {
    let password = password.to_string();
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| UserError::Invalid(format!("falha ao processar a senha: {e}")))
}

fn check_password(password: &str) -> Result<(), UserError> {
    if password.chars().count() < 8 {
        return Err(UserError::Invalid(
            "A senha deve ter pelo menos 8 caracteres".to_string(),
        ));
    }
    Ok(())
}

pub async fn create(
    pool: &SqlitePool,
    username: &str,
    display_name: &str,
    password: &str,
    is_admin: bool,
) -> Result<i64, UserError> {
    let username = username.trim().to_lowercase();
    if !valid_username(&username) {
        return Err(UserError::Invalid(
            "Usuário inválido: use de 3 a 32 caracteres (letras minúsculas, números, ponto, hífen ou sublinhado)".to_string(),
        ));
    }
    let display_name = display_name.trim();
    if !(2..=60).contains(&display_name.chars().count()) {
        return Err(UserError::Invalid(
            "O nome deve ter de 2 a 60 caracteres".to_string(),
        ));
    }
    check_password(password)?;
    let hash = hash_blocking(password).await?;

    let res = sqlx::query(
        "INSERT INTO users (username, display_name, password_hash, is_admin, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&username)
    .bind(display_name)
    .bind(&hash)
    .bind(is_admin)
    .bind(clock::now())
    .execute(pool)
    .await;
    match res {
        Ok(r) => Ok(r.last_insert_rowid()),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(UserError::UsernameTaken),
        Err(e) => Err(UserError::Db(e)),
    }
}

pub async fn find_by_username(
    pool: &SqlitePool,
    username: &str,
) -> sqlx::Result<Option<(User, String)>> {
    let row = sqlx::query_as::<_, UserWithHash>(
        "SELECT id, username, display_name, email, is_admin, active, password_hash
         FROM users WHERE username = ?",
    )
    .bind(username.trim())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.user, r.password_hash)))
}

pub async fn get(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, display_name, email, is_admin, active FROM users WHERE id = ?",
    )
    .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<User>> {
    sqlx::query_as::<_, User>(
        "SELECT id, username, display_name, email, is_admin, active
         FROM users ORDER BY display_name COLLATE NOCASE, id",
    )
    .fetch_all(pool)
    .await
}

pub async fn update_profile(
    pool: &SqlitePool,
    id: i64,
    display_name: &str,
    email: &str,
    is_admin: bool,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET display_name = ?, email = ?, is_admin = ? WHERE id = ?")
        .bind(display_name.trim())
        .bind(email.trim())
        .bind(is_admin)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_password(pool: &SqlitePool, id: i64, password: &str) -> Result<(), UserError> {
    check_password(password)?;
    let hash = hash_blocking(password).await?;
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Ativa/desativa o usuário. Desativar também revoga todos os tokens dele.
pub async fn set_active(pool: &SqlitePool, id: i64, active: bool) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET active = ? WHERE id = ?")
        .bind(active)
        .bind(id)
        .execute(pool)
        .await?;
    if !active {
        revoke_user_tokens(pool, id).await?;
    }
    Ok(())
}
