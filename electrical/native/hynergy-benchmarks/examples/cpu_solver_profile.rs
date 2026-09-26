use hynergy_benchmarks::fixtures::{FullCpuScenario, FullCpuWidth};

fn main() {
    let ticks = std::env::args()
        .nth(1)
        .map(|value| {
            value
                .parse::<usize>()
                .expect("tick count must be a positive integer")
        })
        .unwrap_or(8);

    assert!(ticks > 0, "tick count must be greater than zero");

    for width in FullCpuWidth::ALL {
        profile_cpu(width, ticks);
    }
}

fn profile_cpu(width: FullCpuWidth, ticks: usize) {
    let mut cpu = FullCpuScenario::new_with_width(width);

    println!(
        "{} devices={} nonlinear={} stateful={}",
        width.name(),
        width.device_count(),
        width.nonlinear_device_count(),
        width.stateful_device_count(),
    );

    for tick in 0..ticks {
        cpu.tick().expect("profiled CPU tick must converge");
        let profile = cpu.solver_tick_profile();

        println!(
            "tick={tick:02} islands={} nonlinear_iterations={} solves={} factorizations={} matrix_source_changes={} stability_changes={} discrete_islands={} drivers={} closures={} closure_rounds={} driver_scans={} output_updates={} zero_update_attempts={} barrier_exits={} verification_solves={} fast_fallbacks={} max_delta={:.6e}",
            profile.islands().len(),
            profile.total_nonlinear_iterations(),
            profile.total_mna_solves(),
            profile.total_matrix_factorizations(),
            profile.total_matrix_source_changes(),
            profile.total_stability_changes(),
            profile.planned_discrete_islands(),
            profile.total_qualified_discrete_drivers(),
            profile.total_discrete_closure_attempts(),
            profile.total_discrete_closure_rounds(),
            profile.total_discrete_driver_scans(),
            profile.total_discrete_output_updates(),
            profile.total_discrete_zero_update_attempts(),
            profile.total_discrete_barrier_exits(),
            profile.total_discrete_verification_solves(),
            profile.total_discrete_fallbacks(),
            profile.max_solution_delta(),
        );

        let Some(hottest) = profile
            .islands()
            .iter()
            .filter(|island| island.is_nonlinear() && !island.slept())
            .max_by_key(|island| island.nonlinear_iterations())
        else {
            continue;
        };

        let discrete = hottest.discrete();

        println!(
            "  island={} iterations={} solves={} factorizations={} matrix_source_changes={} stability_changes={} plan={} drivers={} matrix_barriers={} rhs_barriers={} closures={} closure_rounds={} driver_scans={} output_updates={} zero_update_attempts={} barrier_exits={} verification_solves={} budget_fallbacks={} invalid_fallbacks={} max_delta={:.6e}",
            hottest.island_index(),
            hottest.nonlinear_iterations(),
            hottest.mna_solves(),
            hottest.matrix_factorizations(),
            hottest.matrix_source_changes(),
            hottest.stability_changes(),
            discrete.has_plan(),
            discrete.qualified_drivers(),
            discrete.matrix_barriers(),
            discrete.rhs_barriers(),
            discrete.closure_attempts(),
            discrete.closure_rounds(),
            discrete.driver_scans(),
            discrete.output_updates(),
            discrete.zero_update_attempts(),
            discrete.barrier_exits(),
            discrete.verification_solves(),
            discrete.budget_fallbacks(),
            discrete.invalid_fallbacks(),
            hottest.max_solution_delta(),
        );

        for (iteration, sample) in hottest.iterations().iter().enumerate() {
            println!(
                "    iter={:03} matrix_source_changes={:04} stability_changes={:04} max_delta={:.6e}",
                iteration + 1,
                sample.matrix_source_changes(),
                sample.stability_changes(),
                sample.max_solution_delta(),
            );
        }
    }

    println!();
}
