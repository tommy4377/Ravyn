use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};
use tokio::{fs::File, io::AsyncReadExt};

use crate::error::Result;

/// User-facing library categories with stable serialized names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryCategory {
    Downloads,
    Videos,
    Music,
    Documents,
    Images,
    Archives,
    Torrents,
    Playlists,
    Temporary,
    Other,
}

impl LibraryCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Downloads => "downloads",
            Self::Videos => "videos",
            Self::Music => "music",
            Self::Documents => "documents",
            Self::Images => "images",
            Self::Archives => "archives",
            Self::Torrents => "torrents",
            Self::Playlists => "playlists",
            Self::Temporary => "temporary",
            Self::Other => "other",
        }
    }

    pub const fn directory_name(self) -> &'static str {
        match self {
            Self::Downloads | Self::Other => "Downloads",
            Self::Videos => "Videos",
            Self::Music => "Music",
            Self::Documents => "Documents",
            Self::Images => "Images",
            Self::Archives => "Archives",
            Self::Torrents => "Torrents",
            Self::Playlists => "Playlists",
            Self::Temporary => "Temporary",
        }
    }
}

impl std::str::FromStr for LibraryCategory {
    type Err = crate::error::RavynError;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "downloads" => Ok(Self::Downloads),
            "videos" => Ok(Self::Videos),
            "music" => Ok(Self::Music),
            "documents" => Ok(Self::Documents),
            "images" => Ok(Self::Images),
            "archives" => Ok(Self::Archives),
            "torrents" => Ok(Self::Torrents),
            "playlists" => Ok(Self::Playlists),
            "temporary" => Ok(Self::Temporary),
            "other" => Ok(Self::Other),
            _ => Err(crate::error::RavynError::Invalid(format!(
                "unknown library category {value}"
            ))),
        }
    }
}

/// Classifies a local file using extension first, then MIME, then bounded magic-byte sniffing.
pub async fn classify_file(path: &Path, mime_type: Option<&str>) -> Result<LibraryCategory> {
    classify_file_with_overrides(path, mime_type, &BTreeMap::new()).await
}

/// Classifies a local file while allowing persistent extension overrides.
pub async fn classify_file_with_overrides(
    path: &Path,
    mime_type: Option<&str>,
    overrides: &BTreeMap<String, LibraryCategory>,
) -> Result<LibraryCategory> {
    if let Some(category) = classify_name_with_overrides(path, overrides) {
        return Ok(category);
    }
    if let Some(category) = classify_mime(mime_type) {
        return Ok(category);
    }

    let mut header = [0_u8; 512];
    let mut file = File::open(path).await?;
    let read = file.read(&mut header).await?;
    Ok(classify_magic(&header[..read]).unwrap_or(LibraryCategory::Downloads))
}

/// Classifies a filename from its user-visible extension.
pub fn classify_name(path: &Path) -> Option<LibraryCategory> {
    classify_name_with_overrides(path, &BTreeMap::new())
}

/// Classifies a filename with operator-defined extension overrides taking precedence.
pub fn classify_name_with_overrides(
    path: &Path,
    overrides: &BTreeMap<String, LibraryCategory>,
) -> Option<LibraryCategory> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if let Some(category) = overrides.get(&extension) {
        return Some(*category);
    }
    let category = match extension.as_str() {
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "wmv" | "flv" | "m4v" | "mpeg" | "mpg" | "ts" => {
            LibraryCategory::Videos
        }
        "mp3" | "m4a" | "aac" | "flac" | "ogg" | "opus" | "wav" | "wma" | "aiff" | "alac" => {
            LibraryCategory::Music
        }
        "pdf" | "txt" | "md" | "rtf" | "doc" | "docx" | "odt" | "xls" | "xlsx" | "ods" | "ppt"
        | "pptx" | "odp" | "epub" | "mobi" | "csv" | "json" | "xml" => LibraryCategory::Documents,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "tiff" | "tif" | "svg"
        | "heic" | "heif" | "ico" => LibraryCategory::Images,
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "iso" | "cab"
        | "dmg" => LibraryCategory::Archives,
        "torrent" => LibraryCategory::Torrents,
        "m3u" | "m3u8" | "pls" | "xspf" => LibraryCategory::Playlists,
        "part" | "crdownload" | "tmp" | "download" => LibraryCategory::Temporary,
        _ => return None,
    };
    Some(category)
}

/// Validates extension override keys before they are persisted or applied.
pub fn validate_category_overrides(overrides: &BTreeMap<String, LibraryCategory>) -> Result<()> {
    if overrides.len() > 512 {
        return Err(crate::error::RavynError::Invalid(
            "library category overrides may contain at most 512 extensions".into(),
        ));
    }
    for extension in overrides.keys() {
        if extension.is_empty()
            || extension.len() > 32
            || extension.starts_with('.')
            || extension.chars().any(|character| {
                !character.is_ascii_lowercase()
                    && !character.is_ascii_digit()
                    && character != '_'
                    && character != '-'
            })
        {
            return Err(crate::error::RavynError::Invalid(format!(
                "invalid library extension override {extension:?}; use 1-32 lowercase characters without a leading dot"
            )));
        }
    }
    Ok(())
}

fn classify_mime(mime_type: Option<&str>) -> Option<LibraryCategory> {
    let value = mime_type?.split(';').next()?.trim().to_ascii_lowercase();
    if value.starts_with("video/") {
        Some(LibraryCategory::Videos)
    } else if value.starts_with("audio/") {
        Some(LibraryCategory::Music)
    } else if value.starts_with("image/") {
        Some(LibraryCategory::Images)
    } else if value.starts_with("text/")
        || matches!(
            value.as_str(),
            "application/pdf"
                | "application/epub+zip"
                | "application/msword"
                | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        )
    {
        Some(LibraryCategory::Documents)
    } else if matches!(
        value.as_str(),
        "application/zip"
            | "application/x-7z-compressed"
            | "application/vnd.rar"
            | "application/x-rar-compressed"
            | "application/gzip"
            | "application/x-tar"
            | "application/x-iso9660-image"
    ) {
        Some(LibraryCategory::Archives)
    } else if value == "application/x-bittorrent" {
        Some(LibraryCategory::Torrents)
    } else if matches!(
        value.as_str(),
        "application/vnd.apple.mpegurl" | "application/x-mpegurl" | "audio/x-mpegurl"
    ) {
        Some(LibraryCategory::Playlists)
    } else {
        None
    }
}

fn classify_magic(bytes: &[u8]) -> Option<LibraryCategory> {
    if bytes.starts_with(b"%PDF-") {
        return Some(LibraryCategory::Documents);
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")
    {
        return Some(LibraryCategory::Images);
    }
    if bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"7z\xbc\xaf\x27\x1c")
        || bytes.starts_with(b"Rar!\x1a\x07")
        || bytes.starts_with(&[0x1f, 0x8b])
        || bytes.get(257..262) == Some(b"ustar")
    {
        return Some(LibraryCategory::Archives);
    }
    if bytes.starts_with(b"fLaC")
        || bytes.starts_with(b"OggS")
        || bytes.starts_with(b"ID3")
        || bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE")
    {
        return Some(LibraryCategory::Music);
    }
    if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3])
        || bytes.get(4..8) == Some(b"ftyp")
        || bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"AVI ")
    {
        return Some(LibraryCategory::Videos);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_has_stable_category_mapping() {
        assert_eq!(
            classify_name(Path::new("movie.MKV")),
            Some(LibraryCategory::Videos)
        );
        assert_eq!(
            classify_name(Path::new("manual.pdf")),
            Some(LibraryCategory::Documents)
        );
        assert_eq!(classify_name(Path::new("payload.unknown")), None);
    }

    #[test]
    fn operator_extension_overrides_take_precedence() {
        let overrides = BTreeMap::from([("bin".into(), LibraryCategory::Archives)]);
        assert_eq!(
            classify_name_with_overrides(Path::new("payload.bin"), &overrides),
            Some(LibraryCategory::Archives)
        );
        assert!(validate_category_overrides(&overrides).is_ok());
    }

    #[tokio::test]
    async fn extension_wins_over_advisory_mime() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("cover.jpg");
        tokio::fs::write(&path, b"not an image").await.unwrap();
        assert_eq!(
            classify_file(&path, Some("application/zip")).await.unwrap(),
            LibraryCategory::Images
        );
    }

    #[tokio::test]
    async fn magic_is_used_when_name_and_mime_are_unknown() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("payload");
        tokio::fs::write(&path, b"%PDF-1.7\n").await.unwrap();
        assert_eq!(
            classify_file(&path, None).await.unwrap(),
            LibraryCategory::Documents
        );
    }
}
