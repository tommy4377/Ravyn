use axum::{Router, body::Body, http::Response, routing::get};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ravyn::{
    core::bandwidth::{FairBandwidthScheduler, FlowClass, FlowConfig},
    download::planner::{adaptive_segment_count, profile_adjusted_segment_count},
};
use std::{hint::black_box, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{net::TcpListener, runtime::Runtime, task::JoinHandle, time::sleep};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

struct NetworkFixture {
    url: String,
    shutdown: CancellationToken,
    task: JoinHandle<()>,
}

impl NetworkFixture {
    async fn start(delay: Duration, payload: Arc<[u8]>) -> Self {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("loopback benchmark listener must bind");
        let address: SocketAddr = listener
            .local_addr()
            .expect("loopback benchmark listener must have an address");
        let shutdown = CancellationToken::new();
        let server_shutdown = shutdown.clone();
        let app = Router::new().route(
            "/fixture",
            get(move || {
                let payload = Arc::clone(&payload);
                async move {
                    sleep(delay).await;
                    Response::new(Body::from(payload.as_ref().to_vec()))
                }
            }),
        );
        let task = tokio::spawn(async move {
            let result = axum::serve(listener, app)
                .with_graceful_shutdown(server_shutdown.cancelled_owned())
                .await;
            if let Err(error) = result {
                panic!("loopback benchmark server failed: {error}");
            }
        });
        Self {
            url: format!("http://{address}/fixture"),
            shutdown,
            task,
        }
    }

    async fn stop(self) {
        self.shutdown.cancel();
        self.task
            .await
            .expect("loopback benchmark server must shut down cleanly");
    }
}

fn planner_benchmarks(c: &mut Criterion) {
    c.bench_function("adaptive_segment_count", |bench| {
        bench.iter(|| adaptive_segment_count(black_box(8 * 1024 * 1024 * 1024), 32, 32))
    });
    c.bench_function("profile_adjusted_segment_count", |bench| {
        bench.iter(|| profile_adjusted_segment_count(black_box(32), 1, 2, Some(8_000_000)))
    });
}

fn scheduler_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("fair_scheduler_rebalance");
    for flow_count in [1usize, 8, 32, 128] {
        group.bench_with_input(
            BenchmarkId::from_parameter(flow_count),
            &flow_count,
            |bench, &count| {
                bench.iter_batched(
                    || FairBandwidthScheduler::new(100_000_000),
                    |scheduler| {
                        let mut flows = Vec::with_capacity(count);
                        for index in 0..count {
                            flows.push(scheduler.register_scoped(
                                Uuid::new_v4(),
                                FlowConfig {
                                    weight: (index % 8 + 1) as u32,
                                    class: if index % 4 == 0 {
                                        FlowClass::Background
                                    } else {
                                        FlowClass::Foreground
                                    },
                                    min_bps: None,
                                    max_bps: (index % 3 == 0).then_some(2_000_000),
                                },
                            ));
                        }
                        black_box(flows);
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }
    group.finish();
}

fn network_comparative_benchmarks(c: &mut Criterion) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let runtime = Runtime::new().expect("benchmark Tokio runtime must start");
    let payload: Arc<[u8]> = vec![0x5a; 256 * 1024].into();
    let slow = runtime.block_on(NetworkFixture::start(
        Duration::from_millis(12),
        Arc::clone(&payload),
    ));
    let fast = runtime.block_on(NetworkFixture::start(
        Duration::from_millis(2),
        Arc::clone(&payload),
    ));
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("loopback benchmark client must build");

    let mut group = c.benchmark_group("loopback_tail_strategy");
    group.sample_size(20);
    group.bench_function("single_slow_source", |bench| {
        bench.to_async(&runtime).iter(|| async {
            let bytes = client
                .get(&slow.url)
                .send()
                .await
                .expect("fixture request must succeed")
                .bytes()
                .await
                .expect("fixture body must be readable");
            assert_eq!(bytes.as_ref(), payload.as_ref());
            black_box(bytes)
        });
    });
    group.bench_function("guarded_delayed_first_valid", |bench| {
        bench.to_async(&runtime).iter(|| async {
            let slow_request = async {
                client
                    .get(&slow.url)
                    .send()
                    .await?
                    .error_for_status()?
                    .bytes()
                    .await
            };
            let fast_request = async {
                // Model a bounded hedge that is admitted only after the
                // primary has exceeded a per-host tail threshold.
                sleep(Duration::from_millis(4)).await;
                client
                    .get(&fast.url)
                    .send()
                    .await?
                    .error_for_status()?
                    .bytes()
                    .await
            };
            let bytes = tokio::select! {
                result = slow_request => result,
                result = fast_request => result,
            }
            .expect("at least one fixture request must succeed");
            assert_eq!(bytes.as_ref(), payload.as_ref());
            black_box(bytes)
        });
    });
    group.finish();

    runtime.block_on(slow.stop());
    runtime.block_on(fast.stop());
}

criterion_group!(
    benches,
    planner_benchmarks,
    scheduler_benchmarks,
    network_comparative_benchmarks
);
criterion_main!(benches);
