use crate::state::AppState;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use jsonwebtoken::{DecodingKey, Validation};

use super::models::Claims;

pub async fn require_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    request: Request,
    next: Next,
) -> Response {
    let valid = jar
        .get("token")
        .and_then(|c| {
            jsonwebtoken::decode::<Claims>(
                c.value(),
                &DecodingKey::from_secret(state.jwt_secret.as_ref()),
                &Validation::default(),
            )
            .ok()
        })
        .filter(|data| data.claims.sub == "admin")
        .is_some();

    if !valid {
        return Redirect::to("/admin/login").into_response();
    }

    next.run(request).await
}