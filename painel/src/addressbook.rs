//! Catálogo de endereços do app RustDesk: o catálogo compartilhado "Clientes
//! PCEP" (somente leitura, montado a partir dos clientes) e o catálogo
//! pessoal de cada colaborador.

use std::collections::HashSet;

use rand::Rng;
use serde_json::{Map, Value, json};
use sqlx::{FromRow, SqliteConnection, SqlitePool};

use crate::{clients, devices};

pub const SHARED_AB_GUID: &str = "clientes-pcep";
pub const SHARED_AB_NAME: &str = "Clientes PCEP";

/// Mensagem padrão para requisições mal formadas.
pub const INVALID_REQUEST: &str = "Requisição inválida";
const TOO_LARGE: &str = "Valor grande demais";
const PEER_NOT_FOUND: &str = "Peer não encontrado";
const TAG_NOT_FOUND: &str = "Tag não encontrada";

const MAX_ID_LEN: usize = 64;
const MAX_ALIAS_LEN: usize = 120;
const MAX_NOTE_LEN: usize = 1000;
const MAX_TAG_LEN: usize = 60;
const MAX_TAGS_PER_PEER: usize = 50;
const MAX_HASH_LEN: usize = 256;
/// Limite dos campos textuais sem limite próprio (usuário, host, plataforma).
const MAX_FIELD_LEN: usize = 255;

/// Erro das operações do catálogo.
#[derive(Debug)]
pub enum AbError {
    /// Entrada inválida (400).
    Invalid(&'static str),
    /// Peer ou tag inexistente (404).
    NotFound(&'static str),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for AbError {
    fn from(e: sqlx::Error) -> Self {
        AbError::Db(e)
    }
}

/// Catálogo a que um guid se refere, do ponto de vista de um usuário.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Catalog {
    Shared,
    Personal,
}

// ---------------------------------------------------------------------------
// Conversões e cores
// ---------------------------------------------------------------------------

/// Plataforma no formato esperado pelo app, a partir de `devices.os`
/// (ex.: `"Windows / 11"`). Desconhecida → vazio.
pub fn platform_of(os: &str) -> &'static str {
    let lower = os.to_lowercase();
    match lower.split(" / ").next().unwrap_or("").trim() {
        "windows" => "Windows",
        "linux" => "Linux",
        "macos" => "Mac OS",
        "android" => "Android",
        _ if lower.contains("linux") => "Linux",
        _ => "",
    }
}

/// FNV-1a de 32 bits sobre os bytes UTF-8.
pub fn fnv1a32(s: &str) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for byte in s.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

/// Cor ARGB determinística de uma tag (opaca).
pub fn tag_color(name: &str) -> i64 {
    0xFF00_0000 | i64::from(fnv1a32(name) & 0x00FF_FFFF)
}

/// UUID v4 aleatório (sem depender de um crate só para isso).
fn new_guid() -> String {
    let mut b = [0u8; 16];
    rand::rng().fill_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

// ---------------------------------------------------------------------------
// Resolução de catálogos
// ---------------------------------------------------------------------------

/// Guid do catálogo pessoal do usuário (criado na primeira chamada).
pub async fn personal_guid(pool: &SqlitePool, user_id: i64) -> sqlx::Result<String> {
    sqlx::query("INSERT INTO ab_personal (guid, user_id) VALUES (?, ?) ON CONFLICT(user_id) DO NOTHING")
        .bind(new_guid())
        .bind(user_id)
        .execute(pool)
        .await?;
    let (guid,): (String,) = sqlx::query_as("SELECT guid FROM ab_personal WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(guid)
}

/// Resolve um guid para o usuário: o compartilhado, o pessoal dele ou nada
/// (inexistente ou de outro usuário).
pub async fn resolve(pool: &SqlitePool, user_id: i64, guid: &str) -> sqlx::Result<Option<Catalog>> {
    if guid == SHARED_AB_GUID {
        return Ok(Some(Catalog::Shared));
    }
    let own: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM ab_personal WHERE guid = ? AND user_id = ?")
        .bind(guid)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(own.map(|_| Catalog::Personal))
}

// ---------------------------------------------------------------------------
// Catálogo compartilhado
// ---------------------------------------------------------------------------

/// Um peer por computador cadastrado, com os dados do dispositivo (se já
/// houver sinal dele).
pub async fn shared_peers(pool: &SqlitePool) -> sqlx::Result<Vec<Value>> {
    let rows = clients::all_computers_with_client(pool).await?;
    let mut out = Vec::with_capacity(rows.len());
    for (computer, client) in rows {
        let device = devices::get(pool, &computer.peer_id).await?;
        let (hostname, username, platform) = match &device {
            Some(d) => (d.hostname.as_str(), d.username.as_str(), platform_of(&d.os)),
            None => ("", "", ""),
        };
        out.push(json!({
            "id": computer.peer_id,
            "alias": format!("{} – {}", client.name, computer.label),
            "tags": [client.name],
            "hostname": hostname,
            "username": username,
            "platform": platform,
            "note": computer.note,
        }));
    }
    Ok(out)
}

/// Tags do catálogo compartilhado: clientes que têm ao menos um computador.
pub async fn shared_tags(pool: &SqlitePool) -> sqlx::Result<Vec<Value>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT c.name FROM clients c JOIN computers p ON p.client_id = c.id ORDER BY c.name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(name,)| json!({ "color": tag_color(&name), "name": name }))
        .collect())
}

// ---------------------------------------------------------------------------
// Validação de entrada
// ---------------------------------------------------------------------------

/// Campos de um peer enviados pelo app; `None` = não enviado (não altera).
#[derive(Debug, Default)]
pub struct PeerPatch {
    pub alias: Option<String>,
    pub tags: Option<Vec<String>>,
    pub username: Option<String>,
    pub hostname: Option<String>,
    pub platform: Option<String>,
    pub hash: Option<String>,
    pub note: Option<String>,
}

fn text_value(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn object(v: &Value) -> Result<&Map<String, Value>, AbError> {
    v.as_object().ok_or(AbError::Invalid(INVALID_REQUEST))
}

/// Id de peer normalizado (trim); vazio ou grande demais é inválido.
fn peer_id(obj: &Map<String, Value>) -> Result<String, AbError> {
    let id = obj
        .get("id")
        .and_then(text_value)
        .ok_or(AbError::Invalid(INVALID_REQUEST))?;
    if id.is_empty() || id.chars().count() > MAX_ID_LEN {
        return Err(AbError::Invalid(INVALID_REQUEST));
    }
    Ok(id)
}

fn opt_text(obj: &Map<String, Value>, key: &str, max: usize) -> Result<Option<String>, AbError> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.chars().count() <= max => Ok(Some(s.clone())),
        Some(Value::String(_)) => Err(AbError::Invalid(TOO_LARGE)),
        Some(_) => Err(AbError::Invalid(INVALID_REQUEST)),
    }
}

/// Nome de tag normalizado (trim); vazio é inválido.
fn tag_name(raw: &str) -> Result<String, AbError> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(AbError::Invalid(INVALID_REQUEST));
    }
    if name.chars().count() > MAX_TAG_LEN {
        return Err(AbError::Invalid(TOO_LARGE));
    }
    Ok(name.to_string())
}

fn opt_tags(obj: &Map<String, Value>) -> Result<Option<Vec<String>>, AbError> {
    let items = match obj.get("tags") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Array(items)) => items,
        Some(_) => return Err(AbError::Invalid(INVALID_REQUEST)),
    };
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for item in items {
        let Value::String(raw) = item else {
            return Err(AbError::Invalid(INVALID_REQUEST));
        };
        if raw.trim().is_empty() {
            continue;
        }
        let name = tag_name(raw)?;
        if seen.insert(name.clone()) {
            tags.push(name);
        }
    }
    if tags.len() > MAX_TAGS_PER_PEER {
        return Err(AbError::Invalid(TOO_LARGE));
    }
    Ok(Some(tags))
}

/// Interpreta o corpo de `peer/add` e `peer/update`. Campos desconhecidos
/// (e `password`) são ignorados.
pub fn parse_peer(body: &Value) -> Result<(String, PeerPatch), AbError> {
    let obj = object(body)?;
    let id = peer_id(obj)?;
    let patch = PeerPatch {
        alias: opt_text(obj, "alias", MAX_ALIAS_LEN)?,
        tags: opt_tags(obj)?,
        username: opt_text(obj, "username", MAX_FIELD_LEN)?,
        hostname: opt_text(obj, "hostname", MAX_FIELD_LEN)?,
        platform: opt_text(obj, "platform", MAX_FIELD_LEN)?,
        hash: opt_text(obj, "hash", MAX_HASH_LEN)?,
        note: opt_text(obj, "note", MAX_NOTE_LEN)?,
    };
    Ok((id, patch))
}

/// Corpo do `DELETE /peer`: array de ids (vazios são descartados).
pub fn parse_ids(body: &Value) -> Result<Vec<String>, AbError> {
    let items = body.as_array().ok_or(AbError::Invalid(INVALID_REQUEST))?;
    Ok(items
        .iter()
        .filter_map(text_value)
        .filter(|id| !id.is_empty())
        .collect())
}

/// Corpo do `DELETE /tag`: array de nomes de tag.
pub fn parse_tag_names(body: &Value) -> Result<Vec<String>, AbError> {
    let items = body.as_array().ok_or(AbError::Invalid(INVALID_REQUEST))?;
    items
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(tag_name)
        .collect()
}

fn color_of(obj: &Map<String, Value>) -> Result<Option<i64>, AbError> {
    match obj.get("color") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or(AbError::Invalid(INVALID_REQUEST)),
        Some(_) => Err(AbError::Invalid(INVALID_REQUEST)),
    }
}

fn name_of(obj: &Map<String, Value>, key: &str) -> Result<String, AbError> {
    match obj.get(key) {
        Some(Value::String(s)) => tag_name(s),
        _ => Err(AbError::Invalid(INVALID_REQUEST)),
    }
}

/// `tag/add` e `tag/update`: nome e cor (sem cor, usa a cor determinística).
pub fn parse_tag(body: &Value) -> Result<(String, i64), AbError> {
    let obj = object(body)?;
    let name = name_of(obj, "name")?;
    let color = color_of(obj)?.unwrap_or_else(|| tag_color(&name));
    Ok((name, color))
}

/// `tag/rename`: nome antigo e novo.
pub fn parse_rename(body: &Value) -> Result<(String, String), AbError> {
    let obj = object(body)?;
    Ok((name_of(obj, "old")?, name_of(obj, "new")?))
}

// ---------------------------------------------------------------------------
// Catálogo pessoal: peers
// ---------------------------------------------------------------------------

#[derive(FromRow)]
struct PeerRow {
    peer_id: String,
    alias: String,
    tags: String,
    username: String,
    hostname: String,
    platform: String,
    hash: String,
    note: String,
}

fn decode_tags(text: &str) -> Vec<String> {
    serde_json::from_str(text).unwrap_or_default()
}

fn encode_tags(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
}

pub async fn personal_peers(pool: &SqlitePool, guid: &str) -> sqlx::Result<Vec<Value>> {
    let rows: Vec<PeerRow> = sqlx::query_as(
        "SELECT peer_id, alias, tags, username, hostname, platform, hash, note
         FROM ab_peers WHERE ab_guid = ? ORDER BY peer_id",
    )
    .bind(guid)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.peer_id,
                "alias": r.alias,
                "tags": decode_tags(&r.tags),
                "username": r.username,
                "hostname": r.hostname,
                "platform": r.platform,
                "hash": r.hash,
                "note": r.note,
            })
        })
        .collect())
}

/// Cria o peer ou, se o id já existir, atualiza só os campos enviados.
pub async fn upsert_peer(
    pool: &SqlitePool,
    guid: &str,
    id: &str,
    p: &PeerPatch,
) -> sqlx::Result<()> {
    let tags = p.tags.as_deref().map(encode_tags);
    sqlx::query(
        "INSERT INTO ab_peers (ab_guid, peer_id, alias, tags, username, hostname, platform, hash, note)
         VALUES (?1, ?2, COALESCE(?3, ''), COALESCE(?4, '[]'), COALESCE(?5, ''), COALESCE(?6, ''),
                 COALESCE(?7, ''), COALESCE(?8, ''), COALESCE(?9, ''))
         ON CONFLICT(ab_guid, peer_id) DO UPDATE SET
           alias = COALESCE(?3, alias), tags = COALESCE(?4, tags),
           username = COALESCE(?5, username), hostname = COALESCE(?6, hostname),
           platform = COALESCE(?7, platform), hash = COALESCE(?8, hash),
           note = COALESCE(?9, note)",
    )
    .bind(guid)
    .bind(id)
    .bind(&p.alias)
    .bind(tags)
    .bind(&p.username)
    .bind(&p.hostname)
    .bind(&p.platform)
    .bind(&p.hash)
    .bind(&p.note)
    .execute(pool)
    .await?;
    Ok(())
}

/// Atualiza só os campos enviados de um peer existente.
pub async fn update_peer(
    pool: &SqlitePool,
    guid: &str,
    id: &str,
    p: &PeerPatch,
) -> Result<(), AbError> {
    let tags = p.tags.as_deref().map(encode_tags);
    let res = sqlx::query(
        "UPDATE ab_peers SET
           alias = COALESCE(?3, alias), tags = COALESCE(?4, tags),
           username = COALESCE(?5, username), hostname = COALESCE(?6, hostname),
           platform = COALESCE(?7, platform), hash = COALESCE(?8, hash),
           note = COALESCE(?9, note)
         WHERE ab_guid = ?1 AND peer_id = ?2",
    )
    .bind(guid)
    .bind(id)
    .bind(&p.alias)
    .bind(tags)
    .bind(&p.username)
    .bind(&p.hostname)
    .bind(&p.platform)
    .bind(&p.hash)
    .bind(&p.note)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AbError::NotFound(PEER_NOT_FOUND));
    }
    Ok(())
}

pub async fn delete_peers(pool: &SqlitePool, guid: &str, ids: &[String]) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    for id in ids {
        sqlx::query("DELETE FROM ab_peers WHERE ab_guid = ? AND peer_id = ?")
            .bind(guid)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

// ---------------------------------------------------------------------------
// Catálogo pessoal: tags
// ---------------------------------------------------------------------------

pub async fn personal_tags(pool: &SqlitePool, guid: &str) -> sqlx::Result<Vec<Value>> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT name, color FROM ab_tags WHERE ab_guid = ? ORDER BY name")
            .bind(guid)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(name, color)| json!({ "name": name, "color": color }))
        .collect())
}

/// Cria a tag ou, se já existir, troca a cor.
pub async fn add_tag(pool: &SqlitePool, guid: &str, name: &str, color: i64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO ab_tags (ab_guid, name, color) VALUES (?, ?, ?)
         ON CONFLICT(ab_guid, name) DO UPDATE SET color = excluded.color",
    )
    .bind(guid)
    .bind(name)
    .bind(color)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_tag(pool: &SqlitePool, guid: &str, name: &str, color: i64) -> Result<(), AbError> {
    let res = sqlx::query("UPDATE ab_tags SET color = ? WHERE ab_guid = ? AND name = ?")
        .bind(color)
        .bind(guid)
        .bind(name)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AbError::NotFound(TAG_NOT_FOUND));
    }
    Ok(())
}

/// Reescreve as tags de cada peer do catálogo; só grava as linhas que mudaram.
/// Devolve quantas linhas mudaram.
async fn rewrite_peer_tags(
    conn: &mut SqliteConnection,
    guid: &str,
    f: impl Fn(Vec<String>) -> Vec<String>,
) -> sqlx::Result<usize> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT peer_id, tags FROM ab_peers WHERE ab_guid = ?")
            .bind(guid)
            .fetch_all(&mut *conn)
            .await?;
    let mut changed = 0;
    for (peer_id, text) in rows {
        let old = decode_tags(&text);
        let new = f(old.clone());
        if new != old {
            sqlx::query("UPDATE ab_peers SET tags = ? WHERE ab_guid = ? AND peer_id = ?")
                .bind(encode_tags(&new))
                .bind(guid)
                .bind(peer_id)
                .execute(&mut *conn)
                .await?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Renomeia a tag no catálogo e nas listas dos peers (numa transação). Se o
/// nome novo já existir, as tags se fundem e a cor da existente prevalece.
pub async fn rename_tag(
    pool: &SqlitePool,
    guid: &str,
    old: &str,
    new: &str,
) -> Result<(), AbError> {
    if old == new {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let new_exists: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM ab_tags WHERE ab_guid = ? AND name = ?")
            .bind(guid)
            .bind(new)
            .fetch_optional(&mut *tx)
            .await?;
    let tag_rows = if new_exists.is_some() {
        sqlx::query("DELETE FROM ab_tags WHERE ab_guid = ? AND name = ?")
            .bind(guid)
            .bind(old)
            .execute(&mut *tx)
            .await?
    } else {
        sqlx::query("UPDATE ab_tags SET name = ? WHERE ab_guid = ? AND name = ?")
            .bind(new)
            .bind(guid)
            .bind(old)
            .execute(&mut *tx)
            .await?
    }
    .rows_affected();
    let peer_rows = rewrite_peer_tags(&mut tx, guid, |tags| {
        let mut seen = HashSet::new();
        tags.into_iter()
            .map(|t| if t == old { new.to_string() } else { t })
            .filter(|t| seen.insert(t.clone()))
            .collect()
    })
    .await?;
    if tag_rows == 0 && peer_rows == 0 {
        return Err(AbError::NotFound(TAG_NOT_FOUND));
    }
    tx.commit().await?;
    Ok(())
}

/// Remove as tags do catálogo e das listas dos peers (numa transação).
pub async fn delete_tags(pool: &SqlitePool, guid: &str, names: &[String]) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    for name in names {
        sqlx::query("DELETE FROM ab_tags WHERE ab_guid = ? AND name = ?")
            .bind(guid)
            .bind(name)
            .execute(&mut *tx)
            .await?;
    }
    rewrite_peer_tags(&mut tx, guid, |tags| {
        tags.into_iter().filter(|t| !names.contains(t)).collect()
    })
    .await?;
    tx.commit().await
}
