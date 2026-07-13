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

/// Distinct, machine-readable causes for provisioning (managed-engine
/// download/install/rollback/health-check) failures. Each maps to a stable
/// `api_code()` string so frontends and scripts can branch on the cause
/// instead of parsing prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisioningErrorCode {
    ManifestUnavailable,
    PlatformUnsupported,
    InvalidManifestSignature,
    ChecksumMismatch,
    InsufficientSpace,
    DownloadInterrupted,
    QuarantinedByAntivirus,
    HealthCheckFailed,
    RollbackFailed,
    InvalidCustomPath,
    AppInstallFailed,
}

impl ProvisioningErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManifestUnavailable => "PROVISIONING_MANIFEST_UNAVAILABLE",
            Self::PlatformUnsupported => "PROVISIONING_PLATFORM_UNSUPPORTED",
            Self::InvalidManifestSignature => "PROVISIONING_INVALID_MANIFEST_SIGNATURE",
            Self::ChecksumMismatch => "PROVISIONING_CHECKSUM_MISMATCH",
            Self::InsufficientSpace => "PROVISIONING_INSUFFICIENT_SPACE",
            Self::DownloadInterrupted => "PROVISIONING_DOWNLOAD_INTERRUPTED",
            Self::QuarantinedByAntivirus => "PROVISIONING_QUARANTINED",
            Self::HealthCheckFailed => "PROVISIONING_HEALTH_CHECK_FAILED",
            Self::RollbackFailed => "PROVISIONING_ROLLBACK_FAILED",
            Self::InvalidCustomPath => "PROVISIONING_INVALID_CUSTOM_PATH",
            Self::AppInstallFailed => "PROVISIONING_APP_INSTALL_FAILED",
        }
    }

    fn status(self) -> StatusCode {
        match self {
            Self::PlatformUnsupported | Self::InvalidCustomPath => StatusCode::BAD_REQUEST,
            Self::InsufficientSpace => StatusCode::INSUFFICIENT_STORAGE,
            Self::ManifestUnavailable
            | Self::DownloadInterrupted
            | Self::QuarantinedByAntivirus => StatusCode::SERVICE_UNAVAILABLE,
            Self::InvalidManifestSignature
            | Self::ChecksumMismatch
            | Self::HealthCheckFailed
            | Self::AppInstallFailed => StatusCode::BAD_GATEWAY,
            Self::RollbackFailed => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn default_retryable(self) -> bool {
        matches!(
            self,
            Self::ManifestUnavailable | Self::DownloadInterrupted | Self::QuarantinedByAntivirus
        )
    }

    fn failure_class(self) -> FailureClass {
        match self {
            Self::ChecksumMismatch => FailureClass::ChecksumMismatch,
            Self::InsufficientSpace => FailureClass::DiskFull,
            Self::QuarantinedByAntivirus => FailureClass::Permission,
            Self::ManifestUnavailable | Self::DownloadInterrupted => FailureClass::RetryableHttp,
            Self::HealthCheckFailed | Self::RollbackFailed | Self::AppInstallFailed => {
                FailureClass::ExternalTool
            }
            Self::PlatformUnsupported
            | Self::InvalidManifestSignature
            | Self::InvalidCustomPath => FailureClass::PermanentClient,
        }
    }
}

/// Structured context attached to a [`ProvisioningErrorCode`], surfaced
/// verbatim in the API error response's `details` field.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProvisioningErrorDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
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
    #[error("{message}")]
    Provisioning {
        code: ProvisioningErrorCode,
        message: String,
        details: Box<ProvisioningErrorDetails>,
        retryable: bool,
    },
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

impl RavynError {
    /// Builds a structured provisioning failure. Chain the `with_*` methods
    /// to attach whichever context fields are known at the call site.
    pub fn provisioning(code: ProvisioningErrorCode, message: impl Into<String>) -> Self {
        Self::Provisioning {
            code,
            message: message.into(),
            details: Box::default(),
            retryable: code.default_retryable(),
        }
    }

    fn with_details(mut self, apply: impl FnOnce(&mut ProvisioningErrorDetails)) -> Self {
        if let Self::Provisioning { details, .. } = &mut self {
            apply(details);
        }
        self
    }

    pub fn with_component(self, component: impl Into<String>) -> Self {
        let component = component.into();
        self.with_details(|details| details.component = Some(component))
    }

    pub fn with_stage(self, stage: impl Into<String>) -> Self {
        let stage = stage.into();
        self.with_details(|details| details.stage = Some(stage))
    }

    pub fn with_expected_version(self, version: impl Into<String>) -> Self {
        let version = version.into();
        self.with_details(|details| details.expected_version = Some(version))
    }

    pub fn with_detected_version(self, version: impl Into<String>) -> Self {
        let version = version.into();
        self.with_details(|details| details.detected_version = Some(version))
    }

    pub fn with_path(self, path: impl Into<String>) -> Self {
        let path = path.into();
        self.with_details(|details| details.path = Some(path))
    }

    pub fn with_target(self, target: impl Into<String>) -> Self {
        let target = target.into();
        self.with_details(|details| details.target = Some(target))
    }
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
            Self::Provisioning { code, .. } => code.failure_class(),
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
            Self::Provisioning { code, .. } => code.as_str(),
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
        if let Self::Provisioning { retryable, .. } = self {
            return *retryable;
        }
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
            Self::Provisioning { code, .. } => code.status(),
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let code = self.api_code();
        let message = self.public_message();
        let request_id = Uuid::new_v4().to_string();
        let retryable = self.retryable();
        let details = match &self {
            Self::Provisioning { details, .. } => {
                serde_json::to_value(details.as_ref()).unwrap_or_else(|_| serde_json::json!({}))
            }
            _ => serde_json::json!({}),
        };
        let mut response = (
            status,
            Json(ErrorBody {
                code,
                message,
                request_id: request_id.clone(),
                retryable,
                details,
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

    #[tokio::test]
    async fn provisioning_errors_expose_a_distinct_code_and_structured_details() {
        let error = RavynError::provisioning(
            ProvisioningErrorCode::ChecksumMismatch,
            "managed engine checksum verification failed",
        )
        .with_component("ffmpeg")
        .with_stage("install")
        .with_expected_version("7.1.0");
        assert!(!error.retryable());
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["code"], "PROVISIONING_CHECKSUM_MISMATCH");
        assert_eq!(value["details"]["component"], "ffmpeg");
        assert_eq!(value["details"]["stage"], "install");
        assert_eq!(value["details"]["expected_version"], "7.1.0");
        assert!(value["details"].get("detected_version").is_none());
    }
}
