use chrono::{DateTime, Utc};
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    error::{RavynError, Result},
    services::{
        browser::BrowserTokenRecord,
        rules::{Rule, RuleActions, RuleMatcher},
    },
    storage::Repository,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuleInput {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub matcher: RuleMatcher,
    #[serde(default)]
    pub actions: RuleActions,
}

impl RuleInput {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.len() > 160 {
            return Err(RavynError::Invalid(
                "rule name must contain 1 to 160 characters".into(),
            ));
        }
        if self.matcher.domains.len() > 256
            || self.matcher.extensions.len() > 256
            || self.matcher.mime_types.len() > 256
            || self
                .matcher
                .domains
                .iter()
                .chain(self.matcher.extensions.iter())
                .chain(self.matcher.mime_types.iter())
                .any(|value| value.trim().is_empty() || value.len() > 255)
        {
            return Err(RavynError::Invalid(
                "rule matchers exceed the configured count or length limits".into(),
            ));
        }
        if let Some(pattern) = self.matcher.url_regex.as_deref() {
            if pattern.len() > 2_048 {
                return Err(RavynError::Invalid(
                    "rule URL regex may not exceed 2048 characters".into(),
                ));
            }
            regex::Regex::new(pattern)
                .map_err(|error| RavynError::Invalid(format!("invalid rule URL regex: {error}")))?;
        }
        if self
            .matcher
            .mime_types
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(RavynError::Invalid(
                "rule MIME patterns may not be empty".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PageRecord {
    pub page_url: String,
    pub resource_count: u64,
    pub imported_count: u64,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PageResourceRecord {
    pub page_url: String,
    pub resource_url: String,
    pub resource_kind: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub last_imported_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TagRecord {
    pub id: i64,
    pub name: String,
    pub job_count: u64,
}

impl Repository {
    pub async fn list_all_rules(&self) -> Result<Vec<Rule>> {
        sqlx::query(
            "SELECT id,name,enabled,priority,matcher_json,actions_json FROM rules ORDER BY priority DESC,created_at ASC",
        )
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(row_to_rule)
        .collect()
    }

    pub async fn get_rule(&self, id: Uuid) -> Result<Rule> {
        let row = sqlx::query(
            "SELECT id,name,enabled,priority,matcher_json,actions_json FROM rules WHERE id=?",
        )
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?;
        row.map(row_to_rule)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("rule {id}")))
    }

    pub async fn create_rule(&self, input: RuleInput) -> Result<Rule> {
        input.validate()?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query("INSERT INTO rules(id,name,enabled,priority,matcher_json,actions_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(id.to_string())
            .bind(input.name.trim())
            .bind(if input.enabled { 1_i64 } else { 0_i64 })
            .bind(input.priority)
            .bind(serde_json::to_string(&input.matcher)?)
            .bind(serde_json::to_string(&input.actions)?)
            .bind(now)
            .bind(now)
            .execute(self.pool())
            .await?;
        self.get_rule(id).await
    }

    pub async fn update_rule(&self, id: Uuid, input: RuleInput) -> Result<Rule> {
        input.validate()?;
        let changed = sqlx::query("UPDATE rules SET name=?,enabled=?,priority=?,matcher_json=?,actions_json=?,updated_at=? WHERE id=?")
            .bind(input.name.trim())
            .bind(if input.enabled { 1_i64 } else { 0_i64 })
            .bind(input.priority)
            .bind(serde_json::to_string(&input.matcher)?)
            .bind(serde_json::to_string(&input.actions)?)
            .bind(Utc::now())
            .bind(id.to_string())
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("rule {id}")));
        }
        self.get_rule(id).await
    }

    pub async fn delete_rule(&self, id: Uuid) -> Result<()> {
        let changed = sqlx::query("DELETE FROM rules WHERE id=?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("rule {id}")));
        }
        Ok(())
    }

    pub async fn list_tags(&self) -> Result<Vec<TagRecord>> {
        let rows = sqlx::query(
            "SELECT tags.id,tags.name,COUNT(job_tags.job_id) AS job_count FROM tags LEFT JOIN job_tags ON job_tags.tag_id=tags.id GROUP BY tags.id,tags.name ORDER BY tags.name COLLATE NOCASE",
        )
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(TagRecord {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    job_count: u64::try_from(row.try_get::<i64, _>("job_count")?)
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    pub async fn list_job_tags(&self, job_id: Uuid) -> Result<Vec<String>> {
        self.get_job(job_id).await?;
        let rows = sqlx::query("SELECT tags.name FROM tags JOIN job_tags ON job_tags.tag_id=tags.id WHERE job_tags.job_id=? ORDER BY tags.name COLLATE NOCASE")
            .bind(job_id.to_string())
            .fetch_all(self.pool())
            .await?;
        rows.into_iter()
            .map(|row| row.try_get("name").map_err(Into::into))
            .collect()
    }

    pub async fn replace_job_tags(&self, job_id: Uuid, tags: &[String]) -> Result<Vec<String>> {
        self.get_job(job_id).await?;
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM job_tags WHERE job_id=?")
            .bind(job_id.to_string())
            .execute(&mut *tx)
            .await?;
        let mut normalized = tags
            .iter()
            .map(|tag| tag.trim())
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        normalized.sort_by_key(|value| value.to_ascii_lowercase());
        normalized.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        if normalized.len() > 64 {
            return Err(RavynError::Invalid("a job may have at most 64 tags".into()));
        }
        for tag in &normalized {
            if tag.len() > 80 {
                return Err(RavynError::Invalid(
                    "tag names may not exceed 80 characters".into(),
                ));
            }
            sqlx::query("INSERT INTO tags(name) VALUES(?) ON CONFLICT(name) DO NOTHING")
                .bind(tag)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO job_tags(job_id,tag_id) SELECT ?,id FROM tags WHERE name=? COLLATE NOCASE ON CONFLICT DO NOTHING")
                .bind(job_id.to_string())
                .bind(tag)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(normalized)
    }

    pub async fn delete_tag(&self, id: i64) -> Result<()> {
        let changed = sqlx::query("DELETE FROM tags WHERE id=?")
            .bind(id)
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("tag {id}")));
        }
        Ok(())
    }

    pub async fn insert_browser_token(
        &self,
        record: &BrowserTokenRecord,
        token_hash: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO browser_tokens(id,name,token_hash,allowed_origins_json,created_at,last_used_at,revoked_at) VALUES(?,?,?,?,?,?,?)")
            .bind(record.id.to_string())
            .bind(&record.name)
            .bind(token_hash)
            .bind(serde_json::to_string(&record.allowed_origins)?)
            .bind(record.created_at)
            .bind(record.last_used_at)
            .bind(record.revoked_at)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn list_browser_tokens(&self) -> Result<Vec<BrowserTokenRecord>> {
        sqlx::query("SELECT id,name,allowed_origins_json,created_at,last_used_at,revoked_at FROM browser_tokens ORDER BY created_at DESC")
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_browser_token)
            .collect()
    }

    pub async fn revoke_browser_token(&self, id: Uuid) -> Result<()> {
        let changed =
            sqlx::query("UPDATE browser_tokens SET revoked_at=? WHERE id=? AND revoked_at IS NULL")
                .bind(Utc::now())
                .bind(id.to_string())
                .execute(self.pool())
                .await?
                .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("active browser token {id}")));
        }
        Ok(())
    }

    pub async fn verify_browser_token(&self, token_hash: &str, origin: &str) -> Result<bool> {
        let row = sqlx::query("SELECT id,allowed_origins_json FROM browser_tokens WHERE token_hash=? AND revoked_at IS NULL")
            .bind(token_hash)
            .fetch_optional(self.pool())
            .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let allowed = serde_json::from_str::<Vec<String>>(
            &row.try_get::<String, _>("allowed_origins_json")?,
        )?;
        if !crate::services::browser::origin_allowed(&allowed, origin) {
            return Ok(false);
        }
        let now = Utc::now();
        sqlx::query("UPDATE browser_tokens SET last_used_at=? WHERE id=? AND (last_used_at IS NULL OR last_used_at<?)")
            .bind(now)
            .bind(row.try_get::<String, _>("id")?)
            .bind(now - chrono::Duration::minutes(1))
            .execute(self.pool())
            .await?;
        Ok(true)
    }

    pub async fn page_resource_exists(&self, page_url: &str, resource_url: &str) -> Result<bool> {
        Ok(
            sqlx::query("SELECT 1 FROM page_resources WHERE page_url=? AND resource_url=?")
                .bind(page_url)
                .bind(resource_url)
                .fetch_optional(self.pool())
                .await?
                .is_some(),
        )
    }

    pub async fn remember_page_resource(
        &self,
        page_url: &str,
        resource_url: &str,
        kind: &str,
        imported: bool,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query("INSERT INTO page_resources(page_url,resource_url,resource_kind,first_seen_at,last_seen_at,last_imported_at) VALUES(?,?,?,?,?,?) ON CONFLICT(page_url,resource_url) DO UPDATE SET resource_kind=excluded.resource_kind,last_seen_at=excluded.last_seen_at,last_imported_at=CASE WHEN excluded.last_imported_at IS NOT NULL THEN excluded.last_imported_at ELSE page_resources.last_imported_at END")
            .bind(page_url)
            .bind(resource_url)
            .bind(kind)
            .bind(now)
            .bind(now)
            .bind(imported.then_some(now))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn list_pages(&self) -> Result<Vec<PageRecord>> {
        let rows = sqlx::query("SELECT page_url,COUNT(*) AS resource_count,SUM(CASE WHEN last_imported_at IS NOT NULL THEN 1 ELSE 0 END) AS imported_count,MIN(first_seen_at) AS first_seen_at,MAX(last_seen_at) AS last_seen_at FROM page_resources GROUP BY page_url ORDER BY last_seen_at DESC")
            .fetch_all(self.pool())
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PageRecord {
                    page_url: row.try_get("page_url")?,
                    resource_count: u64::try_from(row.try_get::<i64, _>("resource_count")?)
                        .unwrap_or_default(),
                    imported_count: u64::try_from(row.try_get::<i64, _>("imported_count")?)
                        .unwrap_or_default(),
                    first_seen_at: row.try_get("first_seen_at")?,
                    last_seen_at: row.try_get("last_seen_at")?,
                })
            })
            .collect()
    }

    pub async fn list_page_resources(&self, page_url: &str) -> Result<Vec<PageResourceRecord>> {
        let rows = sqlx::query("SELECT page_url,resource_url,resource_kind,first_seen_at,last_seen_at,last_imported_at FROM page_resources WHERE page_url=? ORDER BY resource_url")
            .bind(page_url)
            .fetch_all(self.pool())
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PageResourceRecord {
                    page_url: row.try_get("page_url")?,
                    resource_url: row.try_get("resource_url")?,
                    resource_kind: row.try_get("resource_kind")?,
                    first_seen_at: row.try_get("first_seen_at")?,
                    last_seen_at: row.try_get("last_seen_at")?,
                    last_imported_at: row.try_get("last_imported_at")?,
                })
            })
            .collect()
    }

    pub async fn clear_page_history(&self, page_url: Option<&str>) -> Result<u64> {
        let result = match page_url {
            Some(page_url) => {
                sqlx::query("DELETE FROM page_resources WHERE page_url=?")
                    .bind(page_url)
                    .execute(self.pool())
                    .await?
            }
            None => {
                sqlx::query("DELETE FROM page_resources")
                    .execute(self.pool())
                    .await?
            }
        };
        Ok(result.rows_affected())
    }
}

pub(crate) fn row_to_rule(row: SqliteRow) -> Result<Rule> {
    Ok(Rule {
        id: Uuid::parse_str(&row.try_get::<String, _>("id")?)
            .map_err(|error| RavynError::Internal(error.to_string()))?,
        name: row.try_get("name")?,
        enabled: row.try_get::<i64, _>("enabled")? != 0,
        priority: row.try_get("priority")?,
        matcher: serde_json::from_str(&row.try_get::<String, _>("matcher_json")?)?,
        actions: serde_json::from_str(&row.try_get::<String, _>("actions_json")?)?,
    })
}

pub(crate) fn row_to_browser_token(row: SqliteRow) -> Result<BrowserTokenRecord> {
    Ok(BrowserTokenRecord {
        id: Uuid::parse_str(&row.try_get::<String, _>("id")?)
            .map_err(|error| RavynError::Internal(error.to_string()))?,
        name: row.try_get("name")?,
        allowed_origins: serde_json::from_str(&row.try_get::<String, _>("allowed_origins_json")?)?,
        created_at: row.try_get("created_at")?,
        last_used_at: row.try_get("last_used_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
}

fn default_true() -> bool {
    true
}

impl Repository {
    pub async fn list_schedules(&self) -> Result<Vec<crate::storage::Schedule>> {
        sqlx::query(SCHEDULE_SELECT)
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(crate::storage::Schedule::from_row)
            .collect()
    }

    pub async fn get_schedule(&self, id: Uuid) -> Result<crate::storage::Schedule> {
        let row = sqlx::query(&(SCHEDULE_SELECT.to_owned() + " WHERE id=?"))
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?;
        row.map(crate::storage::Schedule::from_row)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("schedule {id}")))
    }

    pub async fn create_schedule(
        &self,
        input: crate::services::schedules::ScheduleInput,
    ) -> Result<crate::storage::Schedule> {
        let now = Utc::now();
        let next_run_at = input.validate(now)?;
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO schedules(id,enabled,source,kind,destination,mode,automation_json,interval_seconds,cron_expression,next_run_at,timezone_offset_minutes,timezone_name,overlap_policy,missed_run_policy,max_catch_up_runs,catch_up_runs,paused_until,options_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,0,?,?,?,?)")
            .bind(id.to_string())
            .bind(if input.enabled { 1_i64 } else { 0_i64 })
            .bind(input.source.trim())
            .bind(job_kind_text(input.kind))
            .bind(input.destination.to_string_lossy().to_string())
            .bind(schedule_mode_text(input.mode))
            .bind(serde_json::to_string(&input.automation)?)
            .bind(input.interval_seconds)
            .bind(input.cron_expression.as_deref().map(str::trim))
            .bind(next_run_at)
            .bind(input.timezone_offset_minutes)
            .bind(input.timezone_name.as_deref().map(str::trim))
            .bind(schedule_overlap_policy_text(input.overlap_policy))
            .bind(schedule_missed_run_policy_text(input.missed_run_policy))
            .bind(i64::from(input.max_catch_up_runs))
            .bind(input.paused_until)
            .bind(serde_json::to_string(&input.options)?)
            .bind(now)
            .bind(now)
            .execute(self.pool())
            .await?;
        self.get_schedule(id).await
    }

    pub async fn update_schedule(
        &self,
        id: Uuid,
        input: crate::services::schedules::ScheduleInput,
    ) -> Result<crate::storage::Schedule> {
        let now = Utc::now();
        let next_run_at = input.validate(now)?;
        let changed = sqlx::query("UPDATE schedules SET enabled=?,source=?,kind=?,destination=?,mode=?,automation_json=?,interval_seconds=?,cron_expression=?,next_run_at=?,timezone_offset_minutes=?,timezone_name=?,overlap_policy=?,missed_run_policy=?,max_catch_up_runs=?,catch_up_runs=0,paused_until=?,options_json=?,claim_token=NULL,claim_until=NULL,failure_count=0,last_error=NULL,updated_at=? WHERE id=?")
            .bind(if input.enabled { 1_i64 } else { 0_i64 })
            .bind(input.source.trim())
            .bind(job_kind_text(input.kind))
            .bind(input.destination.to_string_lossy().to_string())
            .bind(schedule_mode_text(input.mode))
            .bind(serde_json::to_string(&input.automation)?)
            .bind(input.interval_seconds)
            .bind(input.cron_expression.as_deref().map(str::trim))
            .bind(next_run_at)
            .bind(input.timezone_offset_minutes)
            .bind(input.timezone_name.as_deref().map(str::trim))
            .bind(schedule_overlap_policy_text(input.overlap_policy))
            .bind(schedule_missed_run_policy_text(input.missed_run_policy))
            .bind(i64::from(input.max_catch_up_runs))
            .bind(input.paused_until)
            .bind(serde_json::to_string(&input.options)?)
            .bind(now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("schedule {id}")));
        }
        self.get_schedule(id).await
    }

    pub async fn delete_schedule(&self, id: Uuid) -> Result<()> {
        let changed = sqlx::query("DELETE FROM schedules WHERE id=?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?
            .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("schedule {id}")));
        }
        Ok(())
    }
}

const SCHEDULE_SELECT: &str = "SELECT id,enabled,source,kind,destination,mode,automation_json,interval_seconds,cron_expression,next_run_at,timezone_offset_minutes,timezone_name,overlap_policy,missed_run_policy,max_catch_up_runs,catch_up_runs,paused_until,options_json,last_run_at,failure_count,last_error,created_at,updated_at FROM schedules";

fn job_kind_text(kind: crate::core::models::JobKind) -> &'static str {
    match kind {
        crate::core::models::JobKind::Http => "http",
        crate::core::models::JobKind::Media => "media",
        crate::core::models::JobKind::Torrent => "torrent",
    }
}

impl Repository {
    pub async fn existing_page_resources(
        &self,
        page_url: &str,
        resource_urls: &[String],
    ) -> Result<std::collections::HashSet<String>> {
        let mut existing = std::collections::HashSet::new();
        for chunk in resource_urls.chunks(400) {
            if chunk.is_empty() {
                continue;
            }
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT resource_url FROM page_resources WHERE page_url=? AND resource_url IN ({placeholders})"
            );
            let mut query = sqlx::query(&sql).bind(page_url);
            for url in chunk {
                query = query.bind(url);
            }
            for row in query.fetch_all(self.pool()).await? {
                existing.insert(row.try_get::<String, _>("resource_url")?);
            }
        }
        Ok(existing)
    }

    pub async fn remember_page_resources(
        &self,
        page_url: &str,
        resources: &[(String, String, bool)],
    ) -> Result<()> {
        let now = Utc::now();
        let mut tx = self.pool().begin().await?;
        for (resource_url, kind, imported) in resources {
            sqlx::query("INSERT INTO page_resources(page_url,resource_url,resource_kind,first_seen_at,last_seen_at,last_imported_at) VALUES(?,?,?,?,?,?) ON CONFLICT(page_url,resource_url) DO UPDATE SET resource_kind=excluded.resource_kind,last_seen_at=excluded.last_seen_at,last_imported_at=CASE WHEN excluded.last_imported_at IS NOT NULL THEN excluded.last_imported_at ELSE page_resources.last_imported_at END")
                .bind(page_url)
                .bind(resource_url)
                .bind(kind)
                .bind(now)
                .bind(now)
                .bind(imported.then_some(now))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

fn schedule_mode_text(mode: crate::services::schedules::ScheduleMode) -> &'static str {
    match mode {
        crate::services::schedules::ScheduleMode::Download => "download",
        crate::services::schedules::ScheduleMode::SniffResources => "sniff_resources",
    }
}

fn schedule_overlap_policy_text(
    policy: crate::services::schedules::ScheduleOverlapPolicy,
) -> &'static str {
    match policy {
        crate::services::schedules::ScheduleOverlapPolicy::Skip => "skip",
        crate::services::schedules::ScheduleOverlapPolicy::Queue => "queue",
        crate::services::schedules::ScheduleOverlapPolicy::Replace => "replace",
        crate::services::schedules::ScheduleOverlapPolicy::AllowParallel => "allow_parallel",
    }
}

fn schedule_missed_run_policy_text(
    policy: crate::services::schedules::ScheduleMissedRunPolicy,
) -> &'static str {
    match policy {
        crate::services::schedules::ScheduleMissedRunPolicy::Skip => "skip",
        crate::services::schedules::ScheduleMissedRunPolicy::RunOnce => "run_once",
        crate::services::schedules::ScheduleMissedRunPolicy::CatchUp => "catch_up",
    }
}
