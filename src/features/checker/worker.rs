use reqwest::Client;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{Notify, Semaphore};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const USER_AGENT: &str = "yume-health/1.0";
const CHECK_INTERVAL_SECS: u64 = 300;
const CHECK_TIMEOUT_SECS: u64 = 10;
const DOWN_THRESHOLD: i32 = 3;
const MAX_CONCURRENT_CHECKS: usize = 32;

struct SiteRow {
    id: Uuid,
    url: String,
    consecutive_failures: i32,
    is_online: bool,
    favicon: Option<String>,
}

pub async fn run(
    db: PgPool,
    cache: crate::features::sites::cache::SiteCache,
    notify: Arc<Notify>,
    shutdown: CancellationToken,
) {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(CHECK_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            use std::net::ToSocketAddrs;

            if attempt.previous().len() >= 3 {
                return attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "redirect limit exceeded",
                ));
            }
            let url = attempt.url();
            let scheme = url.scheme();
            if scheme != "http" && scheme != "https" {
                tracing::warn!(redirect_url = %url, "checker: redirect to non-http(s) scheme blocked");
                return attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "redirect blocked",
                ));
            }
            let host = match url.host_str() {
                Some(h) => h.to_owned(),
                None => {
                    tracing::warn!(redirect_url = %url, "checker: redirect with no host blocked");
                    return attempt.error(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "redirect blocked",
                    ));
                }
            };
            let is_public = tokio::task::block_in_place(|| {
                match (host.as_str(), 80u16).to_socket_addrs() {
                    Ok(addrs) => {
                        let v: Vec<_> = addrs.collect();
                        !v.is_empty()
                            && v.iter()
                                .all(|a| crate::features::favicon::is_public_addr(a.ip()))
                    }
                    Err(_) => false,
                }
            });
            if is_public {
                attempt.follow()
            } else {
                tracing::warn!(redirect_url = %url, "checker: redirect to non-public host blocked");
                attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "redirect blocked",
                ))
            }
        }))
        .tcp_keepalive(Duration::from_secs(30))
        .pool_max_idle_per_host(4)
        .https_only(false)
        .build()
        .expect("failed to build reqwest client");

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHECKS));

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(CHECK_INTERVAL_SECS)) => {},
            _ = notify.notified() => {
                // tracing::info!("checker: manual scan triggered");
            },
            _ = shutdown.cancelled() => {
                // tracing::info!("checker: shutting down");
                return;
            },
        }

        let sites = match fetch_sites(&db).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "checker: failed to fetch sites");
                continue;
            }
        };

        let mut set = JoinSet::new();
        let status_changed = Arc::new(AtomicBool::new(false));
        let cache_changed = Arc::new(AtomicBool::new(false));

        for site in sites {
            let client = client.clone();
            let db = db.clone();
            let db2 = db.clone();
            let semaphore = semaphore.clone();
            let status_changed = status_changed.clone();
            let cache_changed = cache_changed.clone();

            set.spawn(async move {
                let _permit = semaphore.acquire().await.expect("semaphore closed");
                let needs_favicon = site.favicon.is_none();
                let site_id = site.id;
                let site_url = site.url.clone();

                if check_site(&client, &db, site).await {
                    status_changed.store(true, Ordering::Relaxed);
                }

                if needs_favicon
                    && let Some(path) = crate::features::favicon::fetch(site_id, &site_url).await
                {
                    if let Err(e) = sqlx::query!(
                        "UPDATE sites SET favicon = $1 WHERE id = $2",
                        path,
                        site_id
                    )
                    .execute(&db2)
                    .await
                    {
                        tracing::error!(error = %e, site_id = %site_id, "favicon: failed to save path");
                    } else {
                        // tracing::info!(site_id = %site_id, path = %path, "favicon saved");
                        cache_changed.store(true, Ordering::Relaxed);
                    }
                }
            });
        }

        while set.join_next().await.is_some() {}

        if (status_changed.load(Ordering::Relaxed) || cache_changed.load(Ordering::Relaxed))
            && let Err(e) = crate::features::sites::cache::reload(&cache, &db).await
        {
            tracing::error!(error = %e, "checker: failed to reload cache");
        }
    }
}

async fn fetch_sites(db: &PgPool) -> Result<Vec<SiteRow>, sqlx::Error> {
    sqlx::query_as!(
        SiteRow,
        "SELECT id, url, consecutive_failures, is_online, favicon FROM sites WHERE enabled = true"
    )
    .fetch_all(db)
    .await
}

async fn check_site(client: &Client, db: &PgPool, site: SiteRow) -> bool {
    let ok = client
        .get(&site.url)
        .send()
        .await
        .map(|r| r.status().is_success() || r.status().is_redirection())
        .unwrap_or(false);

    let (new_failures, new_online) = if ok {
        (0, true)
    } else {
        let failures = site.consecutive_failures + 1;
        let online = failures < DOWN_THRESHOLD;
        (failures, online)
    };

    if new_online != site.is_online || new_failures != site.consecutive_failures {
        if let Err(e) = sqlx::query!(
            "UPDATE sites SET is_online = $1, consecutive_failures = $2, last_checked_at = now() WHERE id = $3",
            new_online,
            new_failures,
            site.id
        )
        .execute(db)
        .await
        {
            tracing::error!(error = %e, site_id = %site.id, "checker: failed to update site status");
            return false;
        }

        if new_online != site.is_online {
            // tracing::info!(site_id = %site.id, url = %site.url, online = new_online, "site status changed");
            return true;
        }
    }

    false
}
