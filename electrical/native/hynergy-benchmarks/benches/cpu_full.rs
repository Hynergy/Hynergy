use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hynergy_benchmarks::fixtures::{FullCpuScenario, FullCpuWidth};
use std::{hint::black_box, time::Duration};

fn bench_cold(c: &mut Criterion) {
    let mut group = c.benchmark_group("world/cpu/full/cold");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(5));

    for width in FullCpuWidth::ALL {
        let device_count = width.device_count();
        group.throughput(Throughput::Elements(device_count as u64));
        group.bench_function(
            BenchmarkId::from_parameter(format!(
                "{}_devices{}_nonlinear{}_stateful{}",
                width.name(),
                device_count,
                width.nonlinear_device_count(),
                width.stateful_device_count()
            )),
            move |b| {
                b.iter_batched_ref(
                    || FullCpuScenario::new_with_width(width),
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

fn bench_program_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("world/cpu/full/program_tick");
    group.sample_size(15);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(7));

    for width in FullCpuWidth::ALL {
        let device_count = width.device_count();
        group.throughput(Throughput::Elements(device_count as u64));
        group.bench_function(
            BenchmarkId::from_parameter(format!("{}_devices{}", width.name(), device_count)),
            move |b| {
                let mut scenario = FullCpuScenario::new_with_width(width);
                scenario.warm(4).unwrap();
                b.iter(|| {
                    scenario.tick().unwrap();
                    black_box(scenario.stateful_device_count());
                });
            },
        );
    }

    group.finish();
}

fn bench_instruction(c: &mut Criterion) {
    let mut group = c.benchmark_group("world/cpu/full/instruction");
    group.sample_size(15);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(7));
    group.throughput(Throughput::Elements(1));

    for width in FullCpuWidth::ALL {
        let device_count = width.device_count();
        group.bench_function(
            BenchmarkId::from_parameter(format!("{}_devices{}", width.name(), device_count)),
            move |b| {
                let mut scenario = FullCpuScenario::new_with_width(width);
                scenario.warm(4).unwrap();
                b.iter_custom(|instructions| scenario.measure_instructions(instructions));
            },
        );
    }

    group.finish();
}

fn bench_cpu_full(c: &mut Criterion) {
    bench_cold(c);
    bench_program_tick(c);
    bench_instruction(c);
}

criterion_group!(benches, bench_cpu_full);
criterion_main!(benches);
