use axum::extract::FromRef;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Notify;

use crate::features::sites::cache::SiteCache;

#[derive(Clone)]
pub struct BaseUrl(pub Arc<str>);

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub site_cache: SiteCache,
    pub jwt_secret: Arc<[u8]>,
    pub admin_password_hash: Arc<str>,
    pub jwt_expiry_hours: u64,
    pub base_url: BaseUrl,
    pub scan_notify: Arc<Notify>,
}

impl FromRef<AppState> for SiteCache {
    fn from_ref(state: &AppState) -> SiteCache {
        state.site_cache.clone()
    }
}

impl FromRef<AppState> for BaseUrl {
    fn from_ref(state: &AppState) -> BaseUrl {
        state.base_url.clone()
    }
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> PgPool {
        state.db.clone()
    }
}