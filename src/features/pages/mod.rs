use crate::state::AppState;
use axum::{Router, routing::get};

mod home;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home::index))
}