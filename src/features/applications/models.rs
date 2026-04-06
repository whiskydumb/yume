use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Application {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub url: String,
    pub contact: String,
    pub description: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
pub struct ApplyForm {
    pub name: String,
    pub slug: String,
    pub url: String,
    pub contact: String,
    pub description: Option<String>,
}
