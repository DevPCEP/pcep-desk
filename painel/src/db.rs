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

/// Grava uma cópia consistente do banco em `dest` (`VACUUM INTO`), sem parar o painel.
///
/// Recusa destino já existente, para nunca sobrescrever um backup anterior.
pub async fn backup(pool: &SqlitePool, dest: &std::path::Path) -> anyhow::Result<()> {
    if dest.exists() {
        anyhow::bail!("o destino do backup já existe: {}", dest.display());
    }
    let dest_str = dest
        .to_str()
        .context("caminho de destino do backup não é UTF-8 válido")?;
    sqlx::query("VACUUM INTO ?")
        .bind(dest_str)
        .execute(pool)
        .await
        .with_context(|| format!("gravando backup em {}", dest.display()))?;
    Ok(())
}

/// Faz o backup do banco indicado em `url`, recusando se o arquivo não existir
/// (para nunca criar um banco vazio por engano, por exemplo com `PAINEL_DB` errada).
pub async fn backup_from_url(url: &str, dest: &std::path::Path) -> anyhow::Result<()> {
    let options = SqliteConnectOptions::from_str(url)
        .with_context(|| format!("URL de banco inválida: {url}"))?;
    let in_memory = url.contains(":memory:") || url.contains("mode=memory");
    if in_memory {
        anyhow::bail!("backup não se aplica a banco em memória");
    }
    let file = options.get_filename();
    if !file.exists() {
        anyhow::bail!("o banco de dados não existe: {}", file.display());
    }
    let pool = connect(url).await?;
    let resultado = backup(&pool, dest).await;
    pool.close().await;
    resultado
}
