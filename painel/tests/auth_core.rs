mod common;

use common::test_app;
use painel::{
    auth::{
        limiter::LoginLimiter,
        password::{hash_password, verify_password},
        tokens::{TokenKind, issue_token, resolve_token, revoke_user_tokens},
    },
    clock,
    users::{self, UserError},
};

#[test]
fn password_roundtrip() {
    let hash = hash_password("segredo123");
    assert!(verify_password("segredo123", &hash));
    assert!(!verify_password("errado", &hash));
    assert!(!verify_password("segredo123", "hash-malformado"));
}

#[tokio::test]
async fn create_user_validates() {
    let app = test_app().await;
    let p = &app.pool;
    assert!(matches!(
        users::create(p, "Jo", "João", "senha12345", false).await,
        Err(UserError::Invalid(_))
    ));
    assert!(matches!(
        users::create(p, "maria", "Maria", "curta", false).await,
        Err(UserError::Invalid(_))
    ));
    users::create(p, "maria", "Maria", "senha12345", false)
        .await
        .unwrap();
    assert!(matches!(
        users::create(p, "maria", "Maria", "senha12345", false).await,
        Err(UserError::UsernameTaken)
    ));
    assert!(matches!(
        users::create(p, "MARIA", "Maria", "senha12345", false).await,
        Err(UserError::UsernameTaken)
    ));
}

#[tokio::test]
async fn token_resolves_and_revokes() {
    let app = test_app().await;
    let p = &app.pool;
    let id = users::create(p, "maria", "Maria", "senha12345", false)
        .await
        .unwrap();
    let t = issue_token(p, id, TokenKind::App).await.unwrap();
    let u = resolve_token(p, &t, TokenKind::App).await.unwrap().unwrap();
    assert_eq!(u.id, id);
    assert!(resolve_token(p, &t, TokenKind::Web).await.unwrap().is_none());
    revoke_user_tokens(p, id).await.unwrap();
    assert!(resolve_token(p, &t, TokenKind::App).await.unwrap().is_none());
}

#[tokio::test]
async fn token_expired() {
    let app = test_app().await;
    let p = &app.pool;
    let id = users::create(p, "maria", "Maria", "senha12345", false)
        .await
        .unwrap();
    let t = issue_token(p, id, TokenKind::Web).await.unwrap();
    sqlx::query("UPDATE tokens SET expires_at = ?")
        .bind(clock::now() - 1)
        .execute(p)
        .await
        .unwrap();
    assert!(resolve_token(p, &t, TokenKind::Web).await.unwrap().is_none());
}

#[tokio::test]
async fn inactive_user_token_none() {
    let app = test_app().await;
    let p = &app.pool;
    let id = users::create(p, "maria", "Maria", "senha12345", false)
        .await
        .unwrap();
    let t = issue_token(p, id, TokenKind::App).await.unwrap();
    users::set_active(p, id, false).await.unwrap();
    assert!(resolve_token(p, &t, TokenKind::App).await.unwrap().is_none());
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tokens WHERE user_id = ?")
        .bind(id)
        .fetch_one(p)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn limiter_blocks_after_five() {
    let l = LoginLimiter::new();
    assert!(l.check("maria", 1000));
    for _ in 0..5 {
        l.record_failure("maria", 1000);
    }
    assert!(!l.check("maria", 1001));
    assert!(l.check("maria", 1000 + 901));
    l.record_failure("joao", 1000);
    assert!(l.check("joao", 1001));
    l.reset("maria");
    assert!(l.check("maria", 1001));
}
