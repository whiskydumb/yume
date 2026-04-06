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

#[cfg(test)]
mod tests {
    use super::super::cache::SiteData;
    use super::super::models::Site;
    use super::*;
    use axum::extract::{Path, State};
    use axum::response::IntoResponse;
    use uuid::Uuid;

    fn make_site(slug: &str, position: i32) -> Site {
        Site {
            id: Uuid::new_v4(),
            name: slug.into(),
            url: format!("https://{slug}.example.com").into(),
            slug: slug.into(),
            description: None,
            favicon: None,
            enabled: true,
            position,
        }
    }

    fn test_cache(sites: Vec<Site>) -> SiteCache {
        Arc::new(arc_swap::ArcSwap::from_pointee(SiteData::new(sites)))
    }

    fn redirect_location(redirect: axum::response::Redirect) -> String {
        let response = redirect.into_response();
        response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    }

    #[tokio::test]
    async fn next_wraps_around() {
        let cache = test_cache(vec![
            make_site("a", 0),
            make_site("b", 1),
            make_site("c", 2),
        ]);
        let result = next(State(cache), Path("c".into())).await.unwrap();
        assert_eq!(redirect_location(result), "https://a.example.com");
    }

    #[tokio::test]
    async fn prev_wraps_around() {
        let cache = test_cache(vec![
            make_site("a", 0),
            make_site("b", 1),
            make_site("c", 2),
        ]);
        let result = prev(State(cache), Path("a".into())).await.unwrap();
        assert_eq!(redirect_location(result), "https://c.example.com");
    }

    #[tokio::test]
    async fn next_middle_element() {
        let cache = test_cache(vec![
            make_site("a", 0),
            make_site("b", 1),
            make_site("c", 2),
        ]);
        let result = next(State(cache), Path("a".into())).await.unwrap();
        assert_eq!(redirect_location(result), "https://b.example.com");
    }

    #[tokio::test]
    async fn prev_middle_element() {
        let cache = test_cache(vec![
            make_site("a", 0),
            make_site("b", 1),
            make_site("c", 2),
        ]);
        let result = prev(State(cache), Path("c".into())).await.unwrap();
        assert_eq!(redirect_location(result), "https://b.example.com");
    }

    #[tokio::test]
    async fn next_unknown_slug_is_not_found() {
        let cache = test_cache(vec![make_site("a", 0)]);
        let result = next(State(cache), Path("nope".into())).await;
        assert!(matches!(result, Err(crate::error::AppError::NotFound)));
    }

    #[tokio::test]
    async fn random_empty_cache_is_not_found() {
        let cache = test_cache(vec![]);
        let result = random(State(cache)).await;
        assert!(matches!(result, Err(crate::error::AppError::NotFound)));
    }

    #[tokio::test]
    async fn random_non_empty_returns_redirect() {
        let cache = test_cache(vec![make_site("only", 0)]);
        let result = random(State(cache)).await;
        assert!(result.is_ok());
    }
}
