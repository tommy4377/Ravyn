//! Native Messaging payload validation and protocol sanitization.

use super::*;

pub(super) fn sanitize_source_context(context: &SourceContext) -> Result<Value, HostError> {
    if context.browser != "firefox" {
        return Err(HostError::new(
            "INVALID_SOURCE_CONTEXT",
            "browser source must be Firefox",
            false,
        ));
    }

    // Preserve only the fact that a request came from private browsing. Page
    // identity and container metadata from an incognito window are deliberately
    // not persisted in the job database.
    if context.incognito {
        return Ok(json!({
            "browser": "firefox",
            "incognito": true,
            "container_id": null,
            "page_url": null,
            "page_title": null,
            "tab_id": null,
            "frame_id": null
        }));
    }

    let page_url = context
        .page_url
        .as_deref()
        .map(validate_optional_url)
        .transpose()?;
    let container_id = context
        .container_id
        .as_deref()
        .map(|value| sanitize_text(value, 200))
        .transpose()?;
    let page_title = context
        .page_title
        .as_deref()
        .map(|value| sanitize_text(value, 500))
        .transpose()?;
    Ok(json!({
        "browser": "firefox",
        "incognito": false,
        "container_id": container_id,
        "page_url": page_url,
        "page_title": page_title,
        "tab_id": context.tab_id,
        "frame_id": context.frame_id
    }))
}

pub(super) fn validate_network_url(value: &str) -> Result<String, HostError> {
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

pub(super) fn validate_optional_url(value: &str) -> Result<String, HostError> {
    validate_network_url(value)
}

pub(super) fn validate_uuid(value: Option<&str>) -> Result<Option<String>, HostError> {
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

pub(super) fn sanitize_filename(value: &str) -> Result<String, HostError> {
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

pub(super) fn sanitize_text(value: &str, max: usize) -> Result<String, HostError> {
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

pub(super) fn sanitize_tags(values: &[String]) -> Result<Vec<String>, HostError> {
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

#[derive(Debug, Clone)]
pub(super) struct SanitizedCookie {
    pub(super) name: String,
    pub(super) value: String,
    domain: String,
    path: String,
    secure: bool,
    http_only: bool,
    same_site: String,
    host_only: bool,
}

impl SanitizedCookie {
    pub(super) fn as_json(&self) -> Value {
        json!({
            "name": self.name,
            "value": self.value,
            "domain": self.domain,
            "path": self.path,
            "secure": self.secure,
            "http_only": self.http_only,
            "same_site": self.same_site,
            "host_only": self.host_only,
        })
    }
}

pub(super) fn sanitize_cookies(
    values: &[CookieValue],
    source: &str,
) -> Result<Vec<SanitizedCookie>, HostError> {
    if values.len() > MAX_COOKIES {
        return Err(HostError::new(
            "INVALID_COOKIES",
            format!("at most {MAX_COOKIES} cookies are accepted"),
            false,
        ));
    }
    let source_url = url::Url::parse(source).map_err(|_| {
        HostError::new("INVALID_COOKIES", "cookie source URL is invalid", false)
    })?;
    let source_host = source_url
        .host_str()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let source_path = source_url.path();
    let mut cookies = Vec::new();
    for cookie in values {
        let name = sanitize_text(&cookie.name, 256)?;
        let value = sanitize_text(&cookie.value, 4_096)?;
        let domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
        let domain_matches = if cookie.host_only {
            source_host == domain
        } else {
            source_host == domain || source_host.ends_with(&format!(".{domain}"))
        };
        let path = if cookie.path.starts_with('/') {
            sanitize_text(&cookie.path, 2_048)?
        } else {
            "/".to_owned()
        };
        let path_matches = source_path == path
            || source_path.starts_with(&path)
                && (path.ends_with('/')
                    || source_path
                        .as_bytes()
                        .get(path.len())
                        .is_some_and(|byte| *byte == b'/'));
        if name.is_empty()
            || !domain_matches
            || !path_matches
            || (cookie.secure && source_url.scheme() != "https")
        {
            continue;
        }
        cookies.push(SanitizedCookie {
            name,
            value,
            domain,
            path,
            secure: cookie.secure,
            http_only: cookie.http_only,
            same_site: sanitize_text(&cookie.same_site, 32)?,
            host_only: cookie.host_only,
        });
    }
    cookies.sort_by(|left, right| right.path.len().cmp(&left.path.len()));
    Ok(cookies)
}

pub(super) fn cookie_header(cookies: &[SanitizedCookie]) -> Option<String> {
    (!cookies.is_empty()).then(|| {
        cookies
            .iter()
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect::<Vec<_>>()
            .join("; ")
    })
}

pub(super) fn sanitize_media_options(value: &BrowserMediaOptions) -> Result<Value, HostError> {
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

pub(super) fn post_actions_for(preset: Option<&str>) -> Result<Vec<Value>, HostError> {
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

pub(super) fn sanitize_browser_intent(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "navigate" => Some("navigate"),
        "add_download" => Some("add_download"),
        "create_schedule" => Some("create_schedule"),
        "scan_page" => Some("scan_page"),
        _ => None,
    }
}

pub(super) fn sanitize_section(value: &str) -> &'static str {
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

pub(super) fn descriptor_path(data_dir: &Path) -> PathBuf {
    data_dir.join("runtime").join(DESCRIPTOR_FILE)
}

pub(super) fn unix_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

