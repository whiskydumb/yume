use super::cache::SiteCache;
use crate::error::AppError;
use axum::{
    Json,
    extract::{Path, State},
    response::Redirect,
};
use std::sync::Arc;

pub async fn next(State(cache): State<SiteCache>, Path(slug): Path<String>) -> Result<Redirect, AppError> {
    let sites = cache.load();

    let idx = sites.iter().position(|s| *s.slug == *slug)
        .ok_or(AppError::NotFound)?;

    let next = &sites[(idx + 1) % sites.len()];
    Ok(Redirect::to(&next.url))
}

pub async fn prev(State(cache): State<SiteCache>, Path(slug): Path<String>) -> Result<Redirect, AppError> {
    let sites = cache.load();

    let idx = sites.iter().position(|s| *s.slug == *slug)
        .ok_or(AppError::NotFound)?;

    let prev = &sites[(idx + sites.len() - 1) % sites.len()];
    Ok(Redirect::to(&prev.url))
}

pub async fn random(State(cache): State<SiteCache>) -> Result<Redirect, AppError> {
    let sites = cache.load();

    if sites.is_empty() {
        return Err(AppError::NotFound);
    }

    let idx = fastrand::usize(..sites.len());
    Ok(Redirect::to(&sites[idx].url))
}

pub async fn list(State(cache): State<SiteCache>) -> Json<Arc<Vec<super::models::Site>>> {
    Json(cache.load_full())
}
