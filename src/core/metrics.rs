use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use uuid::Uuid;

use crate::{
    core::models::{JobKind, ProgressSnapshot},
    error::FailureClass,
};

const DURATION_BUCKETS: [f64; 10] = [0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 15.0, 60.0, 300.0, 900.0];

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
        observe(state.schedule_delays.entry(mode).or_default(), delay);
        observe(
            state
                .schedule_durations
                .entry((mode, outcome(success)))
                .or_default(),
            duration,
        );
    }

    pub fn process_finished(&self, tool: &'static str, success: bool, duration: Duration) {
        observe(
            self.state()
                .process_durations
                .entry((tool, outcome(success)))
                .or_default(),
            duration,
        );
    }

    pub fn post_action_finished(&self, action: &'static str, success: bool, duration: Duration) {
        observe(
            self.state()
                .post_action_durations
                .entry((action, outcome(success)))
                .or_default(),
            duration,
        );
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
        observe(state.durations.entry(engine).or_default(), duration);
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

fn observe(histogram: &mut Histogram, duration: Duration) {
    let seconds = duration.as_secs_f64();
    histogram.count += 1;
    histogram.sum += seconds;
    for (index, upper) in DURATION_BUCKETS.iter().enumerate() {
        if seconds <= *upper {
            histogram.buckets[index] += 1;
        }
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
}
