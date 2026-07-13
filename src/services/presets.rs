//! Download preset application with explicit request values taking precedence.

use std::{collections::BTreeMap, path::PathBuf};

use chrono::{Datelike, Utc};

use crate::{
    core::models::{CreateJob, DownloadOptions, DuplicatePolicy},
    error::{RavynError, Result},
    storage::DownloadPreset,
};

/// Applies a preset and returns any relative subdirectory rendered by its filename template.
pub fn apply(preset: &DownloadPreset, request: &mut CreateJob) -> Result<Option<PathBuf>> {
    let payload = &preset.payload;
    if request.destination.is_none() {
        request.destination = payload.destination.clone();
    }
    if request.priority == 0 {
        if let Some(priority) = payload.priority {
            request.priority = priority;
        }
    }
    if request.speed_limit_bps.is_none() {
        request.speed_limit_bps = payload.speed_limit_bps;
    }
    if request.duplicate_policy == DuplicatePolicy::Allow {
        if let Some(policy) = payload.duplicate_policy {
            request.duplicate_policy = policy;
        }
    }
    if let Some(options) = payload.options.as_ref() {
        merge_options(&mut request.options, options)?;
    }

    let Some(template) = payload.filename_template.as_deref() else {
        return Ok(None);
    };
    if request.filename.is_some() {
        return Ok(None);
    }
    let mut variables = builtin_variables(request);
    variables.extend(payload.template_variables.clone());
    let preview = crate::services::library::render_template(template, &variables)?;
    if !preview.missing_variables.is_empty() {
        return Err(RavynError::Invalid(format!(
            "preset {} is missing template variables: {}",
            preset.name,
            preview.missing_variables.join(", ")
        )));
    }
    let filename = preview
        .rendered
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| RavynError::Invalid("preset template rendered no filename".into()))?
        .to_owned();
    let parent = preview
        .rendered
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .map(PathBuf::from);
    request.filename = Some(filename);
    Ok(parent)
}

fn merge_options(current: &mut DownloadOptions, preset: &DownloadOptions) -> Result<()> {
    let defaults = serde_json::to_value(DownloadOptions::default())?;
    let mut current_value = serde_json::to_value(&*current)?;
    let preset_value = serde_json::to_value(preset)?;
    merge_when_default(&mut current_value, &preset_value, &defaults);
    *current = serde_json::from_value(current_value)?;

    let mut tags = preset.tags.clone();
    tags.extend(current.tags.clone());
    tags.retain(|tag| !tag.trim().is_empty());
    let mut seen = std::collections::HashSet::new();
    tags.retain(|tag| seen.insert(tag.to_ascii_lowercase()));
    current.tags = tags;
    Ok(())
}

fn merge_when_default(
    current: &mut serde_json::Value,
    preset: &serde_json::Value,
    defaults: &serde_json::Value,
) {
    let (Some(current), Some(preset), Some(defaults)) = (
        current.as_object_mut(),
        preset.as_object(),
        defaults.as_object(),
    ) else {
        return;
    };
    for (key, preset_value) in preset {
        let default_value = defaults.get(key).unwrap_or(&serde_json::Value::Null);
        if let Some(current_value) = current.get_mut(key) {
            if current_value == default_value {
                *current_value = preset_value.clone();
            }
        }
    }
}

fn builtin_variables(request: &CreateJob) -> BTreeMap<String, String> {
    let url = url::Url::parse(&request.source).ok();
    let filename = request.filename.clone().unwrap_or_else(|| {
        url.as_ref()
            .map(|value| crate::services::filename::from_url(value.as_str()))
            .unwrap_or_else(|| "download.bin".into())
    });
    let path = std::path::Path::new(&filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download")
        .to_owned();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_owned();
    let now = Utc::now();
    BTreeMap::from([
        ("filename".into(), filename),
        ("stem".into(), stem),
        ("extension".into(), extension),
        (
            "host".into(),
            url.as_ref()
                .and_then(url::Url::host_str)
                .unwrap_or("local")
                .to_owned(),
        ),
        ("year".into(), format!("{:04}", now.year())),
        ("month".into(), format!("{:02}", now.month())),
        ("day".into(), format!("{:02}", now.day())),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{core::models::JobKind, storage::DownloadPresetPayload};

    #[test]
    fn explicit_values_win_and_template_subdirectories_are_returned() {
        let preset = DownloadPreset {
            id: uuid::Uuid::new_v4(),
            name: "Music".into(),
            payload: DownloadPresetPayload {
                filename_template: Some("{artist}/{stem}.flac".into()),
                speed_limit_bps: Some(10),
                template_variables: BTreeMap::from([("artist".into(), "Example".into())]),
                ..DownloadPresetPayload::default()
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let mut request = CreateJob {
            preset_id: Some(preset.id),
            kind: JobKind::Http,
            source: "https://example.test/song.wav".into(),
            destination: None,
            filename: None,
            priority: 0,
            speed_limit_bps: Some(99),
            expected_sha256: None,
            duplicate_policy: DuplicatePolicy::Allow,
            options: DownloadOptions::default(),
        };
        let subdirectory = apply(&preset, &mut request).unwrap().unwrap();
        assert_eq!(subdirectory, PathBuf::from("Example"));
        assert_eq!(request.filename.as_deref(), Some("song.flac"));
        assert_eq!(request.speed_limit_bps, Some(99));
    }
}
