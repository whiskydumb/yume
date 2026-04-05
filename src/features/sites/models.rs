use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Site {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub slug: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub position: i32,
}
