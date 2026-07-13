use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    error::{RavynError, Result},
    services::filename,
};

const MAX_TEMPLATE_LENGTH: usize = 2_048;
const MAX_SEGMENTS: usize = 32;
const MAX_VARIABLES: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplatePreviewRequest {
    pub template: String,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplatePreview {
    pub rendered: PathBuf,
    pub missing_variables: Vec<String>,
}

/// Renders a relative filename template and sanitizes every generated path segment.
pub fn render(
    template: &str,
    variables: &BTreeMap<String, String>,
) -> Result<TemplatePreview> {
    if template.trim().is_empty() || template.len() > MAX_TEMPLATE_LENGTH {
        return Err(RavynError::Invalid(format!(
            "filename templates must contain between 1 and {MAX_TEMPLATE_LENGTH} characters"
        )));
    }
    if variables.len() > MAX_VARIABLES {
        return Err(RavynError::Invalid(format!(
            "filename templates may define at most {MAX_VARIABLES} variables"
        )));
    }
    if std::path::Path::new(template).is_absolute() {
        return Err(RavynError::Invalid(
            "filename templates must be relative".into(),
        ));
    }

    let raw_segments = template.split(['/', '\\']).collect::<Vec<_>>();
    if raw_segments.is_empty() || raw_segments.len() > MAX_SEGMENTS {
        return Err(RavynError::Invalid(format!(
            "filename templates may contain at most {MAX_SEGMENTS} path segments"
        )));
    }

    let mut rendered = PathBuf::new();
    let mut missing = Vec::new();
    for raw_segment in raw_segments {
        if raw_segment.is_empty() || matches!(raw_segment, "." | "..") {
            return Err(RavynError::Invalid(
                "filename templates may not contain empty or traversal segments".into(),
            ));
        }
        let expanded = expand_segment(raw_segment, variables, &mut missing)?;
        let sanitized = filename::sanitize(&expanded);
        if matches!(sanitized.as_str(), "." | "..") {
            return Err(RavynError::Invalid(
                "filename template rendered a traversal segment".into(),
            ));
        }
        rendered.push(sanitized);
    }
    missing.sort();
    missing.dedup();
    Ok(TemplatePreview {
        rendered,
        missing_variables: missing,
    })
}

fn expand_segment(
    segment: &str,
    variables: &BTreeMap<String, String>,
    missing: &mut Vec<String>,
) -> Result<String> {
    let mut output = String::with_capacity(segment.len());
    let mut characters = segment.char_indices().peekable();
    while let Some((_, character)) = characters.next() {
        match character {
            '{' => {
                if characters.peek().is_some_and(|(_, next)| *next == '{') {
                    characters.next();
                    output.push('{');
                    continue;
                }
                let mut name = String::new();
                let mut closed = false;
                for (_, next) in characters.by_ref() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    name.push(next);
                }
                if !closed || !valid_variable_name(&name) {
                    return Err(RavynError::Invalid(
                        "filename template contains an invalid variable expression".into(),
                    ));
                }
                if let Some(value) = variables.get(&name) {
                    output.push_str(value);
                } else {
                    missing.push(name);
                    output.push_str("unknown");
                }
            }
            '}' => {
                if characters.peek().is_some_and(|(_, next)| *next == '}') {
                    characters.next();
                    output.push('}');
                } else {
                    return Err(RavynError::Invalid(
                        "filename template contains an unmatched closing brace".into(),
                    ));
                }
            }
            other => output.push(other),
        }
    }
    Ok(output)
}

fn valid_variable_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_sanitizes_each_segment() {
        let variables = BTreeMap::from([
            ("artist".into(), "Artist: Name".into()),
            ("title".into(), "Track?".into()),
        ]);
        let preview = render("{artist}/{title}.flac", &variables).unwrap();
        assert_eq!(preview.rendered, PathBuf::from("Artist_ Name/Track_.flac"));
        assert!(preview.missing_variables.is_empty());
    }

    #[test]
    fn reports_missing_values_without_allowing_traversal() {
        let preview = render("{album}/{track}.mp3", &BTreeMap::new()).unwrap();
        assert_eq!(preview.missing_variables, vec!["album", "track"]);
        assert!(render("../{track}.mp3", &BTreeMap::new()).is_err());
    }

    #[test]
    fn supports_literal_braces() {
        let preview = render("{{archive}}/{name}", &BTreeMap::from([("name".into(), "a".into())]))
            .unwrap();
        assert_eq!(preview.rendered, PathBuf::from("{archive}/a"));
    }
}
