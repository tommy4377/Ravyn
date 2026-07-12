use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

pub type Result<T, E = RavynError> = std::result::Result<T, E>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    PermanentClient,
    RetryableHttp,
    Timeout,
    DnsOrConnect,
    ConnectionReset,
    MalformedRange,
    DiskFull,
    Permission,
    ChecksumMismatch,
    ExternalTool,
    Cancellation,
    Internal,
}

impl FailureClass {
    pub fn penalizes_host(self) -> bool {
        matches!(
            self,
            Self::RetryableHttp
                | Self::Timeout
                | Self::DnsOrConnect
                | Self::ConnectionReset
                | Self::MalformedRange
        )
    }
}

#[derive(Debug, Error)]
pub enum RavynError {
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("temporarily unavailable: {0}")]
    Unavailable(String),
    #[error("operation was cancelled")]
    Cancelled,
    #[error("HTTP protocol error: {0}")]
    Protocol(String),
    #[error("external process failed: {0}")]
    Process(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Database(sqlx::Error),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<sqlx::Error> for RavynError {
    fn from(error: sqlx::Error) -> Self {
        crate::core::metrics::note_sqlite_error(&error);
        Self::Database(error)
    }
}

impl RavynError {
    pub fn failure_class(&self) -> FailureClass {
        match self {
            Self::Invalid(_) | Self::NotFound(_) | Self::Conflict(_) | Self::Url(_) => {
                FailureClass::PermanentClient
            }
            Self::Unavailable(_) => FailureClass::RetryableHttp,
            Self::Cancelled => FailureClass::Cancellation,
            Self::Process(_) => FailureClass::ExternalTool,
            Self::Http(error) if error.is_timeout() => FailureClass::Timeout,
            Self::Http(error) if error.is_connect() => FailureClass::DnsOrConnect,
            Self::Http(_) => FailureClass::RetryableHttp,
            Self::Io(error) if matches!(error.raw_os_error(), Some(28 | 39 | 112)) => {
                FailureClass::DiskFull
            }
            Self::Io(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                FailureClass::Permission
            }
            Self::Io(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                FailureClass::ConnectionReset
            }
            Self::Protocol(message) if message.starts_with("SHA-256 mismatch") => {
                FailureClass::ChecksumMismatch
            }
            Self::Protocol(message)
                if message.contains("range") || message.contains("Content-Range") =>
            {
                FailureClass::MalformedRange
            }
            Self::Protocol(message)
                if message.contains(" 408")
                    || message.contains(" 429")
                    || (500..=599).any(|status| message.contains(&format!(" {status}"))) =>
            {
                FailureClass::RetryableHttp
            }
            Self::Protocol(_) => FailureClass::PermanentClient,
            Self::Io(_)
            | Self::Database(_)
            | Self::Migration(_)
            | Self::Json(_)
            | Self::Internal(_) => FailureClass::Internal,
        }
    }

    pub fn api_code(&self) -> &'static str {
        match self {
            Self::Invalid(_) | Self::Url(_) | Self::Json(_) => "INVALID_REQUEST",
            Self::NotFound(_) => "RESOURCE_NOT_FOUND",
            Self::Conflict(_) | Self::Cancelled => "STATE_CONFLICT",
            Self::Unavailable(_) => "TEMPORARILY_UNAVAILABLE",
            Self::Protocol(_) => "REMOTE_PROTOCOL_ERROR",
            Self::Process(_) => "EXTERNAL_PROCESS_ERROR",
            Self::Io(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                "FILESYSTEM_PERMISSION_DENIED"
            }
            Self::Io(_) => "FILESYSTEM_ERROR",
            Self::Http(error) if error.is_timeout() => "REMOTE_TIMEOUT",
            Self::Http(_) => "REMOTE_HTTP_ERROR",
            Self::Database(_) | Self::Migration(_) => "DATABASE_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn public_message(&self) -> String {
        match self {
            Self::Database(_) | Self::Migration(_) | Self::Internal(_) => {
                "An internal backend error occurred.".to_owned()
            }
            _ => self.to_string(),
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self.failure_class(),
            FailureClass::RetryableHttp
                | FailureClass::Timeout
                | FailureClass::DnsOrConnect
                | FailureClass::ConnectionReset
        )
    }
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    request_id: String,
    retryable: bool,
    details: serde_json::Value,
}

impl IntoResponse for RavynError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Invalid(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Cancelled => StatusCode::CONFLICT,
            Self::Protocol(_) => StatusCode::BAD_GATEWAY,
            Self::Process(_) => StatusCode::BAD_GATEWAY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let code = self.api_code();
        let message = self.public_message();
        let request_id = Uuid::new_v4().to_string();
        let retryable = self.retryable();
        let mut response = (
            status,
            Json(ErrorBody {
                code,
                message,
                request_id: request_id.clone(),
                retryable,
                details: serde_json::json!({}),
            }),
        )
            .into_response();
        if let Ok(value) = request_id.parse() {
            response.headers_mut().insert("x-request-id", value);
        }
        response
    }
}

#[cfg(test)]
mod response_tests {
    use axum::body::to_bytes;

    use super::*;

    #[tokio::test]
    async fn internal_errors_are_redacted_and_machine_readable() {
        let response = RavynError::Internal("database password leaked".into()).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(response.headers().contains_key("x-request-id"));
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["code"], "INTERNAL_ERROR");
        assert!(!value["message"].as_str().unwrap().contains("password"));
        assert!(value["request_id"].is_string());
        assert_eq!(value["retryable"], false);
    }
}
