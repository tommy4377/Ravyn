use crate::error::{RavynError, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::{fs::File, io::AsyncReadExt};
use tokio_util::sync::CancellationToken;

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
