use std::{collections::BTreeMap, sync::Arc};

use futures_util::StreamExt;
use reqwest::{Client, Response, redirect::Policy};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    config::Config,
    error::{RavynError, Result},
    services::security,
    storage::Repository,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SniffRequest {
    pub url: String,
    pub include_links: bool,
    pub include_images: bool,
    pub include_media: bool,
    pub include_scripts: bool,
    pub include_styles: bool,
    pub extensions: Vec<String>,
    pub only_new: bool,
    pub remember: bool,
    pub max_resources: Option<usize>,
}

impl Default for SniffRequest {
    fn default() -> Self {
        Self {
            url: String::new(),
            include_links: true,
            include_images: true,
            include_media: true,
            include_scripts: false,
            include_styles: false,
            extensions: Vec::new(),
            only_new: false,
            remember: false,
            max_resources: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Link,
    Image,
    Media,
    Script,
    Style,
    Object,
}

impl ResourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Image => "image",
            Self::Media => "media",
            Self::Script => "script",
            Self::Style => "style",
            Self::Object => "object",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniffedResource {
    pub url: String,
    pub kind: ResourceKind,
    pub extension: Option<String>,
    pub is_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniffResult {
    pub requested_url: String,
    pub page_url: String,
    pub resources: Vec<SniffedResource>,
    pub discovered: usize,
    pub returned: usize,
    pub existing_filtered: usize,
    pub truncated: bool,
}

#[derive(Clone)]
pub struct SnifferService {
    config: Arc<Config>,
    repository: Repository,
    client: Client,
}

impl SnifferService {
    pub fn new(config: Arc<Config>, repository: Repository) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(config.connect_timeout())
            .read_timeout(config.read_timeout())
            .redirect(Policy::none())
            .tcp_nodelay(true)
            .pool_max_idle_per_host(4)
            .build()?;
        Ok(Self {
            config,
            repository,
            client,
        })
    }

    async fn fetch_with_validated_redirects(&self, source: &str) -> Result<Response> {
        let mut current = Url::parse(source)?;
        for _ in 0..=10 {
            security::validate_network_source_resolved(&self.config, current.as_str()).await?;
            let response = self.client.get(current.clone()).send().await?;
            if !response.status().is_redirection() {
                return Ok(response);
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| RavynError::Protocol("redirect response omitted Location".into()))?;
            current = current.join(location)?;
        }
        Err(RavynError::Protocol(
            "page request exceeded the redirect limit".into(),
        ))
    }

    pub async fn sniff(&self, request: &SniffRequest) -> Result<SniffResult> {
        if request.extensions.len() > 128
            || request
                .extensions
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 32)
        {
            return Err(RavynError::Invalid(
                "extension filters must contain at most 128 non-empty values of 32 characters"
                    .into(),
            ));
        }
        let response = self.fetch_with_validated_redirects(&request.url).await?;
        if !response.status().is_success() {
            return Err(RavynError::Protocol(format!(
                "page request returned {}",
                response.status()
            )));
        }
        let final_url = response.url().clone();

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !content_type.is_empty()
            && !content_type.starts_with("text/html")
            && !content_type.starts_with("application/xhtml+xml")
        {
            return Err(RavynError::Invalid(format!(
                "resource is not HTML: {content_type}"
            )));
        }

        let maximum = self.config.max_html_mib.saturating_mul(1024 * 1024) as usize;
        if response
            .content_length()
            .is_some_and(|length| length > maximum as u64)
        {
            return Err(RavynError::Invalid(format!(
                "HTML response exceeds the {} MiB limit",
                self.config.max_html_mib
            )));
        }

        let mut body = Vec::with_capacity(response.content_length().unwrap_or(0) as usize);
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if body.len().saturating_add(chunk.len()) > maximum {
                return Err(RavynError::Invalid(format!(
                    "HTML response exceeds the {} MiB limit",
                    self.config.max_html_mib
                )));
            }
            body.extend_from_slice(&chunk);
        }
        let html = String::from_utf8_lossy(&body);
        let maximum_resources = request
            .max_resources
            .unwrap_or(self.config.max_sniff_resources)
            .clamp(1, self.config.max_sniff_resources);
        let (discovered, truncated) =
            extract_resources(&html, &final_url, request, maximum_resources)?;
        let discovered_count = discovered.len();
        let urls = discovered.keys().cloned().collect::<Vec<_>>();
        let existing = self
            .repository
            .existing_page_resources(final_url.as_str(), &urls)
            .await?;

        let mut existing_filtered = 0;
        let mut resources = Vec::with_capacity(discovered.len());
        let mut remembered = Vec::with_capacity(discovered.len());
        for (url, kind) in discovered {
            let existed = existing.contains(&url);
            if request.remember {
                remembered.push((url.clone(), kind.as_str().to_owned(), false));
            }
            if request.only_new && existed {
                existing_filtered += 1;
                continue;
            }
            resources.push(SniffedResource {
                extension: extension_of(&url),
                url,
                kind,
                is_new: !existed,
            });
        }
        if !remembered.is_empty() {
            self.repository
                .remember_page_resources(final_url.as_str(), &remembered)
                .await?;
        }

        Ok(SniffResult {
            requested_url: request.url.clone(),
            page_url: final_url.to_string(),
            returned: resources.len(),
            resources,
            discovered: discovered_count,
            existing_filtered,
            truncated,
        })
    }
}

fn extract_resources(
    html: &str,
    page_url: &Url,
    request: &SniffRequest,
    maximum: usize,
) -> Result<(BTreeMap<String, ResourceKind>, bool)> {
    let mut result = BTreeMap::new();
    let mut base_url = page_url.clone();
    for tag in tags(html) {
        let name = tag.name.as_str();
        if name == "base" {
            if let Some(href) = tag.attribute("href") {
                if let Ok(candidate) = page_url.join(href) {
                    if matches!(candidate.scheme(), "http" | "https") {
                        base_url = candidate;
                    }
                }
            }
            continue;
        }

        let mut candidates = Vec::new();
        match name {
            "a" if request.include_links => {
                push_attribute(&mut candidates, &tag, "href", ResourceKind::Link)
            }
            "img" if request.include_images => {
                push_attribute(&mut candidates, &tag, "src", ResourceKind::Image);
                push_srcset(&mut candidates, &tag, ResourceKind::Image);
            }
            "video" | "audio" | "source" | "track" if request.include_media => {
                push_attribute(&mut candidates, &tag, "src", ResourceKind::Media);
                push_srcset(&mut candidates, &tag, ResourceKind::Media);
            }
            "script" if request.include_scripts => {
                push_attribute(&mut candidates, &tag, "src", ResourceKind::Script)
            }
            "link" => {
                let rel = tag
                    .attribute("rel")
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if request.include_styles
                    && (rel.split_whitespace().any(|item| item == "stylesheet")
                        || rel.split_whitespace().any(|item| item == "preload"))
                {
                    push_attribute(&mut candidates, &tag, "href", ResourceKind::Style);
                } else if request.include_links {
                    push_attribute(&mut candidates, &tag, "href", ResourceKind::Link);
                }
            }
            "object" if request.include_media => {
                push_attribute(&mut candidates, &tag, "data", ResourceKind::Object)
            }
            "embed" if request.include_media => {
                push_attribute(&mut candidates, &tag, "src", ResourceKind::Object)
            }
            _ => {}
        }

        for (value, kind) in candidates {
            let Some(url) = resolve_resource(&base_url, value) else {
                continue;
            };
            if !request.extensions.is_empty() {
                let extension = extension_of(&url).unwrap_or_default();
                if !request.extensions.iter().any(|expected| {
                    expected
                        .trim_start_matches('.')
                        .eq_ignore_ascii_case(&extension)
                }) {
                    continue;
                }
            }
            if result.contains_key(&url) {
                continue;
            }
            if result.len() >= maximum {
                return Ok((result, true));
            }
            result.insert(url, kind);
        }
    }
    Ok((result, false))
}

fn resolve_resource(base: &Url, value: &str) -> Option<String> {
    let decoded = decode_html_attribute(value);
    let value = decoded.trim();
    if value.is_empty()
        || value.starts_with('#')
        || value.starts_with("data:")
        || value.starts_with("blob:")
        || value.starts_with("javascript:")
        || value.starts_with("mailto:")
    {
        return None;
    }
    let mut url = base.join(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.set_fragment(None);
    Some(url.to_string())
}

fn decode_html_attribute(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        output.push_str(&rest[..index]);
        rest = &rest[index..];
        let Some(end) = rest.find(';') else {
            output.push_str(rest);
            return output;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            value if value.starts_with("#x") || value.starts_with("#X") => {
                u32::from_str_radix(&value[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            value if value.starts_with('#') => {
                value[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        if let Some(character) = decoded {
            output.push(character);
        } else {
            output.push_str(&rest[..=end]);
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    output
}

fn extension_of(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    std::path::Path::new(url.path())
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .map(|extension| extension.to_ascii_lowercase())
}

fn push_attribute<'a>(
    output: &mut Vec<(&'a str, ResourceKind)>,
    tag: &'a HtmlTag,
    name: &str,
    kind: ResourceKind,
) {
    if let Some(value) = tag.attribute(name) {
        output.push((value, kind));
    }
}

fn push_srcset<'a>(
    output: &mut Vec<(&'a str, ResourceKind)>,
    tag: &'a HtmlTag,
    kind: ResourceKind,
) {
    let Some(srcset) = tag.attribute("srcset") else {
        return;
    };
    for candidate in srcset.split(',') {
        if let Some(url) = candidate.split_whitespace().next() {
            if !url.is_empty() {
                output.push((url, kind.clone()));
            }
        }
    }
}

#[derive(Debug)]
struct HtmlTag {
    name: String,
    attributes: Vec<(String, String)>,
}

impl HtmlTag {
    fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn tags(html: &str) -> Vec<HtmlTag> {
    let bytes = html.as_bytes();
    let mut tags = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(relative) = bytes[cursor..].iter().position(|byte| *byte == b'<') else {
            break;
        };
        let start = cursor + relative;
        if bytes.get(start + 1) == Some(&b'!') {
            if html[start..].starts_with("<!--") {
                if let Some(end) = html[start + 4..].find("-->") {
                    cursor = start + 4 + end + 3;
                    continue;
                }
            }
            cursor = start + 2;
            continue;
        }
        if matches!(bytes.get(start + 1), Some(b'/') | Some(b'?')) {
            cursor = start + 2;
            continue;
        }
        let mut quote = None;
        let mut end = start + 1;
        while end < bytes.len() {
            let byte = bytes[end];
            if let Some(current) = quote {
                if byte == current {
                    quote = None;
                }
            } else if byte == b'\'' || byte == b'"' {
                quote = Some(byte);
            } else if byte == b'>' {
                break;
            }
            end += 1;
        }
        if end >= bytes.len() {
            break;
        }
        if let Some(tag) = parse_tag(&html[start + 1..end]) {
            tags.push(tag);
        }
        cursor = end + 1;
    }
    tags
}

fn parse_tag(input: &str) -> Option<HtmlTag> {
    let bytes = input.as_bytes();
    let mut cursor = 0;
    skip_whitespace(bytes, &mut cursor);
    let name_start = cursor;
    while cursor < bytes.len() && is_name_byte(bytes[cursor]) {
        cursor += 1;
    }
    if cursor == name_start {
        return None;
    }
    let name = input[name_start..cursor].to_ascii_lowercase();
    let mut attributes = Vec::new();
    while cursor < bytes.len() {
        skip_whitespace(bytes, &mut cursor);
        if cursor >= bytes.len() || bytes[cursor] == b'/' {
            break;
        }
        let key_start = cursor;
        while cursor < bytes.len() && is_attribute_byte(bytes[cursor]) {
            cursor += 1;
        }
        if cursor == key_start {
            cursor += 1;
            continue;
        }
        let key = input[key_start..cursor].to_ascii_lowercase();
        skip_whitespace(bytes, &mut cursor);
        let mut value = String::new();
        if cursor < bytes.len() && bytes[cursor] == b'=' {
            cursor += 1;
            skip_whitespace(bytes, &mut cursor);
            if cursor < bytes.len() && matches!(bytes[cursor], b'\'' | b'"') {
                let quote = bytes[cursor];
                cursor += 1;
                let value_start = cursor;
                while cursor < bytes.len() && bytes[cursor] != quote {
                    cursor += 1;
                }
                value = input[value_start..cursor].to_owned();
                if cursor < bytes.len() {
                    cursor += 1;
                }
            } else {
                let value_start = cursor;
                while cursor < bytes.len()
                    && !bytes[cursor].is_ascii_whitespace()
                    && bytes[cursor] != b'/'
                {
                    cursor += 1;
                }
                value = input[value_start..cursor].to_owned();
            }
        }
        attributes.push((key, value));
    }
    Some(HtmlTag { name, attributes })
}

fn skip_whitespace(bytes: &[u8], cursor: &mut usize) {
    while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':' | b'_')
}

fn is_attribute_byte(byte: u8) -> bool {
    is_name_byte(byte) || byte == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SniffRequest {
        SniffRequest::default()
    }

    #[test]
    fn extracts_and_resolves_common_resources() {
        let html = r#"
            <base href="https://cdn.example.com/assets/">
            <a href="../archive.zip#section">Download</a>
            <img src="cover.png" srcset="small.webp 1x, large.webp 2x">
            <video src="movie.mp4"></video>
        "#;
        let base = Url::parse("https://example.com/page").unwrap();
        let (resources, truncated) = extract_resources(html, &base, &request(), 100).unwrap();
        assert!(!truncated);
        assert!(resources.contains_key("https://cdn.example.com/archive.zip"));
        assert!(resources.contains_key("https://cdn.example.com/assets/cover.png"));
        assert!(resources.contains_key("https://cdn.example.com/assets/large.webp"));
        assert!(resources.contains_key("https://cdn.example.com/assets/movie.mp4"));
    }

    #[test]
    fn ignores_javascript_and_data_urls() {
        let html = r#"<a href="javascript:alert(1)"></a><img src="data:image/png;base64,x">"#;
        let base = Url::parse("https://example.com/").unwrap();
        let (resources, truncated) = extract_resources(html, &base, &request(), 100).unwrap();
        assert!(resources.is_empty());
        assert!(!truncated);
    }

    #[test]
    fn filters_extensions_without_dots() {
        let mut request = request();
        request.extensions = vec!["zip".into()];
        let base = Url::parse("https://example.com/").unwrap();
        let (resources, truncated) = extract_resources(
            r#"<a href="one.zip"></a><a href="two.mp4"></a>"#,
            &base,
            &request,
            100,
        )
        .unwrap();
        assert_eq!(resources.len(), 1);
        assert!(!truncated);
    }

    #[test]
    fn reports_truncation_only_when_an_extra_resource_exists() {
        let base = Url::parse("https://example.com/").unwrap();
        let (exact, exact_truncated) = extract_resources(
            r#"<a href="one"></a><a href="two"></a>"#,
            &base,
            &request(),
            2,
        )
        .unwrap();
        assert_eq!(exact.len(), 2);
        assert!(!exact_truncated);

        let (limited, limited_truncated) = extract_resources(
            r#"<a href="one"></a><a href="two"></a><a href="three"></a>"#,
            &base,
            &request(),
            2,
        )
        .unwrap();
        assert_eq!(limited.len(), 2);
        assert!(limited_truncated);
    }
}
