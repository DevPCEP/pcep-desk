use std::sync::Arc;

use sqlx::SqlitePool;

use crate::{auth::limiter::LoginLimiter, config::Config};

/// Estado compartilhado entre os handlers.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub limiter: Arc<LoginLimiter>,
}
