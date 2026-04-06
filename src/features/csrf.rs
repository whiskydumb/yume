use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::Cookie;

const CSRF_COOKIE: &str = "csrf_token";
const CSRF_FIELD: &str = "csrf_token";

pub async fn set_token(jar: CookieJar, mut request: Request, next: Next) -> Response {
    let (jar, token) = match jar.get(CSRF_COOKIE).map(|c| c.value().to_owned()) {
        Some(token) => (jar, token),
        None => {
            let token = generate_token();
            let cookie = Cookie::build((CSRF_COOKIE, token.clone()))
                .http_only(false)
                .secure(true)
                .same_site(axum_extra::extract::cookie::SameSite::Strict)
                .path("/")
                .build();
            (jar.add(cookie), token)
        }
    };

    request.extensions_mut().insert(CsrfToken(token));
    let response = next.run(request).await;
    (jar, response).into_response()
}

pub async fn verify(jar: CookieJar, request: Request, next: Next) -> Response {
    if request.method() != axum::http::Method::POST {
        return next.run(request).await;
    }

    let cookie_token = match jar.get(CSRF_COOKIE) {
        Some(c) => c.value().to_owned(),
        None => return (StatusCode::FORBIDDEN, "missing csrf token").into_response(),
    };

    let (parts, body) = request.into_parts();
    let bytes = match axum::body::to_bytes(body, 1024 * 16).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "request body too large").into_response(),
    };

    let token = form_field_value(&bytes, CSRF_FIELD).or_else(|| {
        parts
            .headers
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    });

    match token {
        Some(t) if constant_time_eq(t.as_bytes(), cookie_token.as_bytes()) => {
            let request = Request::from_parts(parts, axum::body::Body::from(bytes));
            next.run(request).await
        }
        _ => (StatusCode::FORBIDDEN, "invalid csrf token").into_response(),
    }
}

fn form_field_value(body: &[u8], field: &str) -> Option<String> {
    let body_str = std::str::from_utf8(body).ok()?;
    for pair in body_str.split('&') {
        if let Some((key, value)) = pair.split_once('=')
            && key == field
        {
            return Some(urldecode(value));
        }
    }
    None
}

fn urldecode(s: &str) -> String {
    let s = s.replace('+', " ");
    let mut result = Vec::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let hi = chars.next().and_then(hex_val);
            let lo = chars.next().and_then(hex_val);
            if let (Some(h), Some(l)) = (hi, lo) {
                result.push(h << 4 | l);
            }
        } else {
            result.push(b);
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn generate_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("failed to generate csrf token");
    hex_encode(&buf)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

#[derive(Clone)]
pub struct CsrfToken(pub String);
