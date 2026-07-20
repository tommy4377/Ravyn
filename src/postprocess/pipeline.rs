use crate::{
    config::Config,
    core::models::{FfmpegPreset, PostAction},
    error::{RavynError, Result},
    services::{checksum, process as process_supervisor, security},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{fs, process::Command};
use tokio_util::sync::CancellationToken;

pub async fn run(
    config: Arc<Config>,
    mut current: PathBuf,
    actions: &[PostAction],
    cancellation: CancellationToken,
) -> Result<PathBuf> {
    for action in actions {
        if cancellation.is_cancelled() {
            return Err(RavynError::Cancelled);
        }
        current = match action {
            PostAction::VerifySha256 { expected } => {
                checksum::verify(&current, expected, &cancellation).await?;
                current
            }
            PostAction::Extract {
                destination,
                delete_archive,
            } => {
                let dest = destination
                    .clone()
                    .unwrap_or_else(|| current.with_extension(""));
                security::validate_output_path(&config, &dest)?;
                if fs::try_exists(&dest).await? {
                    return Err(RavynError::Conflict(format!(
                        "extraction destination {} already exists",
                        dest.display()
                    )));
                }
                let parent = dest.parent().unwrap_or(Path::new("."));
                fs::create_dir_all(parent).await?;
                let staging = parent.join(format!(".ravyn-extract-{}", uuid::Uuid::new_v4()));
                fs::create_dir(&staging).await?;
                let permitted_bytes = archive_expansion_limit(&config, &current).await?;
                if let Err(error) = preflight_archive(&config, &current, cancellation.child_token()).await {
                    let _ = fs::remove_dir_all(&staging).await;
                    return Err(error);
                }
                let mut command = Command::new(&config.seven_zip);
                command.args([
                    std::ffi::OsString::from("x"),
                    format!("-o{}", staging.display()).into(),
                    std::ffi::OsString::from("--"),
                    current.as_os_str().into(),
                ]);
                let limits = process_supervisor::ProcessLimits {
                    output_tree_bytes: Some(permitted_bytes),
                    output_tree_files: Some(config.max_extract_files),
                    output_tree_depth: Some(config.max_extract_depth),
                    ..process_supervisor::ProcessLimits::default()
                };
                let extraction = run_checked_process(
                    &mut command,
                    &limits,
                    Some(staging.clone()),
                    cancellation.child_token(),
                )
                .await;
                if let Err(error) = extraction {
                    let _ = fs::remove_dir_all(&staging).await;
                    return Err(error);
                }
                if let Err(error) = validate_extracted_tree(&config, &current, &staging).await {
                    let _ = fs::remove_dir_all(&staging).await;
                    return Err(error);
                }
                if let Err(error) = fs::rename(&staging, &dest).await {
                    let _ = fs::remove_dir_all(&staging).await;
                    return Err(error.into());
                }
                if *delete_archive {
                    fs::remove_file(&current).await?;
                }
                dest
            }
            PostAction::ConvertMedia {
                extension,
                preset,
                arguments,
                unsafe_arguments: _,
                delete_original,
            } => {
                if extension.contains(['/', '\\']) || extension.contains("..") {
                    return Err(RavynError::Invalid("invalid conversion extension".into()));
                }
                let output = current.with_extension(extension.trim_start_matches('.'));
                security::validate_output_path(&config, &output)?;
                let mut args: Vec<std::ffi::OsString> = vec![
                    "-nostdin".into(),
                    "-protocol_whitelist".into(),
                    "file".into(),
                    "-y".into(),
                    "-i".into(),
                    current.as_os_str().into(),
                ];
                if let Some(preset) = preset {
                    args.extend(preset_arguments(*preset).iter().map(Into::into));
                } else {
                    args.extend(arguments.iter().map(Into::into));
                }
                args.push(output.as_os_str().into());
                let output_limit = conversion_output_limit(&current).await?;
                let conversion = process(
                    &config.ffmpeg,
                    args,
                    Some(&output),
                    Some(output_limit),
                    cancellation.child_token(),
                )
                .await;
                if let Err(error) = conversion {
                    let is_avif = extension
                        .trim_start_matches('.')
                        .eq_ignore_ascii_case("avif");
                    if !is_avif
                        || !is_common_image(&current)
                        || matches!(error, RavynError::Cancelled)
                    {
                        return Err(error);
                    }
                    if fs::try_exists(&output).await? {
                        fs::remove_file(&output).await?;
                    }
                    tracing::warn!(%error, "FFmpeg AVIF conversion failed; trying the dedicated image converter");
                    process(
                        &config.image_converter,
                        vec![
                            current.as_os_str().into(),
                            "-quality".into(),
                            config.avif_quality.to_string().into(),
                            output.as_os_str().into(),
                        ],
                        Some(&output),
                        Some(output_limit),
                        cancellation.child_token(),
                    )
                    .await?;
                }
                if *delete_original {
                    fs::remove_file(&current).await?;
                }
                output
            }
            PostAction::Move { destination } => {
                security::validate_output_path(&config, destination)?;
                fs::create_dir_all(destination.parent().unwrap_or(Path::new("."))).await?;
                move_without_replacement(&current, destination).await?;
                destination.clone()
            }
            PostAction::Open => {
                open_path(&current, cancellation.child_token()).await?;
                current
            }
        };
    }
    Ok(current)
}

fn preset_arguments(preset: FfmpegPreset) -> &'static [&'static str] {
    match preset {
        FfmpegPreset::VideoCopy => &["-map", "0", "-c", "copy"],
        FfmpegPreset::VideoH264 => &["-c:v", "libx264", "-c:a", "aac"],
        FfmpegPreset::VideoH265 => &["-c:v", "libx265", "-c:a", "aac"],
        FfmpegPreset::VideoAv1 => &["-c:v", "libaom-av1", "-c:a", "libopus"],
        FfmpegPreset::AudioMp3 => &["-vn", "-c:a", "libmp3lame"],
        FfmpegPreset::AudioAac => &["-vn", "-c:a", "aac"],
        FfmpegPreset::AudioOpus => &["-vn", "-c:a", "libopus"],
        FfmpegPreset::AudioFlac => &["-vn", "-c:a", "flac"],
        FfmpegPreset::ImageAvif => &["-frames:v", "1", "-c:v", "libaom-av1"],
        FfmpegPreset::ImageWebp => &["-frames:v", "1", "-c:v", "libwebp"],
    }
}

async fn validate_extracted_tree(config: &Config, archive: &Path, root: &Path) -> Result<()> {
    let archive_bytes = fs::metadata(archive).await?.len().max(1);
    let maximum_bytes = config.max_extract_mib.saturating_mul(1024 * 1024);
    let ratio_bytes = archive_bytes.saturating_mul(config.max_extract_ratio);
    let permitted_bytes = maximum_bytes.min(ratio_bytes);
    let canonical_root = fs::canonicalize(root).await?;
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut files = 0_usize;
    let mut expanded_bytes = 0_u64;

    while let Some((directory, depth)) = pending.pop() {
        if depth > config.max_extract_depth {
            return Err(RavynError::Invalid(format!(
                "archive exceeds the maximum extraction depth of {}",
                config.max_extract_depth
            )));
        }
        let mut entries = fs::read_dir(&directory).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).await?;
            let file_type = metadata.file_type();
            if file_type.is_symlink() || (!file_type.is_file() && !file_type.is_dir()) {
                return Err(RavynError::Invalid(format!(
                    "archive contains a link or special file: {}",
                    path.display()
                )));
            }
            let canonical = fs::canonicalize(&path).await?;
            if !canonical.starts_with(&canonical_root) {
                return Err(RavynError::Invalid(format!(
                    "archive entry escaped the staging directory: {}",
                    path.display()
                )));
            }
            if file_type.is_dir() {
                pending.push((path, depth + 1));
                continue;
            }
            files = files.saturating_add(1);
            expanded_bytes = expanded_bytes.saturating_add(metadata.len());
            if files > config.max_extract_files {
                return Err(RavynError::Invalid(format!(
                    "archive contains more than {} files",
                    config.max_extract_files
                )));
            }
            if expanded_bytes > permitted_bytes {
                return Err(RavynError::Invalid(format!(
                    "archive expands beyond the configured size or compression-ratio limit ({permitted_bytes} bytes)"
                )));
            }
        }
    }
    Ok(())
}

fn is_common_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tif" | "tiff"
            )
        })
}

async fn process<I>(
    program: &Path,
    args: I,
    output_path: Option<&Path>,
    output_limit_bytes: Option<u64>,
    cancellation: CancellationToken,
) -> Result<()>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let mut command = Command::new(program);
    command.args(args);
    let limits = process_supervisor::ProcessLimits {
        output_file_bytes: output_path.and(output_limit_bytes),
        ..process_supervisor::ProcessLimits::default()
    };
    run_checked_process(
        &mut command,
        &limits,
        output_path.map(Path::to_path_buf),
        cancellation,
    )
    .await
}

async fn open_path(path: &Path, cancellation: CancellationToken) -> Result<()> {
    if cancellation.is_cancelled() {
        return Err(RavynError::Cancelled);
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("explorer.exe");
        c.arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    process_supervisor::hide_console_window(&mut command);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(false);
    command.spawn()?;
    Ok(())
}

async fn run_checked_process(
    command: &mut Command,
    limits: &process_supervisor::ProcessLimits,
    output_path: Option<PathBuf>,
    cancellation: CancellationToken,
) -> Result<()> {
    let output = process_supervisor::run(command, limits, output_path, cancellation).await?;
    if !output.status.success() {
        let stderr = process_supervisor::redact_sensitive_output(&String::from_utf8_lossy(&output.stderr));
        let stderr = stderr.chars().take(4096).collect::<String>();
        return Err(RavynError::Process(format!(
            "external process exited with {}; stderr: {stderr}",
            output.status
        )));
    }
    Ok(())
}

async fn archive_expansion_limit(config: &Config, archive: &Path) -> Result<u64> {
    let archive_bytes = fs::metadata(archive).await?.len().max(1);
    Ok(config
        .max_extract_mib
        .saturating_mul(1024 * 1024)
        .min(archive_bytes.saturating_mul(config.max_extract_ratio)))
}

async fn preflight_archive(
    config: &Config,
    archive: &Path,
    cancellation: CancellationToken,
) -> Result<()> {
    let permitted_bytes = archive_expansion_limit(config, archive).await?;
    let mut command = Command::new(&config.seven_zip);
    command.args([
        std::ffi::OsString::from("l"),
        std::ffi::OsString::from("-slt"),
        std::ffi::OsString::from("--"),
        archive.as_os_str().into(),
    ]);
    let limits = process_supervisor::ProcessLimits {
        stdout_bytes: 16 * 1024 * 1024,
        ..process_supervisor::ProcessLimits::default()
    };
    let output = process_supervisor::run(&mut command, &limits, None, cancellation).await?;
    if !output.status.success() {
        let stderr = process_supervisor::redact_sensitive_output(&String::from_utf8_lossy(&output.stderr));
        return Err(RavynError::Process(format!(
            "7-Zip archive listing failed with {}; stderr: {}",
            output.status,
            stderr.chars().take(4096).collect::<String>()
        )));
    }
    if output.stdout_truncated {
        return Err(RavynError::Invalid(
            "archive listing exceeded the preflight metadata limit".into(),
        ));
    }

    let listing = String::from_utf8_lossy(&output.stdout);
    let mut in_entries = false;
    let mut current_path: Option<String> = None;
    let mut current_size: Option<u64> = None;
    let mut current_folder = false;
    let mut files = 0_usize;
    let mut expanded_bytes = 0_u64;

    let mut finish_entry = |path: &mut Option<String>, size: &mut Option<u64>, folder: &mut bool| -> Result<()> {
        let Some(value) = path.take() else {
            *size = None;
            *folder = false;
            return Ok(());
        };
        if *folder {
            *size = None;
            *folder = false;
            return Ok(());
        }
        let entry = Path::new(&value);
        let mut depth = 0_usize;
        for component in entry.components() {
            match component {
                std::path::Component::Normal(_) => depth = depth.saturating_add(1),
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_) => {
                    return Err(RavynError::Invalid(format!(
                        "archive contains an unsafe path: {value}"
                    )));
                }
            }
        }
        if depth.saturating_sub(1) > config.max_extract_depth {
            return Err(RavynError::Invalid(format!(
                "archive exceeds the maximum extraction depth of {}",
                config.max_extract_depth
            )));
        }
        files = files.saturating_add(1);
        expanded_bytes = expanded_bytes.saturating_add(size.take().unwrap_or_default());
        if files > config.max_extract_files {
            return Err(RavynError::Invalid(format!(
                "archive contains more than {} files",
                config.max_extract_files
            )));
        }
        if expanded_bytes > permitted_bytes {
            return Err(RavynError::Invalid(format!(
                "archive expands beyond the configured size or compression-ratio limit ({permitted_bytes} bytes)"
            )));
        }
        *folder = false;
        Ok(())
    };

    for line in listing.lines() {
        let line = line.trim_end();
        if line == "----------" {
            in_entries = true;
            continue;
        }
        if !in_entries {
            continue;
        }
        if line.is_empty() {
            finish_entry(&mut current_path, &mut current_size, &mut current_folder)?;
            continue;
        }
        if let Some(value) = line.strip_prefix("Path = ") {
            current_path = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("Size = ") {
            current_size = value.parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("Folder = ") {
            current_folder = value.trim() == "+";
        }
    }
    finish_entry(&mut current_path, &mut current_size, &mut current_folder)?;
    Ok(())
}

async fn conversion_output_limit(input: &Path) -> Result<u64> {
    let input_bytes = fs::metadata(input).await?.len().max(1);
    // Conversion can legitimately expand a highly compressed source. Scale the
    // limit with the input rather than imposing a fixed 10 GiB ceiling.
    Ok(input_bytes
        .saturating_mul(32)
        .max(1024 * 1024 * 1024))
}

async fn move_without_replacement(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source).await?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(RavynError::Invalid(
            "move source must be a regular file".into(),
        ));
    }

    if fs::try_exists(destination).await? {
        return Err(RavynError::Conflict(format!(
            "move destination already exists: {}",
            destination.display()
        )));
    }

    // A hard link gives us an atomic create-if-absent activation on the same
    // filesystem. Unlike rename on Unix, it cannot silently replace a file
    // that appears after the existence check above.
    match fs::hard_link(source, destination).await {
        Ok(()) => {
            if let Err(error) = fs::remove_file(source).await {
                let _ = fs::remove_file(destination).await;
                return Err(error.into());
            }
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(RavynError::Conflict(format!(
                "move destination already exists: {}",
                destination.display()
            )));
        }
        Err(_) => {}
    }

    let parent = destination
        .parent()
        .ok_or_else(|| RavynError::Invalid("move destination has no parent".into()))?;
    let temporary = parent.join(format!(".ravyn-move-{}.tmp", uuid::Uuid::new_v4()));
    let copy_result = async {
        let mut input = fs::File::open(source).await?;
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .await?;
        tokio::io::copy(&mut input, &mut output).await?;
        output.sync_all().await?;
        fs::set_permissions(&temporary, metadata.permissions()).await?;

        // The temporary file lives beside the destination, so hard-linking it
        // is an atomic no-clobber activation even if another process races us.
        match fs::hard_link(&temporary, destination).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(RavynError::Conflict(format!(
                    "move destination already exists: {}",
                    destination.display()
                )));
            }
            Err(error) => return Err(error.into()),
        }
        fs::remove_file(&temporary).await?;
        if let Err(error) = fs::remove_file(source).await {
            let _ = fs::remove_file(destination).await;
            return Err(error.into());
        }
        Ok::<(), RavynError>(())
    }
    .await;
    if copy_result.is_err() {
        let _ = fs::remove_file(&temporary).await;
    }
    copy_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_presets_expand_to_bounded_static_arguments() {
        for preset in [
            FfmpegPreset::VideoCopy,
            FfmpegPreset::VideoH264,
            FfmpegPreset::VideoH265,
            FfmpegPreset::VideoAv1,
            FfmpegPreset::AudioMp3,
            FfmpegPreset::AudioAac,
            FfmpegPreset::AudioOpus,
            FfmpegPreset::AudioFlac,
            FfmpegPreset::ImageAvif,
            FfmpegPreset::ImageWebp,
        ] {
            let arguments = preset_arguments(preset);
            assert!(!arguments.is_empty());
            assert!(arguments.len() <= 8);
            assert!(!arguments.iter().any(|argument| argument.contains("://")));
        }
    }

    #[test]
    fn preset_names_match_the_public_contract() {
        let value = serde_json::to_value(FfmpegPreset::VideoH265).unwrap();
        assert_eq!(value, "video-h265");
        let value = serde_json::to_value(FfmpegPreset::ImageAvif).unwrap();
        assert_eq!(value, "image-avif");
    }
}
