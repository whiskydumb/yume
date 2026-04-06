use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

pub type Result<T> = std::result::Result<T, AppError>;

#[allow(dead_code)]
#[derive(Debug)]
pub enum AppError {
    Internal(String),
    BadRequest(&'static str),
    NotFound,
    Unauthorized,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Internal(msg) => {
                tracing::error!(error = %msg, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
            }
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
        }
    }
}

impl From<askama::Error> for AppError {
    fn from(e: askama::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}
