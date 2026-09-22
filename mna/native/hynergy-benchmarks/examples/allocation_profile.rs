use hynergy_benchmarks::allocation::{AllocationStats, CountingAllocator};
use hynergy_benchmarks::fixtures::{
    CircuitFamily, CpuWorkloadSize, ValidatedCpuScenario, WorldScenario,
};
use std::alloc::System;

#[global_allocator]
static ALLOCATOR: CountingAllocator<System> = CountingAllocator::new(System);

fn main() {
    let ticks = std::env::args()
        .nth(1)
        .map(|value| {
            value
                .parse::<u64>()
                .expect("allocation-profile tick count must be an integer")
        })
        .unwrap_or(20);

    assert!(ticks > 0, "allocation-profile tick count must be positive");

    println!(
        "{:<58} {:>10} {:>10} {:>14} {:>14}",
        "case", "alloc/tick", "realloc/tick", "alloc B/tick", "free B/tick"
    );

    profile_warm_sleep(ticks);
    profile_warm_active_medium(ticks);
    profile_warm_active_complex(ticks);
    profile_rhs_dirty(ticks);
    profile_matrix_dirty(ticks);
    profile_validated_cpu_steady(ticks);
    profile_validated_cpu_switching(ticks);
}

fn profile_warm_sleep(ticks: u64) {
    let mut scenario = WorldScenario::uniform(CircuitFamily::Simple, 256);
    scenario.tick().unwrap();

    report_case("world/tick/warm_sleep/simple/count_256", ticks, || {
        measure(|| scenario.tick().unwrap())
    });
}

fn profile_warm_active_medium(ticks: u64) {
    let mut scenario = WorldScenario::uniform(CircuitFamily::Medium, 32);
    scenario.warm(2).unwrap();

    report_case("world/tick/warm_active/medium/count_32", ticks, || {
        measure(|| scenario.tick().unwrap())
    });
}

fn profile_warm_active_complex(ticks: u64) {
    let mut scenario = WorldScenario::uniform(CircuitFamily::Complex, 4);
    scenario.warm(2).unwrap();

    report_case("world/tick/warm_active/complex/count_4", ticks, || {
        measure(|| scenario.tick().unwrap())
    });
}

fn profile_rhs_dirty(ticks: u64) {
    let mut scenario = WorldScenario::uniform(CircuitFamily::Medium, 32);
    scenario.warm(2).unwrap();

    report_case("world/tick/rhs_dirty/medium/count_32", ticks, || {
        scenario.toggle_rhs().unwrap();
        measure(|| scenario.tick().unwrap())
    });
}

fn profile_matrix_dirty(ticks: u64) {
    let mut scenario = WorldScenario::uniform(CircuitFamily::Medium, 32);
    scenario.warm(2).unwrap();

    report_case("world/tick/matrix_dirty/medium/count_32", ticks, || {
        scenario.toggle_matrix().unwrap();
        measure(|| scenario.tick().unwrap())
    });
}

fn profile_validated_cpu_steady(ticks: u64) {
    let mut scenario = ValidatedCpuScenario::new(CpuWorkloadSize::Small);
    scenario.warm(2).unwrap();

    report_case(
        "world/cpu/validated/steady_active/small_lanes12_devices1177",
        ticks,
        || measure(|| scenario.tick().unwrap()),
    );
}

fn profile_validated_cpu_switching(ticks: u64) {
    let mut scenario = ValidatedCpuScenario::new(CpuWorkloadSize::Small);
    scenario.warm(2).unwrap();
    let mut step = 0_u64;

    report_case(
        "world/cpu/validated/switching_heavy/small_lanes12_devices1177",
        ticks,
        || {
            step = step.wrapping_add(1);
            prepare_validated_switching_tick(&mut scenario, step);
            measure(|| scenario.tick().unwrap())
        },
    );
}

fn prepare_validated_switching_tick(scenario: &mut ValidatedCpuScenario, step: u64) {
    for lane_index in 0..scenario.lane_count() {
        let lane_seed = lane_index as u64;
        let a = step
            .wrapping_mul(73)
            .wrapping_add(lane_seed.wrapping_mul(37)) as u8;
        let b = step
            .rotate_left(7)
            .wrapping_mul(29)
            .wrapping_add(lane_seed.wrapping_mul(53)) as u8;
        let carry = ((step ^ lane_seed) & 1) != 0;

        scenario.set_lane_operands(lane_index, a, b, carry).unwrap();
    }
}

fn measure(operation: impl FnOnce()) -> AllocationStats {
    ALLOCATOR.start();
    operation();
    ALLOCATOR.stop()
}

fn report_case(name: &str, ticks: u64, mut iteration: impl FnMut() -> AllocationStats) {
    let mut total = AllocationStats::default();

    for _ in 0..ticks {
        total += iteration();
    }

    let divisor = ticks as f64;

    println!(
        "{name:<58} {:>10.2} {:>10.2} {:>14.1} {:>14.1}",
        total.allocation_calls() as f64 / divisor,
        total.reallocation_calls() as f64 / divisor,
        total.allocated_bytes() as f64 / divisor,
        total.deallocated_bytes() as f64 / divisor,
    );
}
