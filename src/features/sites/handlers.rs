use super::cache::SiteCache;
use axum::{
    Json,
    extract::{Path, State},
    response::Redirect,
};

pub async fn next(State(cache): State<SiteCache>, Path(slug): Path<String>) -> Result<Redirect, (axum::http::StatusCode, &'static str)> {
    let sites = cache.read().await;

    let idx = sites.iter().position(|s| s.slug == slug)
        .ok_or((axum::http::StatusCode::NOT_FOUND, "site not found"))?;

    let next = &sites[(idx + 1) % sites.len()];
    Ok(Redirect::to(&next.url))
}

pub async fn prev(State(cache): State<SiteCache>, Path(slug): Path<String>) -> Result<Redirect, (axum::http::StatusCode, &'static str)> {
    let sites = cache.read().await;

    let idx = sites.iter().position(|s| s.slug == slug)
        .ok_or((axum::http::StatusCode::NOT_FOUND, "site not found"))?;

    let prev = &sites[(idx + sites.len() - 1) % sites.len()];
    Ok(Redirect::to(&prev.url))
}

pub async fn random(State(cache): State<SiteCache>) -> Result<Redirect, (axum::http::StatusCode, &'static str)> {
    let sites = cache.read().await;

    if sites.is_empty() {
        return Err((axum::http::StatusCode::NOT_FOUND, "no sites"));
    }

    let idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as usize) % sites.len();

    Ok(Redirect::to(&sites[idx].url))
}

pub async fn list(State(cache): State<SiteCache>) -> Json<Vec<super::models::Site>> {
    let sites = cache.read().await;
    Json(sites.clone())
}
