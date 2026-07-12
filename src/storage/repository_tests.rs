//! Storage-layer integration and fault-injection tests, kept out of the
//! per-topic modules because each test crosses several of them.

use std::path::PathBuf;
use std::str::FromStr;

use chrono::{Duration, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use uuid::Uuid;

use crate::{
    core::models::{CreateJob, DownloadOptions, Job, JobKind, OutputSourceKind, OutputType},
    error::RavynError,
    storage::{JobListFilter, Repository},
};

#[cfg(test)]
mod resume_identity_tests {
    use super::*;
    use crate::{
        core::models::{DownloadOptions, DuplicatePolicy},
        storage::segments::{self, SegmentRecord},
    };

    async fn repository() -> (tempfile::TempDir, Repository) {
        let temp = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", temp.path().join("test.sqlite3").display());
        let repository = Repository::connect(&url).await.unwrap();
        (temp, repository)
    }

    async fn job(repository: &Repository) -> Job {
        repository
            .insert_job(
                CreateJob {
                    kind: JobKind::Http,
                    source: "https://example.test/file.bin".into(),
                    destination: Some(PathBuf::from("downloads")),
                    filename: Some("file.bin".into()),
                    priority: 0,
                    speed_limit_bps: None,
                    expected_sha256: None,
                    duplicate_policy: DuplicatePolicy::Allow,
                    options: DownloadOptions::default(),
                },
                PathBuf::from("downloads"),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn changed_validator_invalidates_segments_even_when_job_progress_is_zero() {
        let (_temp, repository) = repository().await;
        let job = job(&repository).await;
        assert!(
            !repository
                .set_remote_identity(
                    job.id,
                    "https://example.test/file.bin",
                    Some("\"v1\""),
                    None,
                    Some(1024),
                    None,
                )
                .await
                .unwrap()
        );
        repository
            .set_transfer_mode(job.id, "segmented")
            .await
            .unwrap();
        segments::replace(
            repository.pool(),
            job.id,
            &[SegmentRecord {
                index: 0,
                start: 0,
                end: 1023,
                downloaded: 512,
                completed: false,
            }],
        )
        .await
        .unwrap();

        let reset = repository
            .set_remote_identity(
                job.id,
                "https://example.test/file.bin",
                Some("\"v2\""),
                None,
                Some(1024),
                Some(1024),
            )
            .await
            .unwrap();
        assert!(reset);
        assert!(
            segments::list(repository.pool(), job.id)
                .await
                .unwrap()
                .is_empty()
        );
        let refreshed = repository.get_job(job.id).await.unwrap();
        assert_eq!(refreshed.downloaded_bytes, 0);
        assert_eq!(refreshed.transfer_mode, "none");
    }

    #[tokio::test]
    async fn missing_segmented_partial_file_invalidates_resume_state() {
        let (_temp, repository) = repository().await;
        let job = job(&repository).await;
        repository
            .set_remote_identity(
                job.id,
                "https://example.test/file.bin",
                Some("\"v1\""),
                None,
                Some(1024),
                None,
            )
            .await
            .unwrap();
        repository
            .set_transfer_mode(job.id, "segmented")
            .await
            .unwrap();
        segments::replace(
            repository.pool(),
            job.id,
            &[SegmentRecord {
                index: 0,
                start: 0,
                end: 1023,
                downloaded: 128,
                completed: false,
            }],
        )
        .await
        .unwrap();

        assert!(
            repository
                .set_remote_identity(
                    job.id,
                    "https://example.test/file.bin",
                    Some("\"v1\""),
                    None,
                    Some(1024),
                    None,
                )
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn idempotency_records_round_trip() {
        let (_temp, repository) = repository().await;
        let job = job(&repository).await;
        repository
            .put_idempotent_resource("create_job", "request-1", "abc", job.id)
            .await
            .unwrap();
        assert_eq!(
            repository
                .get_idempotent_resource("create_job", "request-1")
                .await
                .unwrap(),
            Some(("abc".into(), job.id.to_string()))
        );
    }

    #[tokio::test]
    async fn job_pages_are_bounded_and_cursor_stable() {
        let (_temp, repository) = repository().await;
        for _ in 0..3 {
            job(&repository).await;
        }
        let first = repository
            .list_jobs_page(JobListFilter {
                limit: 1,
                ..JobListFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(first.len(), 2);
        let second = repository
            .list_jobs_page(JobListFilter {
                cursor: Some(first[0].id),
                limit: 1,
                ..JobListFilter::default()
            })
            .await
            .unwrap();
        assert!(!second.is_empty());
        assert_ne!(first[0].id, second[0].id);
    }

    #[tokio::test]
    async fn outputs_are_registered_idempotently_and_confined() {
        let (temp, repository) = repository().await;
        let destination = temp.path().join("downloads");
        tokio::fs::create_dir_all(&destination).await.unwrap();
        let job = repository
            .insert_job(
                CreateJob {
                    kind: JobKind::Http,
                    source: "https://example.test/file.bin".into(),
                    destination: Some(destination.clone()),
                    filename: Some("file.bin".into()),
                    priority: 0,
                    speed_limit_bps: None,
                    expected_sha256: None,
                    duplicate_policy: DuplicatePolicy::Allow,
                    options: DownloadOptions::default(),
                },
                destination.clone(),
            )
            .await
            .unwrap();
        let path = destination.join("file.bin");
        tokio::fs::write(&path, b"ravyn").await.unwrap();
        let first = repository
            .register_output(&job, &path, OutputType::Primary, OutputSourceKind::Http)
            .await
            .unwrap();
        let second = repository
            .register_output(&job, &path, OutputType::Primary, OutputSourceKind::Http)
            .await
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(first.relative_path, PathBuf::from("file.bin"));
        assert_eq!(first.size_bytes, Some(5));
        assert_eq!(first.mime_type.as_deref(), None);
        repository
            .set_output_checksum(first.id, "sha256", "0123456789abcdef")
            .await
            .unwrap();
        let converted = destination.join("file.txt");
        tokio::fs::write(&converted, b"ravyn converted")
            .await
            .unwrap();
        let derived = repository
            .register_derived_output(
                &job,
                first.id,
                &converted,
                OutputType::ConvertedFile,
                0,
                serde_json::json!({"action": "convert_media"}),
            )
            .await
            .unwrap();
        assert_eq!(derived.parent_output_id, Some(first.id));
        assert_eq!(derived.producing_action_index, Some(0));
        assert_eq!(derived.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(repository.list_job_outputs(job.id).await.unwrap().len(), 2);
        assert!(
            repository
                .register_output(
                    &job,
                    temp.path(),
                    OutputType::Directory,
                    OutputSourceKind::Http,
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn persistent_settings_round_trip_and_reset() {
        use clap::Parser;

        let (_temp, repository) = repository().await;
        let config = crate::config::Config::parse_from(["ravyn"]);
        let mut settings = crate::config::PersistentSettings::from_config(&config);
        settings.max_active = 7;
        repository
            .save_persistent_settings(&settings)
            .await
            .unwrap();
        assert_eq!(
            repository
                .load_persistent_settings()
                .await
                .unwrap()
                .unwrap()
                .max_active,
            7
        );
        repository.reset_persistent_settings().await.unwrap();
        assert!(
            repository
                .load_persistent_settings()
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn online_backup_preserves_database_integrity() {
        let (temp, repository) = repository().await;
        job(&repository).await;
        let backup = temp.path().join("backup.sqlite3");
        repository.backup_to(&backup).await.unwrap();
        let backup_repository = Repository::connect(&format!("sqlite://{}", backup.display()))
            .await
            .unwrap();
        assert_eq!(backup_repository.integrity_check().await.unwrap(), "ok");
        assert_eq!(backup_repository.list_jobs().await.unwrap().len(), 1);
    }
}

#[cfg(test)]
mod fault_injection_tests {
    use super::*;
    use crate::{
        core::models::DuplicatePolicy,
        services::schedules::{ScheduleInput, ScheduleOverlapPolicy},
    };

    const LEASE: std::time::Duration = std::time::Duration::from_secs(30);

    async fn repository() -> (tempfile::TempDir, String, Repository) {
        let temp = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", temp.path().join("faults.sqlite3").display());
        let repository = Repository::connect(&url).await.unwrap();
        (temp, url, repository)
    }

    async fn job(repository: &Repository) -> Job {
        repository
            .insert_job(
                CreateJob {
                    kind: JobKind::Http,
                    source: "https://example.test/file.bin".into(),
                    destination: Some(PathBuf::from("downloads")),
                    filename: Some("file.bin".into()),
                    priority: 0,
                    speed_limit_bps: None,
                    expected_sha256: None,
                    duplicate_policy: DuplicatePolicy::Allow,
                    options: DownloadOptions::default(),
                },
                PathBuf::from("downloads"),
            )
            .await
            .unwrap()
    }

    async fn due_schedule(repository: &Repository, overlap: ScheduleOverlapPolicy) -> Uuid {
        let schedule = repository
            .create_schedule(ScheduleInput {
                source: "https://example.test/feed.bin".into(),
                destination: PathBuf::from("downloads"),
                interval_seconds: Some(3_600),
                overlap_policy: overlap,
                ..ScheduleInput::default()
            })
            .await
            .unwrap();
        force_due(repository, schedule.id).await;
        schedule.id
    }

    async fn force_due(repository: &Repository, id: Uuid) {
        sqlx::query("UPDATE schedules SET next_run_at=? WHERE id=?")
            .bind(Utc::now() - Duration::seconds(5))
            .bind(id.to_string())
            .execute(repository.pool())
            .await
            .unwrap();
    }

    async fn expire_lease(repository: &Repository, id: Uuid) {
        sqlx::query("UPDATE schedules SET claim_until=? WHERE id=?")
            .bind(Utc::now() - Duration::seconds(5))
            .bind(id.to_string())
            .execute(repository.pool())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn concurrent_schedule_claims_have_exactly_one_owner() {
        let (_temp, _url, repository) = repository().await;
        due_schedule(&repository, ScheduleOverlapPolicy::Queue).await;
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let repository = repository.clone();
            tasks.spawn(async move {
                repository
                    .claim_due_schedule(LEASE)
                    .await
                    .unwrap()
                    .is_some()
            });
        }
        let mut owners = 0;
        while let Some(result) = tasks.join_next().await {
            if result.unwrap() {
                owners += 1;
            }
        }
        assert_eq!(owners, 1);
    }

    #[tokio::test]
    async fn a_lost_lease_is_reclaimable_and_the_stale_owner_conflicts() {
        let (_temp, _url, repository) = repository().await;
        due_schedule(&repository, ScheduleOverlapPolicy::Queue).await;
        let stale = repository.claim_due_schedule(LEASE).await.unwrap().unwrap();
        assert!(
            repository
                .claim_due_schedule(LEASE)
                .await
                .unwrap()
                .is_none()
        );

        expire_lease(&repository, stale.schedule.id).await;
        let fresh = repository.claim_due_schedule(LEASE).await.unwrap().unwrap();
        assert_ne!(stale.token, fresh.token);

        assert!(matches!(
            repository.renew_schedule_claim(&stale, LEASE).await,
            Err(RavynError::Conflict(_))
        ));
        assert!(matches!(
            repository.complete_schedule_claim(&stale).await,
            Err(RavynError::Conflict(_))
        ));
        repository.complete_schedule_claim(&fresh).await.unwrap();
    }

    #[tokio::test]
    async fn replace_overlap_policy_requests_cancellation_of_the_running_execution() {
        let (_temp, _url, repository) = repository().await;
        let schedule_id = due_schedule(&repository, ScheduleOverlapPolicy::Replace).await;
        let first = repository.claim_due_schedule(LEASE).await.unwrap().unwrap();
        let execution_id = repository
            .begin_schedule_execution(&first)
            .await
            .unwrap()
            .unwrap();

        expire_lease(&repository, schedule_id).await;
        force_due(&repository, schedule_id).await;
        let second = repository.claim_due_schedule(LEASE).await.unwrap().unwrap();
        assert_ne!(first.token, second.token);

        let execution = repository
            .get_schedule_execution(execution_id)
            .await
            .unwrap();
        assert!(
            execution
                .error
                .as_deref()
                .is_some_and(|error| error.contains("replaced")),
            "running execution was not marked for replacement: {execution:?}"
        );
    }

    #[tokio::test]
    async fn idempotency_replays_equal_payloads_and_rejects_changed_payloads() {
        let (_temp, _url, repository) = repository().await;
        let first = job(&repository).await;
        let second = job(&repository).await;
        repository
            .put_idempotent_resource("create_job", "key-1", "hash-a", first.id)
            .await
            .unwrap();

        // A replay with the same payload finds the original resource.
        let stored = repository
            .get_idempotent_resource("create_job", "key-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, ("hash-a".into(), first.id.to_string()));

        // A conflicting write for the same key cannot overwrite the record.
        assert!(
            repository
                .put_idempotent_resource("create_job", "key-1", "hash-b", second.id)
                .await
                .is_err()
        );
        let unchanged = repository
            .get_idempotent_resource("create_job", "key-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(unchanged, ("hash-a".into(), first.id.to_string()));
    }

    #[tokio::test]
    async fn sqlite_busy_errors_increment_the_process_wide_counter() {
        let (_temp, url, repository) = repository().await;
        job(&repository).await;

        let connect = |timeout: std::time::Duration| {
            let options = SqliteConnectOptions::from_str(&url)
                .unwrap()
                .busy_timeout(timeout);
            SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
        };
        let holder = connect(std::time::Duration::from_secs(5)).await.unwrap();
        let contender = connect(std::time::Duration::ZERO).await.unwrap();

        let mut write_lock = holder.begin().await.unwrap();
        sqlx::query("UPDATE jobs SET priority=priority+1")
            .execute(&mut *write_lock)
            .await
            .unwrap();

        let busy = sqlx::query("UPDATE jobs SET priority=priority+1")
            .execute(&contender)
            .await
            .expect_err("a zero-timeout writer must fail while another write is active");

        let counter = |encoded: &str| {
            encoded
                .lines()
                .find_map(|line| line.strip_prefix("ravyn_sqlite_busy_total "))
                .and_then(|value| value.trim().parse::<u64>().ok())
                .expect("busy counter line must exist")
        };
        let before = counter(&crate::core::metrics::Metrics::default().encode_openmetrics());
        let _ = RavynError::from(busy);
        let after = counter(&crate::core::metrics::Metrics::default().encode_openmetrics());
        assert!(
            after > before,
            "busy counter did not increase: {before} -> {after}"
        );
        write_lock.rollback().await.unwrap();
    }
}
