use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;

/// Shared application state, cloned cheaply into every handler.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
}
