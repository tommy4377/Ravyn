//! Torrent probe, engine, statistics, and file-selection handlers.

use super::*;

pub(super) async fn probe_torrent(
    State(s): State<ApiState>,
    Json(request): Json<TorrentProbeRequest>,
) -> Result<Json<TorrentProbe>> {
    Ok(Json(s.manager.probe_torrent(&request).await?))
}

pub(super) async fn managed_torrents(
    State(s): State<ApiState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Page<TorrentRecord>>> {
    let window = PageWindow::from_query(&query)?;
    let items = s
        .repository
        .list_torrent_records_page(
            window.offset,
            window.database_limit(),
            query.search.as_deref(),
        )
        .await?;
    Ok(Json(Page::from_extra_item(items, window)))
}

pub(super) async fn list_engine_torrents(
    State(s): State<ApiState>,
) -> Result<Json<TorrentEngineList>> {
    Ok(Json(s.manager.list_engine_torrents().await?))
}

pub(super) async fn torrent_engine_stats(
    State(s): State<ApiState>,
) -> Result<Json<TorrentGlobalStats>> {
    Ok(Json(s.manager.torrent_engine_stats().await?))
}

pub(super) async fn torrent_dht_stats(State(s): State<ApiState>) -> Result<Json<Value>> {
    Ok(Json(s.manager.torrent_dht_stats().await?))
}

pub(super) async fn torrent_dht_table(State(s): State<ApiState>) -> Result<Json<Value>> {
    Ok(Json(s.manager.torrent_dht_table().await?))
}

pub(super) async fn torrent_details(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TorrentDetails>> {
    Ok(Json(s.manager.torrent_details(id).await?))
}

pub(super) async fn torrent_stats(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TorrentSnapshot>> {
    Ok(Json(s.manager.torrent_stats(id).await?))
}

pub(super) async fn torrent_seeding_state(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Option<crate::storage::TorrentSeedingState>>> {
    s.repository.get_job(id).await?;
    Ok(Json(s.repository.get_torrent_seeding_state(id).await?))
}

pub(super) async fn torrent_peers(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TorrentPeerStats>> {
    Ok(Json(s.manager.torrent_peers(id).await?))
}

#[derive(Deserialize)]
pub(super) struct AddTorrentPeers {
    peers: Vec<String>,
}

pub(super) async fn add_torrent_peers(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(request): Json<AddTorrentPeers>,
) -> Result<StatusCode> {
    let result = s.manager.add_torrent_peers(id, &request.peers).await;
    audited(
        &s.repository,
        "torrent.peers.add",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub(super) struct UpdateTorrentFiles {
    files: Vec<usize>,
}

pub(super) async fn update_torrent_files(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateTorrentFiles>,
) -> Result<StatusCode> {
    let result = s.manager.update_torrent_files(id, &request.files).await;
    audited(
        &s.repository,
        "torrent.files.update",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub(super) struct RemoveTorrent {
    #[serde(default)]
    delete_files: bool,
}

pub(super) async fn remove_torrent(
    State(s): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(request): Json<RemoveTorrent>,
) -> Result<StatusCode> {
    let result = s.manager.remove_torrent(id, request.delete_files).await;
    audited(
        &s.repository,
        "torrent.remove",
        "job",
        Some(&id.to_string()),
        result,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
