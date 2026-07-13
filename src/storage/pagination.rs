use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    core::models::JobOutput,
    error::{RavynError, Result},
    services::{browser::BrowserTokenRecord, rules::Rule},
    storage::{
        AuditRecord, JobActionRecord, JobLogRecord, PageRecord, PageResourceRecord, Repository,
        Schedule, ScheduleExecutionRecord, SecretReference, TagRecord, TorrentRecord,
        host_profiles::HostProfile,
    },
};

fn limit_i64(limit: usize) -> Result<i64> {
    i64::try_from(limit.clamp(1, 201))
        .map_err(|_| RavynError::Invalid("pagination limit is invalid".into()))
}

fn offset_i64(offset: u64) -> Result<i64> {
    i64::try_from(offset).map_err(|_| RavynError::Invalid("pagination cursor is too large".into()))
}

fn search_pattern(search: Option<&str>) -> Option<String> {
    search
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{value}%"))
}

impl Repository {
    pub async fn list_job_outputs_page(
        &self,
        job_id: Uuid,
        offset: u64,
        limit: usize,
    ) -> Result<Vec<JobOutput>> {
        self.get_job(job_id).await?;
        sqlx::query(
            "SELECT * FROM job_outputs WHERE job_id=? ORDER BY created_at,id LIMIT ? OFFSET ?",
        )
        .bind(job_id.to_string())
        .bind(limit_i64(limit)?)
        .bind(offset_i64(offset)?)
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(super::outputs::row_to_output)
        .collect()
    }

    pub async fn list_job_actions_page(
        &self,
        job_id: Uuid,
        offset: u64,
        limit: usize,
    ) -> Result<Vec<JobActionRecord>> {
        self.get_job(job_id).await?;
        sqlx::query("SELECT id,job_id,action_index,action_json,input_path,output_path,state,attempts,error,created_at,updated_at FROM job_actions WHERE job_id=? ORDER BY action_index,id LIMIT ? OFFSET ?")
            .bind(job_id.to_string())
            .bind(limit_i64(limit)?)
            .bind(offset_i64(offset)?)
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_job_action)
            .collect()
    }

    pub async fn list_job_logs_page(
        &self,
        job_id: Uuid,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<JobLogRecord>> {
        self.get_job(job_id).await?;
        let pattern = search_pattern(search);
        if let Some(pattern) = pattern {
            sqlx::query("SELECT id,job_id,timestamp,source_module,severity,code,message,metadata_json FROM job_logs WHERE job_id=? AND (message LIKE ? OR code LIKE ? OR source_module LIKE ?) ORDER BY timestamp DESC,id DESC LIMIT ? OFFSET ?")
                .bind(job_id.to_string())
                .bind(&pattern).bind(&pattern).bind(&pattern)
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::audit::row_to_job_log).collect()
        } else {
            sqlx::query("SELECT id,job_id,timestamp,source_module,severity,code,message,metadata_json FROM job_logs WHERE job_id=? ORDER BY timestamp DESC,id DESC LIMIT ? OFFSET ?")
                .bind(job_id.to_string())
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::audit::row_to_job_log).collect()
        }
    }

    pub async fn list_audit_page(
        &self,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<AuditRecord>> {
        let pattern = search_pattern(search);
        if let Some(pattern) = pattern {
            sqlx::query("SELECT id,timestamp,action,resource_type,resource_id,outcome,metadata_json,previous_hash,entry_hash FROM audit_log WHERE action LIKE ? OR resource_type LIKE ? OR COALESCE(resource_id,'') LIKE ? ORDER BY timestamp DESC,id DESC LIMIT ? OFFSET ?")
                .bind(&pattern).bind(&pattern).bind(&pattern)
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::audit::row_to_audit).collect()
        } else {
            sqlx::query("SELECT id,timestamp,action,resource_type,resource_id,outcome,metadata_json,previous_hash,entry_hash FROM audit_log ORDER BY timestamp DESC,id DESC LIMIT ? OFFSET ?")
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::audit::row_to_audit).collect()
        }
    }

    pub async fn list_secret_references_page(
        &self,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<SecretReference>> {
        let pattern = search_pattern(search);
        if let Some(pattern) = pattern {
            sqlx::query("SELECT id,name,secret_type,created_at,updated_at FROM secret_references WHERE name LIKE ? OR secret_type LIKE ? ORDER BY name COLLATE NOCASE,id LIMIT ? OFFSET ?")
                .bind(&pattern).bind(&pattern)
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::secrets::row_to_secret_reference).collect()
        } else {
            sqlx::query("SELECT id,name,secret_type,created_at,updated_at FROM secret_references ORDER BY name COLLATE NOCASE,id LIMIT ? OFFSET ?")
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::secrets::row_to_secret_reference).collect()
        }
    }

    pub async fn list_schedule_executions_page(
        &self,
        schedule_id: Uuid,
        offset: u64,
        limit: usize,
    ) -> Result<Vec<ScheduleExecutionRecord>> {
        self.get_schedule(schedule_id).await?;
        sqlx::query("SELECT id,schedule_id,intended_run_at,state,summary_json,error,started_at,completed_at FROM schedule_executions WHERE schedule_id=? ORDER BY started_at DESC,id DESC LIMIT ? OFFSET ?")
            .bind(schedule_id.to_string())
            .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
            .fetch_all(self.pool()).await?.into_iter()
            .map(super::schedules::row_to_schedule_execution).collect()
    }

    pub async fn list_rules_page(
        &self,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<Rule>> {
        let pattern = search_pattern(search);
        if let Some(pattern) = pattern {
            sqlx::query("SELECT id,name,enabled,priority,matcher_json,actions_json FROM rules WHERE name LIKE ? OR matcher_json LIKE ? ORDER BY priority DESC,created_at ASC,id LIMIT ? OFFSET ?")
                .bind(&pattern).bind(&pattern)
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::automation::row_to_rule).collect()
        } else {
            sqlx::query("SELECT id,name,enabled,priority,matcher_json,actions_json FROM rules ORDER BY priority DESC,created_at ASC,id LIMIT ? OFFSET ?")
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::automation::row_to_rule).collect()
        }
    }

    pub async fn list_tags_page(
        &self,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<TagRecord>> {
        let pattern = search_pattern(search);
        let rows = if let Some(pattern) = pattern {
            sqlx::query("SELECT tags.id,tags.name,COUNT(job_tags.job_id) AS job_count FROM tags LEFT JOIN job_tags ON job_tags.tag_id=tags.id WHERE tags.name LIKE ? GROUP BY tags.id,tags.name ORDER BY tags.name COLLATE NOCASE,tags.id LIMIT ? OFFSET ?")
                .bind(pattern).bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?
        } else {
            sqlx::query("SELECT tags.id,tags.name,COUNT(job_tags.job_id) AS job_count FROM tags LEFT JOIN job_tags ON job_tags.tag_id=tags.id GROUP BY tags.id,tags.name ORDER BY tags.name COLLATE NOCASE,tags.id LIMIT ? OFFSET ?")
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?
        };
        rows.into_iter().map(row_to_tag).collect()
    }

    pub async fn list_browser_tokens_page(
        &self,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<BrowserTokenRecord>> {
        let pattern = search_pattern(search);
        if let Some(pattern) = pattern {
            sqlx::query("SELECT id,name,allowed_origins_json,created_at,last_used_at,revoked_at FROM browser_tokens WHERE name LIKE ? OR allowed_origins_json LIKE ? ORDER BY created_at DESC,id DESC LIMIT ? OFFSET ?")
                .bind(&pattern).bind(&pattern)
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::automation::row_to_browser_token).collect()
        } else {
            sqlx::query("SELECT id,name,allowed_origins_json,created_at,last_used_at,revoked_at FROM browser_tokens ORDER BY created_at DESC,id DESC LIMIT ? OFFSET ?")
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::automation::row_to_browser_token).collect()
        }
    }

    pub async fn list_pages_page(
        &self,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<PageRecord>> {
        let pattern = search_pattern(search);
        let rows = if let Some(pattern) = pattern {
            sqlx::query("SELECT page_url,COUNT(*) AS resource_count,SUM(CASE WHEN last_imported_at IS NOT NULL THEN 1 ELSE 0 END) AS imported_count,MIN(first_seen_at) AS first_seen_at,MAX(last_seen_at) AS last_seen_at FROM page_resources WHERE page_url LIKE ? GROUP BY page_url ORDER BY last_seen_at DESC,page_url LIMIT ? OFFSET ?")
                .bind(pattern).bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?
        } else {
            sqlx::query("SELECT page_url,COUNT(*) AS resource_count,SUM(CASE WHEN last_imported_at IS NOT NULL THEN 1 ELSE 0 END) AS imported_count,MIN(first_seen_at) AS first_seen_at,MAX(last_seen_at) AS last_seen_at FROM page_resources GROUP BY page_url ORDER BY last_seen_at DESC,page_url LIMIT ? OFFSET ?")
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?
        };
        rows.into_iter().map(row_to_page).collect()
    }

    pub async fn list_page_resources_page(
        &self,
        page_url: &str,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<PageResourceRecord>> {
        let pattern = search_pattern(search);
        let rows = if let Some(pattern) = pattern {
            sqlx::query("SELECT page_url,resource_url,resource_kind,first_seen_at,last_seen_at,last_imported_at FROM page_resources WHERE page_url=? AND (resource_url LIKE ? OR resource_kind LIKE ?) ORDER BY resource_url LIMIT ? OFFSET ?")
                .bind(page_url).bind(&pattern).bind(&pattern)
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?
        } else {
            sqlx::query("SELECT page_url,resource_url,resource_kind,first_seen_at,last_seen_at,last_imported_at FROM page_resources WHERE page_url=? ORDER BY resource_url LIMIT ? OFFSET ?")
                .bind(page_url).bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?
        };
        rows.into_iter().map(row_to_page_resource).collect()
    }

    pub async fn list_schedules_page(
        &self,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<Schedule>> {
        let pattern = search_pattern(search);
        let select = "SELECT id,enabled,source,kind,destination,mode,automation_json,interval_seconds,cron_expression,next_run_at,timezone_offset_minutes,timezone_name,overlap_policy,missed_run_policy,max_catch_up_runs,catch_up_runs,paused_until,options_json,last_run_at,failure_count,last_error,created_at,updated_at FROM schedules";
        let rows = if let Some(pattern) = pattern {
            sqlx::query(&format!("{select} WHERE source LIKE ? OR destination LIKE ? ORDER BY next_run_at ASC,id LIMIT ? OFFSET ?"))
                .bind(&pattern).bind(&pattern)
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?
        } else {
            sqlx::query(&format!(
                "{select} ORDER BY next_run_at ASC,id LIMIT ? OFFSET ?"
            ))
            .bind(limit_i64(limit)?)
            .bind(offset_i64(offset)?)
            .fetch_all(self.pool())
            .await?
        };
        rows.into_iter().map(Schedule::from_row).collect()
    }

    pub async fn list_torrent_records_page(
        &self,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<TorrentRecord>> {
        let pattern = search_pattern(search);
        if let Some(pattern) = pattern {
            sqlx::query("SELECT job_id,torrent_id,info_hash,name,state,downloaded_bytes,uploaded_bytes,total_bytes,download_speed_bps,upload_speed_bps,peers_connected,seeders,leechers,raw_json,updated_at FROM torrent_jobs WHERE torrent_id LIKE ? OR COALESCE(info_hash,'') LIKE ? OR COALESCE(name,'') LIKE ? OR state LIKE ? ORDER BY updated_at DESC,job_id DESC LIMIT ? OFFSET ?")
                .bind(&pattern).bind(&pattern).bind(&pattern).bind(&pattern)
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::torrent_policy::row_to_torrent_record).collect()
        } else {
            sqlx::query("SELECT job_id,torrent_id,info_hash,name,state,downloaded_bytes,uploaded_bytes,total_bytes,download_speed_bps,upload_speed_bps,peers_connected,seeders,leechers,raw_json,updated_at FROM torrent_jobs ORDER BY updated_at DESC,job_id DESC LIMIT ? OFFSET ?")
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?.into_iter()
                .map(super::torrent_policy::row_to_torrent_record).collect()
        }
    }

    pub async fn list_host_profiles_page(
        &self,
        offset: u64,
        limit: usize,
        search: Option<&str>,
    ) -> Result<Vec<HostProfile>> {
        let pattern = search_pattern(search);
        let rows = if let Some(pattern) = pattern {
            sqlx::query("SELECT host,successful_downloads,failed_downloads,consecutive_failures,average_throughput_bps,range_failures,circuit_open_until,last_error FROM host_profiles WHERE host LIKE ? ORDER BY updated_at DESC,host LIMIT ? OFFSET ?")
                .bind(pattern).bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?
        } else {
            sqlx::query("SELECT host,successful_downloads,failed_downloads,consecutive_failures,average_throughput_bps,range_failures,circuit_open_until,last_error FROM host_profiles ORDER BY updated_at DESC,host LIMIT ? OFFSET ?")
                .bind(limit_i64(limit)?).bind(offset_i64(offset)?)
                .fetch_all(self.pool()).await?
        };
        rows.into_iter().map(row_to_host_profile).collect()
    }
}

fn row_to_job_action(row: SqliteRow) -> Result<JobActionRecord> {
    Ok(JobActionRecord {
        id: parse_uuid(&row, "id")?,
        job_id: parse_uuid(&row, "job_id")?,
        action_index: usize::try_from(row.try_get::<i64, _>("action_index")?)
            .map_err(|_| RavynError::Internal("invalid job action index".into()))?,
        action: serde_json::from_str(&row.try_get::<String, _>("action_json")?)?,
        input_path: row.try_get::<String, _>("input_path")?.into(),
        output_path: row
            .try_get::<Option<String>, _>("output_path")?
            .map(Into::into),
        state: row.try_get("state")?,
        attempts: u64::try_from(row.try_get::<i64, _>("attempts")?).unwrap_or_default(),
        error: row.try_get("error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_tag(row: SqliteRow) -> Result<TagRecord> {
    Ok(TagRecord {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        job_count: u64::try_from(row.try_get::<i64, _>("job_count")?).unwrap_or_default(),
    })
}

fn row_to_page(row: SqliteRow) -> Result<PageRecord> {
    Ok(PageRecord {
        page_url: row.try_get("page_url")?,
        resource_count: u64::try_from(row.try_get::<i64, _>("resource_count")?).unwrap_or_default(),
        imported_count: u64::try_from(row.try_get::<i64, _>("imported_count")?).unwrap_or_default(),
        first_seen_at: row.try_get("first_seen_at")?,
        last_seen_at: row.try_get("last_seen_at")?,
    })
}

fn row_to_page_resource(row: SqliteRow) -> Result<PageResourceRecord> {
    Ok(PageResourceRecord {
        page_url: row.try_get("page_url")?,
        resource_url: row.try_get("resource_url")?,
        resource_kind: row.try_get("resource_kind")?,
        first_seen_at: row.try_get("first_seen_at")?,
        last_seen_at: row.try_get("last_seen_at")?,
        last_imported_at: row.try_get("last_imported_at")?,
    })
}

fn row_to_host_profile(row: SqliteRow) -> Result<HostProfile> {
    Ok(HostProfile {
        host: row.try_get("host")?,
        successful_downloads: u64::try_from(row.try_get::<i64, _>("successful_downloads")?.max(0))
            .unwrap_or_default(),
        failed_downloads: u64::try_from(row.try_get::<i64, _>("failed_downloads")?.max(0))
            .unwrap_or_default(),
        consecutive_failures: u32::try_from(row.try_get::<i64, _>("consecutive_failures")?.max(0))
            .unwrap_or_default(),
        average_throughput_bps: row
            .try_get::<Option<i64>, _>("average_throughput_bps")?
            .map(|value| u64::try_from(value.max(0)).unwrap_or_default()),
        range_failures: u32::try_from(row.try_get::<i64, _>("range_failures")?.max(0))
            .unwrap_or_default(),
        circuit_open_until: row.try_get("circuit_open_until")?,
        last_error: row.try_get("last_error")?,
    })
}

fn parse_uuid(row: &SqliteRow, column: &str) -> Result<Uuid> {
    Uuid::parse_str(&row.try_get::<String, _>(column)?)
        .map_err(|error| RavynError::Internal(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::models::{CreateJob, DownloadOptions, DuplicatePolicy, JobKind},
        services::rules::{RuleActions, RuleMatcher},
        storage::RuleInput,
    };

    async fn repository() -> (tempfile::TempDir, Repository) {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("ravyn.sqlite3");
        let repository = Repository::connect(&format!("sqlite://{}", database.display()))
            .await
            .unwrap();
        (temp, repository)
    }

    #[tokio::test]
    async fn rule_pages_are_bounded_and_searchable() {
        let (_temp, repository) = repository().await;
        for name in ["Images", "Media", "Archives"] {
            repository
                .create_rule(RuleInput {
                    name: name.into(),
                    enabled: true,
                    priority: 0,
                    matcher: RuleMatcher::default(),
                    actions: RuleActions::default(),
                })
                .await
                .unwrap();
        }
        let first = repository.list_rules_page(0, 2, None).await.unwrap();
        let second = repository.list_rules_page(2, 2, None).await.unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 1);
        let searched = repository
            .list_rules_page(0, 2, Some("Media"))
            .await
            .unwrap();
        assert_eq!(searched.len(), 1);
        assert_eq!(searched[0].name, "Media");
    }

    #[tokio::test]
    async fn output_pages_do_not_load_the_full_job() {
        let (temp, repository) = repository().await;
        let destination = temp.path().join("downloads");
        tokio::fs::create_dir_all(&destination).await.unwrap();
        let job = repository
            .insert_job(
                CreateJob {
                    preset_id: None,
                    kind: JobKind::Http,
                    source: "https://example.test/file".into(),
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
        for index in 0..3 {
            let path = destination.join(format!("file-{index}.bin"));
            tokio::fs::write(&path, [index as u8]).await.unwrap();
            repository
                .register_output(
                    &job,
                    &path,
                    crate::core::models::OutputType::Other,
                    crate::core::models::OutputSourceKind::Http,
                )
                .await
                .unwrap();
        }
        assert_eq!(
            repository
                .list_job_outputs_page(job.id, 0, 2)
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            repository
                .list_job_outputs_page(job.id, 2, 2)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
