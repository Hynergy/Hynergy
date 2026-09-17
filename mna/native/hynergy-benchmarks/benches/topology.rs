use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use hynergy_benchmarks::fixtures::{TopologyMutation, TopologyScenario};
use std::{hint::black_box, time::Duration};

fn bench_topology(c: &mut Criterion) {
    let mut group = c.benchmark_group("world/topology");
    group.sample_size(15);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(5));

    for mutation in TopologyMutation::ALL {
        group.throughput(Throughput::Elements(2));
        group.bench_function(mutation.name(), |b| {
            b.iter_batched_ref(
                || TopologyScenario::new(mutation),
                |scenario| {
                    scenario.run_once().unwrap();
                    black_box(scenario.circuits());
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, bench_topology);
criterion_main!(benches);
