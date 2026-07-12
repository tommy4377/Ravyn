use reqwest::{
    Client, StatusCode,
    header::{
        ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, ETAG, HeaderMap,
        LAST_MODIFIED, RANGE,
    },
};
use serde::Serialize;

use crate::error::{RavynError, Result};

pub enum ProbeResult {
    Metadata(RemoteMetadata),
    Redirect(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteMetadata {
    pub final_url: String,
    pub length: Option<u64>,
    pub range_supported: bool,
    pub content_type: Option<String>,
    pub content_disposition: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// Probes metadata and verifies byte-range support rather than trusting HEAD.
pub async fn probe(client: &Client, url: &str, headers: &HeaderMap) -> Result<ProbeResult> {
    let head = match client.head(url).headers(headers.clone()).send().await {
        Ok(response) if response.status().is_redirection() => {
            return Ok(ProbeResult::Redirect(redirect_location(&response)?));
        }
        Ok(response) if response.status().is_success() => Some(from_response(
            &response,
            header_u64(response.headers(), CONTENT_LENGTH),
            response
                .headers()
                .get(ACCEPT_RANGES)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.eq_ignore_ascii_case("bytes")),
        )),
        _ => None,
    };

    if let Some(metadata) = head.as_ref().filter(|metadata| metadata.range_supported) {
        return Ok(ProbeResult::Metadata(metadata.clone()));
    }

    let range_response = client
        .get(url)
        .headers(headers.clone())
        .header(RANGE, "bytes=0-0")
        .send()
        .await;
    match range_response {
        Ok(response) if response.status().is_redirection() => {
            Ok(ProbeResult::Redirect(redirect_location(&response)?))
        }
        Ok(response) if response.status() == StatusCode::PARTIAL_CONTENT => {
            let total = parse_total_length(
                response
                    .headers()
                    .get(CONTENT_RANGE)
                    .and_then(|value| value.to_str().ok()),
            )?;
            Ok(ProbeResult::Metadata(from_response(
                &response,
                Some(total),
                true,
            )))
        }
        Ok(response) if response.status().is_success() => {
            let mut metadata = from_response(
                &response,
                header_u64(response.headers(), CONTENT_LENGTH),
                false,
            );
            if metadata.length.is_none() {
                metadata.length = head.as_ref().and_then(|value| value.length);
            }
            Ok(ProbeResult::Metadata(metadata))
        }
        Ok(response) => head
            .map(ProbeResult::Metadata)
            .ok_or_else(|| RavynError::Protocol(format!("probe returned {}", response.status()))),
        Err(error) => head.map(ProbeResult::Metadata).ok_or_else(|| error.into()),
    }
}

fn redirect_location(response: &reqwest::Response) -> Result<String> {
    response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .ok_or_else(|| RavynError::Protocol("redirect response omitted Location".into()))
}

fn from_response(
    response: &reqwest::Response,
    length: Option<u64>,
    range_supported: bool,
) -> RemoteMetadata {
    let headers = response.headers();
    RemoteMetadata {
        final_url: response.url().to_string(),
        length,
        range_supported,
        content_type: headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        content_disposition: headers
            .get(CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        etag: headers
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        last_modified: headers
            .get(LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
    }
}

fn header_u64(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<u64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

fn parse_total_length(value: Option<&str>) -> Result<u64> {
    let value = value.ok_or_else(|| RavynError::Protocol("missing Content-Range".into()))?;
    value
        .rsplit('/')
        .next()
        .filter(|length| *length != "*")
        .and_then(|length| length.parse().ok())
        .ok_or_else(|| RavynError::Protocol(format!("invalid Content-Range: {value}")))
}
