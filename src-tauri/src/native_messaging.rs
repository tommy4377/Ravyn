//! Restricted Firefox native-messaging protocol for the Ravyn extension.
//!
//! Firefox launches the installed `Ravyn.exe` as a short-lived stdio host. The
//! host discovers the authenticated desktop backend through a per-user runtime
//! descriptor, validates every command, and exposes only browser-safe actions.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PROTOCOL_VERSION: u32 = 1;
const MAX_MESSAGE_BYTES: usize = 1_048_576;
const MAX_BATCH_ITEMS: usize = 1_000;
const MAX_COOKIES: usize = 500;
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
fn restrict_file_to_current_user(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to restrict the native bridge descriptor: {error}"))
}

#[cfg(not(unix))]
fn restrict_file_to_current_user(_path: &Path) -> Result<(), String> {
    // The file lives below the current user's local application-data folder,
    // which inherits the per-user ACL on Windows.
    Ok(())
}

pub fn try_handle_command_line() -> bool {
    let arguments = std::env::args().collect::<Vec<_>>();
    if !is_native_host_invocation(&arguments) {
        return false;
    }
    if let Err(error) = run_host() {
        let response = NativeResponse::error("startup", "NATIVE_HOST_FAILED", &error, false);
        let _ = write_message(&response);
    }
    true
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
    if request.protocol_version != PROTOCOL_VERSION {
        return NativeResponse::error(
            &request.id,
            "PROTOCOL_MISMATCH",
            "the extension and native host use incompatible protocol versions",
            false,
        );
    }
    let result = match request.command.as_str() {
        "ping" => Ok(json!({ "pong": true, "hostVersion": env!("CARGO_PKG_VERSION") })),
        "get_capabilities" => get_capabilities(client),
        "open_ravyn" => open_ravyn(client, &request.payload),
        "subscribe_events" => Ok(json!({ "subscribed": true, "transport": "request-refresh" })),
        "unsubscribe_events" => Ok(json!({ "subscribed": false })),
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
        "get_rules" => get_rules(client, descriptor),
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
    validate_source_context(&payload.source_context)?;
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
            "cookies": cookies,
            "user_agent": user_agent,
            "referer": referer,
            "tags": tags,
            "initially_paused": payload.paused,
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
                results.push(json!({ "ok": true, "job": job }));
            }
            Err(error) => results.push(json!({
                "ok": false,
                "error": { "code": error.code, "message": error.message, "retryable": error.retryable }
            })),
        }
    }
    Ok(
        json!({ "attempted": downloads.len(), "accepted": accepted, "failed": downloads.len() - accepted, "results": results }),
    )
}

fn probe_media(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    payload: &Value,
) -> Result<Value, HostError> {
    let url = payload.get("url").and_then(Value::as_str).ok_or_else(|| {
        HostError::new("INVALID_MEDIA_PROBE", "media probe requires a URL", false)
    })?;
    let url = validate_network_url(url)?;
    api_request(
        client,
        descriptor,
        reqwest::Method::POST,
        "/v1/media/probe",
        Some(json!({
            "url": url,
            "cookies_from_browser": null,
            "cookies_file": null,
            "proxy": null
        })),
        None,
    )
}

fn download_summary(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
) -> Result<Value, HostError> {
    let page = api_request(
        client,
        descriptor,
        reqwest::Method::GET,
        "/v1/jobs?limit=20",
        None,
        None,
    )?;
    let items = page
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut active = 0usize;
    let mut queued = 0usize;
    let recent = items.iter().take(8).map(|job| {
        let status = job.get("status").and_then(Value::as_str).unwrap_or("unknown");
        if matches!(status, "downloading" | "probing" | "verifying" | "post_processing" | "seeding") { active += 1; }
        if status == "queued" { queued += 1; }
        let downloaded = job.get("downloaded_bytes").and_then(Value::as_i64).unwrap_or(0).max(0) as f64;
        let total = job.get("total_bytes").and_then(Value::as_i64).filter(|value| *value > 0).map(|value| value as f64);
        let progress = total.map(|total| (downloaded / total).clamp(0.0, 1.0));
        json!({
            "id": job.get("id").and_then(Value::as_str).unwrap_or_default(),
            "filename": job.get("filename").and_then(Value::as_str).unwrap_or_else(|| job.get("source").and_then(Value::as_str).unwrap_or("Download")),
            "status": status,
            "progress": progress,
            "speedBps": null
        })
    }).collect::<Vec<_>>();
    Ok(json!({ "active": active, "queued": queued, "speedBps": 0, "recent": recent }))
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
) -> Result<Value, HostError> {
    let page = api_request(
        client,
        descriptor,
        reqwest::Method::GET,
        "/v1/rules?limit=1000",
        None,
        None,
    )?;
    let rules = page
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(Value::Array(rules.into_iter().map(|rule| json!({
        "id": rule.get("id").and_then(Value::as_str).unwrap_or_default(),
        "name": rule.get("name").and_then(Value::as_str).unwrap_or("Rule"),
        "priority": rule.get("priority").and_then(Value::as_i64).unwrap_or(0),
        "enabled": rule.get("enabled").and_then(Value::as_bool).unwrap_or(false),
        "domains": rule.pointer("/matcher/domains").cloned().unwrap_or_else(|| json!([])),
        "extensions": rule.pointer("/matcher/extensions").cloned().unwrap_or_else(|| json!([])),
        "mimePatterns": rule.pointer("/matcher/mime_types").cloned().unwrap_or_else(|| json!([])),
        "urlRegex": rule.pointer("/matcher/url_regex").cloned().unwrap_or(Value::Null),
        "action": "ravyn"
    })).collect()))
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

fn open_ravyn(client: &reqwest::blocking::Client, payload: &Value) -> Result<Value, HostError> {
    let section = sanitize_section(
        payload
            .get("section")
            .and_then(Value::as_str)
            .unwrap_or("downloads"),
    );
    let source = payload
        .get("sourceUrl")
        .or_else(|| payload.get("source_url"))
        .and_then(Value::as_str)
        .map(validate_optional_url)
        .transpose()?;
    let action = crate::browser_integration::BrowserAction {
        section: Some(section.into()),
        source_url: source.clone(),
    };
    if let Ok(descriptor) = load_live_descriptor(client) {
        crate::browser_integration::publish_action(&action)
            .map_err(|error| HostError::new("APP_ACTION_FAILED", error, true))?;
        focus_existing_process(descriptor.process_id);
    } else {
        launch_desktop(Some((section, source.as_deref())))?;
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

fn launch_desktop(action: Option<(&str, Option<&str>)>) -> Result<(), HostError> {
    let executable = std::env::current_exe().map_err(|error| {
        HostError::new(
            "APP_LAUNCH_FAILED",
            format!("failed to resolve Ravyn: {error}"),
            true,
        )
    })?;
    let mut command = std::process::Command::new(&executable);
    if let Some((section, source)) = action {
        command.arg("--browser-action");
        command.arg(format!("--browser-section={}", sanitize_section(section)));
        if let Some(source) = source.and_then(|value| validate_optional_url(value).ok()) {
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
    if descriptor.schema != 1
        || descriptor.api_token.len() < 20
        || descriptor.data_dir != data_dir.display().to_string()
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

fn validate_source_context(context: &SourceContext) -> Result<(), HostError> {
    if context.browser != "firefox" {
        return Err(HostError::new(
            "INVALID_SOURCE_CONTEXT",
            "browser source must be Firefox",
            false,
        ));
    }
    if let Some(url) = context.page_url.as_deref() {
        validate_optional_url(url)?;
    }
    if let Some(value) = context.container_id.as_deref() {
        sanitize_text(value, 200)?;
    }
    if let Some(value) = context.page_title.as_deref() {
        sanitize_text(value, 500)?;
    }
    let _ = (context.incognito, context.tab_id, context.frame_id);
    Ok(())
}

fn validate_network_url(value: &str) -> Result<String, HostError> {
    let parsed = url::Url::parse(value)
        .map_err(|_| HostError::new("INVALID_URL", "download URL is invalid", false))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.username() != ""
        || parsed.password().is_some()
    {
        return Err(HostError::new(
            "INVALID_URL",
            "only credential-free HTTP and HTTPS URLs are accepted",
            false,
        ));
    }
    if value.len() > 16_384 {
        return Err(HostError::new(
            "INVALID_URL",
            "download URL is too long",
            false,
        ));
    }
    Ok(parsed.to_string())
}

fn validate_optional_url(value: &str) -> Result<String, HostError> {
    validate_network_url(value)
}

fn validate_uuid(value: Option<&str>) -> Result<Option<String>, HostError> {
    value
        .map(|value| {
            uuid::Uuid::parse_str(value)
                .map(|id| id.to_string())
                .map_err(|_| {
                    HostError::new("INVALID_IDENTIFIER", "identifier must be a UUID", false)
                })
        })
        .transpose()
}

fn sanitize_filename(value: &str) -> Result<String, HostError> {
    let value = sanitize_text(value, 255)?;
    if value.is_empty()
        || value == "."
        || value == ".."
        || value
            .chars()
            .any(|character| matches!(character, '/' | '\\' | '\0'))
    {
        return Err(HostError::new(
            "INVALID_FILENAME",
            "filename contains invalid path characters",
            false,
        ));
    }
    Ok(value)
}

fn sanitize_text(value: &str, max: usize) -> Result<String, HostError> {
    let trimmed = value.trim();
    if trimmed.len() > max || trimmed.chars().any(char::is_control) {
        return Err(HostError::new(
            "INVALID_TEXT",
            format!("text value exceeds {max} characters or contains control characters"),
            false,
        ));
    }
    Ok(trimmed.to_owned())
}

fn sanitize_tags(values: &[String]) -> Result<Vec<String>, HostError> {
    if values.len() > 50 {
        return Err(HostError::new(
            "INVALID_TAGS",
            "at most 50 tags are accepted",
            false,
        ));
    }
    let mut tags = values
        .iter()
        .map(|value| sanitize_text(value, 64))
        .collect::<Result<Vec<_>, _>>()?;
    tags.retain(|value| !value.is_empty());
    tags.sort();
    tags.dedup();
    Ok(tags)
}

fn sanitize_cookies(
    values: &[CookieValue],
    source: &str,
) -> Result<BTreeMap<String, String>, HostError> {
    if values.len() > MAX_COOKIES {
        return Err(HostError::new(
            "INVALID_COOKIES",
            format!("at most {MAX_COOKIES} cookies are accepted"),
            false,
        ));
    }
    let source_host = url::Url::parse(source)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
    let mut cookies = BTreeMap::new();
    for cookie in values {
        let name = sanitize_text(&cookie.name, 256)?;
        let value = sanitize_text(&cookie.value, 4_096)?;
        let domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
        if name.is_empty()
            || !(source_host == domain || source_host.ends_with(&format!(".{domain}")))
        {
            continue;
        }
        let _ = (
            &cookie.path,
            cookie.secure,
            cookie.http_only,
            &cookie.same_site,
        );
        cookies.insert(name, value);
    }
    Ok(cookies)
}

fn sanitize_media_options(value: &BrowserMediaOptions) -> Result<Value, HostError> {
    let format = value
        .format
        .as_deref()
        .map(|value| sanitize_text(value, 200))
        .transpose()?;
    let audio_format = value
        .audio_format
        .as_deref()
        .map(|value| sanitize_text(value, 20))
        .transpose()?;
    let subtitle_languages = value
        .subtitle_languages
        .as_deref()
        .unwrap_or_default()
        .iter()
        .take(50)
        .map(|value| sanitize_text(value, 32))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "format": format,
        "max_height": value.max_height.map(|height| height.clamp(144, 8640)),
        "audio_only": value.audio_only.unwrap_or(false),
        "audio_format": audio_format,
        "write_subtitles": value.write_subtitles.unwrap_or(false),
        "subtitle_languages": subtitle_languages
    }))
}

fn post_actions_for(preset: Option<&str>) -> Result<Vec<Value>, HostError> {
    let Some(preset) = preset else {
        return Ok(Vec::new());
    };
    let (extension, ffmpeg_preset) = match preset {
        "image-webp" => ("webp", "image-webp"),
        "image-avif" => ("avif", "image-avif"),
        "audio-mp3" => ("mp3", "audio-mp3"),
        "audio-opus" => ("opus", "audio-opus"),
        "video-h264" => ("mp4", "video-h264"),
        "video-h265" => ("mkv", "video-h265"),
        _ => {
            return Err(HostError::new(
                "INVALID_POST_PROCESSING",
                "unsupported browser post-processing preset",
                false,
            ));
        }
    };
    Ok(vec![json!({
        "type": "convert_media",
        "extension": extension,
        "preset": ffmpeg_preset,
        "arguments": [],
        "unsafe_arguments": false,
        "delete_original": false
    })])
}

fn sanitize_section(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "library" => "library",
        "media" => "media",
        "torrents" => "torrents",
        "automation" => "automation",
        "components" => "components",
        "settings" => "settings",
        _ => "downloads",
    }
}

fn descriptor_path(data_dir: &Path) -> PathBuf {
    data_dir.join("runtime").join(DESCRIPTOR_FILE)
}

fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
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
            },
            CookieValue {
                name: "foreign".into(),
                value: "blocked".into(),
                domain: "attacker.invalid".into(),
                path: "/".into(),
                secure: true,
                http_only: false,
                same_site: "none".into(),
            },
        ];
        let sanitized = sanitize_cookies(&cookies, "https://media.example.com/file").unwrap();
        assert_eq!(
            sanitized.get("session").map(String::as_str),
            Some("allowed")
        );
        assert!(!sanitized.contains_key("foreign"));
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
}
