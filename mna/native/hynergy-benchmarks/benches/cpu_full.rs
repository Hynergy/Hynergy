use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hynergy_benchmarks::fixtures::{
    FULL_CPU_DEVICE_COUNT, FULL_CPU_NONLINEAR_DEVICE_COUNT, FULL_CPU_STATEFUL_DEVICE_COUNT,
    FullCpuScenario,
};
use std::{hint::black_box, time::Duration};

fn bench_cold(c: &mut Criterion) {
    let mut group = c.benchmark_group("world/cpu/full/cold");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Elements(FULL_CPU_DEVICE_COUNT as u64));
    group.bench_function(
        BenchmarkId::from_parameter(format!(
            "cpu8_devices{}_nonlinear{}_stateful{}",
            FULL_CPU_DEVICE_COUNT, FULL_CPU_NONLINEAR_DEVICE_COUNT, FULL_CPU_STATEFUL_DEVICE_COUNT
        )),
        |b| {
            b.iter_batched_ref(
                FullCpuScenario::new,
                |scenario| {
                    scenario.tick().unwrap();
                    black_box(scenario.device_count());
                },
                BatchSize::LargeInput,
            );
        },
    );
    group.finish();
}

fn bench_program_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("world/cpu/full/program_tick");
    group.sample_size(15);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(7));
    group.throughput(Throughput::Elements(FULL_CPU_DEVICE_COUNT as u64));
    group.bench_function(
        BenchmarkId::from_parameter(format!("cpu8_devices{FULL_CPU_DEVICE_COUNT}")),
        |b| {
            let mut scenario = FullCpuScenario::new();
            scenario.warm(4).unwrap();
            b.iter(|| {
                scenario.tick().unwrap();
                black_box(scenario.stateful_device_count());
            });
        },
    );
    group.finish();
}

fn bench_instruction(c: &mut Criterion) {
    let mut group = c.benchmark_group("world/cpu/full/instruction");
    group.sample_size(15);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(7));
    group.throughput(Throughput::Elements(1));
    group.bench_function(
        BenchmarkId::from_parameter(format!("cpu8_devices{FULL_CPU_DEVICE_COUNT}")),
        |b| {
            let mut scenario = FullCpuScenario::new();
            scenario.warm(4).unwrap();
            b.iter_custom(|instructions| scenario.measure_instructions(instructions));
        },
    );
    group.finish();
}

fn bench_cpu_full(c: &mut Criterion) {
    bench_cold(c);
    bench_program_tick(c);
    bench_instruction(c);
}

criterion_group!(benches, bench_cpu_full);
criterion_main!(benches);
