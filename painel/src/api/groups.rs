use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    api::{account::user_payload, extract::ApiUser},
    error::ApiError,
    state::AppState,
    users,
};

/// Tamanho máximo de página aceito.
const MAX_PAGE_SIZE: usize = 100;
const DEFAULT_PAGE_SIZE: usize = 20;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/peers", get(empty_page))
        .route("/device-group/accessible", get(empty_page))
}

/// Janela de paginação `(itens a pular, tamanho)` a partir de `current`
/// (1-based) e `pageSize` (limitado a 100).
pub(crate) fn page_window(current: Option<usize>, page_size: Option<usize>) -> (usize, usize) {
    let size = page_size.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
    let skip = current.unwrap_or(1).max(1).saturating_sub(1).saturating_mul(size);
    (skip, size)
}

#[derive(Deserialize)]
pub(crate) struct PageQuery {
    /// Página, a partir de 1.
    pub(crate) current: Option<usize>,
    #[serde(rename = "pageSize")]
    pub(crate) page_size: Option<usize>,
}

async fn list_users(
    State(st): State<AppState>,
    ApiUser(_): ApiUser,
    Query(q): Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    let active: Vec<_> = users::list(&st.pool)
        .await?
        .into_iter()
        .filter(|u| u.active)
        .collect();
    let (skip, size) = page_window(q.current, q.page_size);
    let data: Vec<Value> = active.iter().skip(skip).take(size).map(user_payload).collect();
    Ok(Json(json!({ "total": active.len(), "data": data })))
}

/// O catálogo é gerido pelo painel: o app vê listas vazias de peers e grupos.
async fn empty_page(ApiUser(_): ApiUser) -> Json<Value> {
    Json(json!({ "total": 0, "data": [] }))
}
