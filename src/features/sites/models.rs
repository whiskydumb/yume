use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Site {
    pub id: Uuid,
    pub name: Arc<str>,
    pub url: Arc<str>,
    pub slug: Arc<str>,
    pub description: Option<Arc<str>>,
    pub favicon: Option<Arc<str>>,
    pub enabled: bool,
    pub position: i32,
}

#[allow(dead_code)]
pub struct AdminSite {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub slug: String,
    pub description: Option<String>,
    pub favicon: Option<String>,
    pub enabled: bool,
    pub position: i32,
    pub is_online: bool,
    pub consecutive_failures: i32,
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
}
