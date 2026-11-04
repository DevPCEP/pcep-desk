//! Gestão de colaboradores (somente administrador).

use askama::Template;
use askama_web::WebTemplate;
use axum::{
    Form, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    auth::tokens::revoke_user_tokens,
    state::AppState,
    users::{self, User, UserError},
    web::session::{AdminUser, error_page},
};

const ACTIVE: &str = "colaboradores";
const MAX_EMAIL_LEN: usize = 120;

type PageResult = Result<Response, Response>;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/colaboradores", get(list_page).post(create))
        .route("/colaboradores/novo", get(new_form))
        .route("/colaboradores/{id}", post(update))
        .route("/colaboradores/{id}/editar", get(edit_form))
        .route("/colaboradores/{id}/senha", get(password_form).post(reset_password))
        .route("/colaboradores/{id}/desativar", post(deactivate))
        .route("/colaboradores/{id}/reativar", post(reactivate))
}

#[derive(Template, WebTemplate)]
#[template(path = "colaboradores.html")]
struct ListPage {
    user: User,
    active: &'static str,
    users: Vec<User>,
    error: Option<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "colaborador_form.html")]
struct FormPage {
    user: User,
    active: &'static str,
    /// `None` = novo colaborador; `Some(id)` = edição.
    editing: Option<i64>,
    /// Edição do próprio administrador: o papel não pode ser removido.
    is_self: bool,
    usuario: String,
    nome: String,
    email: String,
    admin: bool,
    error: Option<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "colaborador_senha.html")]
struct PasswordPage {
    user: User,
    active: &'static str,
    target: User,
    error: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ProfileForm {
    usuario: String,
    nome: String,
    email: String,
    senha: String,
    /// Checkbox: presente ("on") quando marcado.
    admin: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PasswordForm {
    senha: String,
}

/// Erro interno: registra e responde 500 sem detalhes.
pub(super) fn internal(e: impl std::fmt::Display) -> Response {
    tracing::error!("erro interno no painel: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, "Erro interno").into_response()
}

fn not_found(admin: User) -> Response {
    error_page(
        StatusCode::NOT_FOUND,
        admin,
        "Colaborador não encontrado",
        "Esse colaborador não existe ou foi removido.",
    )
}

/// Mensagem de `UserError` para o formulário (sem expor detalhes do banco).
fn user_error_message(e: UserError) -> String {
    match e {
        UserError::Db(e) => {
            tracing::error!("erro de banco ao gravar colaborador: {e}");
            "Não foi possível salvar. Tente novamente.".to_string()
        }
        other => other.to_string(),
    }
}

async fn load(pool: &SqlitePool, id: i64, admin: &User) -> Result<User, Response> {
    match users::get(pool, id).await {
        Ok(Some(u)) => Ok(u),
        Ok(None) => Err(not_found(admin.clone())),
        Err(e) => Err(internal(e)),
    }
}

async fn render_list(pool: &SqlitePool, admin: User, error: Option<String>) -> PageResult {
    let users = users::list(pool).await.map_err(internal)?;
    Ok(ListPage {
        user: admin,
        active: ACTIVE,
        users,
        error,
    }
    .into_response())
}

async fn list_page(State(st): State<AppState>, AdminUser(admin): AdminUser) -> PageResult {
    render_list(&st.pool, admin, None).await
}

async fn new_form(AdminUser(admin): AdminUser) -> FormPage {
    FormPage {
        user: admin,
        active: ACTIVE,
        editing: None,
        is_self: false,
        usuario: String::new(),
        nome: String::new(),
        email: String::new(),
        admin: false,
        error: None,
    }
}

async fn edit_form(
    State(st): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
) -> PageResult {
    let target = load(&st.pool, id, &admin).await?;
    Ok(FormPage {
        is_self: target.id == admin.id,
        user: admin,
        active: ACTIVE,
        editing: Some(target.id),
        usuario: target.username,
        nome: target.display_name,
        email: target.email,
        admin: target.is_admin,
        error: None,
    }
    .into_response())
}

/// Valida nome (2–60 caracteres) e e-mail (até 120, pode ser vazio); devolve ambos aparados.
fn check_profile(nome: &str, email: &str) -> Result<(String, String), String> {
    let nome = nome.trim();
    let email = email.trim();
    if !(2..=60).contains(&nome.chars().count()) {
        return Err("O nome deve ter de 2 a 60 caracteres".to_string());
    }
    if email.chars().count() > MAX_EMAIL_LEN {
        return Err(format!(
            "E-mail muito longo: use no máximo {MAX_EMAIL_LEN} caracteres"
        ));
    }
    Ok((nome.to_string(), email.to_string()))
}

async fn create(
    State(st): State<AppState>,
    AdminUser(admin): AdminUser,
    Form(form): Form<ProfileForm>,
) -> PageResult {
    let is_admin = form.admin.is_some();
    let outcome = async {
        let (nome, email) = check_profile(&form.nome, &form.email)?;
        let id = users::create(&st.pool, &form.usuario, &nome, &form.senha, is_admin)
            .await
            .map_err(user_error_message)?;
        if !email.is_empty() {
            users::update_profile(&st.pool, id, &nome, &email, is_admin)
                .await
                .map_err(|e| user_error_message(e.into()))?;
        }
        Ok::<(), String>(())
    }
    .await;
    match outcome {
        Ok(()) => Ok(Redirect::to("/colaboradores").into_response()),
        Err(msg) => Ok(FormPage {
            user: admin,
            active: ACTIVE,
            editing: None,
            is_self: false,
            usuario: form.usuario,
            nome: form.nome,
            email: form.email,
            admin: is_admin,
            error: Some(msg),
        }
        .into_response()),
    }
}

async fn update(
    State(st): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
    Form(form): Form<ProfileForm>,
) -> PageResult {
    let target = load(&st.pool, id, &admin).await?;
    let is_self = target.id == admin.id;
    let is_admin = form.admin.is_some();
    let outcome = async {
        if is_self && !is_admin {
            return Err("Você não pode remover o próprio papel de administrador".to_string());
        }
        let (nome, email) = check_profile(&form.nome, &form.email)?;
        users::update_profile(&st.pool, target.id, &nome, &email, is_admin)
            .await
            .map_err(|e| user_error_message(e.into()))
    }
    .await;
    match outcome {
        Ok(()) => Ok(Redirect::to("/colaboradores").into_response()),
        Err(msg) => Ok(FormPage {
            user: admin,
            active: ACTIVE,
            editing: Some(target.id),
            is_self,
            usuario: target.username,
            nome: form.nome,
            email: form.email,
            // Quem edita a si mesmo continua administrador.
            admin: is_admin || is_self,
            error: Some(msg),
        }
        .into_response()),
    }
}

async fn password_form(
    State(st): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
) -> PageResult {
    let target = load(&st.pool, id, &admin).await?;
    Ok(PasswordPage {
        user: admin,
        active: ACTIVE,
        target,
        error: None,
    }
    .into_response())
}

/// Redefine a senha e derruba todas as sessões da pessoa (app e web).
async fn reset_password(
    State(st): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
    Form(form): Form<PasswordForm>,
) -> PageResult {
    let target = load(&st.pool, id, &admin).await?;
    match users::set_password(&st.pool, target.id, &form.senha).await {
        Ok(()) => {
            revoke_user_tokens(&st.pool, target.id).await.map_err(internal)?;
            Ok(Redirect::to("/colaboradores").into_response())
        }
        Err(e) => Ok(PasswordPage {
            user: admin,
            active: ACTIVE,
            target,
            error: Some(user_error_message(e)),
        }
        .into_response()),
    }
}

async fn deactivate(
    State(st): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
) -> PageResult {
    let target = load(&st.pool, id, &admin).await?;
    if target.id == admin.id {
        return render_list(
            &st.pool,
            admin,
            Some("Você não pode desativar a própria conta".to_string()),
        )
        .await;
    }
    users::set_active(&st.pool, target.id, false)
        .await
        .map_err(internal)?;
    Ok(Redirect::to("/colaboradores").into_response())
}

async fn reactivate(
    State(st): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
) -> PageResult {
    let target = load(&st.pool, id, &admin).await?;
    users::set_active(&st.pool, target.id, true)
        .await
        .map_err(internal)?;
    Ok(Redirect::to("/colaboradores").into_response())
}
