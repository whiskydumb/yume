use axum::{
    Extension,
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Instant;

static TRUST_PROXY: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("TRUST_PROXY")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
});

pub fn real_ip(headers: &axum::http::HeaderMap, fallback: IpAddr) -> IpAddr {
    if !*TRUST_PROXY {
        return fallback;
    }

    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse().ok())
        })
        .unwrap_or(fallback)
}

#[derive(Clone)]
pub struct RateLimiter {
    state: Arc<DashMap<IpAddr, (u32, Instant)>>,
    max_requests: u32,
    window_secs: u64,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            state: Arc::new(DashMap::new()),
            max_requests,
            window_secs,
        }
    }

    fn check(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut entry = self.state.entry(ip).or_insert((0, now));
        let (count, window_start) = entry.value_mut();

        if now.duration_since(*window_start).as_secs() >= self.window_secs {
            *count = 1;
            *window_start = now;
            return true;
        }

        *count += 1;
        *count <= self.max_requests
    }

    pub fn spawn_cleanup(self, interval_secs: u64) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let now = Instant::now();
                self.state.retain(|_, (_, start)| {
                    now.duration_since(*start).as_secs() < self.window_secs
                });
            }
        });
    }
}

pub async fn limit(
    Extension(limiter): Extension<RateLimiter>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let ip = real_ip(request.headers(), addr.ip());
    if !limiter.check(ip) {
        return (StatusCode::TOO_MANY_REQUESTS, "too many requests").into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn real_ip_returns_fallback_when_proxy_not_trusted() {
        let headers = axum::http::HeaderMap::new();
        let fallback = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        assert_eq!(real_ip(&headers, fallback), fallback);
    }

    #[test]
    fn real_ip_ignores_forwarded_header_when_proxy_not_trusted() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "10.0.0.1".parse().unwrap());
        let fallback = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
        assert_eq!(real_ip(&headers, fallback), fallback);
    }

    #[test]
    fn rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(3, 60);
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        assert!(limiter.check(ip));
        assert!(limiter.check(ip));
        assert!(limiter.check(ip));
    }

    #[test]
    fn rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(2, 60);
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        assert!(limiter.check(ip));
        assert!(limiter.check(ip));
        assert!(!limiter.check(ip));
    }

    #[test]
    fn rate_limiter_separate_ips_independent() {
        let limiter = RateLimiter::new(1, 60);
        let ip1 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
        assert!(limiter.check(ip1));
        assert!(limiter.check(ip2));
        assert!(!limiter.check(ip1));
        assert!(!limiter.check(ip2));
    }
}
