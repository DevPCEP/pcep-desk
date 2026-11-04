//! Retenção de dados: apaga o que cresce sem controle pelas rotas sem
//! autenticação (eventos brutos e dispositivos desconhecidos) e tokens vencidos.

use std::time::Duration;

use sqlx::SqlitePool;

const DAY_SECS: i64 = 86_400;
/// Eventos brutos de auditoria são guardados por 90 dias.
const AUDIT_RAW_DAYS: i64 = 90;
/// Dispositivos não vinculados sem sinal há mais de 30 dias são esquecidos.
const UNLINKED_DEVICE_DAYS: i64 = 30;
/// Intervalo entre as limpezas automáticas.
const INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// Quantidade de linhas apagadas em cada tabela.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub audit_raw: u64,
    pub devices: u64,
    pub tokens: u64,
}

/// Aplica a retenção usando `now` (segundos Unix) como referência.
pub async fn run(pool: &SqlitePool, now: i64) -> sqlx::Result<Report> {
    let audit_raw = sqlx::query("DELETE FROM audit_raw WHERE received_at < ?")
        .bind(now - AUDIT_RAW_DAYS * DAY_SECS)
        .execute(pool)
        .await?
        .rows_affected();
    // Só dispositivos que ninguém vinculou (nem computador de cliente nem colaborador).
    let devices = sqlx::query(
        "DELETE FROM devices
         WHERE last_seen_at < ?
           AND peer_id NOT IN (SELECT peer_id FROM computers)
           AND peer_id NOT IN (SELECT peer_id FROM user_devices)",
    )
    .bind(now - UNLINKED_DEVICE_DAYS * DAY_SECS)
    .execute(pool)
    .await?
    .rows_affected();
    let tokens = sqlx::query("DELETE FROM tokens WHERE expires_at < ?")
        .bind(now)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(Report {
        audit_raw,
        devices,
        tokens,
    })
}

/// Roda a retenção agora e depois a cada 24 h, em segundo plano. Falhas só
/// são registradas no log (a limpeza tenta de novo no próximo ciclo).
pub fn spawn(pool: SqlitePool) {
    tokio::spawn(async move {
        // O primeiro tick é imediato (execução na inicialização).
        let mut ticker = tokio::time::interval(INTERVAL);
        loop {
            ticker.tick().await;
            match run(&pool, crate::clock::now()).await {
                Ok(r) => tracing::info!(
                    audit_raw = r.audit_raw,
                    devices = r.devices,
                    tokens = r.tokens,
                    "retenção aplicada"
                ),
                Err(e) => tracing::error!("falha na retenção de dados: {e}"),
            }
        }
    });
}
