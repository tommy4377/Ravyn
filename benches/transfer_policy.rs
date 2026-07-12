use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ravyn::{
    core::bandwidth::{FairBandwidthScheduler, FlowClass, FlowConfig},
    download::planner::{adaptive_segment_count, profile_adjusted_segment_count},
};
use std::hint::black_box;
use uuid::Uuid;

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

criterion_group!(benches, planner_benchmarks, scheduler_benchmarks);
criterion_main!(benches);
