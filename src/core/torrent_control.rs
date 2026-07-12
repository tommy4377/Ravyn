//! Torrent probing, engine statistics, file selection, and removal.

use uuid::Uuid;

use crate::{
    adapters::torrent::{
        TorrentDependencyStatus, TorrentDetails, TorrentEngineList, TorrentGlobalStats,
        TorrentPeerStats, TorrentProbe, TorrentProbeRequest, TorrentSnapshot,
    },
    core::models::{JobKind, JobStatus},
    error::{RavynError, Result},
    storage::TorrentRecord,
};

use crate::core::manager::JobManager;

impl JobManager {
    pub async fn torrent_dependencies(&self) -> TorrentDependencyStatus {
        self.torrent.dependency_status().await
    }

    pub async fn probe_torrent(&self, request: &TorrentProbeRequest) -> Result<TorrentProbe> {
        self.torrent.probe(request).await
    }

    pub async fn managed_torrents(&self) -> Result<Vec<TorrentRecord>> {
        self.repository.list_torrent_records().await
    }

    pub async fn list_engine_torrents(&self) -> Result<TorrentEngineList> {
        self.torrent.list().await
    }

    pub async fn torrent_engine_stats(&self) -> Result<TorrentGlobalStats> {
        self.torrent.global_stats().await
    }

    pub async fn torrent_dht_stats(&self) -> Result<serde_json::Value> {
        self.torrent.dht_stats().await
    }

    pub async fn torrent_dht_table(&self) -> Result<serde_json::Value> {
        self.torrent.dht_table().await
    }

    pub async fn torrent_details(&self, job_id: Uuid) -> Result<TorrentDetails> {
        let record = self
            .repository
            .get_torrent_record(job_id)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("torrent mapping for {job_id}")))?;
        self.torrent.details(&record.torrent_id).await
    }

    pub async fn torrent_stats(&self, job_id: Uuid) -> Result<TorrentSnapshot> {
        let record = self
            .repository
            .get_torrent_record(job_id)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("torrent mapping for {job_id}")))?;
        self.torrent.stats(&record.torrent_id).await
    }

    pub async fn torrent_peers(&self, job_id: Uuid) -> Result<TorrentPeerStats> {
        let record = self
            .repository
            .get_torrent_record(job_id)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("torrent mapping for {job_id}")))?;
        self.torrent.peer_stats(&record.torrent_id).await
    }

    pub async fn add_torrent_peers(&self, job_id: Uuid, peers: &[String]) -> Result<()> {
        let record = self
            .repository
            .get_torrent_record(job_id)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("torrent mapping for {job_id}")))?;
        self.torrent.add_peers(&record.torrent_id, peers).await
    }

    pub async fn update_torrent_files(&self, job_id: Uuid, files: &[usize]) -> Result<()> {
        let record = self
            .repository
            .get_torrent_record(job_id)
            .await?
            .ok_or_else(|| RavynError::NotFound(format!("torrent mapping for {job_id}")))?;
        self.torrent.update_files(&record.torrent_id, files).await
    }

    pub async fn remove_torrent(&self, job_id: Uuid, delete_files: bool) -> Result<()> {
        let job = self.repository.get_job(job_id).await?;
        if job.kind != JobKind::Torrent {
            return Err(RavynError::Invalid(format!(
                "job {job_id} is not a torrent"
            )));
        }
        if !matches!(
            job.status,
            JobStatus::Cancelled
                | JobStatus::Completed
                | JobStatus::Partial
                | JobStatus::Failed
                | JobStatus::Seeding
        ) {
            self.cancel(job_id).await?;
        }
        self.torrent.remove_job(job_id, delete_files).await?;
        if let Some(state) = self.repository.get_torrent_seeding_state(job_id).await? {
            if state.stopped_at.is_none() {
                self.repository
                    .stop_torrent_seeding(job_id, "removed", state.last_ratio)
                    .await?;
            }
        }
        self.repository
            .set_status(job_id, JobStatus::Cancelled, None)
            .await?;
        Ok(())
    }
}
