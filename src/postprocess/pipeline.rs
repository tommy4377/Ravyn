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
                let extraction = process(
                    &config.seven_zip,
                    vec![
                        "x".into(),
                        current.as_os_str().into(),
                        format!("-o{}", staging.display()).into(),
                    ],
                    None,
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
                let conversion = process(
                    &config.ffmpeg,
                    args,
                    Some(&output),
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
                match fs::rename(&current, destination).await {
                    Ok(()) => {}
                    Err(error) if error.raw_os_error() == Some(18) => {
                        fs::copy(&current, destination).await?;
                        fs::remove_file(&current).await?;
                    }
                    Err(error) => return Err(error.into()),
                }
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
    cancellation: CancellationToken,
) -> Result<()>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let mut command = Command::new(program);
    command.args(args);
    let limits = process_supervisor::ProcessLimits {
        output_file_bytes: output_path.map(|_| 10 * 1024 * 1024 * 1024),
        ..process_supervisor::ProcessLimits::default()
    };
    let output = process_supervisor::run(
        &mut command,
        &limits,
        output_path.map(Path::to_path_buf),
        cancellation,
    )
    .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.chars().take(4096).collect::<String>();
        return Err(RavynError::Process(format!(
            "external process exited with {}; stderr: {stderr}",
            output.status
        )));
    }
    Ok(())
}

async fn open_path(path: &Path, cancellation: CancellationToken) -> Result<()> {
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
    command.kill_on_drop(true);
    let mut child = command.spawn()?;
    tokio::select! {
        _ = cancellation.cancelled() => { let _=child.kill().await; Err(RavynError::Cancelled) }
        status = child.wait() => { let status=status?; if status.success(){Ok(())}else{Err(RavynError::Process(format!("open command exited with {status}"))) } }
    }
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
