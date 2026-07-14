use std::path::PathBuf;

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    config::PersistentSettings,
    error::{RavynError, Result},
    services::library::{
        LibraryMoveConflictPolicy, LibraryMoveItemRecord, LibraryMovePlan, LibraryMoveState,
        LibraryMoveStatus,
    },
    storage::Repository,
};

impl Repository {
    pub(crate) async fn create_library_move(&self, plan: &LibraryMovePlan) -> Result<()> {
        let now = Utc::now();
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO library_move_transactions(\
                id,source_root,destination_root,conflict_policy,state,total_files,total_bytes,\
                copied_files,copied_bytes,verified_files,reused_files,missing_files,external_entries,\
                conflict_files,cancel_requested,restart_required,started_at,updated_at\
             ) VALUES(?,?,?,?, 'running',?,?,?,?,?,?,?,?,?,0,0,?,?)",
        )
        .bind(plan.id.to_string())
        .bind(plan.source_root.to_string_lossy().to_string())
        .bind(plan.destination_root.to_string_lossy().to_string())
        .bind(plan.conflict_policy.as_str())
        .bind(to_i64(plan.total_files, "Library move file count")?)
        .bind(to_i64_u64(plan.total_bytes, "Library move byte count")?)
        .bind(0_i64)
        .bind(0_i64)
        .bind(0_i64)
        .bind(0_i64)
        .bind(to_i64(plan.missing_files, "Library move missing count")?)
        .bind(to_i64(plan.external_entries, "Library move external count")?)
        .bind(to_i64(plan.conflict_files, "Library move conflict count")?)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        for item in &plan.items {
            sqlx::query(
                "INSERT INTO library_move_items(\
                    transaction_id,entry_id,source_path,destination_path,source_entry_path,\
                    destination_entry_path,was_trashed,expected_sha256,size_bytes,state,\
                    created_destination,error,updated_at\
                 ) VALUES(?,?,?,?,?,?,?,?,?,?,0,NULL,?)",
            )
            .bind(plan.id.to_string())
            .bind(item.entry_id.to_string())
            .bind(item.source_path.to_string_lossy().to_string())
            .bind(item.destination_path.to_string_lossy().to_string())
            .bind(item.source_entry_path.to_string_lossy().to_string())
            .bind(item.destination_entry_path.to_string_lossy().to_string())
            .bind(item.was_trashed)
            .bind(item.expected_sha256.as_deref())
            .bind(to_i64_u64(item.size_bytes, "Library move item size")?)
            .bind(if item.missing { "missing" } else { "pending" })
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn latest_library_move_status(&self) -> Result<Option<LibraryMoveStatus>> {
        sqlx::query(
            "SELECT * FROM library_move_transactions ORDER BY updated_at DESC,id DESC LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await?
        .map(row_to_status)
        .transpose()
    }

    pub async fn active_library_move_status(&self) -> Result<Option<LibraryMoveStatus>> {
        sqlx::query(
            "SELECT * FROM library_move_transactions \
             WHERE state IN ('running','cancelling','restart_required') \
             ORDER BY updated_at DESC,id DESC LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await?
        .map(row_to_status)
        .transpose()
    }

    pub async fn get_library_move_status(&self, id: Uuid) -> Result<Option<LibraryMoveStatus>> {
        sqlx::query("SELECT * FROM library_move_transactions WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .map(row_to_status)
            .transpose()
    }

    pub async fn library_move_blocks_new_jobs(&self) -> Result<bool> {
        let value: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM library_move_transactions \
             WHERE state IN ('running','cancelling','restart_required') LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(value.is_some())
    }

    pub(crate) async fn library_move_cancel_requested(&self, id: Uuid) -> Result<bool> {
        let value: Option<i64> = sqlx::query_scalar(
            "SELECT cancel_requested FROM library_move_transactions WHERE id=?",
        )
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?;
        value
            .map(|value| value != 0)
            .ok_or_else(|| RavynError::NotFound(format!("library move {id}")))
    }

    pub(crate) async fn list_library_move_items(
        &self,
        id: Uuid,
    ) -> Result<Vec<LibraryMoveItemRecord>> {
        sqlx::query(
            "SELECT entry_id,source_path,destination_path,source_entry_path,destination_entry_path,\
                    was_trashed,expected_sha256,size_bytes,state,created_destination \
             FROM library_move_items WHERE transaction_id=? ORDER BY entry_id",
        )
        .bind(id.to_string())
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|row| {
            Ok(LibraryMoveItemRecord {
                entry_id: Uuid::parse_str(&row.try_get::<String, _>("entry_id")?)
                    .map_err(|error| RavynError::Internal(error.to_string()))?,
                source_path: PathBuf::from(row.try_get::<String, _>("source_path")?),
                destination_path: PathBuf::from(
                    row.try_get::<String, _>("destination_path")?,
                ),
                source_entry_path: PathBuf::from(
                    row.try_get::<String, _>("source_entry_path")?,
                ),
                destination_entry_path: PathBuf::from(
                    row.try_get::<String, _>("destination_entry_path")?,
                ),
                was_trashed: row.try_get("was_trashed")?,
                expected_sha256: row.try_get("expected_sha256")?,
                size_bytes: from_i64_u64(
                    row.try_get::<i64, _>("size_bytes")?,
                    "Library move item size",
                )?,
                state: row.try_get("state")?,
                created_destination: row.try_get("created_destination")?,
            })
        })
        .collect()
    }

    pub(crate) async fn update_library_move_item(
        &self,
        transaction_id: Uuid,
        entry_id: Uuid,
        state: &str,
        expected_sha256: Option<&str>,
        created_destination: bool,
        error: Option<&str>,
    ) -> Result<()> {
        if !matches!(
            state,
            "pending"
                | "copying"
                | "committing"
                | "verified"
                | "reused"
                | "missing"
                | "source_removed"
                | "failed"
        ) {
            return Err(RavynError::Invalid(format!(
                "invalid Library move item state {state}"
            )));
        }
        let changed = sqlx::query(
            "UPDATE library_move_items SET state=?,expected_sha256=COALESCE(?,expected_sha256),\
                    created_destination=?,error=?,updated_at=? \
             WHERE transaction_id=? AND entry_id=?",
        )
        .bind(state)
        .bind(expected_sha256)
        .bind(created_destination)
        .bind(error)
        .bind(Utc::now())
        .bind(transaction_id.to_string())
        .bind(entry_id.to_string())
        .execute(self.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!(
                "library move item {transaction_id}/{entry_id}"
            )));
        }
        Ok(())
    }

    pub(crate) async fn recalculate_library_move_progress(&self, id: Uuid) -> Result<()> {
        let row = sqlx::query(
            "SELECT \
                COALESCE(SUM(CASE WHEN created_destination=1 AND state IN ('verified','source_removed') THEN 1 ELSE 0 END),0) AS copied_files,\
                COALESCE(SUM(CASE WHEN created_destination=1 AND state IN ('verified','source_removed') THEN size_bytes ELSE 0 END),0) AS copied_bytes,\
                COALESCE(SUM(CASE WHEN state IN ('verified','reused','source_removed') THEN 1 ELSE 0 END),0) AS verified_files,\
                COALESCE(SUM(CASE WHEN state='reused' THEN 1 ELSE 0 END),0) AS reused_files,\
                COALESCE(SUM(CASE WHEN state='missing' THEN 1 ELSE 0 END),0) AS missing_files \
             FROM library_move_items WHERE transaction_id=?",
        )
        .bind(id.to_string())
        .fetch_one(self.pool())
        .await?;
        let changed = sqlx::query(
            "UPDATE library_move_transactions SET copied_files=?,copied_bytes=?,verified_files=?,\
                    reused_files=?,missing_files=?,updated_at=? WHERE id=?",
        )
        .bind(row.try_get::<i64, _>("copied_files")?)
        .bind(row.try_get::<i64, _>("copied_bytes")?)
        .bind(row.try_get::<i64, _>("verified_files")?)
        .bind(row.try_get::<i64, _>("reused_files")?)
        .bind(row.try_get::<i64, _>("missing_files")?)
        .bind(Utc::now())
        .bind(id.to_string())
        .execute(self.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("library move {id}")));
        }
        Ok(())
    }

    pub(crate) async fn request_library_move_cancel(&self, id: Uuid) -> Result<()> {
        let changed = sqlx::query(
            "UPDATE library_move_transactions SET state='cancelling',cancel_requested=1,updated_at=? \
             WHERE id=? AND state='running'",
        )
        .bind(Utc::now())
        .bind(id.to_string())
        .execute(self.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(RavynError::Conflict(
                "the Library move can no longer be cancelled".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn activate_library_move(
        &self,
        id: Uuid,
        settings: &PersistentSettings,
    ) -> Result<()> {
        let now = Utc::now();
        let mut tx = self.pool().begin().await?;
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM library_move_transactions WHERE id=?")
                .bind(id.to_string())
                .fetch_optional(&mut *tx)
                .await?;
        if state.as_deref() != Some("running") {
            return Err(RavynError::Conflict(
                "the Library move is not ready for activation".into(),
            ));
        }
        let items = sqlx::query(
            "SELECT entry_id,destination_path,destination_entry_path,was_trashed,state \n             FROM library_move_items \
             WHERE transaction_id=? ORDER BY entry_id",
        )
        .bind(id.to_string())
        .fetch_all(&mut *tx)
        .await?;
        for item in items {
            let state: String = item.try_get("state")?;
            if !matches!(state.as_str(), "verified" | "reused" | "missing") {
                return Err(RavynError::Conflict(format!(
                    "Library move item is not verified: {}",
                    item.try_get::<String, _>("entry_id")?
                )));
            }
            let destination: String = item.try_get("destination_path")?;
            let destination_entry: String = item.try_get("destination_entry_path")?;
            let was_trashed: bool = item.try_get("was_trashed")?;
            let changed = sqlx::query(
                "UPDATE library_entries SET path=?,trash_path=?,updated_at=? WHERE id=?",
            )
            .bind(&destination_entry)
            .bind(was_trashed.then_some(destination))
            .bind(now)
            .bind(item.try_get::<String, _>("entry_id")?)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if changed == 0 {
                return Err(RavynError::NotFound(
                    "a Library entry disappeared during relocation".into(),
                ));
            }
        }
        sqlx::query(
            "INSERT INTO runtime_settings(id,settings_json,updated_at) VALUES(1,?,?) \
             ON CONFLICT(id) DO UPDATE SET settings_json=excluded.settings_json,\
                updated_at=excluded.updated_at",
        )
        .bind(serde_json::to_string(settings)?)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE library_move_transactions SET state='restart_required',restart_required=1,\
                    cancel_requested=0,error=NULL,updated_at=? WHERE id=?",
        )
        .bind(now)
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn rollback_library_move_activation(
        &self,
        id: Uuid,
        settings: &PersistentSettings,
        reason: &str,
    ) -> Result<()> {
        let now = Utc::now();
        let mut tx = self.pool().begin().await?;
        let items = sqlx::query(
            "SELECT entry_id,source_path,source_entry_path,was_trashed FROM library_move_items \
             WHERE transaction_id=? ORDER BY entry_id",
        )
        .bind(id.to_string())
        .fetch_all(&mut *tx)
        .await?;
        for item in items {
            let source: String = item.try_get("source_path")?;
            let source_entry: String = item.try_get("source_entry_path")?;
            let was_trashed: bool = item.try_get("was_trashed")?;
            sqlx::query(
                "UPDATE library_entries SET path=?,trash_path=?,updated_at=? WHERE id=?",
            )
            .bind(&source_entry)
            .bind(was_trashed.then_some(source))
            .bind(now)
            .bind(item.try_get::<String, _>("entry_id")?)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO runtime_settings(id,settings_json,updated_at) VALUES(1,?,?) \
             ON CONFLICT(id) DO UPDATE SET settings_json=excluded.settings_json,\
                updated_at=excluded.updated_at",
        )
        .bind(serde_json::to_string(settings)?)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE library_move_transactions SET state='rolled_back',restart_required=0,\
                    cancel_requested=0,error=?,updated_at=?,completed_at=? WHERE id=?",
        )
        .bind(reason)
        .bind(now)
        .bind(now)
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn finish_library_move(
        &self,
        id: Uuid,
        state: LibraryMoveState,
        error: Option<&str>,
        restart_required: bool,
    ) -> Result<()> {
        if !matches!(
            state,
            LibraryMoveState::Cancelled
                | LibraryMoveState::Failed
                | LibraryMoveState::Completed
                | LibraryMoveState::RolledBack
        ) {
            return Err(RavynError::Invalid(format!(
                "Library move cannot be finished in state {}",
                state.as_str()
            )));
        }
        let now = Utc::now();
        let changed = sqlx::query(
            "UPDATE library_move_transactions SET state=?,restart_required=?,cancel_requested=0,\
                    error=?,updated_at=?,completed_at=? WHERE id=?",
        )
        .bind(state.as_str())
        .bind(restart_required)
        .bind(error)
        .bind(now)
        .bind(now)
        .bind(id.to_string())
        .execute(self.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("library move {id}")));
        }
        Ok(())
    }
}

fn row_to_status(row: sqlx::sqlite::SqliteRow) -> Result<LibraryMoveStatus> {
    Ok(LibraryMoveStatus {
        run_id: Some(
            Uuid::parse_str(&row.try_get::<String, _>("id")?)
                .map_err(|error| RavynError::Internal(error.to_string()))?,
        ),
        state: LibraryMoveState::from_str(&row.try_get::<String, _>("state")?)?,
        source_root: Some(PathBuf::from(row.try_get::<String, _>("source_root")?)),
        destination_root: Some(PathBuf::from(
            row.try_get::<String, _>("destination_root")?,
        )),
        conflict_policy: LibraryMoveConflictPolicy::from_str(
            &row.try_get::<String, _>("conflict_policy")?,
        )?,
        total_files: from_i64(
            row.try_get::<i64, _>("total_files")?,
            "Library move total files",
        )?,
        total_bytes: from_i64_u64(
            row.try_get::<i64, _>("total_bytes")?,
            "Library move total bytes",
        )?,
        copied_files: from_i64(
            row.try_get::<i64, _>("copied_files")?,
            "Library move copied files",
        )?,
        copied_bytes: from_i64_u64(
            row.try_get::<i64, _>("copied_bytes")?,
            "Library move copied bytes",
        )?,
        verified_files: from_i64(
            row.try_get::<i64, _>("verified_files")?,
            "Library move verified files",
        )?,
        reused_files: from_i64(
            row.try_get::<i64, _>("reused_files")?,
            "Library move reused files",
        )?,
        missing_files: from_i64(
            row.try_get::<i64, _>("missing_files")?,
            "Library move missing files",
        )?,
        external_entries: from_i64(
            row.try_get::<i64, _>("external_entries")?,
            "Library move external entries",
        )?,
        conflict_files: from_i64(
            row.try_get::<i64, _>("conflict_files")?,
            "Library move conflict files",
        )?,
        cancel_requested: row.try_get("cancel_requested")?,
        restart_required: row.try_get("restart_required")?,
        error: row.try_get("error")?,
        started_at: Some(row.try_get::<DateTime<Utc>, _>("started_at")?),
        updated_at: Some(row.try_get::<DateTime<Utc>, _>("updated_at")?),
        completed_at: row.try_get("completed_at")?,
    })
}

fn to_i64(value: usize, label: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| RavynError::Invalid(format!("{label} is too large")))
}

fn to_i64_u64(value: u64, label: &str) -> Result<i64> {
    i64::try_from(value).map_err(|_| RavynError::Invalid(format!("{label} is too large")))
}

fn from_i64(value: i64, label: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| RavynError::Internal(format!("{label} is negative")))
}

fn from_i64_u64(value: i64, label: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| RavynError::Internal(format!("{label} is negative")))
}
