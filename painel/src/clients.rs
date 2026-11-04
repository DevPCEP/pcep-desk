//! Clientes do escritório e seus computadores (código de conexão do RustDesk).

use sqlx::SqlitePool;

use crate::clock;

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Client {
    pub id: i64,
    pub name: String,
    pub document: String,
    pub note: String,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Computer {
    pub id: i64,
    pub client_id: i64,
    pub peer_id: String,
    pub label: String,
    pub note: String,
}

#[derive(Debug)]
pub enum ClientError {
    NameTaken,
    InvalidPeerId,
    PeerIdTaken { client_name: String },
    Invalid(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::NameTaken => write!(f, "Já existe um cliente com esse nome"),
            ClientError::InvalidPeerId => write!(f, "Código de conexão inválido"),
            ClientError::PeerIdTaken { client_name } => {
                write!(f, "Esse código já está cadastrado no cliente {client_name}")
            }
            ClientError::Invalid(msg) => write!(f, "{msg}"),
            ClientError::Db(e) => write!(f, "erro de banco de dados: {e}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<sqlx::Error> for ClientError {
    fn from(e: sqlx::Error) -> Self {
        ClientError::Db(e)
    }
}

/// Normaliza o código de conexão digitado pelo usuário.
///
/// Remove espaços, pontos e hífens; se sobrarem só dígitos (6 a 12), é um ID
/// numérico do RustDesk. Caso contrário, o valor original (sem espaços nas
/// pontas) precisa ser um ID personalizado `^[A-Za-z0-9_-]{6,16}$`.
pub fn normalize_peer_id(raw: &str) -> Option<String> {
    let stripped: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '.' && *c != '-')
        .collect();
    if (6..=12).contains(&stripped.len()) && stripped.chars().all(|c| c.is_ascii_digit()) {
        return Some(stripped);
    }
    let trimmed = raw.trim();
    let custom = (6..=16).contains(&trimmed.len())
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    custom.then(|| trimmed.to_string())
}

fn check_client_fields(
    name: &str,
    document: &str,
    note: &str,
) -> Result<(String, String, String), ClientError> {
    let (name, document, note) = (name.trim(), document.trim(), note.trim());
    if !(2..=120).contains(&name.chars().count()) {
        return Err(ClientError::Invalid(
            "Nome do cliente deve ter entre 2 e 120 caracteres".to_string(),
        ));
    }
    if document.chars().count() > 20 {
        return Err(ClientError::Invalid(
            "Documento deve ter no máximo 20 caracteres".to_string(),
        ));
    }
    if note.chars().count() > 1000 {
        return Err(ClientError::Invalid(
            "Observação deve ter no máximo 1000 caracteres".to_string(),
        ));
    }
    Ok((name.to_string(), document.to_string(), note.to_string()))
}

fn check_computer_fields(label: &str, note: &str) -> Result<(String, String), ClientError> {
    let (label, note) = (label.trim(), note.trim());
    if !(1..=60).contains(&label.chars().count()) {
        return Err(ClientError::Invalid(
            "Rótulo do computador deve ter entre 1 e 60 caracteres".to_string(),
        ));
    }
    if note.chars().count() > 1000 {
        return Err(ClientError::Invalid(
            "Observação deve ter no máximo 1000 caracteres".to_string(),
        ));
    }
    Ok((label.to_string(), note.to_string()))
}

fn map_name_error(e: sqlx::Error) -> ClientError {
    match e {
        sqlx::Error::Database(d) if d.is_unique_violation() => ClientError::NameTaken,
        e => ClientError::Db(e),
    }
}

pub async fn create(
    pool: &SqlitePool,
    name: &str,
    document: &str,
    note: &str,
) -> Result<i64, ClientError> {
    let (name, document, note) = check_client_fields(name, document, note)?;
    sqlx::query("INSERT INTO clients (name, document, note, created_at) VALUES (?, ?, ?, ?)")
        .bind(&name)
        .bind(&document)
        .bind(&note)
        .bind(clock::now())
        .execute(pool)
        .await
        .map(|r| r.last_insert_rowid())
        .map_err(map_name_error)
}

pub async fn update(
    pool: &SqlitePool,
    id: i64,
    name: &str,
    document: &str,
    note: &str,
) -> Result<(), ClientError> {
    let (name, document, note) = check_client_fields(name, document, note)?;
    let res = sqlx::query("UPDATE clients SET name = ?, document = ?, note = ? WHERE id = ?")
        .bind(&name)
        .bind(&document)
        .bind(&note)
        .bind(id)
        .execute(pool)
        .await
        .map_err(map_name_error)?;
    if res.rows_affected() == 0 {
        return Err(ClientError::Invalid("Cliente não encontrado".to_string()));
    }
    Ok(())
}

/// Remove o cliente; os computadores saem junto (ON DELETE CASCADE).
pub async fn delete(pool: &SqlitePool, id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM clients WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get(pool: &SqlitePool, id: i64) -> sqlx::Result<Option<Client>> {
    sqlx::query_as("SELECT id, name, document, note FROM clients WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Escapa `%`, `_` e `\` para uso em `LIKE ... ESCAPE '\'`.
fn escape_like(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Busca clientes por nome, documento e rótulo/código dos computadores.
/// Busca vazia lista todos. Resultado ordenado por nome, sem repetições.
/// (O `LIKE` do SQLite ignora maiúsculas só para letras ASCII.)
pub async fn search(pool: &SqlitePool, q: &str, limit: i64) -> sqlx::Result<Vec<Client>> {
    let pattern = format!("%{}%", escape_like(q.trim()));
    sqlx::query_as(
        r"SELECT DISTINCT c.id, c.name, c.document, c.note
          FROM clients c
          LEFT JOIN computers p ON p.client_id = c.id
          WHERE c.name LIKE ?1 ESCAPE '\'
             OR c.document LIKE ?1 ESCAPE '\'
             OR p.label LIKE ?1 ESCAPE '\'
             OR p.peer_id LIKE ?1 ESCAPE '\'
          ORDER BY c.name
          LIMIT ?2",
    )
    .bind(pattern)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn add_computer(
    pool: &SqlitePool,
    client_id: i64,
    peer_id_raw: &str,
    label: &str,
    note: &str,
) -> Result<i64, ClientError> {
    let peer_id = normalize_peer_id(peer_id_raw).ok_or(ClientError::InvalidPeerId)?;
    let (label, note) = check_computer_fields(label, note)?;
    let res = sqlx::query(
        "INSERT INTO computers (client_id, peer_id, label, note, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(client_id)
    .bind(&peer_id)
    .bind(&label)
    .bind(&note)
    .bind(clock::now())
    .execute(pool)
    .await;
    match res {
        Ok(r) => Ok(r.last_insert_rowid()),
        Err(sqlx::Error::Database(d)) if d.is_unique_violation() => {
            // Único índice único informado pelo usuário: computers.peer_id.
            let owner: Option<String> = sqlx::query_scalar(
                "SELECT c.name FROM computers p JOIN clients c ON c.id = p.client_id
                 WHERE p.peer_id = ?",
            )
            .bind(&peer_id)
            .fetch_optional(pool)
            .await?;
            match owner {
                Some(client_name) => Err(ClientError::PeerIdTaken { client_name }),
                None => Err(ClientError::Db(sqlx::Error::Database(d))),
            }
        }
        Err(sqlx::Error::Database(d)) if d.is_foreign_key_violation() => {
            Err(ClientError::Invalid("Cliente não encontrado".to_string()))
        }
        Err(e) => Err(ClientError::Db(e)),
    }
}

pub async fn update_computer(
    pool: &SqlitePool,
    id: i64,
    label: &str,
    note: &str,
) -> Result<(), ClientError> {
    let (label, note) = check_computer_fields(label, note)?;
    let res = sqlx::query("UPDATE computers SET label = ?, note = ? WHERE id = ?")
        .bind(&label)
        .bind(&note)
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(ClientError::Invalid("Computador não encontrado".to_string()));
    }
    Ok(())
}

pub async fn remove_computer(pool: &SqlitePool, id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM computers WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_computers(pool: &SqlitePool, client_id: i64) -> sqlx::Result<Vec<Computer>> {
    sqlx::query_as(
        "SELECT id, client_id, peer_id, label, note FROM computers
         WHERE client_id = ? ORDER BY label COLLATE NOCASE, id",
    )
    .bind(client_id)
    .fetch_all(pool)
    .await
}

/// Linha da junção; colunas com alias para evitar nomes repetidos (`id`, `note`).
#[derive(sqlx::FromRow)]
struct ComputerWithClient {
    id: i64,
    client_id: i64,
    peer_id: String,
    label: String,
    note: String,
    client_name: String,
    client_document: String,
    client_note: String,
}

/// Todos os computadores com seu cliente (base do catálogo compartilhado).
pub async fn all_computers_with_client(pool: &SqlitePool) -> sqlx::Result<Vec<(Computer, Client)>> {
    let rows: Vec<ComputerWithClient> = sqlx::query_as(
        "SELECT p.id, p.client_id, p.peer_id, p.label, p.note,
                c.name AS client_name, c.document AS client_document, c.note AS client_note
         FROM computers p JOIN clients c ON c.id = p.client_id
         ORDER BY c.name, p.label COLLATE NOCASE, p.id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                Computer {
                    id: r.id,
                    client_id: r.client_id,
                    peer_id: r.peer_id,
                    label: r.label,
                    note: r.note,
                },
                Client {
                    id: r.client_id,
                    name: r.client_name,
                    document: r.client_document,
                    note: r.client_note,
                },
            )
        })
        .collect())
}
