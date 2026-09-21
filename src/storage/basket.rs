//! Deferred download basket persistence and stable ordering.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, sqlite::SqliteRow};
use uuid::Uuid;

use crate::{
    core::models::CreateJob,
    error::{RavynError, Result},
    storage::Repository,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasketItem {
    pub id: Uuid,
    pub position: usize,
    pub request: CreateJob,
    pub preset_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutBasketItem {
    pub request: CreateJob,
    pub preset_id: Option<Uuid>,
}

impl Repository {
    pub async fn add_basket_item(&self, input: PutBasketItem) -> Result<BasketItem> {
        if let Some(preset_id) = input.preset_id {
            self.get_download_preset(preset_id).await?;
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        // The position is allocated inside the INSERT statement so concurrent writers
        // cannot observe the same maximum position between a separate read and write.
        sqlx::query(
            "INSERT INTO basket_items(id,position,request_json,preset_id,created_at,updated_at) \
             SELECT ?,COALESCE(MAX(position),-1)+1,?,?,?,? FROM basket_items",
        )
        .bind(id.to_string())
        .bind(serde_json::to_string(&input.request)?)
        .bind(input.preset_id.map(|value| value.to_string()))
        .bind(now)
        .bind(now)
        .execute(self.pool())
        .await?;
        self.get_basket_item(id).await
    }

    pub async fn update_basket_item(&self, id: Uuid, input: PutBasketItem) -> Result<BasketItem> {
        if let Some(preset_id) = input.preset_id {
            self.get_download_preset(preset_id).await?;
        }
        let changed = sqlx::query(
            "UPDATE basket_items SET request_json=?,preset_id=?,updated_at=? WHERE id=?",
        )
        .bind(serde_json::to_string(&input.request)?)
        .bind(input.preset_id.map(|value| value.to_string()))
        .bind(Utc::now())
        .bind(id.to_string())
        .execute(self.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(RavynError::NotFound(format!("basket item {id}")));
        }
        self.get_basket_item(id).await
    }

    pub async fn get_basket_item(&self, id: Uuid) -> Result<BasketItem> {
        sqlx::query("SELECT * FROM basket_items WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?
            .map(row_to_basket_item)
            .transpose()?
            .ok_or_else(|| RavynError::NotFound(format!("basket item {id}")))
    }

    pub async fn list_basket_items(&self) -> Result<Vec<BasketItem>> {
        sqlx::query("SELECT * FROM basket_items ORDER BY position,id")
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(row_to_basket_item)
            .collect()
    }

    pub async fn reorder_basket(&self, ids: &[Uuid]) -> Result<Vec<BasketItem>> {
        if ids.len() > 10_000 {
            return Err(RavynError::Invalid(
                "basket reorder may contain at most 10000 items".into(),
            ));
        }
        let mut transaction = self.pool().begin().await?;
        let existing_ids = sqlx::query_scalar::<_, String>("SELECT id FROM basket_items")
            .fetch_all(&mut *transaction)
            .await?
            .into_iter()
            .map(|value| {
                Uuid::parse_str(&value).map_err(|error| RavynError::Internal(error.to_string()))
            })
            .collect::<Result<std::collections::HashSet<_>>>()?;
        let requested_ids = ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        if existing_ids != requested_ids || requested_ids.len() != ids.len() {
            return Err(RavynError::Invalid(
                "basket reorder must contain every item exactly once".into(),
            ));
        }
        sqlx::query("UPDATE basket_items SET position=position+1000000")
            .execute(&mut *transaction)
            .await?;
        for (position, id) in ids.iter().enumerate() {
            let position = i64::try_from(position)
                .map_err(|_| RavynError::Invalid("basket position is too large".into()))?;
            sqlx::query("UPDATE basket_items SET position=?,updated_at=? WHERE id=?")
                .bind(position)
                .bind(Utc::now())
                .bind(id.to_string())
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
        self.list_basket_items().await
    }

    pub async fn delete_basket_item(&self, id: Uuid) -> Result<()> {
        let mut transaction = self.pool().begin().await?;
        let position: Option<i64> =
            sqlx::query_scalar("SELECT position FROM basket_items WHERE id=?")
                .bind(id.to_string())
                .fetch_optional(&mut *transaction)
                .await?;
        let Some(position) = position else {
            return Err(RavynError::NotFound(format!("basket item {id}")));
        };
        sqlx::query("DELETE FROM basket_items WHERE id=?")
            .bind(id.to_string())
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE basket_items SET position=position-1,updated_at=? WHERE position>?")
            .bind(Utc::now())
            .bind(position)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn clear_basket(&self) -> Result<u64> {
        Ok(sqlx::query("DELETE FROM basket_items")
            .execute(self.pool())
            .await?
            .rows_affected())
    }
}

fn row_to_basket_item(row: SqliteRow) -> Result<BasketItem> {
    Ok(BasketItem {
        id: Uuid::parse_str(&row.try_get::<String, _>("id")?)
            .map_err(|error| RavynError::Internal(error.to_string()))?,
        position: usize::try_from(row.try_get::<i64, _>("position")?)
            .map_err(|_| RavynError::Internal("basket position is negative".into()))?,
        request: serde_json::from_str(&row.try_get::<String, _>("request_json")?)?,
        preset_id: row
            .try_get::<Option<String>, _>("preset_id")?
            .map(|value| Uuid::parse_str(&value))
            .transpose()
            .map_err(|error| RavynError::Internal(error.to_string()))?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{DownloadOptions, DuplicatePolicy, JobKind};

    async fn repository() -> (tempfile::TempDir, Repository) {
        let temporary = tempfile::tempdir().unwrap();
        let database = temporary.path().join("ravyn.sqlite3");
        let repository = Repository::connect(&format!("sqlite://{}", database.display()))
            .await
            .unwrap();
        (temporary, repository)
    }

    fn request(source: &str) -> CreateJob {
        CreateJob {
            preset_id: None,
            kind: JobKind::Http,
            source: source.into(),
            destination: None,
            filename: None,
            priority: 0,
            speed_limit_bps: None,
            expected_sha256: None,
            duplicate_policy: DuplicatePolicy::Allow,
            options: DownloadOptions::default(),
        }
    }

    #[tokio::test]
    async fn concurrent_adds_receive_unique_dense_positions() {
        let (_temporary, repository) = repository().await;
        let mut tasks = Vec::new();
        for index in 0..16 {
            let repository = repository.clone();
            tasks.push(tokio::spawn(async move {
                repository
                    .add_basket_item(PutBasketItem {
                        request: request(&format!("https://example.test/{index}")),
                        preset_id: None,
                    })
                    .await
                    .unwrap();
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }

        let items = repository.list_basket_items().await.unwrap();
        assert_eq!(items.len(), 16);
        assert_eq!(
            items.iter().map(|item| item.position).collect::<Vec<_>>(),
            (0..16).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn basket_reorder_and_delete_keep_positions_dense() {
        let (_temporary, repository) = repository().await;
        let first = repository
            .add_basket_item(PutBasketItem {
                request: request("https://example.test/first"),
                preset_id: None,
            })
            .await
            .unwrap();
        let second = repository
            .add_basket_item(PutBasketItem {
                request: request("https://example.test/second"),
                preset_id: None,
            })
            .await
            .unwrap();

        let reordered = repository
            .reorder_basket(&[second.id, first.id])
            .await
            .unwrap();
        assert_eq!(reordered[0].id, second.id);
        repository.delete_basket_item(second.id).await.unwrap();
        let remaining = repository.list_basket_items().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].position, 0);
    }
}
