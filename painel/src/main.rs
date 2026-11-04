use std::sync::Arc;

use anyhow::Context;
use painel::{
    auth::limiter::LoginLimiter,
    build_router,
    config::Config,
    db, maintenance,
    state::AppState,
    users::{self, UserError},
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => serve().await,
        Some("criar-admin") => criar_admin(&args[1..]).await,
        Some("backup") => backup(&args[1..]).await,
        Some(outro) => anyhow::bail!(
            "subcomando desconhecido: {outro} (use `criar-admin --usuario <u> --nome \"<nome>\"`, `backup <destino>` ou nenhum para iniciar o servidor)"
        ),
    }
}

/// Valor da opção `--nome-da-opcao <valor>` nos argumentos.
fn opcao<'a>(args: &'a [String], nome: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == nome)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// `painel criar-admin --usuario <u> --nome "<nome>"`; senha em `PAINEL_SENHA_ADMIN` ou stdin.
async fn criar_admin(args: &[String]) -> anyhow::Result<()> {
    let usuario = opcao(args, "--usuario").context("faltou --usuario <usuario>")?;
    let nome = opcao(args, "--nome").context("faltou --nome \"<nome>\"")?;
    let senha = match std::env::var("PAINEL_SENHA_ADMIN") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            eprintln!("Senha do administrador (PAINEL_SENHA_ADMIN não definida), digite e tecle Enter:");
            let mut linha = String::new();
            std::io::stdin()
                .read_line(&mut linha)
                .context("lendo a senha da entrada padrão")?;
            linha.trim_end_matches(['\r', '\n']).to_string()
        }
    };

    let config = Config::from_env()?;
    let pool = db::connect(&config.db_url).await?;
    match users::create(&pool, usuario, nome, &senha, true).await {
        Ok(_) => {
            println!("Administrador criado: {}", usuario.trim().to_lowercase());
            Ok(())
        }
        Err(UserError::UsernameTaken) => anyhow::bail!("usuário já existe: {usuario}"),
        Err(e) => anyhow::bail!("{e}"),
    }
}

/// `painel backup <destino>`: grava uma cópia consistente do banco configurado.
async fn backup(args: &[String]) -> anyhow::Result<()> {
    let destino = args.first().context("faltou o destino: painel backup <destino>")?;
    let config = Config::from_env()?;
    db::backup_from_url(&config.db_url, std::path::Path::new(destino)).await?;
    println!("Backup gravado em {destino}");
    Ok(())
}

async fn serve() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    let pool = db::connect(&config.db_url).await?;
    let bind = config.bind.clone();
    // Retenção de dados: já na inicialização e depois a cada 24 h.
    maintenance::spawn(pool.clone());
    let state = AppState {
        pool,
        config: Arc::new(config),
        limiter: Arc::new(LoginLimiter::new()),
    };
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("escutando em {bind}"))?;
    tracing::info!("painel escutando em {bind}");
    // `ConnectInfo` dá o IP da conexão quando não há `X-Forwarded-For` (heartbeat e audit).
    axum::serve(
        listener,
        build_router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
        .context("servidor HTTP")?;
    Ok(())
}
