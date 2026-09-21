//! Verified artifact materialization and filesystem installation helpers.

use super::*;

pub(super) async fn materialize_verified_artifact(
    artifact: &EngineArtifact,
    temporary: &Path,
    destination: &Path,
    version_dir: &Path,
    cancellation: &CancellationToken,
) -> Result<String> {
    tokio::fs::create_dir(version_dir).await?;
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if let Some(installer) = &artifact.installer {
        match installer.kind {
            EngineInstallerKind::MsiAdministrative => {
                install_msi_administrative(temporary, version_dir, cancellation).await?;
            }
        }
        if cancellation.is_cancelled() {
            return Err(RavynError::Cancelled);
        }
        let metadata = tokio::fs::metadata(destination).await.map_err(|error| {
            RavynError::provisioning(
                ProvisioningErrorCode::AppInstallFailed,
                format!(
                    "managed engine installer did not produce {}: {error}",
                    artifact.filename
                ),
            )
            .with_component(&artifact.engine)
            .with_expected_version(&artifact.version)
        })?;
        if !metadata.is_file() || metadata.len() > MAX_ENGINE_BYTES {
            return Err(RavynError::provisioning(
                ProvisioningErrorCode::AppInstallFailed,
                "managed engine installer produced an invalid executable",
            )
            .with_component(&artifact.engine)
            .with_expected_version(&artifact.version));
        }
        set_executable(destination).await?;
        tokio::fs::remove_file(temporary).await?;
        return hash_file(destination).await;
    }

    if let Some(member) = &artifact.archive_member {
        let member_bytes =
            extract_archive_member(temporary, member, artifact.activation_sha256()).await?;
        let extracted = temporary.with_extension("extract");
        if let Some(parent) = extracted.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut extracted_file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&extracted)
            .await?;
        extracted_file.write_all(&member_bytes).await?;
        extracted_file.sync_all().await?;
        drop(extracted_file);
        set_executable(&extracted).await?;
        if cancellation.is_cancelled() {
            let _ = tokio::fs::remove_file(&extracted).await;
            return Err(RavynError::Cancelled);
        }
        atomic_replace(&extracted, destination).await?;
        tokio::fs::remove_file(temporary).await?;
        return Ok(artifact.activation_sha256().to_ascii_lowercase());
    }

    set_executable(temporary).await?;
    if cancellation.is_cancelled() {
        return Err(RavynError::Cancelled);
    }
    atomic_replace(temporary, destination).await?;
    Ok(artifact.activation_sha256().to_ascii_lowercase())
}

#[cfg(windows)]
pub(super) async fn install_msi_administrative(
    installer: &Path,
    target: &Path,
    cancellation: &CancellationToken,
) -> Result<()> {
    use tokio::process::Command;

    let target_argument = format!("TARGETDIR={}", target.display());
    let mut command = Command::new("msiexec.exe");
    crate::services::process::hide_console_window(&mut command);
    let mut child = command
        .arg("/a")
        .arg(installer)
        .arg("/qn")
        .arg("/norestart")
        .arg(target_argument)
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            RavynError::provisioning(
                ProvisioningErrorCode::AppInstallFailed,
                format!("failed to start Windows Installer: {error}"),
            )
        })?;
    let status = tokio::select! {
        _ = cancellation.cancelled() => {
            let _ = child.kill().await;
            return Err(RavynError::Cancelled);
        }
        status = child.wait() => status?,
    };
    if !status.success() {
        return Err(RavynError::provisioning(
            ProvisioningErrorCode::AppInstallFailed,
            format!("Windows Installer administrative extraction exited with {status}"),
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) async fn install_msi_administrative(
    _installer: &Path,
    _target: &Path,
    _cancellation: &CancellationToken,
) -> Result<()> {
    Err(RavynError::provisioning(
        ProvisioningErrorCode::PlatformUnsupported,
        "MSI administrative extraction is available only on Windows",
    ))
}

pub(super) async fn cleanup_candidate(temporary: &Path, version_dir: &Path) {
    for path in [temporary.to_path_buf(), temporary.with_extension("extract")] {
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                %error,
                path = %path.display(),
                "failed to clean managed-engine temporary file"
            ),
        }
    }
    match tokio::fs::remove_dir_all(version_dir).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(
            %error,
            path = %version_dir.display(),
            "failed to clean managed-engine candidate directory"
        ),
    }
}

/// Summary of what an [`EngineManager::cleanup_versions`] pass removed.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EngineCleanupReport {
    pub removed_versions: Vec<String>,
    pub removed_temp_files: Vec<String>,
    pub bytes_freed: u64,
}

pub(super) async fn remove_download_temp_files(dir: &Path, removed: &mut Vec<String>) -> Result<u64> {
    let mut freed = 0_u64;
    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !(name.starts_with('.') && (name.ends_with(".download") || name.ends_with(".extract"))) {
            continue;
        }
        if let Ok(metadata) = entry.metadata().await {
            freed = freed.saturating_add(metadata.len());
        }
        tokio::fs::remove_file(&path).await?;
        removed.push(name.to_owned());
    }
    Ok(freed)
}

/// Reads and verifies a single member out of a downloaded ZIP archive,
/// bounded to [`MAX_ENGINE_BYTES`]. Runs on a blocking thread since the `zip`
/// crate is synchronous and decompression is CPU-bound.
pub(super) async fn extract_archive_member(
    archive_path: &Path,
    member: &str,
    expected_sha256: &str,
) -> Result<Vec<u8>> {
    let archive_path = archive_path.to_owned();
    let member = member.to_owned();
    let expected_sha256 = expected_sha256.to_owned();
    tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let file = std::fs::File::open(&archive_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|error| RavynError::Protocol(format!("invalid engine archive: {error}")))?;
        let mut entry = archive.by_name(&member).map_err(|error| {
            RavynError::provisioning(
                ProvisioningErrorCode::DownloadInterrupted,
                format!("engine archive is missing expected member {member:?}: {error}"),
            )
        })?;
        if entry.size() > MAX_ENGINE_BYTES {
            return Err(RavynError::Protocol(
                "engine archive member exceeds the maximum managed engine size".into(),
            ));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_ENGINE_BYTES {
            return Err(RavynError::Protocol(
                "engine archive member exceeds the maximum managed engine size".into(),
            ));
        }
        let actual = hex::encode(Sha256::digest(&bytes));
        if !actual.eq_ignore_ascii_case(&expected_sha256) {
            return Err(RavynError::provisioning(
                ProvisioningErrorCode::ChecksumMismatch,
                "engine archive member failed checksum verification",
            ));
        }
        Ok(bytes)
    })
    .await
    .map_err(|error| RavynError::Internal(format!("archive extraction task failed: {error}")))?
}

pub(super) async fn directory_size(dir: &Path) -> Result<u64> {
    let mut total = 0_u64;
    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if let Ok(metadata) = entry.metadata().await {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

pub(super) async fn read_engine_metadata(path: &Path) -> Result<Vec<u8>> {
    let metadata = tokio::fs::metadata(path).await?;
    if !metadata.is_file() || metadata.len() > MAX_ENGINE_METADATA_BYTES {
        return Err(RavynError::Invalid(format!(
            "managed engine metadata must be a regular file no larger than {MAX_ENGINE_METADATA_BYTES} bytes"
        )));
    }
    Ok(tokio::fs::read(path).await?)
}

pub(super) async fn write_metadata_atomic(
    temporary: &Path,
    destination: &Path,
    active: &ActiveEngine,
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(active)?;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(temporary)
        .await?;
    file.write_all(&bytes).await?;
    file.sync_all().await?;
    drop(file);
    atomic_replace(temporary, destination).await
}

pub(super) async fn hash_file(path: &Path) -> Result<String> {
    let metadata = tokio::fs::metadata(path).await?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ENGINE_BYTES {
        return Err(RavynError::Unavailable(
            "managed engine executable has an invalid size".into(),
        ));
    }
    let mut file = tokio::fs::File::open(path).await?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(not(windows))]
pub(super) async fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
    tokio::fs::rename(source, destination).await?;
    Ok(())
}

#[cfg(windows)]
pub(super) async fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, REPLACEFILE_WRITE_THROUGH,
        ReplaceFileW,
    };

    let source = source.to_owned();
    let destination = destination.to_owned();
    tokio::task::spawn_blocking(move || {
        let source_wide = source
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let destination_wide = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let replaced = unsafe {
            if destination.exists() {
                ReplaceFileW(
                    destination_wide.as_ptr(),
                    source_wide.as_ptr(),
                    ptr::null(),
                    REPLACEFILE_WRITE_THROUGH,
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            } else {
                MoveFileExW(
                    source_wide.as_ptr(),
                    destination_wide.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
        };
        if replaced == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    })
    .await
    .map_err(|error| RavynError::Internal(format!("engine activation task failed: {error}")))??;
    Ok(())
}

pub(super) fn validate_token(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return Err(RavynError::Invalid(format!(
            "managed engine {label} is invalid"
        )));
    }
    Ok(())
}

pub(super) fn validate_relative_path(value: &str, label: &str) -> Result<()> {
    if value.is_empty() || value.len() > 512 || value.contains('\\') {
        return Err(RavynError::Invalid(format!(
            "managed engine {label} must be a safe relative forward-slash path"
        )));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || value.split('/').any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.contains(':')
        })
    {
        return Err(RavynError::Invalid(format!(
            "managed engine {label} must be a safe relative forward-slash path"
        )));
    }
    Ok(())
}

#[cfg(unix)]
pub(super) async fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = tokio::fs::metadata(path).await?.permissions();
    permissions.set_mode(0o755);
    tokio::fs::set_permissions(path, permissions).await?;
    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

