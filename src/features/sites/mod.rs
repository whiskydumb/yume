pub mod cache;
pub mod handlers;
pub mod models;

use crate::state::AppState;
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sites", get(handlers::list))
        .route("/{slug}/next", get(handlers::next))
        .route("/{slug}/prev", get(handlers::prev))
        .route("/{slug}/random", get(handlers::random))
}
