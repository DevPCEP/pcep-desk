use axum::{
    extract::{FromRequest, FromRequestParts, Request, rejection::JsonRejection},
    http::{HeaderMap, header, request::Parts},
};
use serde::de::DeserializeOwned;

use crate::{
    auth::tokens::{TokenKind, resolve_token},
    error::ApiError,
    state::AppState,
    users::User,
};

/// Extrai o token do cabeçalho `Authorization: Bearer <token>`.
pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

/// Usuário autenticado pelo token do app. Falha com 401 se o token for
/// ausente, inválido, expirado, revogado ou o usuário estiver desativado.
pub struct ApiUser(pub User);

impl FromRequestParts<AppState> for ApiUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(&parts.headers).ok_or_else(ApiError::unauthorized)?;
        resolve_token(&state.pool, token, TokenKind::App)
            .await?
            .map(ApiUser)
            .ok_or_else(ApiError::unauthorized)
    }
}

/// `Json<T>` que responde erros no formato `{"error": ...}` (400 corpo
/// inválido, 413 corpo grande demais, 415 tipo de conteúdo errado).
pub struct ApiJson<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(v)) => Ok(ApiJson(v)),
            Err(rej) => Err(ApiError::new(rej.status(), rejection_message(&rej))),
        }
    }
}

fn rejection_message(rej: &JsonRejection) -> &'static str {
    match rej {
        JsonRejection::BytesRejection(_) => "Requisição grande demais",
        JsonRejection::MissingJsonContentType(_) => "Tipo de conteúdo inválido",
        _ => "Corpo da requisição inválido",
    }
}
