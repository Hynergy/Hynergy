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
            "tick={tick:02} islands={} nonlinear_iterations={} solves={} factorizations={} stability_changes={} max_delta={:.6e}",
            profile.islands().len(),
            profile.total_nonlinear_iterations(),
            profile.total_mna_solves(),
            profile.total_matrix_factorizations(),
            profile.total_stability_changes(),
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

        println!(
            "  island={} iterations={} solves={} factorizations={} stability_changes={} max_delta={:.6e}",
            hottest.island_index(),
            hottest.nonlinear_iterations(),
            hottest.mna_solves(),
            hottest.matrix_factorizations(),
            hottest.stability_changes(),
            hottest.max_solution_delta(),
        );

        for (iteration, sample) in hottest.iterations().iter().enumerate() {
            println!(
                "    iter={:03} changes={:04} max_delta={:.6e}",
                iteration + 1,
                sample.stability_changes(),
                sample.max_solution_delta(),
            );
        }
    }

    println!();
}
