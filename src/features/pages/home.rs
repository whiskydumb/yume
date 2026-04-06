use crate::error;
use crate::features::sites::cache::SiteCache;
use crate::features::sites::models::Site;
use crate::state::BaseUrl;
use askama::Template;
use axum::extract::State;
use axum::response::Html;
use std::sync::Arc;

#[derive(Template)]
#[template(path = "home/index.html")]
struct IndexTemplate {
    sites: Arc<Vec<Site>>,
    base_url: Arc<str>,
}

pub async fn index(
    State(cache): State<SiteCache>,
    State(base_url): State<BaseUrl>,
) -> error::Result<Html<String>> {
    let sites = cache.load_full();
    Ok(Html((IndexTemplate { sites, base_url: base_url.0 }).render()?))
}