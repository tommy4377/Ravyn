//! Replay-aware SSE bridge between the embedded backend and Firefox Native Messaging.

use super::{BackendDescriptor, PROTOCOL_VERSION, load_live_descriptor, write_message};
use serde_json::{Map, Value, json};
use std::io::BufRead;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static EVENT_STREAM_CONTROL: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// Starts the backend event proxy once. Repeated subscriptions reuse the live
/// worker instead of creating duplicate SSE readers and duplicate extension events.
pub(super) fn start_event_stream(client: &reqwest::blocking::Client) {
    let cancellation = {
        let mut control = EVENT_STREAM_CONTROL
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if control
            .as_ref()
            .is_some_and(|flag| !flag.load(Ordering::Acquire))
        {
            return;
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        *control = Some(cancellation.clone());
        cancellation
    };
    let fallback_client = client.clone();
    std::thread::spawn(move || {
        let event_client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .read_timeout(Duration::from_secs(2))
            .user_agent(format!("Ravyn-Native-Events/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or(fallback_client);
        run_event_stream(&event_client, &cancellation);
    });
}

pub(super) fn stop_event_stream() {
    if let Some(cancellation) = EVENT_STREAM_CONTROL
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
    {
        cancellation.store(true, Ordering::Release);
    }
}

fn run_event_stream(client: &reqwest::blocking::Client, cancellation: &AtomicBool) {
    let mut last_event_id: Option<String> = None;
    while !cancellation.load(Ordering::Acquire) {
        if let Ok(descriptor) = load_live_descriptor(client) {
            let _ = stream_events_once(
                client,
                &descriptor,
                cancellation,
                &mut last_event_id,
            );
        }
        for _ in 0..30 {
            if cancellation.load(Ordering::Acquire) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn stream_events_once(
    client: &reqwest::blocking::Client,
    descriptor: &BackendDescriptor,
    cancellation: &AtomicBool,
    last_event_id: &mut Option<String>,
) -> Result<(), String> {
    let mut request = client
        .get(format!("{}/v1/events", descriptor.base_url))
        .bearer_auth(&descriptor.api_token);
    if let Some(cursor) = last_event_id.as_deref() {
        request = request.header("Last-Event-ID", cursor);
    }
    let response = request.send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("event stream returned HTTP {}", response.status()));
    }

    let mut reader = std::io::BufReader::new(response);
    let mut data_lines: Vec<String> = Vec::new();
    let mut event_name: Option<String> = None;
    let mut event_id: Option<String> = None;
    loop {
        if cancellation.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut line = String::new();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if bytes_read == 0 || cancellation.load(Ordering::Acquire) {
            return Ok(());
        }
        let line = line.trim_end_matches(['\n', '\r']);
        if line.is_empty() {
            if !data_lines.is_empty() {
                let payload = data_lines.join("\n");
                if forward_event(event_name.as_deref(), event_id.as_deref(), &payload) {
                    if let Some(id) = event_id.take() {
                        *last_event_id = Some(id);
                    }
                }
                data_lines.clear();
            }
            event_name = None;
            event_id = None;
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start().to_owned());
            continue;
        }
        if let Some(name) = line.strip_prefix("event:") {
            let name = name.trim();
            if !name.is_empty() && name.len() <= 100 {
                event_name = Some(name.to_owned());
            }
            continue;
        }
        if let Some(id) = line.strip_prefix("id:") {
            let id = id.trim();
            // Ravyn sequence identifiers are u64 values. Restricting the
            // cursor also prevents arbitrary header data on reconnect.
            if id.parse::<u64>().is_ok() {
                event_id = Some(id.to_owned());
            }
        }
    }
}

fn forward_event(event_name: Option<&str>, event_id: Option<&str>, payload: &str) -> bool {
    let Ok(mut value) = serde_json::from_str::<Value>(payload) else {
        return false;
    };
    let event_type = event_name
        .filter(|name| *name != "message")
        .map(str::to_owned)
        .or_else(|| value.get("type").and_then(Value::as_str).map(str::to_owned));
    let Some(event_type) = event_type else {
        return false;
    };

    // Named SSE control events such as subscriber_lagged do not carry the
    // regular flattened event envelope. Add the type and numeric sequence
    // when available so extension consumers receive one stable payload shape.
    if let Value::Object(object) = &mut value {
        object
            .entry("type".to_owned())
            .or_insert_with(|| Value::String(event_type.clone()));
        if !object.contains_key("sequence") {
            if let Some(sequence) = event_id.and_then(|id| id.parse::<u64>().ok()) {
                object.insert("sequence".to_owned(), Value::Number(sequence.into()));
            }
        }
    } else {
        let mut object = Map::new();
        object.insert("type".to_owned(), Value::String(event_type.clone()));
        object.insert("data".to_owned(), value);
        if let Some(sequence) = event_id.and_then(|id| id.parse::<u64>().ok()) {
            object.insert("sequence".to_owned(), Value::Number(sequence.into()));
        }
        value = Value::Object(object);
    }

    write_message(&json!({
        "type": "event",
        "protocolVersion": PROTOCOL_VERSION,
        "event": event_type,
        "payload": value,
    }))
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_numeric_sse_cursors() {
        assert!("42".parse::<u64>().is_ok());
        assert!("42\r\nInjected: value".parse::<u64>().is_err());
    }
}
