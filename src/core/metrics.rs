use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use uuid::Uuid;

use crate::{
    core::models::{JobKind, ProgressSnapshot},
    error::FailureClass,
};

const DURATION_BUCKETS: [f64; 10] = [0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 60.0, 300.0, 900.0];
/// Millisecond-scale buckets for DNS lookups and SQLite statements.
const FAST_BUCKETS: [f64; 10] = [0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 1.0, 5.0];

/// Process-wide count of SQLite busy/locked errors. Incremented from the
/// central `sqlx::Error` conversion so every query path is covered without
/// threading a metrics handle through the storage layer.
static SQLITE_BUSY_ERRORS: AtomicU64 = AtomicU64::new(0);

/// Records a SQLite busy or locked failure if the error is one. Called from
/// the `From<sqlx::Error>` conversion in `crate::error`.
pub fn note_sqlite_error(error: &sqlx::Error) {
    if let sqlx::Error::Database(database) = error {
        let code = database.code();
        let message = database.message();
        // SQLITE_BUSY (5), SQLITE_LOCKED (6) and their extended codes.
        let busy = matches!(code.as_deref(), Some("5" | "6" | "261" | "262" | "517"))
            || message.contains("database is locked")
            || message.contains("database table is locked");
        if busy {
            SQLITE_BUSY_ERRORS.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Clone, Default)]
pub struct Metrics(Arc<Mutex<MetricsState>>);

#[derive(Default)]
struct MetricsState {
    jobs_started: BTreeMap<&'static str, u64>,
    jobs_finished: BTreeMap<(&'static str, &'static str), u64>,
    retries: BTreeMap<&'static str, u64>,
    failures: BTreeMap<(&'static str, &'static str), u64>,
    durations: BTreeMap<&'static str, Histogram>,
    active_engines: BTreeMap<Uuid, &'static str>,
    last_bytes: BTreeMap<Uuid, u64>,
    rates: BTreeMap<Uuid, u64>,
    bytes: BTreeMap<&'static str, u64>,
    progress_writer_backlog: u64,
    events: BTreeMap<&'static str, u64>,
    torrent_telemetry: BTreeMap<Uuid, (u64, u64, u64)>,
    process_durations: BTreeMap<(&'static str, &'static str), Histogram>,
    post_action_durations: BTreeMap<(&'static str, &'static str), Histogram>,
    schedule_durations: BTreeMap<(&'static str, &'static str), Histogram>,
    schedule_delays: BTreeMap<&'static str, Histogram>,
    work_unit_durations: BTreeMap<&'static str, Histogram>,
    dns_durations: BTreeMap<&'static str, Histogram>,
    sqlite_durations: BTreeMap<&'static str, Histogram>,
    seeding_stops: BTreeMap<&'static str, u64>,
}

#[derive(Default)]
struct Histogram {
    buckets: [u64; DURATION_BUCKETS.len()],
    count: u64,
    sum: f64,
}

impl Metrics {
    pub fn schedule_finished(
        &self,
        mode: &'static str,
        success: bool,
        delay: Duration,
        duration: Duration,
    ) {
        let mut state = self.state();
        observe(
            state.schedule_delays.entry(mode).or_default(),
            &DURATION_BUCKETS,
            delay,
        );
        observe(
            state
                .schedule_durations
                .entry((mode, outcome(success)))
                .or_default(),
            &DURATION_BUCKETS,
            duration,
        );
    }

    pub fn process_finished(&self, tool: &'static str, success: bool, duration: Duration) {
        observe(
            self.state()
                .process_durations
                .entry((tool, outcome(success)))
                .or_default(),
            &DURATION_BUCKETS,
            duration,
        );
    }

    pub fn post_action_finished(&self, action: &'static str, success: bool, duration: Duration) {
        observe(
            self.state()
                .post_action_durations
                .entry((action, outcome(success)))
                .or_default(),
            &DURATION_BUCKETS,
            duration,
        );
    }

    /// Records one completed segmented-download work unit. The outcome label
    /// is bounded to `success`, `failure`, or `cancelled`.
    pub fn work_unit_finished(&self, outcome: &'static str, duration: Duration) {
        observe(
            self.state().work_unit_durations.entry(outcome).or_default(),
            &DURATION_BUCKETS,
            duration,
        );
    }

    /// Records the latency of one pre-connection DNS resolution.
    pub fn dns_resolved(&self, success: bool, duration: Duration) {
        observe(
            self.state()
                .dns_durations
                .entry(outcome(success))
                .or_default(),
            &FAST_BUCKETS,
            duration,
        );
    }

    /// Records the latency of a named hot-path SQLite operation. Operation
    /// names are static identifiers chosen at the call site, never derived
    /// from request data.
    pub fn db_query(&self, operation: &'static str, duration: Duration) {
        observe(
            self.state().sqlite_durations.entry(operation).or_default(),
            &FAST_BUCKETS,
            duration,
        );
    }

    /// Counts a torrent seeding stop by bounded policy reason.
    pub fn seeding_stopped(&self, reason: &str) {
        let reason = match reason {
            "ratio_limit" => "ratio_limit",
            "time_limit" => "time_limit",
            "engine_missing" => "engine_missing",
            "removed" => "removed",
            "cancelled" => "cancelled",
            _ => "other",
        };
        *self.state().seeding_stops.entry(reason).or_default() += 1;
    }

    pub fn event(&self, name: &'static str) {
        *self.state().events.entry(name).or_default() += 1;
    }

    pub fn torrent_telemetry(&self, job_id: Uuid, download_bps: u64, upload_bps: u64, peers: u64) {
        self.state()
            .torrent_telemetry
            .insert(job_id, (download_bps, upload_bps, peers));
    }

    pub fn progress_writer_backlog(&self, value: usize) {
        self.state().progress_writer_backlog = u64::try_from(value).unwrap_or(u64::MAX);
    }

    pub fn job_retried(&self, kind: JobKind) {
        let mut state = self.state();
        *state.retries.entry(engine(kind)).or_default() += 1;
    }

    pub fn job_started(&self, job_id: Uuid, kind: JobKind) {
        let mut state = self.state();
        let engine = engine(kind);
        state.active_engines.insert(job_id, engine);
        state.last_bytes.insert(job_id, 0);
        *state.jobs_started.entry(engine).or_default() += 1;
    }

    pub fn progress(&self, snapshot: &ProgressSnapshot) {
        let mut state = self.state();
        let Some(engine) = state.active_engines.get(&snapshot.job_id).copied() else {
            return;
        };
        let previous = state
            .last_bytes
            .insert(snapshot.job_id, snapshot.downloaded_bytes)
            .unwrap_or_default();
        *state.bytes.entry(engine).or_default() +=
            snapshot.downloaded_bytes.saturating_sub(previous);
        state
            .rates
            .insert(snapshot.job_id, snapshot.bytes_per_second);
    }

    pub fn job_finished(
        &self,
        job_id: Uuid,
        kind: JobKind,
        outcome: &'static str,
        duration: Duration,
        failure: Option<FailureClass>,
    ) {
        let engine = engine(kind);
        let mut state = self.state();
        state.active_engines.remove(&job_id);
        state.last_bytes.remove(&job_id);
        state.rates.remove(&job_id);
        state.torrent_telemetry.remove(&job_id);
        *state.jobs_finished.entry((engine, outcome)).or_default() += 1;
        if let Some(failure) = failure {
            *state
                .failures
                .entry((engine, failure_label(failure)))
                .or_default() += 1;
        }
        observe(
            state.durations.entry(engine).or_default(),
            &DURATION_BUCKETS,
            duration,
        );
    }

    pub fn encode_openmetrics(&self) -> String {
        let state = self.state();
        let mut body = String::from(
            "# HELP ravyn_jobs_started_total Jobs whose execution started.\n# TYPE ravyn_jobs_started_total counter\n",
        );
        for (engine, value) in &state.jobs_started {
            body.push_str(&format!(
                "ravyn_jobs_started_total{{engine=\"{engine}\"}} {value}\n"
            ));
        }
        body.push_str("# HELP ravyn_jobs_finished_total Terminal job executions by outcome.\n# TYPE ravyn_jobs_finished_total counter\n");
        for ((engine, outcome), value) in &state.jobs_finished {
            body.push_str(&format!(
                "ravyn_jobs_finished_total{{engine=\"{engine}\",outcome=\"{outcome}\"}} {value}\n"
            ));
        }
        body.push_str("# HELP ravyn_job_failures_total Job failures by stable failure class.\n# TYPE ravyn_job_failures_total counter\n");
        for ((engine, code), value) in &state.failures {
            body.push_str(&format!(
                "ravyn_job_failures_total{{engine=\"{engine}\",code=\"{code}\"}} {value}\n"
            ));
        }
        body.push_str("# HELP ravyn_job_retries_total Explicit job retry requests.\n# TYPE ravyn_job_retries_total counter\n");
        for (engine, value) in &state.retries {
            body.push_str(&format!(
                "ravyn_job_retries_total{{engine=\"{engine}\"}} {value}\n"
            ));
        }
        body.push_str("# HELP ravyn_transfer_bytes_total Bytes transferred during this process lifetime.\n# TYPE ravyn_transfer_bytes_total counter\n");
        for (engine, value) in &state.bytes {
            body.push_str(&format!(
                "ravyn_transfer_bytes_total{{engine=\"{engine}\"}} {value}\n"
            ));
        }
        body.push_str("# HELP ravyn_transfer_throughput_bytes_per_second Current aggregate transfer throughput.\n# TYPE ravyn_transfer_throughput_bytes_per_second gauge\n");
        for engine in ["http", "media", "torrent"] {
            let value: u64 = state
                .rates
                .iter()
                .filter(|(job_id, _)| state.active_engines.get(job_id).copied() == Some(engine))
                .map(|(_, value)| *value)
                .sum();
            body.push_str(&format!(
                "ravyn_transfer_throughput_bytes_per_second{{engine=\"{engine}\"}} {value}\n"
            ));
        }
        body.push_str("# HELP ravyn_progress_writer_backlog Progress updates currently waiting for durable persistence.\n# TYPE ravyn_progress_writer_backlog gauge\n");
        body.push_str(&format!(
            "ravyn_progress_writer_backlog {}\n",
            state.progress_writer_backlog
        ));
        for (event, help) in [
            (
                "http_range_fallbacks",
                "Segmented HTTP transfers falling back to a single stream.",
            ),
            (
                "http_circuit_rejections",
                "HTTP transfers rejected by an open host circuit.",
            ),
            (
                "http_redirects",
                "Validated HTTP redirects followed while probing.",
            ),
            (
                "http_range_splits",
                "Active segmented ranges split into new work units by idle workers.",
            ),
            (
                "http_speculation_wins",
                "Speculative duplicate range requests that completed a slow work unit.",
            ),
            (
                "http_speculation_losses",
                "Speculative duplicate range requests that lost the race or failed.",
            ),
        ] {
            body.push_str(&format!("# HELP ravyn_{event}_total {help}\n# TYPE ravyn_{event}_total counter\nravyn_{event}_total {}\n", state.events.get(event).copied().unwrap_or_default()));
        }
        body.push_str("# HELP ravyn_torrent_transfer_bytes_per_second Current aggregate rqbit transfer rates.\n# TYPE ravyn_torrent_transfer_bytes_per_second gauge\n");
        let (torrent_download_bps, torrent_upload_bps, torrent_peers) = state
            .torrent_telemetry
            .values()
            .fold((0_u64, 0_u64, 0_u64), |totals, value| {
                (
                    totals.0.saturating_add(value.0),
                    totals.1.saturating_add(value.1),
                    totals.2.saturating_add(value.2),
                )
            });
        body.push_str(&format!("ravyn_torrent_transfer_bytes_per_second{{direction=\"download\"}} {torrent_download_bps}\nravyn_torrent_transfer_bytes_per_second{{direction=\"upload\"}} {torrent_upload_bps}\n"));
        body.push_str("# HELP ravyn_torrent_peers Current connected torrent peers.\n# TYPE ravyn_torrent_peers gauge\n");
        body.push_str(&format!("ravyn_torrent_peers {torrent_peers}\n"));
        encode_histograms(
            &mut body,
            "ravyn_external_process_duration_seconds",
            "External process execution duration.",
            "tool",
            &state.process_durations,
        );
        encode_histograms(
            &mut body,
            "ravyn_post_action_duration_seconds",
            "Post-processing action duration.",
            "action",
            &state.post_action_durations,
        );
        encode_histograms(
            &mut body,
            "ravyn_schedule_execution_duration_seconds",
            "Schedule execution duration.",
            "mode",
            &state.schedule_durations,
        );
        body.push_str("# HELP ravyn_schedule_delay_seconds Delay between intended and actual schedule execution.\n# TYPE ravyn_schedule_delay_seconds histogram\n");
        for (mode, histogram) in &state.schedule_delays {
            for (index, upper) in DURATION_BUCKETS.iter().enumerate() {
                body.push_str(&format!(
                    "ravyn_schedule_delay_seconds_bucket{{mode=\"{mode}\",le=\"{upper}\"}} {}\n",
                    histogram.buckets[index]
                ));
            }
            body.push_str(&format!("ravyn_schedule_delay_seconds_bucket{{mode=\"{mode}\",le=\"+Inf\"}} {}\nravyn_schedule_delay_seconds_sum{{mode=\"{mode}\"}} {}\nravyn_schedule_delay_seconds_count{{mode=\"{mode}\"}} {}\n", histogram.count, histogram.sum, histogram.count));
        }
        encode_single_label_histograms(
            &mut body,
            "ravyn_work_unit_duration_seconds",
            "Segmented HTTP work-unit execution duration.",
            "outcome",
            &state.work_unit_durations,
            &DURATION_BUCKETS,
        );
        encode_single_label_histograms(
            &mut body,
            "ravyn_dns_resolution_duration_seconds",
            "Pre-connection DNS resolution duration.",
            "outcome",
            &state.dns_durations,
            &FAST_BUCKETS,
        );
        encode_single_label_histograms(
            &mut body,
            "ravyn_sqlite_query_duration_seconds",
            "Hot-path SQLite statement duration by named operation.",
            "operation",
            &state.sqlite_durations,
            &FAST_BUCKETS,
        );
        body.push_str("# HELP ravyn_sqlite_busy_total SQLite busy or locked errors observed process-wide.\n# TYPE ravyn_sqlite_busy_total counter\n");
        body.push_str(&format!(
            "ravyn_sqlite_busy_total {}\n",
            SQLITE_BUSY_ERRORS.load(Ordering::Relaxed)
        ));
        body.push_str("# HELP ravyn_torrent_seeding_stops_total Torrent seeding stops by bounded policy reason.\n# TYPE ravyn_torrent_seeding_stops_total counter\n");
        for (reason, value) in &state.seeding_stops {
            body.push_str(&format!(
                "ravyn_torrent_seeding_stops_total{{reason=\"{reason}\"}} {value}\n"
            ));
        }
        body.push_str("# HELP ravyn_job_duration_seconds End-to-end job execution duration.\n# TYPE ravyn_job_duration_seconds histogram\n");
        for (engine, histogram) in &state.durations {
            for (index, upper) in DURATION_BUCKETS.iter().enumerate() {
                body.push_str(&format!(
                    "ravyn_job_duration_seconds_bucket{{engine=\"{engine}\",le=\"{upper}\"}} {}\n",
                    histogram.buckets[index]
                ));
            }
            body.push_str(&format!(
                "ravyn_job_duration_seconds_bucket{{engine=\"{engine}\",le=\"+Inf\"}} {}\n",
                histogram.count
            ));
            body.push_str(&format!(
                "ravyn_job_duration_seconds_sum{{engine=\"{engine}\"}} {}\n",
                histogram.sum
            ));
            body.push_str(&format!(
                "ravyn_job_duration_seconds_count{{engine=\"{engine}\"}} {}\n",
                histogram.count
            ));
        }
        body
    }

    fn state(&self) -> std::sync::MutexGuard<'_, MetricsState> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn outcome(success: bool) -> &'static str {
    if success { "success" } else { "failure" }
}

fn observe(histogram: &mut Histogram, buckets: &[f64; DURATION_BUCKETS.len()], duration: Duration) {
    let seconds = duration.as_secs_f64();
    histogram.count += 1;
    histogram.sum += seconds;
    for (index, upper) in buckets.iter().enumerate() {
        if seconds <= *upper {
            histogram.buckets[index] += 1;
        }
    }
}

fn encode_single_label_histograms(
    body: &mut String,
    name: &str,
    help: &str,
    label_name: &str,
    values: &BTreeMap<&'static str, Histogram>,
    buckets: &[f64; DURATION_BUCKETS.len()],
) {
    body.push_str(&format!("# HELP {name} {help}\n# TYPE {name} histogram\n"));
    for (label, histogram) in values {
        for (index, upper) in buckets.iter().enumerate() {
            body.push_str(&format!(
                "{name}_bucket{{{label_name}=\"{label}\",le=\"{upper}\"}} {}\n",
                histogram.buckets[index]
            ));
        }
        body.push_str(&format!(
            "{name}_bucket{{{label_name}=\"{label}\",le=\"+Inf\"}} {}\n{name}_sum{{{label_name}=\"{label}\"}} {}\n{name}_count{{{label_name}=\"{label}\"}} {}\n",
            histogram.count, histogram.sum, histogram.count
        ));
    }
}

fn encode_histograms(
    body: &mut String,
    name: &str,
    help: &str,
    label_name: &str,
    values: &BTreeMap<(&'static str, &'static str), Histogram>,
) {
    body.push_str(&format!("# HELP {name} {help}\n# TYPE {name} histogram\n"));
    for ((label, result), histogram) in values {
        for (index, upper) in DURATION_BUCKETS.iter().enumerate() {
            body.push_str(&format!("{name}_bucket{{{label_name}=\"{label}\",outcome=\"{result}\",le=\"{upper}\"}} {}\n", histogram.buckets[index]));
        }
        body.push_str(&format!("{name}_bucket{{{label_name}=\"{label}\",outcome=\"{result}\",le=\"+Inf\"}} {}\n{name}_sum{{{label_name}=\"{label}\",outcome=\"{result}\"}} {}\n{name}_count{{{label_name}=\"{label}\",outcome=\"{result}\"}} {}\n", histogram.count, histogram.sum, histogram.count));
    }
}

fn engine(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Http => "http",
        JobKind::Media => "media",
        JobKind::Torrent => "torrent",
    }
}

/// Returns the free bytes available to this process on the filesystem that
/// contains `path`, or `None` when the query fails.
#[cfg(windows)]
pub fn free_disk_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut available = 0_u64;
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(available)
}

/// Returns the free bytes available to this process on the filesystem that
/// contains `path`, or `None` when the query fails.
#[cfg(unix)]
pub fn free_disk_space(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::statvfs(path.as_ptr(), &mut stats) };
    (result == 0).then(|| u64::from(stats.f_bavail).saturating_mul(u64::from(stats.f_frsize)))
}

const TEMP_SCAN_MAX_ENTRIES: usize = 10_000;
const TEMP_SCAN_MAX_DEPTH: usize = 6;

/// Sums the bytes currently held by Ravyn temporary artifacts (`*.ravyn.part`
/// partial files and `.ravyn-extract-*` staging directories) below `root`.
/// The walk is bounded in depth and entry count so a huge download tree
/// cannot stall the metrics endpoint.
pub fn temporary_disk_usage(root: &Path) -> u64 {
    let mut visited = 0_usize;
    let mut total = 0_u64;
    scan_temporary(root, 0, false, &mut visited, &mut total);
    total
}

fn scan_temporary(dir: &Path, depth: usize, count_all: bool, visited: &mut usize, total: &mut u64) {
    if depth > TEMP_SCAN_MAX_DEPTH || *visited >= TEMP_SCAN_MAX_ENTRIES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        *visited += 1;
        if *visited >= TEMP_SCAN_MAX_ENTRIES {
            return;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let name = name.to_string_lossy().into_owned();
        if file_type.is_file() {
            if count_all || name.ends_with(".ravyn.part") {
                if let Ok(metadata) = entry.metadata() {
                    *total = total.saturating_add(metadata.len());
                }
            }
        } else if file_type.is_dir() {
            let staging = count_all || name.starts_with(".ravyn-extract-");
            scan_temporary(&entry.path(), depth + 1, staging, visited, total);
        }
    }
}

fn failure_label(failure: FailureClass) -> &'static str {
    match failure {
        FailureClass::PermanentClient => "permanent_client",
        FailureClass::RetryableHttp => "retryable_http",
        FailureClass::Timeout => "timeout",
        FailureClass::DnsOrConnect => "dns_or_connect",
        FailureClass::ConnectionReset => "connection_reset",
        FailureClass::MalformedRange => "malformed_range",
        FailureClass::DiskFull => "disk_full",
        FailureClass::Permission => "permission",
        FailureClass::ChecksumMismatch => "checksum_mismatch",
        FailureClass::ExternalTool => "external_tool",
        FailureClass::Cancellation => "cancellation",
        FailureClass::Internal => "internal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_bounded_and_do_not_include_error_text() {
        let metrics = Metrics::default();
        let job_id = Uuid::new_v4();
        metrics.job_started(job_id, JobKind::Http);
        metrics.progress(&ProgressSnapshot {
            job_id,
            downloaded_bytes: 512,
            total_bytes: Some(1024),
            bytes_per_second: 256,
        });
        metrics.job_finished(
            job_id,
            JobKind::Http,
            "failed",
            Duration::from_millis(600),
            Some(FailureClass::Timeout),
        );
        let encoded = metrics.encode_openmetrics();
        assert!(encoded.contains("engine=\"http\""));
        assert!(encoded.contains("code=\"timeout\""));
        assert!(encoded.contains("le=\"1\"} 1"));
        assert!(encoded.contains("ravyn_transfer_bytes_total{engine=\"http\"} 512"));
    }

    #[test]
    fn operational_metrics_use_bounded_labels() {
        let metrics = Metrics::default();
        metrics.work_unit_finished("success", Duration::from_millis(300));
        metrics.work_unit_finished("cancelled", Duration::from_millis(20));
        metrics.dns_resolved(true, Duration::from_millis(4));
        metrics.db_query("claim_next_queued", Duration::from_micros(750));
        metrics.seeding_stopped("ratio_limit");
        metrics.seeding_stopped("free-form text with job ids should be collapsed");
        let encoded = metrics.encode_openmetrics();
        assert!(encoded.contains("ravyn_work_unit_duration_seconds_count{outcome=\"success\"} 1"));
        assert!(
            encoded.contains("ravyn_work_unit_duration_seconds_count{outcome=\"cancelled\"} 1")
        );
        assert!(
            encoded.contains("ravyn_dns_resolution_duration_seconds_count{outcome=\"success\"} 1")
        );
        assert!(encoded.contains(
            "ravyn_sqlite_query_duration_seconds_count{operation=\"claim_next_queued\"} 1"
        ));
        assert!(encoded.contains("ravyn_sqlite_busy_total"));
        assert!(encoded.contains("ravyn_torrent_seeding_stops_total{reason=\"ratio_limit\"} 1"));
        assert!(encoded.contains("ravyn_torrent_seeding_stops_total{reason=\"other\"} 1"));
        assert!(!encoded.contains("free-form text"));
    }

    #[test]
    fn temporary_disk_usage_counts_only_ravyn_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("movie.mkv.ravyn.part"), vec![0_u8; 100]).unwrap();
        std::fs::write(temp.path().join("finished.mkv"), vec![0_u8; 999]).unwrap();
        let staging = temp.path().join(".ravyn-extract-abc");
        std::fs::create_dir(&staging).unwrap();
        std::fs::write(staging.join("inner.bin"), vec![0_u8; 50]).unwrap();
        let nested = temp.path().join("sub");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("clip.mp4.ravyn.part"), vec![0_u8; 25]).unwrap();
        assert_eq!(temporary_disk_usage(temp.path()), 175);
    }

    #[test]
    fn free_disk_space_reports_a_positive_value() {
        let temp = tempfile::tempdir().unwrap();
        let free = free_disk_space(temp.path());
        assert!(free.is_some_and(|value| value > 0));
    }
}
