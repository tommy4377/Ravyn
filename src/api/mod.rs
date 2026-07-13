mod openapi;
mod pagination;
mod routes;
use crate::{Ravyn, error::Result};
use axum::{
    Json,
    extract::{ConnectInfo, DefaultBodyLimit, State},
    http::{
        HeaderValue, Method, Request, StatusCode,
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN, AUTHORIZATION, ORIGIN, RETRY_AFTER, VARY,
        },
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

pub async fn serve(app: Ravyn) -> Result<()> {
    if !app.config.listen.ip().is_loopback()
        && (!app.config.allow_remote_api
            || !app.config.remote_api_behind_tls_proxy
            || app.config.api_token.as_deref().is_none_or(str::is_empty))
    {
        return Err(crate::error::RavynError::Invalid(
            "non-loopback API binding requires --allow-remote-api, --remote-api-behind-tls-proxy, and RAVYN_API_TOKEN".into(),
        ));
    }
    let body_limit = app.config.max_api_body_mib.saturating_mul(1024 * 1024);
    let auth = AuthState {
        global_token: app
            .config
            .api_token
            .clone()
            .filter(|value| !value.is_empty()),
        repository: app.repository.clone(),
    };
    let protection = ApiProtectionState::new(
        app.config.api_max_concurrent_requests,
        app.config.api_rate_limit_per_minute,
        app.config.api_rate_limit_burst,
        Duration::from_secs(app.config.api_request_timeout_secs),
    );
    let state = routes::ApiState {
        repository: app.repository.clone(),
        manager: app.manager.clone(),
        base_config: app.base_config.clone(),
        protection: protection.clone(),
        library_import_status: std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::services::library::LibraryImportStatus::default(),
        )),
    };
    let router = routes::router(state)
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(middleware::from_fn_with_state(protection, protect_api))
        .layer(middleware::from_fn_with_state(auth, require_token))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid));
    let listener = tokio::net::TcpListener::bind(app.config.listen).await?;
    tracing::info!(address=%app.config.listen,"Ravyn backend listening");
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(app.manager))
    .await
    .map_err(|e| crate::error::RavynError::Internal(e.to_string()))
}
async fn shutdown_signal(manager: Arc<crate::core::manager::JobManager>) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {_=ctrl_c=>{},_=terminate=>{}}
    manager.shutdown().await;
}

#[derive(Clone)]
pub(crate) struct ApiProtectionState {
    semaphore: Arc<crate::core::manager::ConcurrencyGate>,
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    config: Arc<RwLock<ApiProtectionConfig>>,
}

#[derive(Clone, Copy)]
struct ApiProtectionConfig {
    burst: f64,
    refill_per_second: f64,
    request_timeout: Duration,
}

impl ApiProtectionState {
    fn new(
        maximum_concurrency: usize,
        requests_per_minute: u64,
        burst: u64,
        request_timeout: Duration,
    ) -> Self {
        Self {
            semaphore: Arc::new(crate::core::manager::ConcurrencyGate::new(
                maximum_concurrency,
            )),
            buckets: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(RwLock::new(ApiProtectionConfig {
                burst: burst.max(1) as f64,
                refill_per_second: requests_per_minute.max(1) as f64 / 60.0,
                request_timeout,
            })),
        }
    }

    pub(crate) async fn reconfigure(
        &self,
        maximum_concurrency: usize,
        requests_per_minute: u64,
        burst: u64,
        request_timeout: Duration,
    ) {
        let mut config = self.config.write().await;
        *config = ApiProtectionConfig {
            burst: burst.max(1) as f64,
            refill_per_second: requests_per_minute.max(1) as f64 / 60.0,
            request_timeout,
        };
        self.semaphore.set_limit(maximum_concurrency);
        self.buckets.lock().await.clear();
    }

    async fn request_timeout(&self) -> Duration {
        self.config.read().await.request_timeout
    }

    async fn consume(&self, identity: &str) -> bool {
        let config = *self.config.read().await;
        let mut buckets = self.buckets.lock().await;
        if buckets.len() > 4_096 {
            let now = Instant::now();
            buckets.retain(|_, bucket| {
                now.duration_since(bucket.last_seen) < Duration::from_secs(3_600)
            });
        }
        buckets
            .entry(identity.to_owned())
            .or_insert_with(|| TokenBucket::new(config.burst, config.refill_per_second))
            .consume()
    }
}

struct TokenBucket {
    tokens: f64,
    burst: f64,
    refill_per_second: f64,
    updated_at: Instant,
    last_seen: Instant,
}

impl TokenBucket {
    fn new(burst: f64, refill_per_second: f64) -> Self {
        let now = Instant::now();
        Self {
            tokens: burst,
            burst,
            refill_per_second,
            updated_at: now,
            last_seen: now,
        }
    }

    fn consume(&mut self) -> bool {
        let now = Instant::now();
        self.last_seen = now;
        let elapsed = now.duration_since(self.updated_at).as_secs_f64();
        self.updated_at = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_second).min(self.burst);
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

async fn protect_api(
    State(state): State<ApiProtectionState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let path = request.uri().path().to_owned();
    if matches!(path.as_str(), "/health" | "/health/live") {
        let request_timeout = state.request_timeout().await;
        return match tokio::time::timeout(
            request_timeout.min(Duration::from_secs(5)),
            next.run(request),
        )
        .await
        {
            Ok(response) => response,
            Err(_) => api_rejection(
                StatusCode::REQUEST_TIMEOUT,
                "HEALTH_REQUEST_TIMEOUT",
                "the liveness request exceeded its timeout",
                request_id.as_deref(),
                true,
            ),
        };
    }
    let permit = match state.semaphore.try_acquire() {
        Some(permit) => permit,
        None => {
            return api_rejection(
                StatusCode::SERVICE_UNAVAILABLE,
                "API_OVERLOADED",
                "the API concurrency limit has been reached",
                request_id.as_deref(),
                true,
            );
        }
    };
    let identity = request
        .extensions()
        .get::<ApiIdentity>()
        .map(|identity| identity.0.as_str())
        .unwrap_or("local-anonymous");
    if path != "/health/ready" && !state.consume(identity).await {
        drop(permit);
        return api_rejection(
            StatusCode::TOO_MANY_REQUESTS,
            "API_RATE_LIMITED",
            "the API request rate limit for this client has been reached",
            request_id.as_deref(),
            true,
        );
    }

    let response = if path == "/v1/events" {
        next.run(request).await
    } else {
        let request_timeout = state.request_timeout().await;
        match tokio::time::timeout(request_timeout, next.run(request)).await {
            Ok(response) => response,
            Err(_) => api_rejection(
                StatusCode::REQUEST_TIMEOUT,
                "API_REQUEST_TIMEOUT",
                "the API request exceeded the configured timeout",
                request_id.as_deref(),
                true,
            ),
        }
    };
    drop(permit);
    response
}

fn api_rejection(
    status: StatusCode,
    code: &str,
    message: &str,
    request_id: Option<&str>,
    retryable: bool,
) -> Response {
    let mut response = (
        status,
        Json(serde_json::json!({
            "code": code,
            "message": message,
            "request_id": request_id,
            "retryable": retryable,
            "details": {},
        })),
    )
        .into_response();
    if matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE
    ) {
        response
            .headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from_static("1"));
    }
    response
}

#[derive(Clone)]
struct ApiIdentity(String);

#[derive(Clone)]
struct AuthState {
    global_token: Option<String>,
    repository: crate::storage::Repository,
}

async fn require_token(
    State(state): State<AuthState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    if matches!(path.as_str(), "/health" | "/health/live") {
        return next.run(request).await;
    }
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let browser_scoped = matches!(path.as_str(), "/v1/browser/sniff" | "/v1/browser/import");
    let origin = request
        .headers()
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    if browser_scoped && request.method() == Method::OPTIONS {
        let Some(origin) = origin
            .as_deref()
            .and_then(|value| crate::services::browser::normalize_origin(value).ok())
        else {
            return api_rejection(
                StatusCode::FORBIDDEN,
                "BROWSER_ORIGIN_INVALID",
                "the browser request origin is invalid or not allowed",
                request_id.as_deref(),
                false,
            );
        };
        return browser_cors_response(StatusCode::NO_CONTENT.into_response(), &origin);
    }

    let bearer = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let global_valid = state.global_token.as_deref().is_some_and(|expected| {
        bearer.is_some_and(|value| constant_time_eq(value.as_bytes(), expected.as_bytes()))
    });

    let browser_valid = if browser_scoped && !global_valid {
        match (bearer, origin.as_deref()) {
            (Some(token), Some(origin)) => state
                .repository
                .verify_browser_token(&crate::services::browser::hash_token(token), origin)
                .await
                .unwrap_or(false),
            _ => false,
        }
    } else {
        false
    };

    let allowed = if browser_scoped {
        global_valid || browser_valid
    } else {
        state.global_token.is_none() || global_valid
    };
    if !allowed {
        return api_rejection(
            StatusCode::UNAUTHORIZED,
            "AUTHENTICATION_REQUIRED",
            "a valid bearer token is required",
            request_id.as_deref(),
            false,
        );
    }

    let identity = if global_valid {
        format!(
            "global:{}",
            crate::services::browser::hash_token(bearer.unwrap_or_default())
        )
    } else if browser_valid {
        format!(
            "browser:{}",
            crate::services::browser::hash_token(bearer.unwrap_or_default())
        )
    } else {
        request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(address)| format!("client:{}", address.ip()))
            .unwrap_or_else(|| "local-anonymous".to_owned())
    };
    request
        .extensions_mut()
        .insert(ApiIdentity(identity.clone()));

    let response = next.run(request).await;
    if path.starts_with("/v1/")
        && matches!(
            method,
            Method::POST | Method::PATCH | Method::DELETE | Method::PUT
        )
    {
        let outcome = if response.status().is_success() {
            "success"
        } else {
            "failure"
        };
        let metadata = serde_json::json!({
            "actor": identity,
            "request_id": request_id,
            "method": method.as_str(),
            "status": response.status().as_u16(),
        });
        if let Err(error) = state
            .repository
            .append_audit_with_metadata("api.mutation", "api_route", Some(&path), outcome, metadata)
            .await
        {
            tracing::warn!(%error, %path, "failed to persist request-level audit record");
        }
    }
    if browser_scoped {
        if let Some(origin) = origin
            .as_deref()
            .and_then(|value| crate::services::browser::normalize_origin(value).ok())
        {
            return browser_cors_response(response, &origin);
        }
    }
    response
}

fn browser_cors_response(mut response: Response, origin: &str) -> Response {
    if let Ok(origin) = HeaderValue::from_str(origin) {
        response
            .headers_mut()
            .insert(ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
    response.headers_mut().insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("authorization, content-type"),
    );
    response.headers_mut().insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    response
        .headers_mut()
        .insert(VARY, HeaderValue::from_static("Origin"));
    response
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_bucket_enforces_burst_capacity() {
        let mut bucket = TokenBucket::new(2.0, 0.0);
        assert!(bucket.consume());
        assert!(bucket.consume());
        assert!(!bucket.consume());
    }

    #[test]
    fn constant_time_comparison_requires_equal_content_and_length() {
        assert!(constant_time_eq(b"token", b"token"));
        assert!(!constant_time_eq(b"token", b"other"));
        assert!(!constant_time_eq(b"token", b"token-longer"));
    }

    #[tokio::test]
    async fn protection_reconfiguration_updates_all_limits_and_resets_buckets() {
        let state = ApiProtectionState::new(2, 60, 2, Duration::from_secs(30));
        assert!(state.consume("client").await);
        assert!(state.consume("client").await);
        assert!(!state.consume("client").await);
        state.reconfigure(1, 1, 1, Duration::from_secs(7)).await;
        assert_eq!(state.request_timeout().await, Duration::from_secs(7));
        assert!(state.consume("client").await);
        assert!(!state.consume("client").await);
        let permit = state.semaphore.try_acquire().unwrap();
        assert!(state.semaphore.try_acquire().is_none());
        drop(permit);
    }
}
