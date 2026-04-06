pub mod handlers;
pub mod middleware;
pub mod models;

use crate::features::rate_limit::RateLimiter;
use crate::state::AppState;
use axum::{Extension, Router, middleware as axum_mw, routing::{get, post}};

pub fn router(state: AppState) -> Router<AppState> {
    let login_limiter = RateLimiter::new(10, 60);
    login_limiter.clone().spawn_cleanup(300);

    let protected = Router::new()
        .route("/admin", get(handlers::admin_redirect))
        .route("/admin/sites", get(handlers::dashboard))
        .route("/admin/applications", get(handlers::applications))
        .route("/admin/sites/add", post(handlers::add_site))
        .route("/admin/sites/{id}/update", post(handlers::update_site))
        .route("/admin/sites/{id}/toggle", post(handlers::toggle_site))
        .route("/admin/sites/reorder", post(handlers::reorder_sites))
        .route("/admin/sites/{id}/delete", post(handlers::delete_site))
        .route("/admin/applications/{id}/approve", post(handlers::approve_application))
        .route("/admin/applications/{id}/reject", post(handlers::reject_application))
        .route("/admin/scan", post(handlers::trigger_scan))
        .route("/admin/logout", post(handlers::logout))
        .route_layer(axum_mw::from_fn(crate::features::csrf::verify))
        .route_layer(axum_mw::from_fn(crate::features::csrf::set_token))
        .route_layer(axum_mw::from_fn_with_state(state, middleware::require_auth));

    let public = Router::new()
        .route("/admin/login", get(handlers::login_page))
        .route("/admin/login", post(handlers::login_post))
        .route_layer(axum_mw::from_fn(crate::features::csrf::verify))
        .route_layer(axum_mw::from_fn(crate::features::csrf::set_token))
        .route_layer(axum_mw::from_fn(crate::features::rate_limit::limit))
        .layer(Extension(login_limiter));

    Router::new().merge(protected).merge(public)
}