use crate::error::{RavynError, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::{
    fs::{self, File},
    io::AsyncReadExt,
};
use tokio_util::sync::CancellationToken;

pub fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RavynError::Invalid(
            "SHA-256 values must contain exactly 64 hexadecimal characters".into(),
        ));
    }
    Ok(())
}

pub async fn sha256(path: &Path, cancellation: &CancellationToken) -> Result<String> {
    let mut file = File::open(path).await?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = tokio::select! {
            _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
            read = file.read(&mut buffer) => read?,
        };
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(hex::encode(hash.finalize()))
}

pub async fn verify_and_return(
    path: &Path,
    expected: &str,
    cancellation: &CancellationToken,
) -> Result<String> {
    validate_sha256(expected)?;
    let actual = sha256(path, cancellation).await?;
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(RavynError::Protocol(format!(
            "SHA-256 mismatch: expected {expected}, got {actual}"
        )));
    }
    Ok(actual)
}

pub async fn verify(path: &Path, expected: &str, cancellation: &CancellationToken) -> Result<()> {
    verify_and_return(path, expected, cancellation)
        .await
        .map(|_| ())
}

/// Verifies a file against an ordered SHA-256 piece ledger without allocating
/// a piece-sized buffer. The final piece may be shorter than `piece_length`.
pub async fn verify_pieces(
    path: &Path,
    piece_length: u64,
    expected: &[String],
    cancellation: &CancellationToken,
) -> Result<()> {
    if piece_length == 0 || expected.is_empty() {
        return Err(RavynError::Invalid(
            "piece verification requires a positive length and at least one hash".into(),
        ));
    }
    for hash in expected {
        validate_sha256(hash)?;
    }
    let metadata = fs::metadata(path).await?;
    let expected_count = metadata.len().div_ceil(piece_length);
    if expected_count != expected.len() as u64 {
        return Err(RavynError::Protocol(format!(
            "piece ledger length mismatch: expected {expected_count} hashes, received {}",
            expected.len()
        )));
    }

    let mut file = File::open(path).await?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    for (index, expected_hash) in expected.iter().enumerate() {
        let piece_start = index as u64 * piece_length;
        let mut remaining = piece_length.min(metadata.len().saturating_sub(piece_start));
        let mut hasher = Sha256::new();
        while remaining > 0 {
            let requested =
                usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
            let read = tokio::select! {
                _ = cancellation.cancelled() => return Err(RavynError::Cancelled),
                read = file.read(&mut buffer[..requested]) => read?,
            };
            if read == 0 {
                return Err(RavynError::Protocol(
                    "file ended during piece checksum verification".into(),
                ));
            }
            hasher.update(&buffer[..read]);
            remaining -= read as u64;
        }
        let actual = hex::encode(hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected_hash) {
            return Err(RavynError::Protocol(format!(
                "SHA-256 mismatch for piece {index}: expected {expected_hash}, got {actual}"
            )));
        }
    }
    Ok(())
}

/// Moves an integrity-failed output away from its user-visible destination.
/// If the move cannot be completed (for example, across filesystems), the
/// corrupt file is removed so it can never masquerade as a completed output.
pub async fn quarantine_corrupt_output(
    path: &Path,
    quarantine_root: &Path,
    job_id: uuid::Uuid,
) -> Result<Option<PathBuf>> {
    if !fs::try_exists(path).await? {
        return Ok(None);
    }
    fs::create_dir_all(quarantine_root).await?;
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("output");
    let destination = quarantine_root.join(format!("{job_id}-{filename}.corrupt"));
    match fs::rename(path, &destination).await {
        Ok(()) => Ok(Some(destination)),
        Err(rename_error) => match fs::remove_file(path).await {
            Ok(()) => {
                tracing::warn!(
                    %rename_error,
                    source = %path.display(),
                    destination = %destination.display(),
                    "could not quarantine corrupt output; removed it instead"
                );
                Ok(None)
            }
            Err(remove_error) => Err(RavynError::Internal(format!(
                "could not quarantine corrupt output ({rename_error}) or remove it ({remove_error})"
            ))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_contract_is_exact_and_hexadecimal() {
        assert!(validate_sha256(&"a".repeat(64)).is_ok());
        assert!(validate_sha256(&"A".repeat(64)).is_ok());
        assert!(validate_sha256(&"a".repeat(63)).is_err());
        assert!(validate_sha256(&format!("{}g", "a".repeat(63))).is_err());
    }

    #[tokio::test]
    async fn corrupt_outputs_are_removed_from_their_final_destination() {
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("download.bin");
        tokio::fs::write(&output, b"corrupt").await.unwrap();
        let job_id = uuid::Uuid::new_v4();

        let quarantined =
            quarantine_corrupt_output(&output, &temporary.path().join("quarantine"), job_id)
                .await
                .unwrap()
                .unwrap();

        assert!(!tokio::fs::try_exists(output).await.unwrap());
        assert_eq!(tokio::fs::read(quarantined).await.unwrap(), b"corrupt");
    }

    #[tokio::test]
    async fn shutdown_during_checksum_stops_promptly_and_leaves_the_file_alone() {
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("large.bin");
        let body = vec![0x3c_u8; 4 * 1024 * 1024];
        tokio::fs::write(&output, &body).await.unwrap();
        let expected = hex::encode(Sha256::digest(&body));
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let result = verify(&output, &expected, &cancellation).await;

        assert!(matches!(result, Err(RavynError::Cancelled)));
        assert_eq!(
            tokio::fs::metadata(&output).await.unwrap().len(),
            body.len() as u64
        );
    }

    #[tokio::test]
    async fn verifies_ordered_piece_hashes_and_detects_corruption() {
        let temporary = tempfile::tempdir().unwrap();
        let output = temporary.path().join("pieces.bin");
        tokio::fs::write(&output, b"abcdefghij").await.unwrap();
        let hashes = [b"abcd".as_slice(), b"efgh".as_slice(), b"ij".as_slice()]
            .map(|piece| hex::encode(Sha256::digest(piece)));
        let cancellation = CancellationToken::new();

        verify_pieces(&output, 4, &hashes, &cancellation)
            .await
            .unwrap();
        tokio::fs::write(&output, b"abcdEfghij").await.unwrap();
        assert!(
            verify_pieces(&output, 4, &hashes, &cancellation)
                .await
                .is_err()
        );
    }
}
