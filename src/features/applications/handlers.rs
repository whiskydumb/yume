use crate::error::{self, AppError};
use crate::features::csrf::CsrfToken;
use crate::features::flash::{self, Flash};
use crate::features::rate_limit::real_ip;
use crate::features::validation::{validate_name, validate_slug, validate_url};
use crate::state::AppState;
use askama::Template;
use axum::{
    Extension, Form,
    extract::{ConnectInfo, State},
    response::{Html, Response},
};
use axum_extra::extract::cookie::CookieJar;
use sqlx::types::ipnetwork;
use std::net::SocketAddr;

use super::models::ApplyForm;

#[derive(Template)]
#[template(path = "apply.html")]
struct ApplyTemplate {
    csrf_token: String,
}

pub async fn show_form(Extension(csrf): Extension<CsrfToken>) -> error::Result<Html<String>> {
    Ok(Html((ApplyTemplate { csrf_token: csrf.0 }).render()?))
}

pub async fn submit(
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Form(form): Form<ApplyForm>,
) -> error::Result<Response> {
    let name = form.name.trim();
    let slug = form.slug.trim();
    let url = form.url.trim();
    let contact = form.contact.trim();
    let description = form
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    validate_name(name)?;
    validate_slug(slug)?;
    validate_url(url)?;
    if contact.is_empty() || contact.len() > 200 {
        return Err(AppError::BadRequest("contact must be 1-200 characters"));
    }

    let ip: ipnetwork::IpNetwork = real_ip(&headers, addr.ip()).into();

    if let Err(e) = sqlx::query!(
        "INSERT INTO applications (name, slug, url, contact, description, ip) VALUES ($1, $2, $3, $4, $5, $6)",
        name,
        slug,
        url,
        contact,
        description,
        ip,
    )
    .execute(&state.db)
    .await {
        let msg = match crate::error::unique_constraint_name(&e) {
            Some("applications_slug_pending") => "this slug is already taken",
            Some(_) | None => {
                if crate::error::is_unique_violation(&e) {
                    "conflict"
                } else {
                    return Err(e.into());
                }
            }
        };
        return Ok(flash::redirect(jar, Flash::Error(msg), "/apply"));
    }

    Ok(flash::redirect(
        jar,
        Flash::Success("application submitted"),
        "/apply",
    ))
}
