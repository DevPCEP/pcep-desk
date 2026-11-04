//! Catálogo de endereços no "modo novo" do app RustDesk: leituras `POST`
//! (com ou sem corpo) e ações que respondem 200 com corpo vazio.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State, rejection::BytesRejection},
    http::StatusCode,
    routing::{delete, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    addressbook::{self, AbError, Catalog, INVALID_REQUEST, SHARED_AB_GUID, SHARED_AB_NAME},
    api::{
        extract::ApiUser,
        groups::{PageQuery, page_window},
    },
    error::ApiError,
    state::AppState,
    users::User,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ab/settings", post(settings))
        .route("/ab/personal", post(personal))
        .route("/ab/shared/profiles", post(shared_profiles))
        .route("/ab/peers", post(list_peers))
        .route("/ab/tags/{guid}", post(list_tags))
        .route("/ab/peer/add/{guid}", post(peer_add))
        .route("/ab/peer/update/{guid}", put(peer_update))
        .route("/ab/peer/{guid}", delete(peer_delete))
        .route("/ab/tag/add/{guid}", post(tag_add))
        .route("/ab/tag/rename/{guid}", put(tag_rename))
        .route("/ab/tag/update/{guid}", put(tag_update))
        .route("/ab/tag/{guid}", delete(tag_delete))
}

impl From<AbError> for ApiError {
    fn from(e: AbError) -> Self {
        match e {
            AbError::Invalid(msg) => ApiError::bad_request(msg),
            AbError::NotFound(msg) => ApiError::not_found(msg),
            AbError::Db(e) => ApiError::from(e),
        }
    }
}

/// Resolve o guid para o usuário; inexistente ou de outro usuário é 404.
async fn catalog(st: &AppState, user: &User, guid: &str) -> Result<Catalog, ApiError> {
    addressbook::resolve(&st.pool, user.id, guid)
        .await?
        .ok_or_else(|| ApiError::not_found("Catálogo não encontrado"))
}

/// Garante que o guid é o catálogo pessoal do usuário (o compartilhado é
/// somente leitura).
async fn writable(st: &AppState, user: &User, guid: &str) -> Result<(), ApiError> {
    match catalog(st, user, guid).await? {
        Catalog::Personal => Ok(()),
        Catalog::Shared => Err(ApiError::forbidden("Catálogo somente leitura")),
    }
}

/// Corpo JSON das ações, sem exigir `Content-Type`.
fn json_body(body: Result<Bytes, BytesRejection>) -> Result<Value, ApiError> {
    let bytes = body.map_err(|rej| ApiError::new(rej.status(), "Requisição grande demais"))?;
    serde_json::from_slice(&bytes).map_err(|_| ApiError::bad_request("Corpo da requisição inválido"))
}

/// Resposta de sucesso das ações: 200 com corpo vazio.
type Done = Result<StatusCode, ApiError>;

/// Página `{"total", "data"}` de uma lista já completa.
fn page(items: Vec<Value>, current: Option<usize>, page_size: Option<usize>) -> Json<Value> {
    let (skip, size) = page_window(current, page_size);
    let total = items.len();
    let data: Vec<Value> = items.into_iter().skip(skip).take(size).collect();
    Json(json!({ "total": total, "data": data }))
}

async fn settings(ApiUser(_): ApiUser) -> Json<Value> {
    Json(json!({ "max_peer_one_ab": 0 }))
}

async fn personal(State(st): State<AppState>, ApiUser(user): ApiUser) -> Result<Json<Value>, ApiError> {
    let guid = addressbook::personal_guid(&st.pool, user.id).await?;
    Ok(Json(json!({ "guid": guid })))
}

async fn shared_profiles(ApiUser(_): ApiUser, Query(q): Query<PageQuery>) -> Json<Value> {
    let profile = json!({
        "guid": SHARED_AB_GUID,
        "name": SHARED_AB_NAME,
        "owner": "PCEP",
        "note": "",
        "rule": 1,
    });
    page(vec![profile], q.current, q.page_size)
}

#[derive(Deserialize)]
struct PeersQuery {
    ab: Option<String>,
    current: Option<usize>,
    #[serde(rename = "pageSize")]
    page_size: Option<usize>,
}

async fn list_peers(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Query(q): Query<PeersQuery>,
) -> Result<Json<Value>, ApiError> {
    let guid = q.ab.as_deref().ok_or_else(|| ApiError::bad_request(INVALID_REQUEST))?;
    let items = match catalog(&st, &user, guid).await? {
        Catalog::Shared => addressbook::shared_peers(&st.pool).await?,
        Catalog::Personal => addressbook::personal_peers(&st.pool, guid).await?,
    };
    Ok(page(items, q.current, q.page_size))
}

/// Responde uma lista JSON (não um objeto), como o app espera.
async fn list_tags(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(guid): Path<String>,
) -> Result<Json<Vec<Value>>, ApiError> {
    let tags = match catalog(&st, &user, &guid).await? {
        Catalog::Shared => addressbook::shared_tags(&st.pool).await?,
        Catalog::Personal => addressbook::personal_tags(&st.pool, &guid).await?,
    };
    Ok(Json(tags))
}

async fn peer_add(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(guid): Path<String>,
    body: Result<Bytes, BytesRejection>,
) -> Done {
    writable(&st, &user, &guid).await?;
    let (id, patch) = addressbook::parse_peer(&json_body(body)?)?;
    addressbook::upsert_peer(&st.pool, &guid, &id, &patch).await?;
    Ok(StatusCode::OK)
}

async fn peer_update(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(guid): Path<String>,
    body: Result<Bytes, BytesRejection>,
) -> Done {
    writable(&st, &user, &guid).await?;
    let (id, patch) = addressbook::parse_peer(&json_body(body)?)?;
    addressbook::update_peer(&st.pool, &guid, &id, &patch).await?;
    Ok(StatusCode::OK)
}

async fn peer_delete(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(guid): Path<String>,
    body: Result<Bytes, BytesRejection>,
) -> Done {
    writable(&st, &user, &guid).await?;
    let ids = addressbook::parse_ids(&json_body(body)?)?;
    addressbook::delete_peers(&st.pool, &guid, &ids).await?;
    Ok(StatusCode::OK)
}

async fn tag_add(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(guid): Path<String>,
    body: Result<Bytes, BytesRejection>,
) -> Done {
    writable(&st, &user, &guid).await?;
    let (name, color) = addressbook::parse_tag(&json_body(body)?)?;
    addressbook::add_tag(&st.pool, &guid, &name, color).await?;
    Ok(StatusCode::OK)
}

async fn tag_rename(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(guid): Path<String>,
    body: Result<Bytes, BytesRejection>,
) -> Done {
    writable(&st, &user, &guid).await?;
    let (old, new) = addressbook::parse_rename(&json_body(body)?)?;
    addressbook::rename_tag(&st.pool, &guid, &old, &new).await?;
    Ok(StatusCode::OK)
}

async fn tag_update(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(guid): Path<String>,
    body: Result<Bytes, BytesRejection>,
) -> Done {
    writable(&st, &user, &guid).await?;
    let (name, color) = addressbook::parse_tag(&json_body(body)?)?;
    addressbook::update_tag(&st.pool, &guid, &name, color).await?;
    Ok(StatusCode::OK)
}

async fn tag_delete(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(guid): Path<String>,
    body: Result<Bytes, BytesRejection>,
) -> Done {
    writable(&st, &user, &guid).await?;
    let names = addressbook::parse_tag_names(&json_body(body)?)?;
    addressbook::delete_tags(&st.pool, &guid, &names).await?;
    Ok(StatusCode::OK)
}
