use axum::extract::FromRef;
use sqlx::PgPool;

use crate::features::sites::cache::SiteCache;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub site_cache: SiteCache,
}

impl FromRef<AppState> for SiteCache {
    fn from_ref(state: &AppState) -> SiteCache {
        state.site_cache.clone()
    }
}
