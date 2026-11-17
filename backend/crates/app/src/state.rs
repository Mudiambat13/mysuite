use crate::config::Config;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Hash Argon2 factice pour égaliser le temps de réponse quand l'email est inconnu.
    pub dummy_hash: Arc<String>,
}

impl AppState {
    pub fn new(db: PgPool, config: Config) -> Self {
        let dummy_hash =
            crate::auth::password::hash("dummy-password-for-timing").expect("hash factice");
        Self {
            db,
            config: Arc::new(config),
            dummy_hash: Arc::new(dummy_hash),
        }
    }
}
