use reqwest::Client;
use scraper::{Html, Selector};
use std::path::Path;
use tokio::fs;
use uuid::Uuid;

const FAVICON_DIR: &str = "data/favicons";

pub async fn fetch(client: &Client, site_id: Uuid, site_url: &str) -> Option<String> {
    let base = site_url.trim_end_matches('/');

    if let Some(path) = try_from_html(client, site_id, base).await {
        return Some(path);
    }

    let ico_url = format!("{base}/favicon.ico");
    download_and_save(client, site_id, &ico_url).await
}

async fn try_from_html(client: &Client, site_id: Uuid, base_url: &str) -> Option<String> {
    let resp = client.get(base_url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let body = resp.text().await.ok()?;

    let href = extract_favicon_href(&body, base_url)?;
    download_and_save(client, site_id, &href).await
}

fn extract_favicon_href(html: &str, base_url: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse(r#"link[rel~="icon"]"#).ok()?;

    let mut best_href: Option<String> = None;
    let mut best_score: i32 = -1;

    for element in document.select(&selector) {
        let href = match element.value().attr("href") {
            Some(h) if !h.is_empty() => h,
            _ => continue,
        };

        let score = score_favicon(element.value().attr("sizes"), href);
        if score > best_score {
            best_score = score;
            best_href = Some(resolve_url(base_url, href));
        }
    }

    best_href
}

fn score_favicon(sizes: Option<&str>, href: &str) -> i32 {
    let mut score = 0i32;

    if let Some(sizes) = sizes
        && let Some(dim) = sizes.split('x').next().and_then(|s| s.parse::<i32>().ok())
    {
        score += dim;
    }

    let lower = href.to_lowercase();
    if lower.contains(".svg") {
        score += 1000;
    } else if lower.contains(".png") {
        score += 500;
    } else if lower.contains(".webp") {
        score += 400;
    } else if lower.contains(".ico") {
        score += 100;
    }

    score
}

fn resolve_url(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        let scheme = if base.starts_with("https") {
            "https:"
        } else {
            "http:"
        };
        return format!("{scheme}{href}");
    }
    if href.starts_with('/')
        && let Some(idx) = base.find("://")
    {
        let after_scheme = &base[idx + 3..];
        let origin_end = after_scheme.find('/').unwrap_or(after_scheme.len());
        let origin = &base[..idx + 3 + origin_end];
        return format!("{origin}{href}");
    }
    format!("{base}/{href}")
}

async fn download_and_save(client: &Client, site_id: Uuid, url: &str) -> Option<String> {
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let ext = extension_from_content_type(content_type)
        .or_else(|| extension_from_url(url))
        .unwrap_or("ico");

    let bytes = resp.bytes().await.ok()?;
    if bytes.is_empty() || bytes.len() > 512 * 1024 {
        return None;
    }

    let filename = format!("{site_id}.{ext}");
    let dir = Path::new(FAVICON_DIR);

    if let Err(e) = fs::create_dir_all(dir).await {
        tracing::error!(error = %e, "favicon: failed to create directory");
        return None;
    }

    remove_old_favicons(dir, site_id, &filename).await;

    let path = dir.join(&filename);
    if let Err(e) = fs::write(&path, &bytes).await {
        tracing::error!(error = %e, "favicon: failed to write file");
        return None;
    }

    Some(format!("favicons/{filename}"))
}

async fn remove_old_favicons(dir: &Path, site_id: Uuid, keep: &str) {
    let prefix = site_id.to_string();
    let mut entries = match fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && *name != *keep {
            let _ = fs::remove_file(entry.path()).await;
        }
    }
}

fn extension_from_content_type(ct: &str) -> Option<&'static str> {
    let ct = ct.split(';').next().unwrap_or(ct).trim();
    match ct {
        "image/png" => Some("png"),
        "image/svg+xml" => Some("svg"),
        "image/webp" => Some("webp"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("ico"),
        _ => None,
    }
}

fn extension_from_url(url: &str) -> Option<&'static str> {
    let path = url.split('?').next().unwrap_or(url);
    let lower = path.to_lowercase();
    if lower.ends_with(".png") {
        Some("png")
    } else if lower.ends_with(".svg") {
        Some("svg")
    } else if lower.ends_with(".webp") {
        Some("webp")
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("jpg")
    } else if lower.ends_with(".gif") {
        Some("gif")
    } else if lower.ends_with(".ico") {
        Some("ico")
    } else {
        None
    }
}
