use std::str::FromStr;

use anyhow::Context;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

/// Conecta ao SQLite (criando arquivo e diretório se faltarem) e aplica as migrations.
///
/// Banco em memória usa uma única conexão, pois cada conexão teria o seu próprio banco.
pub async fn connect(url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(url)
        .with_context(|| format!("URL de banco inválida: {url}"))?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));

    let in_memory = url.contains(":memory:") || url.contains("mode=memory");
    if !in_memory {
        let file = options.get_filename();
        if let Some(dir) = file.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("criando diretório {}", dir.display()))?;
        }
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(if in_memory { 1 } else { 5 })
        .connect_with(options)
        .await
        .context("conectando ao banco")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("aplicando migrations")?;
    Ok(pool)
}
