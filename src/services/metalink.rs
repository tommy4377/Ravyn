//! Bounded Metalink v4 parsing for single-file download jobs.

use std::path::Path;

use quick_xml::{Reader, events::Event};

use crate::error::{RavynError, Result};

const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
const MAX_MIRRORS: usize = 16;
const MAX_PIECES: usize = 16_384;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetalinkFile {
    pub name: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub mirrors: Vec<MetalinkMirror>,
    pub pieces: Option<MetalinkPieces>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetalinkMirror {
    pub url: String,
    pub priority: u32,
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetalinkPieces {
    pub length: u64,
    pub sha256: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextTarget {
    None,
    Size,
    WholeHash,
    PieceHash,
    Url,
}

pub fn parse(document: &[u8]) -> Result<MetalinkFile> {
    if document.is_empty() || document.len() > MAX_DOCUMENT_BYTES {
        return Err(RavynError::Invalid(format!(
            "Metalink document must be between 1 and {MAX_DOCUMENT_BYTES} bytes"
        )));
    }
    let source = std::str::from_utf8(document)
        .map_err(|_| RavynError::Invalid("Metalink document must be UTF-8".into()))?;
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut file_count = 0_usize;
    let mut root_seen = false;
    let mut name = None;
    let mut size = None;
    let mut whole_hash = None;
    let mut mirrors = Vec::new();
    let mut pieces_length = None;
    let mut piece_hashes = Vec::new();
    let mut in_pieces = false;
    let mut text_target = TextTarget::None;
    let mut url_priority = 999_999_u32;
    let mut url_location = None;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) => match element.local_name().as_ref() {
                b"metalink" => {
                    if root_seen {
                        return Err(RavynError::Invalid(
                            "Metalink document contains multiple roots".into(),
                        ));
                    }
                    root_seen = true;
                    let mut namespace_valid = false;
                    for attribute in element.attributes().with_checks(true) {
                        let attribute = attribute.map_err(xml_error)?;
                        if attribute.key.as_ref() == b"xmlns" {
                            namespace_valid = attribute
                                .decode_and_unescape_value(reader.decoder())
                                .map_err(xml_error)?
                                .as_ref()
                                == "urn:ietf:params:xml:ns:metalink";
                        }
                    }
                    if !namespace_valid {
                        return Err(RavynError::Invalid(
                            "Metalink document must use the v4 namespace".into(),
                        ));
                    }
                }
                b"file" => {
                    if !root_seen {
                        return Err(RavynError::Invalid(
                            "Metalink file appeared before its root element".into(),
                        ));
                    }
                    file_count += 1;
                    if file_count > 1 {
                        return Err(RavynError::Invalid(
                            "a Ravyn Metalink job must describe exactly one file".into(),
                        ));
                    }
                    for attribute in element.attributes().with_checks(true) {
                        let attribute = attribute.map_err(xml_error)?;
                        if attribute.key.local_name().as_ref() == b"name" {
                            name = Some(
                                attribute
                                    .decode_and_unescape_value(reader.decoder())
                                    .map_err(xml_error)?
                                    .into_owned(),
                            );
                        }
                    }
                }
                b"size" => text_target = TextTarget::Size,
                b"hash" => {
                    let mut supported = in_pieces;
                    let mut declared = false;
                    for attribute in element.attributes().with_checks(true) {
                        let attribute = attribute.map_err(xml_error)?;
                        if attribute.key.local_name().as_ref() == b"type" {
                            declared = true;
                            let value = attribute
                                .decode_and_unescape_value(reader.decoder())
                                .map_err(xml_error)?;
                            supported =
                                matches!(value.to_ascii_lowercase().as_str(), "sha-256" | "sha256");
                        }
                    }
                    if (!in_pieces && !declared) || !supported {
                        return Err(RavynError::Invalid(
                            "Metalink hashes must use SHA-256".into(),
                        ));
                    }
                    text_target = if supported {
                        if in_pieces {
                            TextTarget::PieceHash
                        } else {
                            TextTarget::WholeHash
                        }
                    } else {
                        TextTarget::None
                    };
                }
                b"pieces" => {
                    in_pieces = true;
                    let mut supported = false;
                    for attribute in element.attributes().with_checks(true) {
                        let attribute = attribute.map_err(xml_error)?;
                        let value = attribute
                            .decode_and_unescape_value(reader.decoder())
                            .map_err(xml_error)?;
                        match attribute.key.local_name().as_ref() {
                            b"type" => {
                                supported = matches!(
                                    value.to_ascii_lowercase().as_str(),
                                    "sha-256" | "sha256"
                                )
                            }
                            b"length" => pieces_length = value.parse::<u64>().ok(),
                            _ => {}
                        }
                    }
                    if !supported || pieces_length == Some(0) || pieces_length.is_none() {
                        return Err(RavynError::Invalid(
                            "Metalink pieces require SHA-256 and a positive length".into(),
                        ));
                    }
                }
                b"url" => {
                    text_target = TextTarget::Url;
                    url_priority = 999_999;
                    url_location = None;
                    for attribute in element.attributes().with_checks(true) {
                        let attribute = attribute.map_err(xml_error)?;
                        let value = attribute
                            .decode_and_unescape_value(reader.decoder())
                            .map_err(xml_error)?;
                        match attribute.key.local_name().as_ref() {
                            b"priority" => {
                                url_priority = value.parse::<u32>().map_err(|_| {
                                    RavynError::Invalid(
                                        "Metalink URL priority must be an unsigned integer".into(),
                                    )
                                })?
                            }
                            b"location" => url_location = Some(value.into_owned()),
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Text(text)) => {
                let value = text.decode().map_err(xml_error)?.trim().to_owned();
                if value.is_empty() {
                    buffer.clear();
                    continue;
                }
                match text_target {
                    TextTarget::Size => {
                        size = Some(value.parse::<u64>().map_err(|_| {
                            RavynError::Invalid("Metalink size must be an unsigned integer".into())
                        })?)
                    }
                    TextTarget::WholeHash => whole_hash = Some(validate_sha256(&value)?),
                    TextTarget::PieceHash => {
                        if piece_hashes.len() >= MAX_PIECES {
                            return Err(RavynError::Invalid(
                                "Metalink contains too many piece hashes".into(),
                            ));
                        }
                        piece_hashes.push(validate_sha256(&value)?);
                    }
                    TextTarget::Url => {
                        if mirrors.len() >= MAX_MIRRORS {
                            return Err(RavynError::Invalid(
                                "Metalink contains too many mirrors".into(),
                            ));
                        }
                        let parsed = url::Url::parse(&value)?;
                        if !matches!(parsed.scheme(), "https" | "http")
                            || parsed.host_str().is_none()
                            || !parsed.username().is_empty()
                            || parsed.password().is_some()
                            || parsed.fragment().is_some()
                        {
                            return Err(RavynError::Invalid(
                                "Metalink mirrors must be HTTP(S) URLs without credentials or fragments"
                                    .into(),
                            ));
                        }
                        mirrors.push(MetalinkMirror {
                            url: parsed.into(),
                            priority: url_priority,
                            location: url_location.take(),
                        });
                    }
                    TextTarget::None => {}
                }
            }
            Ok(Event::End(element)) => match element.local_name().as_ref() {
                b"pieces" => in_pieces = false,
                b"size" | b"hash" | b"url" => text_target = TextTarget::None,
                _ => {}
            },
            Ok(Event::DocType(_)) => {
                return Err(RavynError::Invalid(
                    "Metalink DTDs and external entities are forbidden".into(),
                ));
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(xml_error(error)),
        }
        buffer.clear();
    }

    if file_count != 1 {
        return Err(RavynError::Invalid(
            "Metalink must contain exactly one file".into(),
        ));
    }
    let name = name.ok_or_else(|| RavynError::Invalid("Metalink file has no name".into()))?;
    if name.is_empty()
        || name.len() > 255
        || Path::new(&name)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(name.as_str())
        || name.contains(['/', '\\'])
    {
        return Err(RavynError::Invalid(
            "Metalink filename must be a safe single path component".into(),
        ));
    }
    let size = size
        .filter(|value| *value > 0)
        .ok_or_else(|| RavynError::Invalid("Metalink file requires a positive size".into()))?;
    if mirrors.is_empty() {
        return Err(RavynError::Invalid(
            "Metalink file must contain at least one mirror".into(),
        ));
    }
    mirrors.sort_by_key(|mirror| mirror.priority);
    let mut seen_mirrors = std::collections::BTreeSet::new();
    mirrors.retain(|mirror| seen_mirrors.insert(mirror.url.clone()));
    let pieces = match pieces_length {
        Some(length) => {
            let expected = size.div_ceil(length);
            if piece_hashes.len() as u64 != expected {
                return Err(RavynError::Invalid(format!(
                    "Metalink piece count mismatch: expected {expected}, received {}",
                    piece_hashes.len()
                )));
            }
            Some(MetalinkPieces {
                length,
                sha256: piece_hashes,
            })
        }
        None if piece_hashes.is_empty() => None,
        None => {
            return Err(RavynError::Invalid(
                "Metalink piece hashes require a pieces declaration".into(),
            ));
        }
    };
    Ok(MetalinkFile {
        name,
        size,
        sha256: whole_hash,
        mirrors,
        pieces,
    })
}

fn validate_sha256(value: &str) -> Result<String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RavynError::Invalid(
            "Metalink SHA-256 hashes must contain 64 hexadecimal characters".into(),
        ));
    }
    Ok(value.to_ascii_lowercase())
}

fn xml_error(error: impl std::fmt::Display) -> RavynError {
    RavynError::Invalid(format!("invalid Metalink XML: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn parses_and_prioritizes_a_piece_verified_file() {
        let xml = format!(
            r#"<?xml version="1.0"?><metalink xmlns="urn:ietf:params:xml:ns:metalink"><file name="archive.bin"><size>8</size><hash type="sha-256">{HASH}</hash><pieces length="4" type="sha-256"><hash>{HASH}</hash><hash>{HASH}</hash></pieces><url priority="2">https://b.example/archive.bin</url><url priority="1" location="it">https://a.example/archive.bin</url></file></metalink>"#
        );
        let file = parse(xml.as_bytes()).unwrap();
        assert_eq!(file.name, "archive.bin");
        assert_eq!(file.size, 8);
        assert_eq!(file.mirrors[0].url, "https://a.example/archive.bin");
        assert_eq!(file.pieces.unwrap().sha256.len(), 2);
    }

    #[test]
    fn rejects_traversal_dtds_and_piece_count_mismatches() {
        let traversal = br#"<metalink><file name="../x"><size>1</size><url>https://example.test/x</url></file></metalink>"#;
        assert!(parse(traversal).is_err());
        let dtd = br#"<!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]><metalink><file name="x"><size>1</size><url>https://example.test/x</url></file></metalink>"#;
        assert!(parse(dtd).is_err());
        let mismatch = format!(
            r#"<metalink><file name="x"><size>8</size><pieces length="4" type="sha-256"><hash>{HASH}</hash></pieces><url>https://example.test/x</url></file></metalink>"#
        );
        assert!(parse(mismatch.as_bytes()).is_err());
    }

    #[test]
    fn requires_v4_namespace_and_sha256() {
        let missing_namespace =
            br#"<metalink><file name="x"><size>1</size><url>https://example.test/x</url></file></metalink>"#;
        assert!(parse(missing_namespace).is_err());
        let unsupported = br#"<metalink xmlns="urn:ietf:params:xml:ns:metalink"><file name="x"><size>1</size><hash type="md5">aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa</hash><url>https://example.test/x</url></file></metalink>"#;
        assert!(parse(unsupported).is_err());
    }
}
