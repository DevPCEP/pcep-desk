use std::sync::OnceLock;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{State, rejection::BytesRejection},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::{
    api::extract::{ApiJson, ApiUser, bearer_token},
    auth::{
        password::{hash_password, verify_password},
        tokens::{TokenKind, issue_token, revoke_token},
    },
    clock,
    error::ApiError,
    state::AppState,
    users::{self, User},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login-options", get(login_options))
        .route("/login", post(login))
        .route("/currentUser", post(current_user))
        .route("/logout", post(logout))
}

/// Dados do usuário no formato esperado pelo app.
pub fn user_payload(u: &User) -> Value {
    json!({
        "name": u.username,
        "display_name": u.display_name,
        "email": u.email,
        "note": "",
        "status": 1,
        "is_admin": u.is_admin,
    })
}

/// Erro de login: o app espera status 200 com `{"error": ...}`.
fn login_error(msg: &str) -> Json<Value> {
    Json(json!({ "error": msg }))
}

async fn login_options() -> Json<Value> {
    Json(json!([]))
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    uuid: String,
    #[serde(default, rename = "type")]
    login_type: Option<String>,
}

/// Hash fictício (calculado uma vez) para gastar o mesmo tempo de CPU
/// quando o usuário não existe.
fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password("senha-fictícia-para-igualar-o-tempo"))
}

async fn verify_blocking(password: String, hash: Option<String>) -> Result<bool, ApiError> {
    tokio::task::spawn_blocking(move || match hash {
        Some(h) => verify_password(&password, &h),
        None => {
            // Resultado descartado: só iguala o tempo de resposta.
            verify_password(&password, dummy_hash());
            false
        }
    })
    .await
    .map_err(ApiError::internal)
}

async fn login(
    State(st): State<AppState>,
    ApiJson(req): ApiJson<LoginReq>,
) -> Result<Json<Value>, ApiError> {
    if !matches!(req.login_type.as_deref(), None | Some("account")) {
        return Ok(login_error("Método de login não suportado"));
    }
    let key = req.username.trim().to_lowercase();
    if !st.limiter.check(&key, clock::now()) {
        return Ok(login_error("Muitas tentativas. Aguarde 15 minutos."));
    }

    let found = users::find_by_username(&st.pool, &req.username).await?;
    let (user, hash) = match found {
        Some((u, h)) => (Some(u), Some(h)),
        None => (None, None),
    };
    let password_ok = verify_blocking(req.password, hash).await?;
    let user = match user {
        Some(u) if password_ok => u,
        _ => {
            st.limiter.record_failure(&key, clock::now());
            return Ok(login_error("Usuário ou senha incorretos"));
        }
    };
    if !user.active {
        return Ok(login_error("Usuário desativado"));
    }

    st.limiter.reset(&key);
    let token = issue_token(&st.pool, user.id, TokenKind::App).await?;
    record_device(&st.pool, user.id, &req.id, &req.uuid).await?;
    Ok(Json(json!({
        "type": "access_token",
        "access_token": token,
        "user": user_payload(&user),
    })))
}

/// Associa o computador (código do RustDesk) ao colaborador. O computador
/// pode passar a ser de outro colaborador, por isso o upsert.
async fn record_device(
    pool: &SqlitePool,
    user_id: i64,
    peer_id: &str,
    uuid: &str,
) -> sqlx::Result<()> {
    let peer_id = peer_id.trim();
    if peer_id.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO user_devices (peer_id, user_id, uuid, last_login_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(peer_id) DO UPDATE SET
           user_id = excluded.user_id,
           uuid = excluded.uuid,
           last_login_at = excluded.last_login_at",
    )
    .bind(peer_id)
    .bind(user_id)
    .bind(uuid)
    .bind(clock::now())
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Deserialize, Default)]
struct CurrentUserReq {
    #[serde(default)]
    id: String,
    #[serde(default)]
    uuid: String,
}

async fn current_user(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<Value>, ApiError> {
    let body = body.map_err(|e| ApiError::new(e.status(), "Requisição grande demais"))?;
    // O app só informa id/uuid; corpo vazio ou estranho não deve derrubar a sessão.
    let req: CurrentUserReq = serde_json::from_slice(&body).unwrap_or_default();
    record_device(&st.pool, user.id, &req.id, &req.uuid).await?;
    Ok(Json(user_payload(&user)))
}

/// Sempre 200 com corpo vazio, mesmo sem token ou com token inválido.
async fn logout(State(st): State<AppState>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    if let Some(token) = bearer_token(&headers) {
        revoke_token(&st.pool, token).await?;
    }
    Ok(StatusCode::OK)
}
