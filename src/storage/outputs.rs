//! First-class output artifacts and their lineage.

use std::path::PathBuf;

use chrono::Utc;
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    core::models::{Job, JobOutput, OutputSourceKind, OutputState, OutputType},
    error::{RavynError, Result},
    storage::Repository,
};

impl Repository {
    pub async fn register_output(
        &self,
        job: &Job,
        path: &std::path::Path,
        output_type: OutputType,
        source_kind: OutputSourceKind,
    ) -> Result<JobOutput> {
        self.register_output_with_metadata(
            job,
            path,
            output_type,
            source_kind,
            serde_json::Value::Object(Default::default()),
        )
        .await
    }

    pub async fn register_output_with_metadata(
        &self,
        job: &Job,
        path: &std::path::Path,
        output_type: OutputType,
        source_kind: OutputSourceKind,
        metadata_json: serde_json::Value,
    ) -> Result<JobOutput> {
        let metadata = tokio::fs::metadata(path).await?;
        let destination = std::path::Path::new(&job.destination);
        let relative = path.strip_prefix(destination).map_err(|_| {
            RavynError::Invalid(format!(
                "output is outside the job destination: {}",
                path.display()
            ))
        })?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        let size = if metadata.is_file() {
            Some(i64::try_from(metadata.len()).map_err(|_| {
                RavynError::Invalid("output size exceeds SQLite integer range".into())
            })?)
        } else {
            None
        };
        let mime_type = inferred_mime_type(path, metadata.is_dir());
        sqlx::query("INSERT INTO job_outputs(id,job_id,output_type,original_path,current_path,relative_path,size_bytes,mime_type,state,source_kind,metadata_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?, 'ready',?,?,?,?) ON CONFLICT(job_id,original_path) DO UPDATE SET current_path=excluded.current_path,relative_path=excluded.relative_path,size_bytes=excluded.size_bytes,mime_type=excluded.mime_type,output_type=excluded.output_type,state='ready',source_kind=excluded.source_kind,metadata_json=excluded.metadata_json,updated_at=excluded.updated_at")
            .bind(id.to_string())
            .bind(job.id.to_string())
            .bind(output_type_text(output_type))
            .bind(path.to_string_lossy().to_string())
            .bind(path.to_string_lossy().to_string())
            .bind(relative.to_string_lossy().to_string())
            .bind(size)
            .bind(mime_type)
            .bind(output_source_text(source_kind))
            .bind(serde_json::to_string(&metadata_json)?)
            .bind(now)
            .bind(now)
            .execute(self.pool())
            .await?;
        self.get_output_by_original(job.id, path).await
    }

    pub async fn update_output_path(
        &self,
        job: &Job,
        output_id: Uuid,
        current_path: &std::path::Path,
        state: OutputState,
    ) -> Result<()> {
        let relative = relative_output_path(job, current_path)?;
        let metadata = tokio::fs::metadata(current_path).await.ok();
        let size = metadata
            .as_ref()
            .filter(|value| value.is_file())
            .map(|value| i64::try_from(value.len()))
            .transpose()
            .map_err(|_| RavynError::Invalid("output size exceeds SQLite integer range".into()))?;
        let mime_type = inferred_mime_type(
            current_path,
            metadata.as_ref().is_some_and(|value| value.is_dir()),
        );
        sqlx::query("UPDATE job_outputs SET current_path=?,relative_path=?,size_bytes=COALESCE(?,size_bytes),mime_type=COALESCE(?,mime_type),state=?,updated_at=? WHERE id=?")
            .bind(current_path.to_string_lossy().to_string())
            .bind(relative.to_string_lossy().to_string())
            .bind(size)
            .bind(mime_type)
            .bind(output_state_text(state))
            .bind(Utc::now())
            .bind(output_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn set_output_state(&self, output_id: Uuid, state: OutputState) -> Result<()> {
        sqlx::query("UPDATE job_outputs SET state=?,updated_at=? WHERE id=?")
            .bind(output_state_text(state))
            .bind(Utc::now())
            .bind(output_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn set_output_checksum(
        &self,
        output_id: Uuid,
        algorithm: &str,
        value: &str,
    ) -> Result<()> {
        if algorithm.trim().is_empty() || value.trim().is_empty() {
            return Err(RavynError::Invalid(
                "checksum algorithm and value must not be empty".into(),
            ));
        }
        sqlx::query(
            "UPDATE job_outputs SET checksum_algorithm=?,checksum_value=?,updated_at=? WHERE id=?",
        )
        .bind(algorithm.to_ascii_lowercase())
        .bind(value.to_ascii_lowercase())
        .bind(Utc::now())
        .bind(output_id.to_string())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn register_derived_output(
        &self,
        job: &Job,
        parent_output_id: Uuid,
        path: &std::path::Path,
        output_type: OutputType,
        action_index: usize,
        metadata: serde_json::Value,
    ) -> Result<JobOutput> {
        let file_metadata = tokio::fs::metadata(path).await?;
        let relative = relative_output_path(job, path)?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        let size = if file_metadata.is_file() {
            Some(i64::try_from(file_metadata.len()).map_err(|_| {
                RavynError::Invalid("output size exceeds SQLite integer range".into())
            })?)
        } else {
            None
        };
        let action_index = i64::try_from(action_index)
            .map_err(|_| RavynError::Invalid("post-action index exceeds SQLite range".into()))?;
        sqlx::query("INSERT INTO job_outputs(id,job_id,output_type,original_path,current_path,relative_path,size_bytes,mime_type,state,source_kind,parent_output_id,producing_action_index,metadata_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?, 'ready','post_process',?,?,?,?,?) ON CONFLICT(job_id,original_path) DO UPDATE SET current_path=excluded.current_path,relative_path=excluded.relative_path,size_bytes=excluded.size_bytes,mime_type=excluded.mime_type,output_type=excluded.output_type,state='ready',source_kind='post_process',parent_output_id=excluded.parent_output_id,producing_action_index=excluded.producing_action_index,metadata_json=excluded.metadata_json,updated_at=excluded.updated_at")
            .bind(id.to_string())
            .bind(job.id.to_string())
            .bind(output_type_text(output_type))
            .bind(path.to_string_lossy().to_string())
            .bind(path.to_string_lossy().to_string())
            .bind(relative.to_string_lossy().to_string())
            .bind(size)
            .bind(inferred_mime_type(path, file_metadata.is_dir()))
            .bind(parent_output_id.to_string())
            .bind(action_index)
            .bind(serde_json::to_string(&metadata)?)
            .bind(now)
            .bind(now)
            .execute(self.pool())
            .await?;
        self.get_output_by_original(job.id, path).await
    }

    async fn get_output_by_original(
        &self,
        job_id: Uuid,
        path: &std::path::Path,
    ) -> Result<JobOutput> {
        sqlx::query("SELECT * FROM job_outputs WHERE job_id=? AND original_path=?")
            .bind(job_id.to_string())
            .bind(path.to_string_lossy().to_string())
            .fetch_optional(self.pool())
            .await?
            .map(row_to_output)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("output for {}", path.display())))
    }

    pub async fn list_job_outputs(&self, job_id: Uuid) -> Result<Vec<JobOutput>> {
        sqlx::query("SELECT * FROM job_outputs WHERE job_id=? ORDER BY created_at,id")
            .bind(job_id.to_string())
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_output)
            .collect()
    }

    pub async fn find_job_output_by_path(
        &self,
        job_id: Uuid,
        path: &std::path::Path,
    ) -> Result<Option<JobOutput>> {
        let path = path.to_string_lossy().to_string();
        sqlx::query(
            "SELECT * FROM job_outputs WHERE job_id=? AND (original_path=? OR current_path=?) ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(job_id.to_string())
        .bind(&path)
        .bind(&path)
        .fetch_optional(self.pool())
        .await?
        .map(row_to_output)
        .transpose()
    }
}

fn relative_output_path(job: &Job, path: &std::path::Path) -> Result<PathBuf> {
    let destination = std::path::Path::new(&job.destination);
    if let Ok(relative) = path.strip_prefix(destination) {
        return Ok(relative.to_path_buf());
    }
    path.file_name().map(PathBuf::from).ok_or_else(|| {
        RavynError::Invalid(format!(
            "cannot derive a relative output path for {}",
            path.display()
        ))
    })
}

fn inferred_mime_type(path: &std::path::Path, directory: bool) -> Option<&'static str> {
    if directory {
        return Some("inode/directory");
    }
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp4" => Some("video/mp4"),
        "mkv" => Some("video/x-matroska"),
        "webm" => Some("video/webm"),
        "mov" => Some("video/quicktime"),
        "avi" => Some("video/x-msvideo"),
        "mp3" => Some("audio/mpeg"),
        "m4a" => Some("audio/mp4"),
        "aac" => Some("audio/aac"),
        "flac" => Some("audio/flac"),
        "opus" => Some("audio/opus"),
        "wav" => Some("audio/wav"),
        "srt" => Some("application/x-subrip"),
        "vtt" => Some("text/vtt"),
        "json" => Some("application/json"),
        "txt" | "description" => Some("text/plain"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        "gif" => Some("image/gif"),
        "pdf" => Some("application/pdf"),
        "zip" => Some("application/zip"),
        "7z" => Some("application/x-7z-compressed"),
        "rar" => Some("application/vnd.rar"),
        "gz" => Some("application/gzip"),
        "tar" => Some("application/x-tar"),
        "torrent" => Some("application/x-bittorrent"),
        _ => None,
    }
}

pub(crate) fn row_to_output(row: SqliteRow) -> Result<JobOutput> {
    let parse_uuid = |column: &str| -> Result<Uuid> {
        Uuid::parse_str(row.try_get::<String, _>(column)?.as_str())
            .map_err(|error| RavynError::Internal(format!("invalid output {column}: {error}")))
    };
    let size_bytes = row
        .try_get::<Option<i64>, _>("size_bytes")?
        .map(u64::try_from)
        .transpose()
        .map_err(|_| RavynError::Internal("negative output size in database".into()))?;
    Ok(JobOutput {
        id: parse_uuid("id")?,
        job_id: parse_uuid("job_id")?,
        output_type: parse_output_type(&row.try_get::<String, _>("output_type")?)?,
        original_path: PathBuf::from(row.try_get::<String, _>("original_path")?),
        current_path: PathBuf::from(row.try_get::<String, _>("current_path")?),
        relative_path: PathBuf::from(row.try_get::<String, _>("relative_path")?),
        size_bytes,
        mime_type: row.try_get("mime_type")?,
        checksum_algorithm: row.try_get("checksum_algorithm")?,
        checksum_value: row.try_get("checksum_value")?,
        state: parse_output_state(&row.try_get::<String, _>("state")?)?,
        source_kind: parse_output_source(&row.try_get::<String, _>("source_kind")?)?,
        parent_output_id: row
            .try_get::<Option<String>, _>("parent_output_id")?
            .map(|value| Uuid::parse_str(&value))
            .transpose()
            .map_err(|error| {
                RavynError::Internal(format!("invalid parent output UUID: {error}"))
            })?,
        producing_action_index: row
            .try_get::<Option<i64>, _>("producing_action_index")?
            .map(usize::try_from)
            .transpose()
            .map_err(|_| RavynError::Internal("invalid output action index".into()))?,
        metadata: serde_json::from_str(&row.try_get::<String, _>("metadata_json")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn output_type_text(value: OutputType) -> &'static str {
    match value {
        OutputType::Primary => "primary",
        OutputType::Video => "video",
        OutputType::Audio => "audio",
        OutputType::Subtitle => "subtitle",
        OutputType::Thumbnail => "thumbnail",
        OutputType::Metadata => "metadata",
        OutputType::TorrentFile => "torrent_file",
        OutputType::ExtractedFile => "extracted_file",
        OutputType::ConvertedFile => "converted_file",
        OutputType::Archive => "archive",
        OutputType::Directory => "directory",
        OutputType::Temporary => "temporary",
        OutputType::Other => "other",
    }
}

fn output_state_text(value: OutputState) -> &'static str {
    match value {
        OutputState::Planned => "planned",
        OutputState::Creating => "creating",
        OutputState::Ready => "ready",
        OutputState::Failed => "failed",
        OutputState::Deleted => "deleted",
        OutputState::Moved => "moved",
        OutputState::Replaced => "replaced",
    }
}

fn output_source_text(value: OutputSourceKind) -> &'static str {
    match value {
        OutputSourceKind::Http => "http",
        OutputSourceKind::Media => "media",
        OutputSourceKind::Torrent => "torrent",
        OutputSourceKind::PostProcess => "post_process",
    }
}

fn parse_output_type(value: &str) -> Result<OutputType> {
    match value {
        "primary" => Ok(OutputType::Primary),
        "video" => Ok(OutputType::Video),
        "audio" => Ok(OutputType::Audio),
        "subtitle" => Ok(OutputType::Subtitle),
        "thumbnail" => Ok(OutputType::Thumbnail),
        "metadata" => Ok(OutputType::Metadata),
        "torrent_file" => Ok(OutputType::TorrentFile),
        "extracted_file" => Ok(OutputType::ExtractedFile),
        "converted_file" => Ok(OutputType::ConvertedFile),
        "archive" => Ok(OutputType::Archive),
        "directory" => Ok(OutputType::Directory),
        "temporary" => Ok(OutputType::Temporary),
        "other" => Ok(OutputType::Other),
        other => Err(RavynError::Internal(format!("invalid output type {other}"))),
    }
}

fn parse_output_state(value: &str) -> Result<OutputState> {
    match value {
        "planned" => Ok(OutputState::Planned),
        "creating" => Ok(OutputState::Creating),
        "ready" => Ok(OutputState::Ready),
        "failed" => Ok(OutputState::Failed),
        "deleted" => Ok(OutputState::Deleted),
        "moved" => Ok(OutputState::Moved),
        "replaced" => Ok(OutputState::Replaced),
        other => Err(RavynError::Internal(format!(
            "invalid output state {other}"
        ))),
    }
}

fn parse_output_source(value: &str) -> Result<OutputSourceKind> {
    match value {
        "http" => Ok(OutputSourceKind::Http),
        "media" => Ok(OutputSourceKind::Media),
        "torrent" => Ok(OutputSourceKind::Torrent),
        "post_process" => Ok(OutputSourceKind::PostProcess),
        other => Err(RavynError::Internal(format!(
            "invalid output source {other}"
        ))),
    }
}
