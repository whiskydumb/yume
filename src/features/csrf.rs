use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::Cookie;
use subtle::ConstantTimeEq;

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
        Some(t) if bool::from(t.as_bytes().ct_eq(cookie_token.as_bytes())) => {
            let request = Request::from_parts(parts, axum::body::Body::from(bytes));
            next.run(request).await
        }
        _ => (StatusCode::FORBIDDEN, "invalid csrf token").into_response(),
    }
}

fn form_field_value(body: &[u8], field: &str) -> Option<String> {
    serde_urlencoded::from_bytes::<Vec<(String, String)>>(body)
        .ok()?
        .into_iter()
        .find(|(k, _)| k == field)
        .map(|(_, v)| v)
}

fn generate_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("failed to generate csrf token");
    hex::encode(buf)
}

#[derive(Clone)]
pub struct CsrfToken(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_token_returns_64_char_hex() {
        let token = generate_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn form_field_value_finds_target_field() {
        let body = b"foo=bar&csrf_token=abc123&baz=qux";
        assert_eq!(
            form_field_value(body, "csrf_token"),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn form_field_value_returns_none_when_absent() {
        let body = b"foo=bar&baz=qux";
        assert_eq!(form_field_value(body, "csrf_token"), None);
    }

    #[test]
    fn form_field_value_decodes_percent_encoded_value() {
        let body = b"csrf_token=hello%20world";
        assert_eq!(
            form_field_value(body, "csrf_token"),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn form_field_value_decodes_plus_as_space() {
        let body = b"csrf_token=hello+world";
        assert_eq!(
            form_field_value(body, "csrf_token"),
            Some("hello world".to_string())
        );
    }
}
