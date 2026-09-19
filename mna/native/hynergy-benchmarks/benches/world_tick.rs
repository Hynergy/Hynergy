use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hynergy_benchmarks::fixtures::{BenchSuite, CircuitFamily, WorldScenario};
use std::{hint::black_box, time::Duration};

fn bench_cold(c: &mut Criterion, suite: BenchSuite) {
    let mut group = c.benchmark_group("world/tick/cold");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(5));

    for family in CircuitFamily::ALL {
        for &count in suite.counts(family) {
            group.throughput(Throughput::Elements(count as u64));
            group.bench_with_input(
                BenchmarkId::new(family.name(), format!("count_{count}")),
                &count,
                |b, &count| {
                    b.iter_batched_ref(
                        || WorldScenario::uniform(family, count),
                        |scenario| {
                            scenario.tick().unwrap();
                            black_box(scenario.metadata().devices());
                        },
                        BatchSize::LargeInput,
                    );
                },
            );
        }
    }

    group.finish();
}

fn bench_warm_sleep(c: &mut Criterion, suite: BenchSuite) {
    let mut group = c.benchmark_group("world/tick/warm_sleep/simple");
    group.sample_size(30);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(6));

    for &count in suite.counts(CircuitFamily::Simple) {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("count_{count}")),
            &count,
            |b, &count| {
                let mut scenario = WorldScenario::uniform(CircuitFamily::Simple, count);
                scenario.tick().unwrap();
                b.iter(|| {
                    scenario.tick().unwrap();
                    black_box(scenario.subscription_update_count());
                });
            },
        );
    }

    group.finish();
}

fn bench_warm_active(c: &mut Criterion, suite: BenchSuite) {
    let mut group = c.benchmark_group("world/tick/warm_active");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(6));

    for family in [CircuitFamily::Medium, CircuitFamily::Complex] {
        for &count in suite.counts(family) {
            group.throughput(Throughput::Elements(count as u64));
            group.bench_with_input(
                BenchmarkId::new(family.name(), format!("count_{count}")),
                &count,
                |b, &count| {
                    let mut scenario = WorldScenario::uniform(family, count);
                    scenario.warm(2).unwrap();
                    b.iter(|| {
                        scenario.tick().unwrap();
                        black_box(scenario.subscription_update_count());
                    });
                },
            );
        }
    }

    group.finish();
}

fn bench_dirty(c: &mut Criterion, suite: BenchSuite, matrix: bool) {
    let name = if matrix { "matrix_dirty" } else { "rhs_dirty" };
    let mut group = c.benchmark_group(format!("world/tick/{name}"));
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(6));

    for family in CircuitFamily::ALL {
        for &count in suite.counts(family) {
            group.throughput(Throughput::Elements(count as u64));
            group.bench_with_input(
                BenchmarkId::new(family.name(), format!("count_{count}")),
                &count,
                |b, &count| {
                    let mut scenario = WorldScenario::uniform(family, count);
                    scenario.warm(2).unwrap();
                    if matrix {
                        b.iter_custom(|iterations| scenario.measure_matrix_dirty_ticks(iterations));
                    } else {
                        b.iter_custom(|iterations| scenario.measure_rhs_dirty_ticks(iterations));
                    }
                },
            );
        }
    }

    group.finish();
}

fn bench_mixed(c: &mut Criterion, suite: BenchSuite) {
    let mut group = c.benchmark_group("world/tick/mixed");
    group.sample_size(15);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(7));

    for &profile in suite.mixed_profiles() {
        let (simple, medium, complex) = profile.counts();
        group.throughput(Throughput::Elements((simple + medium + complex) as u64));
        group.bench_function(
            BenchmarkId::from_parameter(format!(
                "{}_s{}_m{}_c{}",
                profile.name(),
                simple,
                medium,
                complex
            )),
            |b| {
                let mut scenario = WorldScenario::mixed(profile);
                scenario.warm(2).unwrap();
                b.iter(|| {
                    scenario.tick().unwrap();
                    black_box(scenario.subscription_update_count());
                });
            },
        );
    }

    group.finish();
}

fn bench_world_tick(c: &mut Criterion) {
    let suite = BenchSuite::from_env();
    bench_cold(c, suite);
    bench_warm_sleep(c, suite);
    bench_warm_active(c, suite);
    bench_dirty(c, suite, false);
    bench_dirty(c, suite, true);
    bench_mixed(c, suite);
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(6));
    targets = bench_world_tick
}
criterion_main!(benches);
