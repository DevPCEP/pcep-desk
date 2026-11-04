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
        login::{LoginFailure, authenticate},
        tokens::{TokenKind, issue_token, revoke_token},
    },
    clients::normalize_peer_id,
    clock,
    error::ApiError,
    state::AppState,
    users::User,
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

async fn login(
    State(st): State<AppState>,
    ApiJson(req): ApiJson<LoginReq>,
) -> Result<Json<Value>, ApiError> {
    if !matches!(req.login_type.as_deref(), None | Some("account")) {
        return Ok(login_error("Método de login não suportado"));
    }
    let user = match authenticate(&st, &req.username, req.password).await {
        Ok(u) => u,
        Err(LoginFailure::Internal(msg)) => return Err(ApiError::internal(msg)),
        Err(f) => return Ok(login_error(f.message())),
    };
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
///
/// O código vem do app e não é confiável: é normalizado e, se for inválido ou
/// já for computador de um cliente (`computers`), não é gravado, para que um
/// colaborador não reivindique o código de outro computador. A troca de dono
/// entre colaboradores é permitida, mas fica registrada no log.
async fn record_device(
    pool: &SqlitePool,
    user_id: i64,
    peer_id: &str,
    uuid: &str,
) -> sqlx::Result<()> {
    let Some(peer_id) = normalize_peer_id(peer_id) else {
        if !peer_id.trim().is_empty() {
            tracing::warn!(user_id, "login com código de computador inválido: não gravado");
        }
        return Ok(());
    };
    let is_client_computer: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM computers WHERE peer_id = ?)")
            .bind(&peer_id)
            .fetch_one(pool)
            .await?;
    if is_client_computer {
        tracing::warn!(
            user_id,
            peer_id = %peer_id,
            "login com código de um computador de cliente: não gravado"
        );
        return Ok(());
    }
    let previous_owner: Option<i64> =
        sqlx::query_scalar("SELECT user_id FROM user_devices WHERE peer_id = ?")
            .bind(&peer_id)
            .fetch_optional(pool)
            .await?;
    if let Some(previous) = previous_owner.filter(|p| *p != user_id) {
        tracing::warn!(
            peer_id = %peer_id,
            previous_user_id = previous,
            new_user_id = user_id,
            "computador de colaborador trocou de dono"
        );
    }
    sqlx::query(
        "INSERT INTO user_devices (peer_id, user_id, uuid, last_login_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(peer_id) DO UPDATE SET
           user_id = excluded.user_id,
           uuid = excluded.uuid,
           last_login_at = excluded.last_login_at",
    )
    .bind(&peer_id)
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
async fn logout(State(st): State<AppState>, headers: HeaderMap) -> StatusCode {
    if let Some(token) = bearer_token(&headers)
        && let Err(e) = revoke_token(&st.pool, token).await
    {
        tracing::error!("falha ao revogar token no logout: {e}");
    }
    StatusCode::OK
}
