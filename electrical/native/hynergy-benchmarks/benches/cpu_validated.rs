use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hynergy_benchmarks::fixtures::{BenchSuite, ValidatedCpuScenario};
use std::{hint::black_box, time::Duration};

fn bench_cold(c: &mut Criterion, suite: BenchSuite) {
    let mut group = c.benchmark_group("world/cpu/validated/cold");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(5));

    for &size in suite.cpu_sizes() {
        group.throughput(Throughput::Elements(size.validated_device_count() as u64));
        group.bench_function(
            BenchmarkId::from_parameter(format!(
                "{}_lanes{}_devices{}_nonlinear{}_stateful{}",
                size.name(),
                size.validated_lanes(),
                size.validated_device_count(),
                size.validated_nonlinear_device_count(),
                size.validated_stateful_device_count()
            )),
            |b| {
                b.iter_batched_ref(
                    || ValidatedCpuScenario::new(size),
                    |scenario| {
                        scenario.tick().unwrap();
                        black_box(scenario.device_count());
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_steady(c: &mut Criterion, suite: BenchSuite) {
    let mut group = c.benchmark_group("world/cpu/validated/steady_active");
    group.sample_size(15);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(7));

    for &size in suite.cpu_sizes() {
        group.throughput(Throughput::Elements(size.validated_device_count() as u64));
        group.bench_function(
            BenchmarkId::from_parameter(format!(
                "{}_lanes{}_devices{}",
                size.name(),
                size.validated_lanes(),
                size.validated_device_count()
            )),
            |b| {
                let mut scenario = ValidatedCpuScenario::new(size);
                scenario.warm(2).unwrap();
                b.iter(|| {
                    scenario.tick().unwrap();
                    black_box(scenario.stateful_device_count());
                });
            },
        );
    }

    group.finish();
}

fn bench_switching(c: &mut Criterion, suite: BenchSuite) {
    let mut group = c.benchmark_group("world/cpu/validated/switching_heavy");
    group.sample_size(15);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(7));

    for &size in suite.cpu_sizes() {
        group.throughput(Throughput::Elements(size.validated_device_count() as u64));
        group.bench_function(
            BenchmarkId::from_parameter(format!(
                "{}_lanes{}_devices{}",
                size.name(),
                size.validated_lanes(),
                size.validated_device_count()
            )),
            |b| {
                let mut scenario = ValidatedCpuScenario::new(size);
                scenario.warm(2).unwrap();
                b.iter_custom(|iterations| scenario.measure_switching_ticks(iterations));
            },
        );
    }

    group.finish();
}

fn bench_cpu_validated(c: &mut Criterion) {
    let suite = BenchSuite::from_env();
    bench_cold(c, suite);
    bench_steady(c, suite);
    bench_switching(c, suite);
}

criterion_group!(benches, bench_cpu_validated);
criterion_main!(benches);
