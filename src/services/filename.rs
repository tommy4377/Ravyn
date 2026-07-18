use url::Url;

pub fn from_url(input: &str) -> String {
    Url::parse(input)
        .ok()
        .and_then(|u| {
            u.path_segments()?
                .rfind(|s| !s.is_empty())
                .map(str::to_owned)
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "download.bin".into())
}

pub fn sanitize(input: &str) -> String {
    let invalid = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let mut name: String = input
        .chars()
        .map(|c| {
            if invalid.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    name = name.trim().trim_end_matches(['.', ' ']).to_string();
    if name.is_empty() {
        return "download.bin".into();
    }
    let mut name: String = name.chars().take(240).collect();
    // Truncating to the character limit can land exactly on a character that
    // was previously interior to the string but is now trailing — e.g. the
    // dot in "a".repeat(239) + "." + "b".repeat(20) sits right at the cut —
    // reintroducing the trailing dot/space Windows silently drops from the
    // path it actually creates on disk. Re-trim after truncating, not just
    // before.
    name = name.trim_end_matches(['.', ' ']).to_string();
    if name.is_empty() {
        return "download.bin".into();
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
    if reserved {
        name.insert(0, '_');
    }
    name
}

/// Derives a filename that does not collide with existing files by appending
/// ` (N)` before the extension, matching common download-manager behavior.
/// `is_taken` must report whether a candidate name is already in use.
pub fn next_available(name: &str, mut is_taken: impl FnMut(&str) -> bool) -> String {
    if !is_taken(name) {
        return name.to_owned();
    }
    let (stem, extension) = match name.rfind('.') {
        // A leading dot (".gitignore") is a bare name, not an extension.
        Some(index) if index > 0 => (&name[..index], &name[index..]),
        _ => (name, ""),
    };
    for counter in 1..10_000u32 {
        let candidate = format!("{stem} ({counter}){extension}");
        if !is_taken(&candidate) {
            return candidate;
        }
    }
    format!("{stem} ({}){extension}", uuid::Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_unsafe_characters() {
        assert_eq!(sanitize("a:b?.zip"), "a_b_.zip");
    }

    #[test]
    fn truncation_does_not_reintroduce_a_trailing_dot_or_space() {
        let input = format!("{}.{}", "a".repeat(239), "b".repeat(20));
        let sanitized = sanitize(&input);
        assert!(sanitized.len() <= 240);
        assert!(!sanitized.ends_with('.'));
        assert!(!sanitized.ends_with(' '));
    }

    #[test]
    fn next_available_keeps_free_names() {
        assert_eq!(next_available("file.bin", |_| false), "file.bin");
    }

    #[test]
    fn next_available_appends_counter_before_extension() {
        let taken = ["file.bin", "file (1).bin"];
        assert_eq!(
            next_available("file.bin", |name| taken.contains(&name)),
            "file (2).bin"
        );
    }

    #[test]
    fn next_available_handles_names_without_extension() {
        assert_eq!(next_available("file", |name| name == "file"), "file (1)");
    }

    #[test]
    fn prefixes_windows_reserved_names() {
        assert_eq!(sanitize("CON.txt"), "_CON.txt");
        assert_eq!(sanitize("LPT1"), "_LPT1");
    }
}
