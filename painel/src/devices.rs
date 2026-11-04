//! Dispositivos (apps RustDesk) que enviam heartbeat e sysinfo.

use sqlx::{FromRow, SqlitePool};

/// Janela, em segundos, em que um dispositivo conta como online.
const ONLINE_WINDOW_SECS: i64 = 60;

#[derive(Debug, Clone, FromRow)]
pub struct Device {
    pub peer_id: String,
    pub uuid: String,
    pub version: String,
    pub hostname: String,
    pub os: String,
    pub username: String,
    pub cpu: String,
    pub memory: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
}

/// Dados de sistema enviados pelo app (campos ausentes ficam vazios).
#[derive(Debug, Clone, Default)]
pub struct SysInfo {
    pub hostname: String,
    pub os: String,
    pub username: String,
    pub cpu: String,
    pub memory: String,
    pub version: String,
}

/// Janela, em segundos, em que o último heartbeat ainda autoriza eventos de histórico.
const HEARTBEAT_WINDOW_SECS: i64 = 120;

/// Online se o último sinal chegou há no máximo 60 segundos.
pub fn is_online(last_seen_at: i64, now: i64) -> bool {
    now - last_seen_at <= ONLINE_WINDOW_SECS
}

/// Registra um sinal de vida: cria o dispositivo ou atualiza uuid, versão,
/// IP de origem e `last_seen_at`, sem tocar nos dados de sysinfo. Valores
/// vazios de uuid/versão não sobrescrevem os já gravados; o IP é sempre
/// substituído (vazio = origem desconhecida, que nunca valida o histórico).
pub async fn touch(
    pool: &SqlitePool,
    peer_id: &str,
    uuid: &str,
    version: &str,
    ip: &str,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO devices (peer_id, uuid, version, last_ip, first_seen_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(peer_id) DO UPDATE SET
           uuid = CASE WHEN excluded.uuid = '' THEN devices.uuid ELSE excluded.uuid END,
           version = CASE WHEN excluded.version = '' THEN devices.version ELSE excluded.version END,
           last_ip = excluded.last_ip,
           last_seen_at = excluded.last_seen_at",
    )
    .bind(peer_id)
    .bind(uuid)
    .bind(version)
    .bind(ip)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Grava os dados de sistema (e atualiza `last_seen_at`). A versão do sysinfo
/// só preenche a coluna se ela ainda estiver vazia: o heartbeat é a fonte
/// da versão (formato numérico diferente do informado no sysinfo).
pub async fn upsert_sysinfo(
    pool: &SqlitePool,
    peer_id: &str,
    uuid: &str,
    info: &SysInfo,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO devices
           (peer_id, uuid, version, hostname, os, username, cpu, memory, first_seen_at, last_seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
         ON CONFLICT(peer_id) DO UPDATE SET
           uuid = CASE WHEN excluded.uuid = '' THEN devices.uuid ELSE excluded.uuid END,
           version = CASE WHEN devices.version = '' THEN excluded.version ELSE devices.version END,
           hostname = excluded.hostname,
           os = excluded.os,
           username = excluded.username,
           cpu = excluded.cpu,
           memory = excluded.memory,
           last_seen_at = excluded.last_seen_at",
    )
    .bind(peer_id)
    .bind(uuid)
    .bind(&info.version)
    .bind(&info.hostname)
    .bind(&info.os)
    .bind(&info.username)
    .bind(&info.cpu)
    .bind(&info.memory)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// O computador mandou heartbeat nos últimos 120 s, do mesmo IP `ip`? `ip`
/// vazio (origem desconhecida) nunca confere.
pub async fn has_recent_heartbeat(
    pool: &SqlitePool,
    peer_id: &str,
    ip: &str,
    now: i64,
) -> sqlx::Result<bool> {
    if ip.is_empty() {
        return Ok(false);
    }
    sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM devices
           WHERE peer_id = ?1 AND last_ip = ?2 AND last_seen_at >= ?3)",
    )
    .bind(peer_id)
    .bind(ip)
    .bind(now - HEARTBEAT_WINDOW_SECS)
    .fetch_one(pool)
    .await
}

pub async fn get(pool: &SqlitePool, peer_id: &str) -> sqlx::Result<Option<Device>> {
    sqlx::query_as::<_, Device>(
        "SELECT peer_id, uuid, version, hostname, os, username, cpu, memory,
                first_seen_at, last_seen_at
         FROM devices WHERE peer_id = ?",
    )
    .bind(peer_id)
    .fetch_optional(pool)
    .await
}

/// Máximo de dispositivos devolvidos por `list_unlinked`.
pub const UNLINKED_LIMIT: i64 = 200;

/// Dispositivos ainda não vinculados a um computador de cliente nem a um
/// colaborador, do mais recente para o mais antigo (no máximo `UNLINKED_LIMIT`).
pub async fn list_unlinked(pool: &SqlitePool) -> sqlx::Result<Vec<Device>> {
    sqlx::query_as::<_, Device>(
        "SELECT peer_id, uuid, version, hostname, os, username, cpu, memory,
                first_seen_at, last_seen_at
         FROM devices
         WHERE peer_id NOT IN (SELECT peer_id FROM computers)
           AND peer_id NOT IN (SELECT peer_id FROM user_devices)
         ORDER BY last_seen_at DESC, peer_id
         LIMIT 200",
    )
    .fetch_all(pool)
    .await
}
