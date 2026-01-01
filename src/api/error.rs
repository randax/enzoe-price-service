use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde_json::json;

use crate::storage::StorageError;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    InternalError(String),
    DatabaseError(StorageError),
}

#[derive(Debug)]
pub struct AppErrorWithContext {
    pub error: AppError,
    pub correlation_id: Option<String>,
}

impl AppError {
    pub fn with_correlation_id(self, correlation_id: Option<String>) -> AppErrorWithContext {
        AppErrorWithContext {
            error: self,
            correlation_id,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            AppError::InternalError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg)
            }
            AppError::DatabaseError(e) => {
                if e.is_not_found() {
                    (StatusCode::NOT_FOUND, "NOT_FOUND", e.to_string())
                } else if e.is_connection_error() {
                    (StatusCode::SERVICE_UNAVAILABLE, "DATABASE_UNAVAILABLE", e.to_string())
                } else {
                    (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR", e.to_string())
                }
            }
        };

        let body = json!({
            "error": message,
            "code": code,
            "timestamp": Utc::now().to_rfc3339()
        });

        (status, Json(body)).into_response()
    }
}

impl IntoResponse for AppErrorWithContext {
    fn into_response(self) -> Response {
        let (status, code, message) = match self.error {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            AppError::InternalError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg)
            }
            AppError::DatabaseError(e) => {
                if e.is_not_found() {
                    (StatusCode::NOT_FOUND, "NOT_FOUND", e.to_string())
                } else if e.is_connection_error() {
                    (StatusCode::SERVICE_UNAVAILABLE, "DATABASE_UNAVAILABLE", e.to_string())
                } else {
                    (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR", e.to_string())
                }
            }
        };

        let mut body = json!({
            "error": message,
            "code": code,
            "timestamp": Utc::now().to_rfc3339()
        });

        if let Some(ref correlation_id) = self.correlation_id {
            body["correlation_id"] = json!(correlation_id);
        }

        let mut response = (status, Json(body)).into_response();
        if let Some(correlation_id) = self.correlation_id {
            if let Ok(header_value) = axum::http::header::HeaderValue::from_str(&correlation_id) {
                response.headers_mut().insert("X-Correlation-Id", header_value);
            }
        }
        response
    }
}

impl From<StorageError> for AppError {
    fn from(e: StorageError) -> Self {
        AppError::DatabaseError(e)
    }
}
