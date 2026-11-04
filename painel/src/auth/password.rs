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
