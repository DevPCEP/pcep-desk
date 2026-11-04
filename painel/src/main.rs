use std::sync::Arc;

use anyhow::Context;
use painel::{auth::limiter::LoginLimiter, build_router, config::Config, db, state::AppState};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Subcomandos (ex.: `criar-admin`) entram em tasks seguintes; por ora só `serve`.
    serve().await
}

async fn serve() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let pool = db::connect(&config.db_url).await?;
    let bind = config.bind.clone();
    let state = AppState {
        pool,
        config: Arc::new(config),
        limiter: Arc::new(LoginLimiter::new()),
    };
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("escutando em {bind}"))?;
    tracing::info!("painel escutando em {bind}");
    axum::serve(listener, build_router(state))
        .await
        .context("servidor HTTP")?;
    Ok(())
}
