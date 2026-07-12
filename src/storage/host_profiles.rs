use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::error::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct HostProfile {
    pub host: String,
    pub successful_downloads: u64,
    pub failed_downloads: u64,
    pub consecutive_failures: u32,
    pub average_throughput_bps: Option<u64>,
    pub range_failures: u32,
    pub circuit_open_until: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

pub async fn get(pool: &SqlitePool, host: &str) -> Result<Option<HostProfile>> {
    let row = sqlx::query("SELECT host,successful_downloads,failed_downloads,consecutive_failures,average_throughput_bps,range_failures,circuit_open_until,last_error FROM host_profiles WHERE host=?")
        .bind(host)
        .fetch_optional(pool)
        .await?;
    row.map(|row| {
        Ok(HostProfile {
            host: row.get("host"),
            successful_downloads: row.get::<i64, _>("successful_downloads").max(0) as u64,
            failed_downloads: row.get::<i64, _>("failed_downloads").max(0) as u64,
            consecutive_failures: row.get::<i64, _>("consecutive_failures").max(0) as u32,
            average_throughput_bps: row
                .get::<Option<i64>, _>("average_throughput_bps")
                .map(|value| value.max(0) as u64),
            range_failures: row.get::<i64, _>("range_failures").max(0) as u32,
            circuit_open_until: row.get("circuit_open_until"),
            last_error: row.get("last_error"),
        })
    })
    .transpose()
}

pub async fn record_success(pool: &SqlitePool, host: &str, throughput_bps: u64) -> Result<()> {
    sqlx::query(
        "INSERT INTO host_profiles(host,successful_downloads,failed_downloads,consecutive_failures,average_throughput_bps,range_failures,circuit_open_until,last_error,updated_at) VALUES(?,1,0,0,?,0,NULL,NULL,?) ON CONFLICT(host) DO UPDATE SET successful_downloads=successful_downloads+1,consecutive_failures=0,average_throughput_bps=CASE WHEN average_throughput_bps IS NULL THEN excluded.average_throughput_bps ELSE (average_throughput_bps*3+excluded.average_throughput_bps)/4 END,range_failures=MAX(range_failures-1,0),circuit_open_until=NULL,last_error=NULL,updated_at=excluded.updated_at",
    )
    .bind(host)
    .bind(throughput_bps.min(i64::MAX as u64) as i64)
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_range_failure(pool: &SqlitePool, host: &str, error: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO host_profiles(host,successful_downloads,failed_downloads,consecutive_failures,average_throughput_bps,range_failures,circuit_open_until,last_error,updated_at) VALUES(?,0,0,0,NULL,1,NULL,?,?) ON CONFLICT(host) DO UPDATE SET range_failures=range_failures+1,last_error=excluded.last_error,updated_at=excluded.updated_at",
    )
    .bind(host)
    .bind(error.chars().take(1024).collect::<String>())
    .bind(Utc::now())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_failure(
    pool: &SqlitePool,
    host: &str,
    error: &str,
    range_failure: bool,
    threshold: u32,
    cooldown_seconds: u64,
) -> Result<()> {
    let now = Utc::now();
    let open_until = now + chrono::Duration::seconds(cooldown_seconds as i64);
    sqlx::query(
        "INSERT INTO host_profiles(host,successful_downloads,failed_downloads,consecutive_failures,average_throughput_bps,range_failures,circuit_open_until,last_error,updated_at) VALUES(?,0,1,1,NULL,?,CASE WHEN ?<=1 THEN ? ELSE NULL END,?,?) ON CONFLICT(host) DO UPDATE SET failed_downloads=failed_downloads+1,consecutive_failures=consecutive_failures+1,range_failures=range_failures+excluded.range_failures,circuit_open_until=CASE WHEN consecutive_failures+1>=? THEN ? ELSE circuit_open_until END,last_error=excluded.last_error,updated_at=excluded.updated_at",
    )
    .bind(host)
    .bind(if range_failure { 1_i64 } else { 0_i64 })
    .bind(threshold as i64)
    .bind(open_until)
    .bind(error.chars().take(1024).collect::<String>())
    .bind(now)
    .bind(threshold as i64)
    .bind(open_until)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<HostProfile>> {
    let rows = sqlx::query("SELECT host,successful_downloads,failed_downloads,consecutive_failures,average_throughput_bps,range_failures,circuit_open_until,last_error FROM host_profiles ORDER BY updated_at DESC")
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(HostProfile {
                host: row.get("host"),
                successful_downloads: row.get::<i64, _>("successful_downloads").max(0) as u64,
                failed_downloads: row.get::<i64, _>("failed_downloads").max(0) as u64,
                consecutive_failures: row.get::<i64, _>("consecutive_failures").max(0) as u32,
                average_throughput_bps: row
                    .get::<Option<i64>, _>("average_throughput_bps")
                    .map(|value| value.max(0) as u64),
                range_failures: row.get::<i64, _>("range_failures").max(0) as u32,
                circuit_open_until: row.get("circuit_open_until"),
                last_error: row.get("last_error"),
            })
        })
        .collect()
}

pub async fn clear(pool: &SqlitePool) -> Result<u64> {
    Ok(sqlx::query("DELETE FROM host_profiles")
        .execute(pool)
        .await?
        .rows_affected())
}
