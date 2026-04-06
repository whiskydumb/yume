use crate::error::AppError;

pub fn validate_name(name: &str) -> Result<(), AppError> {
    if name.is_empty() || name.len() > 100 {
        return Err(AppError::BadRequest("name must be 1-100 characters"));
    }
    Ok(())
}

pub fn validate_url(url: &str) -> Result<(), AppError> {
    if url.is_empty()
        || url.len() > 255
        || !(url.starts_with("https://") || url.starts_with("http://"))
    {
        return Err(AppError::BadRequest(
            "url must be a valid http(s) url up to 255 characters",
        ));
    }
    Ok(())
}

pub fn validate_slug(slug: &str) -> Result<(), AppError> {
    if slug.len() < 3
        || slug.len() > 50
        || !slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(AppError::BadRequest(
            "slug must be 3-50 chars: lowercase, digits, hyphens",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_name_rejects_empty() {
        assert!(validate_name("").is_err());
    }

    #[test]
    fn validate_name_rejects_over_100() {
        assert!(validate_name(&"a".repeat(101)).is_err());
    }

    #[test]
    fn validate_name_accepts_100() {
        assert!(validate_name(&"a".repeat(100)).is_ok());
    }

    #[test]
    fn validate_name_accepts_valid() {
        assert!(validate_name("my site").is_ok());
    }

    #[test]
    fn validate_slug_rejects_empty() {
        assert!(validate_slug("").is_err());
    }

    #[test]
    fn validate_slug_rejects_uppercase() {
        assert!(validate_slug("Hello").is_err());
    }

    #[test]
    fn validate_slug_rejects_special_chars() {
        assert!(validate_slug("no spaces").is_err());
        assert!(validate_slug("no_under").is_err());
        assert!(validate_slug("no!bang").is_err());
    }

    #[test]
    fn validate_slug_allows_hyphens() {
        assert!(validate_slug("my-site").is_ok());
    }

    #[test]
    fn validate_slug_allows_digits() {
        assert!(validate_slug("site123").is_ok());
    }

    #[test]
    fn validate_slug_rejects_2_chars() {
        assert!(validate_slug("ab").is_err());
    }

    #[test]
    fn validate_slug_accepts_3_chars() {
        assert!(validate_slug("abc").is_ok());
    }

    #[test]
    fn validate_slug_rejects_over_50() {
        assert!(validate_slug(&"a".repeat(51)).is_err());
    }

    #[test]
    fn validate_slug_accepts_50() {
        assert!(validate_slug(&"a".repeat(50)).is_ok());
    }

    #[test]
    fn validate_url_rejects_empty() {
        assert!(validate_url("").is_err());
    }

    #[test]
    fn validate_url_rejects_no_scheme() {
        assert!(validate_url("example.com").is_err());
    }

    #[test]
    fn validate_url_accepts_https() {
        assert!(validate_url("https://example.com").is_ok());
    }

    #[test]
    fn validate_url_accepts_http() {
        assert!(validate_url("http://example.com").is_ok());
    }

    #[test]
    fn validate_url_rejects_over_255() {
        let url = format!("https://{}", "a".repeat(248)); // 8 + 248 = 256
        assert!(validate_url(&url).is_err());
    }

    #[test]
    fn validate_url_accepts_255() {
        let url = format!("https://{}", "a".repeat(247)); // 8 + 247 = 255
        assert!(validate_url(&url).is_ok());
    }
}
