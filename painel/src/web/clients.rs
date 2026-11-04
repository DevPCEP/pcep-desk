//! Clientes, computadores e dispositivos não vinculados.

use std::collections::HashMap;

use askama::Template;
use askama_web::WebTemplate;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    clients::{self, Client, ClientError, Computer},
    clock,
    connections::{self, HistoryFilter, HistoryRow},
    devices::{self, Device},
    state::AppState,
    users::User,
    web::{
        format::{duration_label, group_code},
        session::{AdminUser, WebUser, error_page},
        users::internal,
    },
};

const ACTIVE: &str = "clientes";
const ACTIVE_UNLINKED: &str = "nao-vinculados";
/// Máximo de clientes na lista (a busca refina o resto).
const LIST_LIMIT: i64 = 200;
const HISTORY_LIMIT: i64 = 20;

type PageResult = Result<Response, Response>;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/clientes", get(list_page).post(create))
        .route("/clientes/lista", get(list_fragment))
        .route("/clientes/novo", get(new_form))
        .route("/clientes/{id}", get(detail).post(update))
        .route("/clientes/{id}/editar", get(edit_form))
        .route("/clientes/{id}/excluir", post(delete))
        .route("/clientes/{id}/computadores/novo", get(new_computer_form))
        .route("/clientes/{id}/computadores", post(add_computer))
        .route("/computadores/{id}/editar", get(edit_computer_form))
        .route("/computadores/{id}", post(update_computer))
        .route("/computadores/{id}/remover", post(remove_computer))
        .route("/nao-vinculados", get(unlinked_page))
        .route("/nao-vinculados/vincular", post(link))
        .route("/nao-vinculados/{peer_id}/vincular", get(link_form))
}

// ---------------------------------------------------------------- exibição

struct ClientRow {
    id: i64,
    name: String,
    document: String,
    computers: usize,
    online: usize,
}

struct ComputerView {
    id: i64,
    label: String,
    peer_id: String,
    code_display: String,
    note: String,
    online: bool,
    /// `None`: o dispositivo nunca falou com o servidor.
    last_seen: Option<String>,
    hostname: String,
    os: String,
    username: String,
}

struct ConnView {
    started: String,
    operator: String,
    computer: String,
    status: &'static str,
    duration: String,
}

struct UnlinkedView {
    peer_id: String,
    code_display: String,
    hostname: String,
    os: String,
    online: bool,
    last_seen: String,
}

fn computer_view(c: Computer, device: Option<Device>, now: i64) -> ComputerView {
    let (online, last_seen, hostname, os, username) = match device {
        Some(d) => (
            devices::is_online(d.last_seen_at, now),
            Some(clock::format_ts(d.last_seen_at)),
            d.hostname,
            d.os,
            d.username,
        ),
        None => (false, None, String::new(), String::new(), String::new()),
    };
    ComputerView {
        id: c.id,
        code_display: group_code(&c.peer_id),
        label: c.label,
        peer_id: c.peer_id,
        note: c.note,
        online,
        last_seen,
        hostname,
        os,
        username,
    }
}

fn conn_view(r: HistoryRow) -> ConnView {
    let status = connections::status_label(&r);
    ConnView {
        started: clock::format_ts(r.started_at),
        operator: r.user_display_name.unwrap_or(r.operator_name),
        computer: r.computer_label.unwrap_or_else(|| group_code(&r.peer_id)),
        status,
        duration: duration_label(r.authorized_at, r.ended_at),
    }
}

fn not_found(user: User) -> Response {
    error_page(
        StatusCode::NOT_FOUND,
        user,
        "Não encontrado",
        "Esse registro não existe ou foi removido.",
    )
}

/// Mensagem de `ClientError` para o formulário (sem expor detalhes do banco).
fn client_error_message(e: ClientError) -> String {
    match e {
        ClientError::Db(e) => {
            tracing::error!("erro de banco ao gravar cliente/computador: {e}");
            "Não foi possível salvar. Tente novamente.".to_string()
        }
        other => other.to_string(),
    }
}

async fn load_client(pool: &SqlitePool, id: i64, user: &User) -> Result<Client, Response> {
    match clients::get(pool, id).await {
        Ok(Some(c)) => Ok(c),
        Ok(None) => Err(not_found(user.clone())),
        Err(e) => Err(internal(e)),
    }
}

async fn load_computer(pool: &SqlitePool, id: i64, user: &User) -> Result<Computer, Response> {
    match clients::get_computer(pool, id).await {
        Ok(Some(c)) => Ok(c),
        Ok(None) => Err(not_found(user.clone())),
        Err(e) => Err(internal(e)),
    }
}

// ------------------------------------------------------------------ lista

#[derive(Deserialize, Default)]
#[serde(default)]
struct SearchQuery {
    q: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "clientes.html")]
struct ListPage {
    user: User,
    active: &'static str,
    q: String,
    rows: Vec<ClientRow>,
    truncated: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "clientes_lista.html")]
struct ListFragment {
    q: String,
    rows: Vec<ClientRow>,
    truncated: bool,
}

/// Clientes da busca com a contagem de computadores e de online.
async fn client_rows(pool: &SqlitePool, q: &str) -> Result<(Vec<ClientRow>, bool), Response> {
    let found = clients::search(pool, q, LIST_LIMIT).await.map_err(internal)?;
    let truncated = found.len() as i64 >= LIST_LIMIT;
    let mut rows: HashMap<i64, ClientRow> = HashMap::new();
    let order: Vec<i64> = found.iter().map(|c| c.id).collect();
    for c in found {
        rows.insert(
            c.id,
            ClientRow {
                id: c.id,
                name: c.name,
                document: c.document,
                computers: 0,
                online: 0,
            },
        );
    }
    let now = clock::now();
    for (computer, _) in clients::all_computers_with_client(pool)
        .await
        .map_err(internal)?
    {
        let Some(row) = rows.get_mut(&computer.client_id) else {
            continue;
        };
        row.computers += 1;
        let device = devices::get(pool, &computer.peer_id).await.map_err(internal)?;
        if device.is_some_and(|d| devices::is_online(d.last_seen_at, now)) {
            row.online += 1;
        }
    }
    let rows = order.iter().filter_map(|id| rows.remove(id)).collect();
    Ok((rows, truncated))
}

async fn list_page(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Query(query): Query<SearchQuery>,
) -> PageResult {
    let (rows, truncated) = client_rows(&st.pool, &query.q).await?;
    Ok(ListPage {
        user,
        active: ACTIVE,
        q: query.q,
        rows,
        truncated,
    }
    .into_response())
}

async fn list_fragment(
    State(st): State<AppState>,
    WebUser(_): WebUser,
    Query(query): Query<SearchQuery>,
) -> PageResult {
    let (rows, truncated) = client_rows(&st.pool, &query.q).await?;
    Ok(ListFragment {
        q: query.q,
        rows,
        truncated,
    }
    .into_response())
}

// ---------------------------------------------------------------- detalhe

#[derive(Template, WebTemplate)]
#[template(path = "cliente.html")]
struct DetailPage {
    user: User,
    active: &'static str,
    client: Client,
    computers: Vec<ComputerView>,
    history: Vec<ConnView>,
}

async fn detail(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Path(id): Path<i64>,
) -> PageResult {
    let client = load_client(&st.pool, id, &user).await?;
    let now = clock::now();
    let mut computers = Vec::new();
    for c in clients::list_computers(&st.pool, id).await.map_err(internal)? {
        let device = devices::get(&st.pool, &c.peer_id).await.map_err(internal)?;
        computers.push(computer_view(c, device, now));
    }
    let history = connections::query(
        &st.pool,
        &HistoryFilter {
            user_id: None,
            client_id: Some(id),
            from: None,
            to: None,
            limit: HISTORY_LIMIT,
        },
    )
    .await
    .map_err(internal)?
    .into_iter()
    .map(conn_view)
    .collect();
    Ok(DetailPage {
        user,
        active: ACTIVE,
        client,
        computers,
        history,
    }
    .into_response())
}

// ------------------------------------------------------- formulário cliente

#[derive(Template, WebTemplate)]
#[template(path = "cliente_form.html")]
struct ClientFormPage {
    user: User,
    active: &'static str,
    /// `None` = novo cliente; `Some(id)` = edição.
    editing: Option<i64>,
    nome: String,
    documento: String,
    observacao: String,
    error: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ClientForm {
    nome: String,
    documento: String,
    observacao: String,
}

fn client_form_page(
    user: User,
    editing: Option<i64>,
    form: ClientForm,
    error: Option<String>,
) -> Response {
    ClientFormPage {
        user,
        active: ACTIVE,
        editing,
        nome: form.nome,
        documento: form.documento,
        observacao: form.observacao,
        error,
    }
    .into_response()
}

async fn new_form(WebUser(user): WebUser) -> Response {
    client_form_page(user, None, ClientForm::default(), None)
}

async fn edit_form(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Path(id): Path<i64>,
) -> PageResult {
    let c = load_client(&st.pool, id, &user).await?;
    let form = ClientForm {
        nome: c.name,
        documento: c.document,
        observacao: c.note,
    };
    Ok(client_form_page(user, Some(c.id), form, None))
}

async fn create(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Form(form): Form<ClientForm>,
) -> PageResult {
    match clients::create(&st.pool, &form.nome, &form.documento, &form.observacao).await {
        Ok(id) => Ok(Redirect::to(&format!("/clientes/{id}")).into_response()),
        Err(e) => Ok(client_form_page(
            user,
            None,
            form,
            Some(client_error_message(e)),
        )),
    }
}

async fn update(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Path(id): Path<i64>,
    Form(form): Form<ClientForm>,
) -> PageResult {
    let c = load_client(&st.pool, id, &user).await?;
    match clients::update(&st.pool, c.id, &form.nome, &form.documento, &form.observacao).await {
        Ok(()) => Ok(Redirect::to(&format!("/clientes/{}", c.id)).into_response()),
        Err(e) => Ok(client_form_page(
            user,
            Some(c.id),
            form,
            Some(client_error_message(e)),
        )),
    }
}

/// Só administrador; os computadores saem junto (cascata no banco).
async fn delete(
    State(st): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
) -> PageResult {
    let c = load_client(&st.pool, id, &admin).await?;
    clients::delete(&st.pool, c.id).await.map_err(internal)?;
    Ok(Redirect::to("/clientes").into_response())
}

// ---------------------------------------------------- formulário computador

#[derive(Template, WebTemplate)]
#[template(path = "computador_form.html")]
struct ComputerFormPage {
    user: User,
    active: &'static str,
    title: &'static str,
    action: String,
    cancel: String,
    /// Cliente fixo (mostrado como texto); `None` quando se escolhe um.
    client_name: Option<String>,
    /// Lista para o `select` ao vincular um dispositivo.
    pick_from: Option<Vec<Client>>,
    cliente_id: i64,
    /// Edição: o código não muda (só rótulo e observação).
    fixed_code: bool,
    codigo: String,
    code_display: String,
    rotulo: String,
    observacao: String,
    error: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ComputerForm {
    codigo: String,
    rotulo: String,
    observacao: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct LinkForm {
    /// Texto, para aceitar o campo vazio sem erro de formato.
    cliente_id: String,
    codigo: String,
    rotulo: String,
    observacao: String,
}

async fn new_computer_form(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Path(id): Path<i64>,
) -> PageResult {
    let client = load_client(&st.pool, id, &user).await?;
    Ok(add_computer_page(user, &client, ComputerForm::default(), None))
}

fn add_computer_page(
    user: User,
    client: &Client,
    form: ComputerForm,
    error: Option<String>,
) -> Response {
    ComputerFormPage {
        user,
        active: ACTIVE,
        title: "Novo computador",
        action: format!("/clientes/{}/computadores", client.id),
        cancel: format!("/clientes/{}", client.id),
        client_name: Some(client.name.clone()),
        pick_from: None,
        cliente_id: client.id,
        fixed_code: false,
        codigo: form.codigo,
        code_display: String::new(),
        rotulo: form.rotulo,
        observacao: form.observacao,
        error,
    }
    .into_response()
}

async fn add_computer(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Path(id): Path<i64>,
    Form(form): Form<ComputerForm>,
) -> PageResult {
    let client = load_client(&st.pool, id, &user).await?;
    match clients::add_computer(&st.pool, client.id, &form.codigo, &form.rotulo, &form.observacao)
        .await
    {
        Ok(_) => Ok(Redirect::to(&format!("/clientes/{}", client.id)).into_response()),
        Err(e) => Ok(add_computer_page(
            user,
            &client,
            form,
            Some(client_error_message(e)),
        )),
    }
}

fn edit_computer_page(
    user: User,
    client_name: String,
    computer: &Computer,
    form: ComputerForm,
    error: Option<String>,
) -> Response {
    ComputerFormPage {
        user,
        active: ACTIVE,
        title: "Editar computador",
        action: format!("/computadores/{}", computer.id),
        cancel: format!("/clientes/{}", computer.client_id),
        client_name: Some(client_name),
        pick_from: None,
        cliente_id: computer.client_id,
        fixed_code: true,
        codigo: computer.peer_id.clone(),
        code_display: group_code(&computer.peer_id),
        rotulo: form.rotulo,
        observacao: form.observacao,
        error,
    }
    .into_response()
}

async fn edit_computer_form(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Path(id): Path<i64>,
) -> PageResult {
    let computer = load_computer(&st.pool, id, &user).await?;
    let client = load_client(&st.pool, computer.client_id, &user).await?;
    let form = ComputerForm {
        codigo: String::new(),
        rotulo: computer.label.clone(),
        observacao: computer.note.clone(),
    };
    Ok(edit_computer_page(user, client.name, &computer, form, None))
}

async fn update_computer(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Path(id): Path<i64>,
    Form(form): Form<ComputerForm>,
) -> PageResult {
    let computer = load_computer(&st.pool, id, &user).await?;
    match clients::update_computer(&st.pool, computer.id, &form.rotulo, &form.observacao).await {
        Ok(()) => Ok(Redirect::to(&format!("/clientes/{}", computer.client_id)).into_response()),
        Err(e) => {
            let client = load_client(&st.pool, computer.client_id, &user).await?;
            Ok(edit_computer_page(
                user,
                client.name,
                &computer,
                form,
                Some(client_error_message(e)),
            ))
        }
    }
}

async fn remove_computer(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Path(id): Path<i64>,
) -> PageResult {
    let computer = load_computer(&st.pool, id, &user).await?;
    clients::remove_computer(&st.pool, computer.id)
        .await
        .map_err(internal)?;
    Ok(Redirect::to(&format!("/clientes/{}", computer.client_id)).into_response())
}

// ---------------------------------------------------------- não vinculados

#[derive(Template, WebTemplate)]
#[template(path = "nao_vinculados.html")]
struct UnlinkedPage {
    user: User,
    active: &'static str,
    devices: Vec<UnlinkedView>,
    /// A lista foi cortada em `UNLINKED_LIMIT`.
    truncated: bool,
}

async fn unlinked_page(State(st): State<AppState>, WebUser(user): WebUser) -> PageResult {
    let now = clock::now();
    let found = devices::list_unlinked(&st.pool).await.map_err(internal)?;
    let truncated = found.len() as i64 >= devices::UNLINKED_LIMIT;
    let devices = found
        .into_iter()
        .map(|d| UnlinkedView {
            code_display: group_code(&d.peer_id),
            online: devices::is_online(d.last_seen_at, now),
            last_seen: clock::format_ts(d.last_seen_at),
            peer_id: d.peer_id,
            hostname: d.hostname,
            os: d.os,
        })
        .collect();
    Ok(UnlinkedPage {
        user,
        active: ACTIVE_UNLINKED,
        devices,
        truncated,
    }
    .into_response())
}

fn link_page(
    user: User,
    clients: Vec<Client>,
    form: LinkForm,
    cliente_id: i64,
    error: Option<String>,
) -> Response {
    ComputerFormPage {
        user,
        active: ACTIVE_UNLINKED,
        title: "Vincular dispositivo a um cliente",
        action: "/nao-vinculados/vincular".to_string(),
        cancel: "/nao-vinculados".to_string(),
        client_name: None,
        pick_from: Some(clients),
        cliente_id,
        fixed_code: false,
        codigo: form.codigo,
        code_display: String::new(),
        rotulo: form.rotulo,
        observacao: form.observacao,
        error,
    }
    .into_response()
}

async fn link_form(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Path(peer_id): Path<String>,
) -> PageResult {
    let device = match devices::get(&st.pool, &peer_id).await {
        Ok(Some(d)) => d,
        Ok(None) => return Err(not_found(user)),
        Err(e) => return Err(internal(e)),
    };
    let clients = clients::search(&st.pool, "", i64::MAX).await.map_err(internal)?;
    let form = LinkForm {
        cliente_id: String::new(),
        codigo: device.peer_id,
        // Sugestão: o nome do computador informado pelo app.
        rotulo: device.hostname,
        observacao: String::new(),
    };
    Ok(link_page(user, clients, form, 0, None))
}

async fn link(
    State(st): State<AppState>,
    WebUser(user): WebUser,
    Form(form): Form<LinkForm>,
) -> PageResult {
    let cliente_id = form.cliente_id.trim().parse::<i64>().unwrap_or(0);
    let result = if cliente_id == 0 {
        Err("Escolha o cliente".to_string())
    } else {
        clients::add_computer(&st.pool, cliente_id, &form.codigo, &form.rotulo, &form.observacao)
            .await
            .map_err(client_error_message)
    };
    match result {
        Ok(_) => Ok(Redirect::to(&format!("/clientes/{cliente_id}")).into_response()),
        Err(msg) => {
            let clients = clients::search(&st.pool, "", i64::MAX).await.map_err(internal)?;
            Ok(link_page(user, clients, form, cliente_id, Some(msg)))
        }
    }
}
