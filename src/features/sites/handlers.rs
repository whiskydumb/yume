use super::cache::SiteCache;
use crate::error::AppError;
use axum::{
    Json,
    extract::{Path, State},
    response::Redirect,
};
use std::sync::Arc;

pub async fn next(
    State(cache): State<SiteCache>,
    Path(slug): Path<String>,
) -> Result<Redirect, AppError> {
    let data = cache.load();

    let idx = data.index_by_slug(&slug).ok_or(AppError::NotFound)?;

    let next = &data.sites[(idx + 1) % data.sites.len()];
    Ok(Redirect::to(&next.url))
}

pub async fn prev(
    State(cache): State<SiteCache>,
    Path(slug): Path<String>,
) -> Result<Redirect, AppError> {
    let data = cache.load();

    let idx = data.index_by_slug(&slug).ok_or(AppError::NotFound)?;

    let prev = &data.sites[(idx + data.sites.len() - 1) % data.sites.len()];
    Ok(Redirect::to(&prev.url))
}

pub async fn random(State(cache): State<SiteCache>) -> Result<Redirect, AppError> {
    let data = cache.load();

    if data.sites.is_empty() {
        return Err(AppError::NotFound);
    }

    let idx = fastrand::usize(..data.sites.len());
    Ok(Redirect::to(&data.sites[idx].url))
}

pub async fn list(State(cache): State<SiteCache>) -> Json<Arc<Vec<super::models::Site>>> {
    let data = cache.load_full();
    Json(Arc::new(data.sites.clone()))
}
