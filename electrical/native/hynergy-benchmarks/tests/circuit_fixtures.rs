use hynergy_benchmarks::fixtures::digital::{
    full_adder_truth_table, inverter_truth_table, logic_level, mux_truth_table, nand_truth_table,
    nor_truth_table, tick_delay_trace,
};
use hynergy_benchmarks::fixtures::workloads::{build_mixed_with_counts, build_subscription_case};
use hynergy_benchmarks::fixtures::{
    BenchSuite, CircuitFamily, CpuScenario, CpuWorkloadSize, FullCpuScenario, MixedProfile,
    SubscriptionProfile, TopologyMutation, TopologyScenario, ValidatedCpuScenario, WorldScenario,
};

#[test]
fn benchmark_suites_keep_core_representative_and_full_complete() {
    assert_eq!(BenchSuite::Core.counts(CircuitFamily::Simple), &[256]);
    assert_eq!(BenchSuite::Core.counts(CircuitFamily::Medium), &[32]);
    assert_eq!(BenchSuite::Core.counts(CircuitFamily::Complex), &[4]);
    assert_eq!(BenchSuite::Core.cpu_sizes(), &[CpuWorkloadSize::Small]);
    assert_eq!(BenchSuite::Core.protocol_sizes(), &[128]);
    assert_eq!(
        BenchSuite::Core.mixed_profiles(),
        &[MixedProfile::Representative],
    );
    assert_eq!(MixedProfile::Representative.counts(), (128, 16, 1));

    assert_eq!(
        BenchSuite::Full.counts(CircuitFamily::Simple),
        &[16, 256, 4096]
    );
    assert_eq!(
        BenchSuite::Full.counts(CircuitFamily::Medium),
        &[4, 32, 256]
    );
    assert_eq!(BenchSuite::Full.counts(CircuitFamily::Complex), &[1, 4, 16]);
    assert_eq!(
        BenchSuite::Full.cpu_sizes(),
        &[
            CpuWorkloadSize::Tiny,
            CpuWorkloadSize::Small,
            CpuWorkloadSize::Stress,
        ],
    );
    assert_eq!(BenchSuite::Full.protocol_sizes(), &[8, 32, 128, 512]);
    assert_eq!(BenchSuite::Full.mixed_profiles(), &MixedProfile::ALL);
}

#[test]
fn baseline_circuit_families_build_and_tick() {
    for family in CircuitFamily::ALL {
        let mut scenario = WorldScenario::uniform(family, 1);

        let ticks = if family.is_stateful() { 8 } else { 2 };

        for _ in 0..ticks {
            scenario.tick().unwrap();
        }

        assert_eq!(scenario.metadata().circuits(), 1);
        assert_eq!(scenario.metadata().family_count(family), 1);
        assert_eq!(scenario.metadata().devices(), family.devices_per_circuit(),);
    }
}

#[test]
fn rhs_only_and_matrix_dirty_mutations_remain_solvable() {
    for family in CircuitFamily::ALL {
        let mut rhs = WorldScenario::uniform(family, 2);
        rhs.tick().unwrap();

        for _ in 0..4 {
            rhs.toggle_rhs().unwrap();
            rhs.tick().unwrap();
        }

        assert_eq!(rhs.metadata().devices(), 2 * family.devices_per_circuit());

        let mut matrix = WorldScenario::uniform(family, 2);
        matrix.tick().unwrap();

        for _ in 0..4 {
            matrix.toggle_matrix().unwrap();
            matrix.tick().unwrap();
        }

        assert_eq!(
            matrix.metadata().devices(),
            2 * family.devices_per_circuit(),
        );
    }
}

#[test]
fn small_mixed_world_metadata_matches_construction() {
    let mut scenario = build_mixed_with_counts(3, 2, 1, false);

    assert_eq!(scenario.metadata().circuits(), 6);
    assert_eq!(scenario.metadata().family_count(CircuitFamily::Simple), 3);
    assert_eq!(scenario.metadata().family_count(CircuitFamily::Medium), 2);
    assert_eq!(scenario.metadata().family_count(CircuitFamily::Complex), 1);

    let expected_devices = 3 * CircuitFamily::Simple.devices_per_circuit()
        + 2 * CircuitFamily::Medium.devices_per_circuit()
        + CircuitFamily::Complex.devices_per_circuit();

    assert_eq!(scenario.metadata().devices(), expected_devices);

    for _ in 0..4 {
        scenario.tick().unwrap();
    }
}

#[test]
fn configured_mixed_profiles_have_stable_counts() {
    assert_eq!(MixedProfile::Typical.counts(), (1024, 64, 4));
    assert_eq!(MixedProfile::Active.counts(), (256, 128, 8));
    assert_eq!(MixedProfile::Stress.counts(), (1024, 256, 16));
}

#[test]
fn topology_merge_and_split_cases_remain_solvable() {
    let mut merge = TopologyScenario::new(TopologyMutation::Merge);
    merge.run_once().unwrap();
    assert_eq!(merge.circuits(), 2);

    let mut split = TopologyScenario::new(TopologyMutation::Split);
    split.run_once().unwrap();
    assert_eq!(split.circuits(), 2);
}

#[test]
fn subscription_profiles_install_expected_subscription_counts() {
    let sparse = build_subscription_case(SubscriptionProfile::Sparse, 8);
    assert_eq!(sparse.subscription_count(), 1);
    assert_eq!(sparse.metadata().subscriptions(), 1);

    let dense_sleep = build_subscription_case(SubscriptionProfile::DenseSleeping, 8);
    assert_eq!(dense_sleep.subscription_count(), 8);
    assert_eq!(dense_sleep.metadata().subscriptions(), 8);

    let dense_changing = build_subscription_case(SubscriptionProfile::DenseChanging, 8);
    assert_eq!(dense_changing.subscription_count(), 8);
    assert_eq!(dense_changing.metadata().subscriptions(), 8);
}

#[test]
fn subscription_cases_tick_repeatedly() {
    let mut sleeping = build_subscription_case(SubscriptionProfile::DenseSleeping, 8);
    sleeping.tick().unwrap();
    sleeping.tick().unwrap();
    assert_eq!(sleeping.subscription_update_count(), 0);

    let mut changing = build_subscription_case(SubscriptionProfile::DenseChanging, 8);
    changing.tick().unwrap();

    for _ in 0..4 {
        changing.toggle_rhs().unwrap();
        changing.tick().unwrap();
        assert_eq!(changing.subscription_update_count(), 8);
    }
}

#[test]
fn cpu_workload_sizes_have_stable_shapes() {
    assert_eq!(CpuWorkloadSize::Tiny.logic_nodes(), 128);
    assert_eq!(CpuWorkloadSize::Small.logic_nodes(), 512);
    assert_eq!(CpuWorkloadSize::Stress.logic_nodes(), 2048);

    assert_eq!(CpuWorkloadSize::Tiny.device_count(), 706);
    assert_eq!(CpuWorkloadSize::Small.device_count(), 2818);
    assert_eq!(CpuWorkloadSize::Stress.device_count(), 11266);

    assert_eq!(CpuWorkloadSize::Tiny.nonlinear_device_count(), 256);
    assert_eq!(CpuWorkloadSize::Small.nonlinear_device_count(), 1024);
    assert_eq!(CpuWorkloadSize::Stress.nonlinear_device_count(), 4096);

    assert_eq!(CpuWorkloadSize::Tiny.stateful_device_count(), 64);
    assert_eq!(CpuWorkloadSize::Small.stateful_device_count(), 256);
    assert_eq!(CpuWorkloadSize::Stress.stateful_device_count(), 1024);
}

#[test]
fn cpu_fixture_builds_ticks_and_switches_clock() {
    let mut scenario = CpuScenario::for_test(16);

    assert_eq!(scenario.logic_nodes(), 16);
    assert_eq!(scenario.device_count(), 90);
    assert_eq!(scenario.nonlinear_device_count(), 32);
    assert_eq!(scenario.stateful_device_count(), 8);

    scenario.tick().unwrap();

    for _ in 0..4 {
        scenario.toggle_clock().unwrap();
        scenario.tick().unwrap();
    }
}

fn assert_logic_voltage(actual: f64, expected: bool) {
    let level = logic_level(actual)
        .unwrap_or_else(|| panic!("voltage {actual:.6} V is not a valid digital LOW/HIGH level"));
    assert_eq!(level, expected, "unexpected logic level at {actual:.6} V");
}

#[test]
fn validated_inverter_nand_nor_and_mux_match_truth_tables() {
    for (input, voltage) in inverter_truth_table() {
        assert_logic_voltage(voltage, !input);
    }

    for ((a, b), voltage) in nand_truth_table() {
        assert_logic_voltage(voltage, !(a && b));
    }

    for ((a, b), voltage) in nor_truth_table() {
        assert_logic_voltage(voltage, !(a || b));
    }

    for ((a, b, select), voltage) in mux_truth_table() {
        assert_logic_voltage(voltage, if select { b } else { a });
    }
}

#[test]
fn validated_full_adder_matches_all_input_combinations() {
    for ((a, b, carry_in), sum_voltage, carry_voltage) in full_adder_truth_table() {
        let total = u8::from(a) + u8::from(b) + u8::from(carry_in);
        assert_logic_voltage(sum_voltage, (total & 1) != 0);
        assert_logic_voltage(carry_voltage, total >= 2);
    }
}

#[test]
fn validated_tick_delay_exposes_previous_tick_input() {
    let trace = tick_delay_trace();
    assert_eq!(trace.len(), 4);

    assert_logic_voltage(trace[0], false);
    assert_logic_voltage(trace[1], false);
    assert_logic_voltage(trace[2], true);
    assert_logic_voltage(trace[3], false);
}

#[test]
fn validated_cpu_datapath_registers_known_additions() {
    let mut scenario = ValidatedCpuScenario::for_test(1);
    assert_eq!(scenario.device_count(), 99);
    assert_eq!(scenario.nonlinear_device_count(), 72);
    assert_eq!(scenario.stateful_device_count(), 9);

    scenario.set_lane_operands(0, 37, 19, false).unwrap();
    scenario.tick().unwrap();
    assert_eq!(scenario.registered_result(0), Some((0, false)));
    scenario.tick().unwrap();
    assert_eq!(scenario.registered_result(0), Some((56, false)));

    scenario.set_lane_operands(0, 200, 100, true).unwrap();
    scenario.tick().unwrap();
    assert_eq!(scenario.registered_result(0), Some((56, false)));
    scenario.tick().unwrap();
    assert_eq!(scenario.registered_result(0), Some((45, true)));
}

#[test]
fn validated_cpu_sizes_match_primitive_gate_topology() {
    assert_eq!(CpuWorkloadSize::Tiny.validated_device_count(), 295);
    assert_eq!(CpuWorkloadSize::Small.validated_device_count(), 1177);
    assert_eq!(CpuWorkloadSize::Stress.validated_device_count(), 4705);

    assert_eq!(
        CpuWorkloadSize::Tiny.validated_nonlinear_device_count(),
        216
    );
    assert_eq!(
        CpuWorkloadSize::Small.validated_nonlinear_device_count(),
        864
    );
    assert_eq!(
        CpuWorkloadSize::Stress.validated_nonlinear_device_count(),
        3456
    );

    assert_eq!(CpuWorkloadSize::Tiny.validated_stateful_device_count(), 27);
    assert_eq!(
        CpuWorkloadSize::Small.validated_stateful_device_count(),
        108
    );
    assert_eq!(
        CpuWorkloadSize::Stress.validated_stateful_device_count(),
        432
    );
}

fn assert_full_cpu_program(
    width: hynergy_benchmarks::fixtures::FullCpuWidth,
    counts: (usize, usize, usize),
    overflow_rhs: u32,
) {
    let mut cpu = FullCpuScenario::for_test_width(width);

    assert_eq!(cpu.width(), width);
    assert_eq!(cpu.device_count(), counts.0);
    assert_eq!(cpu.nonlinear_device_count(), counts.1);
    assert_eq!(cpu.stateful_device_count(), counts.2);

    cpu.tick().unwrap();

    let expected: [(u8, u8, u32, u8, u32, u32, bool); 14] = [
        // addr, opcode, imm, next_pc, A, B, carry
        (0, 1, 5, 1, 5, 0, false),
        (1, 2, 7, 2, 5, 7, false),
        (2, 3, 0, 3, 12, 7, false),
        (3, 2, 3, 4, 12, 3, false),
        (4, 4, 0, 5, 15, 3, false),
        (5, 2, 15, 6, 15, 15, false),
        (6, 5, 0, 7, 15, 15, false),
        (7, 2, overflow_rhs, 8, 15, overflow_rhs, false),
        (8, 3, 0, 9, 0, overflow_rhs, true),
        (9, 6, 11, 11, 0, overflow_rhs, true),
        (11, 1, 42, 12, 42, overflow_rhs, true),
        (12, 2, 1, 13, 42, 1, true),
        (13, 3, 0, 14, 43, 1, false),
        (14, 7, 0, 0, 43, 1, false),
    ];

    for &(address, opcode, immediate, next_pc, a, b, carry) in &expected {
        cpu.tick().unwrap();
        let executing = cpu
            .snapshot()
            .expect("full CPU signals must settle to valid digital levels");

        assert!(executing.execute_phase());
        assert_eq!(executing.pc(), address);
        assert_eq!(executing.opcode(), opcode);
        assert_eq!(executing.immediate(), immediate);

        if let Err(error) = cpu.tick() {
            panic!("initial CPU tick failed for {width:?}: {error:?}");
        }

        let committed = cpu
            .snapshot()
            .expect("full CPU register outputs must be valid after execution");

        assert!(!committed.execute_phase());
        assert_eq!(committed.pc(), next_pc);
        assert_eq!(committed.a(), a);
        assert_eq!(committed.b(), b);
        assert_eq!(committed.carry(), carry);
    }
}

#[test]
fn full_cpu_executes_program_and_updates_architectural_state() {
    assert_full_cpu_program(
        hynergy_benchmarks::fixtures::FullCpuWidth::Bits8,
        (549, 515, 33),
        241,
    );
}

#[test]
fn full_cpu_32_bit_executes_program_and_updates_architectural_state() {
    assert_full_cpu_program(
        hynergy_benchmarks::fixtures::FullCpuWidth::Bits32,
        (1557, 1451, 105),
        u32::MAX - 14,
    );
}

#[cfg(feature = "solver-profiling")]
#[test]
fn full_cpu_exposes_solver_tick_profile() {
    let mut cpu =
        FullCpuScenario::new_with_width(hynergy_benchmarks::fixtures::FullCpuWidth::Bits8);

    cpu.tick().unwrap();

    let profile = cpu.solver_tick_profile();

    assert!(!profile.islands().is_empty());
    assert!(profile.total_mna_solves() > 0);
    assert!(profile.total_matrix_factorizations() > 0);
    assert!(profile.total_nonlinear_iterations() > 0);
}
