use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier},
};

/// Gera o hash Argon2id (parâmetros padrão do crate, sal aleatório) em formato PHC.
pub fn hash_password(password: &str) -> String {
    Argon2::default()
        .hash_password(password.as_bytes())
        .expect("falha ao gerar hash de senha")
        .to_string()
}

/// Confere a senha contra o hash PHC. Hash malformado resulta em `false`.
pub fn verify_password(password: &str, hash: &str) -> bool {
    Argon2::default()
        .verify_password(password.as_bytes(), hash)
        .is_ok()
}

/// Máximo de operações Argon2 simultâneas (cada uma usa 19 MiB e muita CPU).
const MAX_CONCURRENT_HASHES: usize = 4;

/// Roda `f` (trabalho pesado de Argon2) numa thread de bloqueio, no máximo
/// `MAX_CONCURRENT_HASHES` por vez no processo todo. As demais esperam na fila,
/// o que impede que uma rajada de logins esgote CPU e memória.
pub async fn run_limited<T, F>(f: F) -> Result<T, tokio::task::JoinError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    static SLOTS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
        std::sync::OnceLock::new();
    let slots = SLOTS
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_HASHES)))
        .clone();
    // O semáforo nunca é fechado: `acquire_owned` só falharia nesse caso.
    let permit = slots.acquire_owned().await.expect("semáforo do Argon2 fechado");
    let result = tokio::task::spawn_blocking(f).await;
    drop(permit);
    result
}
