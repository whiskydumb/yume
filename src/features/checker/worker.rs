use reqwest::Client;
use sqlx::PgPool;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::interval;
use uuid::Uuid;

const USER_AGENT: &str = "yume-maid/1.0";
const CHECK_INTERVAL_SECS: u64 = 300;
const CHECK_TIMEOUT_SECS: u64 = 10;
const DOWN_THRESHOLD: i32 = 3;

struct SiteRow {
    id: Uuid,
    url: String,
    consecutive_failures: i32,
    is_online: bool,
}

pub async fn run(db: PgPool, cache: crate::features::sites::cache::SiteCache) {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(CHECK_TIMEOUT_SECS))
        .tcp_keepalive(Duration::from_secs(30))
        .pool_max_idle_per_host(4)
        .https_only(false)
        .build()
        .expect("failed to build reqwest client");

    let mut ticker = interval(Duration::from_secs(CHECK_INTERVAL_SECS));

    loop {
        ticker.tick().await;

        let sites = match fetch_sites(&db).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("checker: failed to fetch sites: {e}");
                continue;
            }
        };

        let mut set = JoinSet::new();

        for site in sites {
            let client = client.clone();
            let db = db.clone();
            let cache = cache.clone();

            set.spawn(async move {
                check_site(client, db, cache, site).await;
            });
        }

        while set.join_next().await.is_some() {}
    }
}

async fn fetch_sites(db: &PgPool) -> Result<Vec<SiteRow>, sqlx::Error> {
    sqlx::query_as!(
        SiteRow,
        "SELECT id, url, consecutive_failures, is_online FROM sites WHERE enabled = true"
    )
    .fetch_all(db)
    .await
}

async fn check_site(client: Client, db: PgPool, cache: crate::features::sites::cache::SiteCache, site: SiteRow) {
    let ok = client.get(&site.url).send().await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    let (new_failures, new_online) = if ok {
        (0, true)
    } else {
        let failures = site.consecutive_failures + 1;
        let online = failures < DOWN_THRESHOLD;
        (failures, online)
    };

    if new_online != site.is_online || new_failures != site.consecutive_failures {
        let _ = sqlx::query!(
            "UPDATE sites SET is_online = $1, consecutive_failures = $2, last_checked_at = now() WHERE id = $3",
            new_online,
            new_failures,
            site.id
        )
        .execute(&db)
        .await;

        if new_online != site.is_online {
            let _ = crate::features::sites::cache::reload(&cache, &db).await;
        }
    }

}
