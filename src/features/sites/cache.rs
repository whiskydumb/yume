use super::models::Site;
use arc_swap::ArcSwap;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

pub struct SiteData {
    pub sites: Vec<Site>,
    slug_index: HashMap<Arc<str>, usize>,
}

impl SiteData {
    pub(crate) fn new(sites: Vec<Site>) -> Self {
        let slug_index = sites
            .iter()
            .enumerate()
            .map(|(i, s)| (Arc::clone(&s.slug), i))
            .collect();
        Self { sites, slug_index }
    }

    pub fn index_by_slug(&self, slug: &str) -> Option<usize> {
        self.slug_index.get(slug).copied()
    }
}

pub type SiteCache = Arc<ArcSwap<SiteData>>;

pub fn new() -> SiteCache {
    Arc::new(ArcSwap::from_pointee(SiteData::new(Vec::new())))
}

pub async fn reload(cache: &SiteCache, db: &PgPool) -> Result<(), sqlx::Error> {
    struct SiteRow {
        id: uuid::Uuid,
        name: String,
        url: String,
        slug: String,
        description: Option<String>,
        favicon: Option<String>,
        enabled: bool,
        position: i32,
    }

    let rows = sqlx::query_as!(
        SiteRow,
        "SELECT id, name, url, slug, description, favicon, enabled, position
         FROM sites
         WHERE enabled = true
         ORDER BY position ASC"
    )
    .fetch_all(db)
    .await?;

    let sites: Vec<Site> = rows
        .into_iter()
        .map(|r| Site {
            id: r.id,
            name: r.name.into(),
            url: r.url.into(),
            slug: r.slug.into(),
            description: r.description.map(Into::into),
            favicon: r.favicon.map(Into::into),
            enabled: r.enabled,
            position: r.position,
        })
        .collect();

    cache.store(Arc::new(SiteData::new(sites)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::models::Site;
    use super::*;
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

    #[test]
    fn empty_sites_index_returns_none() {
        let data = SiteData::new(vec![]);
        assert!(data.index_by_slug("anything").is_none());
    }

    #[test]
    fn index_by_slug_finds_correct_position() {
        let sites = vec![
            make_site("slug0", 0),
            make_site("slug1", 1),
            make_site("slug2", 2),
        ];
        let data = SiteData::new(sites);
        assert_eq!(data.index_by_slug("slug0"), Some(0));
        assert_eq!(data.index_by_slug("slug1"), Some(1));
        assert_eq!(data.index_by_slug("slug2"), Some(2));
    }

    #[test]
    fn index_by_slug_missing_returns_none() {
        let sites = vec![make_site("exists", 0)];
        let data = SiteData::new(sites);
        assert!(data.index_by_slug("nope").is_none());
    }
}
