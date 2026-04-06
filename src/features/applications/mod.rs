pub mod handlers;
pub mod models;

use crate::features::rate_limit::RateLimiter;
use crate::state::AppState;
use axum::{Extension, Router, middleware as axum_mw, routing::get};

pub fn router() -> Router<AppState> {
    let apply_limiter = RateLimiter::new(5, 60);
    apply_limiter.clone().spawn_cleanup(300);

    Router::new()
        .route("/apply", get(handlers::show_form).post(handlers::submit))
        .route_layer(axum_mw::from_fn(crate::features::csrf::verify))
        .route_layer(axum_mw::from_fn(crate::features::csrf::set_token))
        .route_layer(axum_mw::from_fn(crate::features::rate_limit::limit))
        .layer(Extension(apply_limiter))
}