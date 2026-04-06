use crate::error::AppError;

pub fn validate_name(name: &str) -> Result<(), AppError> {
    if name.is_empty() || name.len() > 100 {
        return Err(AppError::BadRequest("name must be 1-100 characters"));
    }
    Ok(())
}

pub fn validate_url(url: &str) -> Result<(), AppError> {
    if url.is_empty()
        || url.len() > 500
        || !(url.starts_with("https://") || url.starts_with("http://"))
    {
        return Err(AppError::BadRequest(
            "url must be a valid http(s) url up to 500 characters",
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
