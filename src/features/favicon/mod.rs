use futures::StreamExt;
use scraper::{Html, Selector};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use url::Url;
use uuid::Uuid;

const FAVICON_DIR: &str = "data/favicons";

/// returns true only if `ip` is a globally routable address.
/// rejects loopback, private, link-local, broadcast, unspecified, documentation,
/// CGNAT (100.64.0.0/10), IPv6 unique-local (fc00::/7), and link-local (fe80::/10).
pub(crate) fn is_public_addr(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_documentation()
            {
                return false;
            }
            // CGNAT 100.64.0.0/10
            let o = v4.octets();
            if o[0] == 100 && (o[1] & 0xC0) == 64 {
                return false;
            }
            true
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return false;
            }
            let s = v6.segments();
            // fc00::/7 — unique local
            if (s[0] & 0xFE00) == 0xFC00 {
                return false;
            }
            // fe80::/10 — link-local
            if (s[0] & 0xFFC0) == 0xFE80 {
                return false;
            }
            true
        }
    }
}

/// resolves `host` once, validates all addresses are public, returns them
/// for pinning in reqwest. fails closed on DNS error or non-public addr.
pub(crate) async fn resolve_public_addrs(host: &str, port: u16) -> Option<Vec<SocketAddr>> {
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port)).await.ok()?.collect();
    if addrs.is_empty() || !addrs.iter().all(|a| is_public_addr(a.ip())) {
        return None;
    }
    Some(addrs)
}

pub async fn fetch(site_id: Uuid, site_url: &str) -> Option<String> {
    let base = site_url.trim_end_matches('/');

    if let Some(path) = try_from_html(site_id, base).await {
        return Some(path);
    }

    let ico_url = format!("{base}/favicon.ico");
    download_and_save(site_id, &ico_url).await
}

async fn try_from_html(site_id: Uuid, base_url: &str) -> Option<String> {
    let parsed = Url::parse(base_url).ok()?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = parsed.host_str()?.to_owned();
    let port = parsed.port_or_known_default().unwrap_or(80);

    // resolve once and pin - prevents DNS rebinding TOCTOU between our check
    // and reqwest's internal resolution (fix/favicon-ssrf-dns-rebinding)
    let Some(addrs) = resolve_public_addrs(&host, port).await else {
        tracing::warn!(url = %base_url, "favicon: SSRF blocked (try_from_html)");
        return None;
    };

    let pinned = reqwest::Client::builder()
        .user_agent("yume-health/1.0")
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&host, &addrs)
        .https_only(false)
        .build()
        .ok()?;

    let resp = pinned.get(base_url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    // stream HTML with a hard 2 MB cap to prevent OOM DoS
    const HTML_LIMIT: usize = 2 * 1024 * 1024;
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.ok()?;
        if buf.len() + chunk.len() > HTML_LIMIT {
            tracing::warn!(url = %base_url, "favicon: HTML body exceeds 2 MB, skipping");
            return None;
        }
        buf.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&buf).into_owned();

    let href = extract_favicon_href(&body, base_url)?;
    download_and_save(site_id, &href).await
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

        let Some(resolved) = resolve_url(base_url, href) else {
            continue;
        };

        let score = score_favicon(element.value().attr("sizes"), href);
        if score > best_score {
            best_score = score;
            best_href = Some(resolved);
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

/// resolves `href` relative to `base_url` using RFC 3986 URL joining.
/// returns None for parse/join errors or non-http/s schemes (file://, data:, javascript:, etc.).
fn resolve_url(base_url: &str, href: &str) -> Option<String> {
    let base = Url::parse(base_url).ok()?;
    let joined = base.join(href).ok()?;
    let scheme = joined.scheme();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    Some(joined.to_string())
}

async fn download_and_save(site_id: Uuid, url: &str) -> Option<String> {
    // SSRF pre-check: scheme must be http/https
    let parsed = Url::parse(url).ok()?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = parsed.host_str()?.to_owned();
    let port = parsed.port_or_known_default().unwrap_or(80);

    // resolve once and pin - prevents DNS rebinding TOCTOU
    let Some(addrs) = resolve_public_addrs(&host, port).await else {
        tracing::warn!(url = %url, "favicon: SSRF blocked (download_and_save)");
        return None;
    };

    let pinned = reqwest::Client::builder()
        .user_agent("yume-health/1.0")
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&host, &addrs)
        .https_only(false)
        .build()
        .ok()?;

    let resp = pinned.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    // early rejection on Content-Length to avoid starting the stream at all
    if let Some(len) = resp.content_length()
        && len > 512 * 1024
    {
        tracing::warn!(url = %url, "favicon: Content-Length exceeds 512 KB");
        return None;
    }

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();

    let ext = extension_from_content_type(&content_type)
        .or_else(|| extension_from_url(url))
        .unwrap_or("ico");

    // stream with a hard 512 KB cap to prevent OOM DoS
    const FAVICON_LIMIT: usize = 512 * 1024;
    let mut stream = resp.bytes_stream();
    let mut bytes: Vec<u8> = Vec::with_capacity(4096);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.ok()?;
        if bytes.len() + chunk.len() > FAVICON_LIMIT {
            tracing::warn!(url = %url, "favicon: body exceeds 512 KB limit");
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn is_public_addr_rejects_private_and_special() {
        // IPv4 loopback
        assert!(!is_public_addr(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        // RFC 1918 10.0.0.0/8
        assert!(!is_public_addr(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        // link-local / APIPA 169.254.0.0/16
        assert!(!is_public_addr(IpAddr::V4(Ipv4Addr::new(
            169, 254, 169, 254
        ))));
        // RFC 1918 192.168.0.0/16
        assert!(!is_public_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1))));
        // CGNAT 100.64.0.0/10
        assert!(!is_public_addr(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))));
        assert!(!is_public_addr(IpAddr::V4(Ipv4Addr::new(
            100, 127, 255, 255
        ))));
        // IPv6 loopback ::1
        assert!(!is_public_addr(IpAddr::V6(Ipv6Addr::new(
            0, 0, 0, 0, 0, 0, 0, 1
        ))));
        // IPv6 unique-local fc00::1
        assert!(!is_public_addr(IpAddr::V6(Ipv6Addr::new(
            0xfc00, 0, 0, 0, 0, 0, 0, 1
        ))));
        // IPv6 unique-local fd00::/8
        assert!(!is_public_addr(IpAddr::V6(Ipv6Addr::new(
            0xfd00, 0, 0, 0, 0, 0, 0, 1
        ))));
        // IPv6 link-local fe80::1
        assert!(!is_public_addr(IpAddr::V6(Ipv6Addr::new(
            0xfe80, 0, 0, 0, 0, 0, 0, 1
        ))));
    }

    #[test]
    fn is_public_addr_allows_public() {
        assert!(is_public_addr(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(is_public_addr(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(is_public_addr(
            "2001:4860:4860::8888".parse::<IpAddr>().unwrap()
        ));
        // 100.128.0.0 is just outside CGNAT range and should be allowed
        assert!(is_public_addr(IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1))));
    }

    #[test]
    fn resolve_url_absolute_https() {
        assert_eq!(
            resolve_url("https://example.com/", "https://other.com/fav.ico"),
            Some("https://other.com/fav.ico".to_string())
        );
    }

    #[test]
    fn resolve_url_protocol_relative() {
        assert_eq!(
            resolve_url("https://example.com/page", "//cdn.example.com/fav.ico"),
            Some("https://cdn.example.com/fav.ico".to_string())
        );
    }

    #[test]
    fn resolve_url_root_relative() {
        assert_eq!(
            resolve_url("https://example.com/path/page", "/favicon.ico"),
            Some("https://example.com/favicon.ico".to_string())
        );
    }

    #[test]
    fn resolve_url_relative_path() {
        assert_eq!(
            resolve_url("https://example.com/path/page", "images/fav.ico"),
            Some("https://example.com/path/images/fav.ico".to_string())
        );
    }

    #[test]
    fn resolve_url_rejects_file_scheme() {
        assert_eq!(
            resolve_url("https://example.com/", "file:///etc/passwd"),
            None
        );
    }

    #[test]
    fn resolve_url_rejects_data_scheme() {
        assert_eq!(
            resolve_url("https://example.com/", "data:image/png;base64,abc"),
            None
        );
    }

    #[test]
    fn resolve_url_rejects_javascript_scheme() {
        assert_eq!(
            resolve_url("https://example.com/", "javascript:void(0)"),
            None
        );
    }

    #[test]
    fn resolve_url_absolute_http() {
        assert_eq!(
            resolve_url("https://example.com/", "http://cdn.example.com/fav.ico"),
            Some("http://cdn.example.com/fav.ico".to_string())
        );
    }

    #[tokio::test]
    async fn resolve_public_addrs_rejects_localhost() {
        // localhost resolves to 127.0.0.1 (loopback) - must be rejected
        assert!(resolve_public_addrs("localhost", 80).await.is_none());
    }
}
