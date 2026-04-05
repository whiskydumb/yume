use super::models::Site;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SiteCache = Arc<RwLock<Vec<Site>>>;

pub fn new() -> SiteCache {
    Arc::new(RwLock::new(Vec::new()))
}

pub async fn reload(cache: &SiteCache, db: &PgPool) -> Result<(), sqlx::Error> {
    let sites = sqlx::query_as!(
        Site,
        "SELECT id, name, url, slug, description, enabled, position
         FROM sites
         WHERE enabled = true
         ORDER BY position ASC"
    )
    .fetch_all(db)
    .await?;

    *cache.write().await = sites;
    Ok(())
}
