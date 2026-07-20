//! Restricted Firefox native-messaging protocol for the Ravyn extension.
//!
//! Firefox launches the installed `Ravyn.exe` as a short-lived stdio host. The
//! host discovers the authenticated desktop backend through a per-user runtime
//! descriptor, validates every command, and exposes only browser-safe actions.

mod event_stream;
mod validation;

use validation::*;

use event_stream::{start_event_stream, stop_event_stream};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PROTOCOL_VERSION: u32 = 2;
/// Oldest extension protocol this host still accepts. The extension and the
/// desktop application update on independent cadences (AMO vs. the app
/// updater), so version skew is a normal condition — requests inside the
/// window are served, requests outside fail with an explicit
/// `PROTOCOL_MISMATCH` naming the supported range.
const MIN_PROTOCOL_VERSION: u32 = 2;
const MAX_MESSAGE_BYTES: usize = 1_048_576;
const MAX_BATCH_ITEMS: usize = 50;
const RULE_PAGE_SIZE: usize = 25;
const MAX_RULE_RESPONSE_BYTES: usize = 900_000;
const MAX_COOKIES: usize = 100;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const BACKEND_START_TIMEOUT: Duration = Duration::from_secs(20);
const DESCRIPTOR_FILE: &str = "native-bridge.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendDescriptor {
    schema: u32,
    process_id: u32,
    version: String,
    base_url: String,
    api_token: String,
    data_dir: String,
    written_at_unix_ms: u128,
}

pub struct BackendDescriptorGuard {
    path: PathBuf,
    process_id: u32,
}

impl BackendDescriptorGuard {
    pub fn publish(info: &crate::backend::BackendInfo) -> Result<Self, String> {
        let path = descriptor_path(Path::new(&info.data_dir));
        let parent = path
            .parent()
            .ok_or_else(|| "the native bridge descriptor has no parent directory".to_owned())?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create the native bridge directory: {error}"))?;
        restrict_directory_to_current_user(parent)?;
        let descriptor = BackendDescriptor {
            schema: 1,
            process_id: std::process::id(),
            version: env!("CARGO_PKG_VERSION").into(),
            base_url: info.base_url.clone(),
            api_token: info.api_token.clone(),
            data_dir: info.data_dir.clone(),
            written_at_unix_ms: unix_time_ms(),
        };
        let bytes = serde_json::to_vec_pretty(&descriptor).map_err(|error| {
            format!("failed to serialize the native bridge descriptor: {error}")
        })?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes)
            .map_err(|error| format!("failed to write the native bridge descriptor: {error}"))?;
        restrict_file_to_current_user(&temporary)?;
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| {
                format!("failed to replace the native bridge descriptor: {error}")
            })?;
        }
        std::fs::rename(&temporary, &path)
            .map_err(|error| format!("failed to activate the native bridge descriptor: {error}"))?;
        Ok(Self {
            path,
            process_id: descriptor.process_id,
        })
    }
}

impl Drop for BackendDescriptorGuard {
    fn drop(&mut self) {
        let remove = std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<BackendDescriptor>(&bytes).ok())
            .is_some_and(|descriptor| descriptor.process_id == self.process_id);
        if remove {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
pub(crate) fn restrict_file_to_current_user(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to restrict the native bridge descriptor: {error}"))
}

#[cfg(unix)]
pub(crate) fn restrict_directory_to_current_user(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("failed to restrict the native bridge directory: {error}"))
}

#[cfg(windows)]
pub(crate) fn restrict_file_to_current_user(path: &Path) -> Result<(), String> {
    restrict_windows_acl(path, "file")
}

#[cfg(windows)]
pub(crate) fn restrict_directory_to_current_user(path: &Path) -> Result<(), String> {
    restrict_windows_acl(path, "directory")
}

#[cfg(windows)]
fn restrict_windows_acl(path: &Path, label: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let username = std::env::var("USERNAME")
        .map_err(|_| format!("failed to resolve the current Windows user for native bridge {label}"))?;
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let identity = if domain.trim().is_empty() {
        username
    } else {
        format!("{domain}\\{username}")
    };
    let grant = format!("{identity}:(F)");
    let output = std::process::Command::new("icacls")
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &grant])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("failed to configure native bridge {label} ACL: {error}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!(
            "failed to restrict native bridge {label} ACL{}",
            if message.is_empty() { String::new() } else { format!(": {message}") }
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn restrict_file_to_current_user(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn restrict_directory_to_current_user(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub fn try_handle_command_line() -> Option<i32> {
    let arguments = std::env::args().collect::<Vec<_>>();
    if !is_native_host_invocation(&arguments) {
        return None;
    }
    match run_host() {
        Ok(()) => Some(0),
        Err(error) => {
            let response = NativeResponse::error("startup", "NATIVE_HOST_FAILED", &error, false);
            let _ = write_message(&response);
            Some(1)
        }
    }
}

fn is_native_host_invocation(arguments: &[String]) -> bool {
    if arguments
        .iter()
        .skip(1)
        .any(|argument| argument == "--native-messaging-host")
    {
        return true;
    }
    arguments.iter().skip(1).any(|argument| {
        argument == crate::browser_integration::EXTENSION_ID
            || argument.ends_with(crate::browser_integration::HOST_MANIFEST_FILE)
    })
}

fn run_host() -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(4))
        .timeout(REQUEST_TIMEOUT)
        .user_agent(format!("Ravyn-Native-Host/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("failed to create the native host HTTP client: {error}"))?;
    let mut stdin = std::io::stdin().lock();
    loop {
        let Some(value) = read_message(&mut stdin)? else {
            return Ok(());
        };
        let request = match serde_json::from_value::<NativeRequest>(value) {
            Ok(request) => request,
            Err(error) => {
                write_message(&NativeResponse::error(
                    "unknown",
                    "INVALID_REQUEST",
                    &format!("invalid native request: {error}"),
                    false,
                ))?;
                continue;
            }
        };
        let response = handle_request(&client, request);
        write_message(&response)?;
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeRequest {
    id: String,
    protocol_version: u32,
    command: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Serialize)]
struct NativeResponse {
    id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<NativeError>,
}

#[derive(Debug, Serialize)]
struct NativeError {
    code: String,
    message: String,
    retryable: bool,
}

impl NativeResponse {
    fn success(id: &str, result: Value) -> Self {
        Self {
            id: id.into(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: &str, code: &str, message: &str, retryable: bool) -> Self {
        Self {
            id: id.into(),
            ok: false,
            result: None,
            error: Some(NativeError {
                code: code.into(),
                message: message.chars().take(1_000).collect(),
                retryable,
            }),
        }
    }
}

fn handle_request(client: &reqwest::blocking::Client, request: NativeRequest) -> NativeResponse {
    if request.id.is_empty() || request.id.len() > 200 {
        return NativeResponse::error(
            "unknown",
            "INVALID_REQUEST_ID",
            "request id must contain between 1 and 200 characters",
            false,
        );
    }
    if !(MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&request.protocol_version) {
        return NativeResponse::error(
            &request.id,
            "PROTOCOL_MISMATCH",
            &format!(
                "the extension speaks native protocol {} but this Ravyn supports {}–{}; update Ravyn or the extension",
                request.protocol_version, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION
            ),
            false,
        );
    }
    let result = match request.command.as_str() {
        "ping" => Ok(json!({ "pong": true, "hostVersion": env!("CARGO_PKG_VERSION") })),
        "get_capabilities" => get_capabilities(client),
        "open_ravyn" => open_ravyn(client, &request.payload),
        "subscribe_events" => {
            start_event_stream(client);
            Ok(json!({ "subscribed": true, "transport": "sse" }))
        }
        "unsubscribe_events" => {
            stop_event_stream();
            Ok(json!({ "subscribed": false }))
        },
        command @ ("create_download"
        | "create_batch"
        | "probe_media"
        | "get_download_summary"
        | "get_job"
        | "pause_job"
        | "resume_job"
        | "cancel_job"
        | "pause_all"
        | "resume_all"
        | "get_rules"
        | "list_presets"
        | "evaluate_url") => with_backend(client, |descriptor| {
            dispatch_backend(client, descriptor, command, &request.payload)
        }),
        command => Err(HostError::new(
            "UNKNOWN_COMMAND",
            format!("unsupported native command: {command}"),
            false,
        )),
    };
    match result {
        Ok(value) => NativeResponse::success(&request.id, value),
        Err(error) => {
            NativeResponse::error(&request.id, error.code, &error.message, error.retryable)
        }
    }
}

#[derive(Debug)]
struct HostError {
    code: &'static str,
    message: String,
    retryable: bool,
}

impl HostError {
    fn new(code: &'static str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

fn get_capabilities(client: &reqwest::blocking::Client) -> Result<Value, HostError> {
    let backend_connected = load_live_descriptor(client).is_ok();
    Ok(json!({
        "protocolVersion": PROTOCOL_VERSION,
        "minProtocolVersion": MIN_PROTOCOL_VERSION,
        "hostVersion": env!("CARGO_PKG_VERSION"),
        "backendConnected": backend_connected,
        "features": [
            "download-interception", "batch-import", "media-probe", "browser-rules",
            "job-control", "site-cookie-opt-in", "private-window-metadata", "page-resource-scanning"
        ]
    }))
}

fn with_backend<F>(client: &reqwest::blocking::Client, operation: F) -> Result<Value, HostError>
where
    F: FnOnce(&BackendDescriptor) -> Result<Value, HostError>,
{
    let descriptor = match load_live_descriptor(client) {
        Ok(descriptor) => descriptor,
        Err(_) => {
            launch_desktop(None)?;
            wait_for_backend(client)?
        }
    };
    operation(&descriptor)
}

fn dispatch_backend(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    command: &str,
    payload: &Value,
) -> Result<Value, HostError> {
    match command {
        "create_download" => create_download(client, descriptor, payload),
        "create_batch" => create_batch(client, descriptor, payload),
        "probe_media" => probe_media(client, descriptor, payload),
        "get_download_summary" => download_summary(client, descriptor),
        "get_job" => job_action(client, descriptor, payload, "get"),
        "pause_job" => job_action(client, descriptor, payload, "pause"),
        "resume_job" => job_action(client, descriptor, payload, "resume"),
        "cancel_job" => job_action(client, descriptor, payload, "cancel"),
        "pause_all" => bulk_action(client, descriptor, "pause"),
        "resume_all" => bulk_action(client, descriptor, "resume"),
        "get_rules" => get_rules(client, descriptor, payload),
        "list_presets" => list_presets(client, descriptor),
        "evaluate_url" => evaluate_url(client, descriptor, payload),
        _ => Err(HostError::new(
            "UNKNOWN_COMMAND",
            format!("unsupported native command: {command}"),
            false,
        )),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceContext {
    browser: String,
    #[serde(default)]
    container_id: Option<String>,
    incognito: bool,
    #[serde(default)]
    page_url: Option<String>,
    #[serde(default)]
    page_title: Option<String>,
    #[serde(default)]
    tab_id: Option<i64>,
    #[serde(default)]
    frame_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CookieValue {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    http_only: bool,
    same_site: String,
    #[serde(default)]
    host_only: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BrowserMediaOptions {
    format: Option<String>,
    max_height: Option<u32>,
    audio_only: Option<bool>,
    audio_format: Option<String>,
    write_subtitles: Option<bool>,
    subtitle_languages: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateDownloadPayload {
    url: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    paused: bool,
    #[serde(default)]
    priority: i32,
    #[serde(default)]
    preset_id: Option<String>,
    #[serde(default)]
    referer: Option<String>,
    #[serde(default)]
    user_agent: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    cookies: Vec<CookieValue>,
    #[serde(default)]
    media: Option<BrowserMediaOptions>,
    #[serde(default)]
    post_processing_preset: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    source_context: SourceContext,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProbeMediaPayload {
    url: String,
    #[serde(default)]
    cookies: Vec<CookieValue>,
    source_context: SourceContext,
}

fn create_download(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    payload: &Value,
) -> Result<Value, HostError> {
    let payload: CreateDownloadPayload =
        serde_json::from_value(payload.clone()).map_err(|error| {
            HostError::new(
                "INVALID_DOWNLOAD",
                format!("invalid download request: {error}"),
                false,
            )
        })?;
    let source_context = sanitize_source_context(&payload.source_context)?;
    let source = validate_network_url(&payload.url)?;
    let kind = match payload.kind.as_deref().unwrap_or("http") {
        "http" => "http",
        "media" => "media",
        _ => {
            return Err(HostError::new(
                "INVALID_DOWNLOAD_KIND",
                "download kind must be http or media",
                false,
            ));
        }
    };
    let filename = payload
        .filename
        .as_deref()
        .map(sanitize_filename)
        .transpose()?;
    let preset_id = validate_uuid(payload.preset_id.as_deref())?;
    let referer = payload
        .referer
        .as_deref()
        .map(validate_optional_url)
        .transpose()?;
    let user_agent = payload
        .user_agent
        .as_deref()
        .map(|value| sanitize_text(value, 512))
        .transpose()?;
    let tags = sanitize_tags(&payload.tags)?;
    let cookies = sanitize_cookies(&payload.cookies, &source)?;
    let browser_cookies = cookies.iter().map(SanitizedCookie::as_json).collect::<Vec<_>>();
    let media = payload
        .media
        .as_ref()
        .map(sanitize_media_options)
        .transpose()?;
    let post_actions = post_actions_for(payload.post_processing_preset.as_deref())?;
    let body = json!({
        "preset_id": preset_id,
        "kind": kind,
        "source": source,
        "destination": null,
        "filename": filename,
        "priority": payload.priority.clamp(-100, 100),
        "speed_limit_bps": null,
        "expected_sha256": null,
        "duplicate_policy": "allow",
        "options": {
            "headers": {},
            "cookies": {},
            "browser_cookies": browser_cookies,
            "user_agent": user_agent,
            "referer": referer,
            "tags": tags,
            "initially_paused": payload.paused,
            "source_context": source_context,
            "post_actions": post_actions,
            "media": media
        }
    });
    let idempotency = payload
        .idempotency_key
        .as_deref()
        .map(|value| sanitize_text(value, 200))
        .transpose()?;
    let job = api_request(
        client,
        descriptor,
        reqwest::Method::POST,
        "/v1/jobs",
        Some(body),
        idempotency.as_deref(),
    )?;
    Ok(job)
}

fn create_batch(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    payload: &Value,
) -> Result<Value, HostError> {
    let downloads = payload
        .get("downloads")
        .and_then(Value::as_array)
        .ok_or_else(|| HostError::new("INVALID_BATCH", "downloads must be an array", false))?;
    if downloads.is_empty() || downloads.len() > MAX_BATCH_ITEMS {
        return Err(HostError::new(
            "INVALID_BATCH",
            format!("batch size must be between 1 and {MAX_BATCH_ITEMS}"),
            false,
        ));
    }
    let mut accepted = 0usize;
    let mut results = Vec::with_capacity(downloads.len());
    for download in downloads {
        match create_download(client, descriptor, download) {
            Ok(job) => {
                accepted += 1;
                results.push(json!({
                    "ok": true,
                    "jobId": job.get("id").and_then(Value::as_str).unwrap_or_default(),
                }));
            }
            Err(error) => results.push(json!({
                "ok": false,
                "error": { "code": error.code, "message": error.message, "retryable": error.retryable }
            })),
        }
    }
    Ok(json!({
        "attempted": downloads.len(),
        "accepted": accepted,
        "failed": downloads.len() - accepted,
        "results": results,
    }))
}

fn probe_media(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    payload: &Value,
) -> Result<Value, HostError> {
    let payload: ProbeMediaPayload =
        serde_json::from_value(payload.clone()).map_err(|error| {
            HostError::new(
                "INVALID_MEDIA_PROBE",
                format!("invalid media probe request: {error}"),
                false,
            )
        })?;
    let _source_context = sanitize_source_context(&payload.source_context)?;
    let url = validate_network_url(&payload.url)?;
    let cookies = sanitize_cookies(&payload.cookies, &url)?;
    let cookie_header = cookie_header(&cookies);
    let probe = api_request(
        client,
        descriptor,
        reqwest::Method::POST,
        "/v1/media/probe",
        Some(json!({
            "url": url,
            "cookies": {},
            "cookie_header": cookie_header,
            "cookies_from_browser": null,
            "cookies_file": null,
            "proxy": null
        })),
        None,
    )?;
    let formats = probe
        .get("formats")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(json!({
        "title": probe.get("title").cloned().unwrap_or(Value::Null),
        "duration": probe.get("duration").cloned().unwrap_or(Value::Null),
        "formats": formats.into_iter().map(|format| json!({
            "formatId": format.get("format_id").and_then(Value::as_str).unwrap_or_default(),
            "extension": format.get("extension").cloned().unwrap_or(Value::Null),
            "width": format.get("width").cloned().unwrap_or(Value::Null),
            "height": format.get("height").cloned().unwrap_or(Value::Null),
            "fps": format.get("fps").cloned().unwrap_or(Value::Null),
            "videoCodec": format.get("video_codec").cloned().unwrap_or(Value::Null),
            "audioCodec": format.get("audio_codec").cloned().unwrap_or(Value::Null),
            "bitrateKbps": format.get("bitrate_kbps").cloned().unwrap_or(Value::Null),
            "audioBitrateKbps": format.get("audio_bitrate_kbps").cloned().unwrap_or(Value::Null),
            "filesize": format.get("filesize").cloned()
                .filter(|value| !value.is_null())
                .or_else(|| format.get("filesize_approx").cloned())
                .unwrap_or(Value::Null),
            "protocol": format.get("protocol").cloned().unwrap_or(Value::Null),
            "note": format.get("note").cloned().unwrap_or(Value::Null),
        })).collect::<Vec<_>>()
    }))
}

fn download_summary(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
) -> Result<Value, HostError> {
    let summary = api_request(
        client,
        descriptor,
        reqwest::Method::GET,
        "/v1/jobs/summary",
        None,
        None,
    )?;
    let recent = summary
        .get("recent")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|job| {
            json!({
                "id": job.get("id").and_then(Value::as_str).unwrap_or_default(),
                "filename": job.get("filename").and_then(Value::as_str).unwrap_or("Download"),
                "status": job.get("status").and_then(Value::as_str).unwrap_or("unknown"),
                "progress": job.get("progress").cloned().unwrap_or(Value::Null),
                "speedBps": job.get("speed_bps").and_then(Value::as_u64).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "active": summary.get("active").and_then(Value::as_u64).unwrap_or_default(),
        "queued": summary.get("queued").and_then(Value::as_u64).unwrap_or_default(),
        "speedBps": summary.get("speed_bps").and_then(Value::as_u64).unwrap_or_default(),
        "recent": recent,
    }))
}

fn job_action(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    payload: &Value,
    action: &str,
) -> Result<Value, HostError> {
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| HostError::new("INVALID_JOB_ID", "job command requires an id", false))?;
    let id = validate_uuid(Some(id))?
        .ok_or_else(|| HostError::new("INVALID_JOB_ID", "job id is invalid", false))?;
    if action == "get" {
        return api_request(
            client,
            descriptor,
            reqwest::Method::GET,
            &format!("/v1/jobs/{id}"),
            None,
            None,
        );
    }
    api_request(
        client,
        descriptor,
        reqwest::Method::POST,
        &format!("/v1/jobs/{id}/{action}"),
        Some(json!({})),
        None,
    )
}

fn bulk_action(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    action: &str,
) -> Result<Value, HostError> {
    api_request(
        client,
        descriptor,
        reqwest::Method::POST,
        "/v1/jobs/actions",
        Some(json!({ "ids": [], "action": action })),
        None,
    )
}

fn get_rules(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    payload: &Value,
) -> Result<Value, HostError> {
    let (backend_cursor, offset) = parse_rule_cursor(payload)?;
    let path = backend_cursor.as_ref().map_or_else(
        || format!("/v1/rules?limit={RULE_PAGE_SIZE}"),
        |cursor| format!("/v1/rules?limit={RULE_PAGE_SIZE}&cursor={cursor}"),
    );
    let page = api_request(
        client,
        descriptor,
        reqwest::Method::GET,
        &path,
        None,
        None,
    )?;
    let raw_items = page
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if offset > raw_items.len() {
        return Err(HostError::new(
            "INVALID_RULE_CURSOR",
            "rule cursor offset is outside the backend page",
            false,
        ));
    }
    let backend_next = page
        .get("next_cursor")
        .and_then(Value::as_str)
        .map(validate_rule_backend_cursor)
        .transpose()?;

    let mut items = Vec::new();
    for raw in raw_items.iter().skip(offset) {
        let mapped = browser_rule_value(raw)?;
        let consumed = offset + items.len() + 1;
        let provisional_next = if consumed < raw_items.len() {
            Some(encode_rule_chunk_cursor(backend_cursor.as_deref(), consumed))
        } else {
            backend_next.clone()
        };
        let mut candidate_items = items.clone();
        candidate_items.push(mapped.clone());
        let candidate = json!({
            "items": candidate_items,
            "nextCursor": provisional_next,
        });
        let encoded_len = serde_json::to_vec(&candidate)
            .map_err(|error| HostError::new("RULE_SERIALIZATION_FAILED", error.to_string(), true))?
            .len();
        if encoded_len > MAX_RULE_RESPONSE_BYTES {
            if items.is_empty() {
                return Err(HostError::new(
                    "RULE_TOO_LARGE",
                    "one browser rule exceeds the native messaging frame budget",
                    false,
                ));
            }
            break;
        }
        items.push(mapped);
    }

    let consumed = offset + items.len();
    let next_cursor = if consumed < raw_items.len() {
        Some(encode_rule_chunk_cursor(backend_cursor.as_deref(), consumed))
    } else {
        backend_next
    };
    Ok(json!({
        "items": items,
        "nextCursor": next_cursor,
    }))
}

fn parse_rule_cursor(payload: &Value) -> Result<(Option<String>, usize), HostError> {
    let Some(raw) = payload.get("cursor").and_then(Value::as_str) else {
        return Ok((None, 0));
    };
    let raw = sanitize_text(raw, 128)?;
    if let Some(rest) = raw.strip_prefix("r2:") {
        let (base, offset) = rest.rsplit_once(':').ok_or_else(|| {
            HostError::new("INVALID_RULE_CURSOR", "malformed rule chunk cursor", false)
        })?;
        let offset = offset.parse::<usize>().map_err(|_| {
            HostError::new("INVALID_RULE_CURSOR", "invalid rule chunk offset", false)
        })?;
        if offset > RULE_PAGE_SIZE {
            return Err(HostError::new(
                "INVALID_RULE_CURSOR",
                "rule chunk offset exceeds the backend page size",
                false,
            ));
        }
        let backend = if base == "-" {
            None
        } else {
            Some(validate_rule_backend_cursor(base)?)
        };
        return Ok((backend, offset));
    }
    Ok((Some(validate_rule_backend_cursor(&raw)?), 0))
}

fn validate_rule_backend_cursor(value: &str) -> Result<String, HostError> {
    if value.len() != 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(HostError::new(
            "INVALID_RULE_CURSOR",
            "rule cursor is not a valid opaque backend cursor",
            false,
        ));
    }
    Ok(value.to_ascii_lowercase())
}

fn encode_rule_chunk_cursor(backend_cursor: Option<&str>, offset: usize) -> String {
    format!("r2:{}:{offset}", backend_cursor.unwrap_or("-"))
}

fn browser_rule_value(rule: &Value) -> Result<Value, HostError> {
    let id = validate_uuid(rule.get("id").and_then(Value::as_str))?
        .ok_or_else(|| HostError::new("INVALID_RULE", "browser rule has no valid id", false))?;
    let name = sanitize_text(
        rule.get("name").and_then(Value::as_str).unwrap_or("Rule"),
        160,
    )?;
    let domains = sanitize_rule_matchers(rule.pointer("/matcher/domains"))?;
    let extensions = sanitize_rule_matchers(rule.pointer("/matcher/extensions"))?;
    let mime_patterns = sanitize_rule_matchers(rule.pointer("/matcher/mime_types"))?;
    let url_regex = rule
        .pointer("/matcher/url_regex")
        .and_then(Value::as_str)
        .map(|value| sanitize_text(value, 2_048))
        .transpose()?;
    Ok(json!({
        "id": id,
        "name": name,
        "priority": rule.get("priority").and_then(Value::as_i64).unwrap_or(0),
        "enabled": rule.get("enabled").and_then(Value::as_bool).unwrap_or(false),
        "domains": domains,
        "extensions": extensions,
        "mimePatterns": mime_patterns,
        "urlRegex": url_regex,
        "action": "ravyn"
    }))
}

fn sanitize_rule_matchers(value: Option<&Value>) -> Result<Vec<String>, HostError> {
    let Some(values) = value.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    if values.len() > 256 {
        return Err(HostError::new(
            "INVALID_RULE",
            "browser rule contains too many matcher values",
            false,
        ));
    }
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| {
                    HostError::new("INVALID_RULE", "rule matcher must be text", false)
                })
                .and_then(|value| sanitize_text(value, 255))
        })
        .collect()
}

fn evaluate_url(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    payload: &Value,
) -> Result<Value, HostError> {
    let url = payload.get("url").and_then(Value::as_str).ok_or_else(|| {
        HostError::new(
            "INVALID_RULE_INPUT",
            "rule evaluation requires a URL",
            false,
        )
    })?;
    let source = validate_network_url(url)?;
    let mime = payload
        .get("mime")
        .and_then(Value::as_str)
        .map(|value| sanitize_text(value, 200))
        .transpose()?;
    let extension = payload
        .get("extension")
        .and_then(Value::as_str)
        .map(|value| sanitize_text(value, 32))
        .transpose()?;
    api_request(
        client,
        descriptor,
        reqwest::Method::POST,
        "/v1/rules/preview",
        Some(json!({
            "request": {
                "preset_id": null,
                "kind": "http",
                "source": source,
                "destination": null,
                "filename": null,
                "priority": 0,
                "speed_limit_bps": null,
                "expected_sha256": null,
                "duplicate_policy": "allow",
                "options": {}
            },
            "mime": mime,
            "extension": extension
        })),
        None,
    )
}

fn list_presets(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
) -> Result<Value, HostError> {
    let presets = api_request(
        client,
        descriptor,
        reqwest::Method::GET,
        "/v1/presets",
        None,
        None,
    )?;
    let presets = presets.as_array().cloned().unwrap_or_default();
    Ok(Value::Array(
        presets
            .into_iter()
            .map(|preset| {
                json!({
                    "id": preset.get("id").and_then(Value::as_str).unwrap_or_default(),
                    "name": preset.get("name").and_then(Value::as_str).unwrap_or("Preset"),
                })
            })
            .collect(),
    ))
}

fn open_ravyn(client: &reqwest::blocking::Client, payload: &Value) -> Result<Value, HostError> {
    let section = sanitize_section(
        payload
            .get("section")
            .and_then(Value::as_str)
            .unwrap_or("downloads"),
    );
    let intent = payload
        .get("intent")
        .and_then(Value::as_str)
        .and_then(sanitize_browser_intent);
    let source = payload
        .get("sourceUrl")
        .or_else(|| payload.get("source_url"))
        .and_then(Value::as_str)
        .map(validate_optional_url)
        .transpose()?;
    let action = crate::browser_integration::BrowserAction {
        intent: intent.map(str::to_owned),
        section: Some(section.into()),
        source_url: source,
    };

    // Always launch a regular Ravyn process carrying the action. When the app
    // is already running, the Tauri single-instance plugin forwards these
    // arguments to the primary process and emits the browser-action event
    // immediately. This avoids the old disk-queue-only path that could strand
    // actions until another event or restart.
    let running_process = load_live_descriptor(client).ok().map(|descriptor| descriptor.process_id);
    launch_desktop(Some(&action))?;
    if let Some(process_id) = running_process {
        focus_existing_process(process_id);
    }
    Ok(json!({ "opened": true }))
}

#[cfg(windows)]
fn focus_existing_process(process_id: u32) {
    let script = format!(
        "$shell = New-Object -ComObject WScript.Shell; [void]$shell.AppActivate({process_id})"
    );
    let mut command = std::process::Command::new("powershell.exe");
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &script,
    ]);
    configure_detached_process(&mut command);
    let _ = command.spawn();
}

#[cfg(not(windows))]
fn focus_existing_process(_process_id: u32) {}

fn launch_desktop(action: Option<&crate::browser_integration::BrowserAction>) -> Result<(), HostError> {
    let executable = std::env::current_exe().map_err(|error| {
        HostError::new(
            "APP_LAUNCH_FAILED",
            format!("failed to resolve Ravyn: {error}"),
            true,
        )
    })?;
    let mut command = std::process::Command::new(&executable);
    if let Some(action) = action {
        command.arg("--browser-action");
        if let Some(intent) = action.intent.as_deref().and_then(sanitize_browser_intent) {
            command.arg(format!("--browser-intent={intent}"));
        }
        if let Some(section) = action.section.as_deref() {
            command.arg(format!("--browser-section={}", sanitize_section(section)));
        }
        if let Some(source) = action
            .source_url
            .as_deref()
            .and_then(|value| validate_optional_url(value).ok())
        {
            command.arg(format!(
                "--browser-source={}",
                percent_encoding::utf8_percent_encode(&source, percent_encoding::NON_ALPHANUMERIC)
            ));
        }
    } else {
        command.arg("--browser-bridge-start");
    }
    if let Some(parent) = executable.parent() {
        command.current_dir(parent);
    }
    configure_detached_process(&mut command);
    command.spawn().map(|_| ()).map_err(|error| {
        HostError::new(
            "APP_LAUNCH_FAILED",
            format!("failed to launch Ravyn: {error}"),
            true,
        )
    })
}

#[cfg(windows)]
fn configure_detached_process(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // DETACHED_PROCESS alone still lets a console flash briefly before the
    // spawned PowerShell script applies its own -WindowStyle Hidden; every
    // other PowerShell spawn in this codebase also sets CREATE_NO_WINDOW.
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS | CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_detached_process(_command: &mut std::process::Command) {}

static STDOUT_LOCK: Mutex<()> = Mutex::new(());

fn wait_for_backend(client: &reqwest::blocking::Client) -> Result<BackendDescriptor, HostError> {
    let deadline = std::time::Instant::now() + BACKEND_START_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if let Ok(descriptor) = load_live_descriptor(client) {
            return Ok(descriptor);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(HostError::new(
        "BACKEND_UNAVAILABLE",
        "Ravyn did not start its local backend in time",
        true,
    ))
}

fn load_live_descriptor(
    client: &reqwest::blocking::Client,
) -> Result<BackendDescriptor, HostError> {
    let data_dir = crate::backend::resolve_data_dir();
    let path = descriptor_path(&data_dir);
    let bytes = std::fs::read(&path).map_err(|error| {
        HostError::new(
            "BACKEND_UNAVAILABLE",
            format!("Ravyn backend descriptor is unavailable: {error}"),
            true,
        )
    })?;
    let descriptor: BackendDescriptor = serde_json::from_slice(&bytes).map_err(|error| {
        HostError::new(
            "BACKEND_DESCRIPTOR_INVALID",
            format!("invalid Ravyn backend descriptor: {error}"),
            true,
        )
    })?;
    // Path comparison (not string equality): the descriptor is written by the
    // desktop process and read by the Firefox-spawned host, whose environments
    // can express the same directory with different casing or separators — a
    // mismatch here used to brick the bridge until the file was deleted.
    if descriptor.schema != 1
        || descriptor.api_token.len() < 20
        || !crate::browser_integration::same_path(Path::new(&descriptor.data_dir), &data_dir)
    {
        return Err(HostError::new(
            "BACKEND_DESCRIPTOR_INVALID",
            "Ravyn backend descriptor failed validation",
            true,
        ));
    }
    let url = url::Url::parse(&descriptor.base_url).map_err(|_| {
        HostError::new(
            "BACKEND_DESCRIPTOR_INVALID",
            "Ravyn backend URL is invalid",
            true,
        )
    })?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(HostError::new(
            "BACKEND_DESCRIPTOR_INVALID",
            "Ravyn backend descriptor must be a credential-free IPv4 loopback origin",
            true,
        ));
    }
    let response = client
        .get(format!("{}/health/ready", descriptor.base_url))
        .bearer_auth(&descriptor.api_token)
        .send()
        .map_err(|error| {
            HostError::new(
                "BACKEND_UNAVAILABLE",
                format!("Ravyn backend is unreachable: {error}"),
                true,
            )
        })?;
    if !response.status().is_success() {
        return Err(HostError::new(
            "BACKEND_UNAVAILABLE",
            format!(
                "Ravyn backend readiness returned HTTP {}",
                response.status()
            ),
            true,
        ));
    }
    Ok(descriptor)
}

fn api_request(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
    idempotency_key: Option<&str>,
) -> Result<Value, HostError> {
    let mut request = client
        .request(method, format!("{}{}", descriptor.base_url, path))
        .bearer_auth(&descriptor.api_token);
    if let Some(key) = idempotency_key {
        request = request.header("Idempotency-Key", key);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().map_err(|error| {
        HostError::new(
            "BACKEND_REQUEST_FAILED",
            format!("Ravyn backend request failed: {error}"),
            true,
        )
    })?;
    let status = response.status();
    let bytes = response.bytes().map_err(|error| {
        HostError::new(
            "BACKEND_RESPONSE_INVALID",
            format!("failed to read the Ravyn backend response: {error}"),
            true,
        )
    })?;
    if !status.is_success() {
        let message = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|value| {
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| {
                String::from_utf8_lossy(&bytes)
                    .chars()
                    .take(1_000)
                    .collect()
            });
        return Err(HostError::new(
            "BACKEND_REJECTED",
            format!("Ravyn rejected the request with HTTP {status}: {message}"),
            status.is_server_error(),
        ));
    }
    if bytes.is_empty() {
        return Ok(json!({ "ok": true }));
    }
    serde_json::from_slice(&bytes).map_err(|error| {
        HostError::new(
            "BACKEND_RESPONSE_INVALID",
            format!("Ravyn returned invalid JSON: {error}"),
            true,
        )
    })
}

fn read_message(reader: &mut impl Read) -> Result<Option<Value>, String> {
    let mut length = [0u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(format!("failed to read the native message length: {error}")),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_MESSAGE_BYTES {
        return Err(format!(
            "native message length must be between 1 and {MAX_MESSAGE_BYTES} bytes"
        ));
    }
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|error| format!("failed to read the native message body: {error}"))?;
    ravyn::native_protocol::decode_json_body(&body, MAX_MESSAGE_BYTES).map(Some)
}

fn write_message(value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode the native response: {error}"))?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err("native response exceeds the protocol size limit".into());
    }
    // The event-stream thread and the main request/response loop both write
    // framed messages to the same stdout — without this lock their two-part
    // writes (length prefix, then body) could interleave and corrupt the
    // framing that the extension's length-prefixed reader depends on.
    let _guard = STDOUT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .and_then(|_| stdout.write_all(&bytes))
        .and_then(|_| stdout.flush())
        .map_err(|error| format!("failed to write the native response: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_firefox_native_host_arguments() {
        assert!(is_native_host_invocation(&[
            "Ravyn.exe".into(),
            r"C:\Users\Test\com.ravyn.download_manager.json".into(),
            crate::browser_integration::EXTENSION_ID.into(),
        ]));
        assert!(!is_native_host_invocation(&[
            "Ravyn.exe".into(),
            "--browser-action".into()
        ]));
    }

    #[test]
    fn native_message_round_trip_uses_little_endian_frame() {
        let body = br#"{"id":"one"}"#;
        let mut input = Vec::new();
        input.extend_from_slice(&(body.len() as u32).to_le_bytes());
        input.extend_from_slice(body);
        let value = read_message(&mut input.as_slice()).unwrap().unwrap();
        assert_eq!(value["id"], "one");
    }

    #[test]
    fn rejects_path_like_filenames() {
        assert!(sanitize_filename("../secret.txt").is_err());
        assert!(sanitize_filename("folder/file.txt").is_err());
        assert_eq!(sanitize_filename("file.txt").unwrap(), "file.txt");
    }

    #[test]
    fn rejects_invalid_native_frames() {
        assert!(read_message(&mut [0, 0, 0, 0].as_slice()).is_err());
        let oversized = ((MAX_MESSAGE_BYTES + 1) as u32).to_le_bytes();
        assert!(read_message(&mut oversized.as_slice()).is_err());
        let body = b"not-json";
        let mut frame = Vec::new();
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(body);
        assert!(read_message(&mut frame.as_slice()).is_err());
    }

    #[test]
    fn rejects_unsupported_urls_and_credentials() {
        assert!(validate_network_url("file:///tmp/value").is_err());
        assert!(validate_network_url("https://user:secret@example.com/file").is_err());
        assert!(validate_network_url("https://example.com/file").is_ok());
    }

    #[test]
    fn cookie_forwarding_is_limited_to_the_source_host() {
        let cookies = vec![
            CookieValue {
                name: "session".into(),
                value: "allowed".into(),
                domain: ".example.com".into(),
                path: "/".into(),
                secure: true,
                http_only: true,
                same_site: "lax".into(),
                host_only: false,
            },
            CookieValue {
                name: "foreign".into(),
                value: "blocked".into(),
                domain: "attacker.invalid".into(),
                path: "/".into(),
                secure: true,
                http_only: false,
                same_site: "none".into(),
                host_only: true,
            },
        ];
        let sanitized = sanitize_cookies(&cookies, "https://media.example.com/file").unwrap();
        assert_eq!(sanitized.len(), 1);
        assert_eq!(sanitized[0].name, "session");
        assert_eq!(sanitized[0].value, "allowed");
    }

    #[test]
    fn post_processing_uses_only_named_presets() {
        assert!(post_actions_for(Some("video-h264")).is_ok());
        assert!(post_actions_for(Some("-i input -f rawvideo")).is_err());
    }

    #[test]
    fn unknown_commands_are_rejected_without_backend_dispatch() {
        let client = reqwest::blocking::Client::new();
        let response = handle_request(
            &client,
            NativeRequest {
                id: "request-one".into(),
                protocol_version: PROTOCOL_VERSION,
                command: "execute_shell".into(),
                payload: json!({}),
            },
        );
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "UNKNOWN_COMMAND");
    }

    #[test]
    fn protocol_versions_outside_the_supported_window_are_rejected() {
        let client = reqwest::blocking::Client::new();
        for version in [MIN_PROTOCOL_VERSION - 1, PROTOCOL_VERSION + 1] {
            let response = handle_request(
                &client,
                NativeRequest {
                    id: "request-proto".into(),
                    protocol_version: version,
                    command: "ping".into(),
                    payload: json!({}),
                },
            );
            assert!(!response.ok);
            let error = response.error.unwrap();
            assert_eq!(error.code, "PROTOCOL_MISMATCH");
            assert!(
                error
                    .message
                    .contains(&format!("{MIN_PROTOCOL_VERSION}\u{2013}{PROTOCOL_VERSION}"))
            );
        }
        let response = handle_request(
            &client,
            NativeRequest {
                id: "request-proto-ok".into(),
                protocol_version: PROTOCOL_VERSION,
                command: "ping".into(),
                payload: json!({}),
            },
        );
        assert!(response.ok);
    }
}
