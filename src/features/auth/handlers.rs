use crate::error::{self, AppError};
use crate::features::applications::models::Application;
use crate::features::csrf::CsrfToken;
use crate::features::flash::{self, Flash};
use crate::features::sites::cache;
use crate::features::sites::models::AdminSite;
use crate::features::validation::{validate_name, validate_slug, validate_url};
use crate::state::AppState;
use askama::Template;
use axum::{
    Extension, Form, Json,
    extract::State,
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use serde::Deserialize;
use std::sync::Arc;

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

#[derive(Template)]
#[template(path = "admin/login.html")]
struct LoginTemplate {
    csrf_token: String,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub password: String,
}

pub async fn login_page(Extension(csrf): Extension<CsrfToken>) -> error::Result<Html<String>> {
    Ok(Html((LoginTemplate { csrf_token: csrf.0 }).render()?))
}

pub async fn login_post(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Result<Response, AppError> {
    let hash: Arc<str> = state.admin_password_hash.clone();
    let password = form.password;

    let valid = tokio::task::spawn_blocking(move || {
        use argon2::{Argon2, PasswordHash, PasswordVerifier};
        let parsed = match PasswordHash::new(&hash) {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(error = %e, "failed to parse admin password hash");
                return None;
            }
        };
        Argon2::default().verify_password(password.as_bytes(), &parsed).ok()?;
        Some(())
    })
    .await
    .ok()
    .flatten()
    .is_some();

    if !valid {
        return Ok(flash::redirect(jar, Flash::Error("invalid password"), "/admin/login"));
    }

    let expiry = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() + state.jwt_expiry_hours * 3600;

    let claims = super::models::Claims {
        sub: "admin".into(),
        exp: expiry,
    };

    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_ref()),
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let cookie = Cookie::build(("token", token))
        .http_only(true)
        .secure(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .path("/")
        .build();

    Ok((jar.add(cookie), Redirect::to("/admin")).into_response())
}

pub async fn admin_redirect() -> Redirect {
    Redirect::to("/admin/sites")
}

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct DashboardTemplate {
    sites: Vec<AdminSite>,
    csrf_token: String,
}

pub async fn dashboard(
    State(state): State<AppState>,
    Extension(csrf): Extension<CsrfToken>,
) -> error::Result<Html<String>> {
    let sites = sqlx::query_as!(
        AdminSite,
        "SELECT id, name, url, slug, description, favicon, enabled, position, is_online, consecutive_failures, last_checked_at
         FROM sites
         ORDER BY position ASC"
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Html((DashboardTemplate { sites, csrf_token: csrf.0 }).render()?))
}

#[derive(Template)]
#[template(path = "admin/applications.html")]
struct ApplicationsTemplate {
    pending: Vec<Application>,
    recent: Vec<Application>,
    all: Vec<Application>,
    csrf_token: String,
}

pub async fn applications(
    State(state): State<AppState>,
    Extension(csrf): Extension<CsrfToken>,
) -> error::Result<Html<String>> {
    let rows = sqlx::query_as!(
        Application,
        "SELECT id, name, slug, url, contact, description, status, created_at
         FROM applications
         ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await?;

    let mut pending = Vec::new();
    let mut resolved = Vec::new();

    for app in &rows {
        if app.status == "pending" {
            pending.push(app.clone());
        } else {
            resolved.push(app.clone());
        }
    }

    let recent: Vec<Application> = resolved.iter().take(5).cloned().collect();

    Ok(Html((ApplicationsTemplate { pending, recent, all: rows, csrf_token: csrf.0 }).render()?))
}

pub async fn logout(jar: CookieJar) -> (CookieJar, Redirect) {
    let jar = jar.remove(Cookie::from("token"));
    (jar, Redirect::to("/admin/login"))
}

pub async fn trigger_scan(State(state): State<AppState>, jar: CookieJar) -> Response {
    state.scan_notify.notify_one();
    flash::redirect(jar, Flash::Success("scan triggered"), "/admin/sites")
}

#[derive(Deserialize)]
pub struct AddSiteForm {
    pub slug: String,
    pub name: String,
    pub url: String,
    pub description: Option<String>,
}

pub async fn add_site(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<AddSiteForm>,
) -> error::Result<Response> {
    let slug = form.slug.trim();
    let name = form.name.trim();
    let url = form.url.trim();
    let description = form.description.as_deref().map(str::trim).filter(|s| !s.is_empty());

    validate_slug(slug)?;
    validate_name(name)?;
    validate_url(url)?;

    let mut tx = state.db.begin().await?;

    sqlx::query!("SELECT pg_advisory_xact_lock(1)")
        .execute(&mut *tx)
        .await?;

    if let Err(e) = sqlx::query!(
        "INSERT INTO sites (slug, name, url, description, position)
         VALUES ($1, $2, $3, $4, COALESCE((SELECT MAX(position) FROM sites), 0) + 1)",
        slug,
        name,
        url,
        description,
    )
    .execute(&mut *tx)
    .await {
        return match is_unique_violation(&e) {
            true => Ok(flash::redirect(jar, Flash::Error("slug already exists"), "/admin/sites")),
            false => Err(e.into()),
        };
    }

    tx.commit().await?;

    cache::reload(&state.site_cache, &state.db).await?;

    Ok(flash::redirect(jar, Flash::Success("site added"), "/admin"))
}

#[derive(Deserialize)]
pub struct UpdateSiteForm {
    pub slug: String,
    pub name: String,
    pub url: String,
    pub description: Option<String>,
}

pub async fn update_site(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    Form(form): Form<UpdateSiteForm>,
) -> error::Result<Response> {
    let slug = form.slug.trim();
    let name = form.name.trim();
    let url = form.url.trim();
    let description = form.description.as_deref().map(str::trim).filter(|s| !s.is_empty());

    validate_slug(slug)?;
    validate_name(name)?;
    validate_url(url)?;

    let result = sqlx::query!(
        "UPDATE sites SET slug = $1, name = $2, url = $3, description = $4 WHERE id = $5",
        slug,
        name,
        url,
        description,
        id,
    )
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    cache::reload(&state.site_cache, &state.db).await?;

    Ok(flash::redirect(jar, Flash::Success("site updated"), "/admin"))
}

pub async fn toggle_site(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> error::Result<Response> {
    let result = sqlx::query!(
        "UPDATE sites SET enabled = NOT enabled WHERE id = $1",
        id,
    )
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    cache::reload(&state.site_cache, &state.db).await?;

    Ok(flash::redirect(jar, Flash::Success("site toggled"), "/admin"))
}

#[derive(Deserialize)]
pub struct ReorderBody {
    pub ids: Vec<uuid::Uuid>,
}

pub async fn reorder_sites(
    State(state): State<AppState>,
    Json(body): Json<ReorderBody>,
) -> error::Result<axum::http::StatusCode> {
    if body.ids.is_empty() {
        return Err(AppError::BadRequest("ids cannot be empty"));
    }

    let mut tx = state.db.begin().await?;
    sqlx::query!("SELECT pg_advisory_xact_lock(1)")
        .execute(&mut *tx)
        .await?;

    for (i, id) in body.ids.iter().enumerate() {
        let neg = -((i as i32) + 1);
        sqlx::query!(
            "UPDATE sites SET position = $1 WHERE id = $2",
            neg,
            *id,
        )
        .execute(&mut *tx)
        .await?;
    }

    for (i, id) in body.ids.iter().enumerate() {
        let pos = (i as i32) + 1;
        sqlx::query!(
            "UPDATE sites SET position = $1 WHERE id = $2",
            pos,
            *id,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    cache::reload(&state.site_cache, &state.db).await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn delete_site(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> error::Result<Response> {
    let result = sqlx::query!("DELETE FROM sites WHERE id = $1", id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    cache::reload(&state.site_cache, &state.db).await?;

    Ok(flash::redirect(jar, Flash::Success("site deleted"), "/admin"))
}

pub async fn approve_application(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> error::Result<Response> {
    let app = sqlx::query!(
        "SELECT name, slug, url, description FROM applications WHERE id = $1 AND status = 'pending'",
        id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let mut tx = state.db.begin().await?;

    sqlx::query!("SELECT pg_advisory_xact_lock(1)")
        .execute(&mut *tx)
        .await?;

    if let Err(e) = sqlx::query!(
        "INSERT INTO sites (slug, name, url, description, position)
         VALUES ($1, $2, $3, $4, COALESCE((SELECT MAX(position) FROM sites), 0) + 1)",
        app.slug,
        app.name,
        app.url,
        app.description,
    )
    .execute(&mut *tx)
    .await {
        return match is_unique_violation(&e) {
            true => Ok(flash::redirect(jar, Flash::Error("slug already exists"), "/admin/applications")),
            false => Err(e.into()),
        };
    }

    sqlx::query!(
        "UPDATE applications SET status = 'approved' WHERE id = $1",
        id,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    cache::reload(&state.site_cache, &state.db).await?;

    Ok(flash::redirect(jar, Flash::Success("application approved"), "/admin/applications"))
}

pub async fn reject_application(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
) -> error::Result<Response> {
    let result = sqlx::query!(
        "UPDATE applications SET status = 'rejected' WHERE id = $1 AND status = 'pending'",
        id,
    )
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(flash::redirect(jar, Flash::Success("application rejected"), "/admin/applications"))
}