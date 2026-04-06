use crate::state::AppState;
use axum::{Router, extract::State, http::StatusCode, routing::get};
use sqlx::PgPool;

mod home;

async fn health(State(db): State<PgPool>) -> StatusCode {
    match sqlx::query("SELECT 1").execute(&db).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home::index))
        .route("/health", get(health))
}
