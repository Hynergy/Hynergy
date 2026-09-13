use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hynergy_engine::{Engine, World, WorldCommand};
use hynergy_model::{
    device::{
        definition::{DefinitionId, DeviceId, PrimitiveElementKind, TerminalId},
        registry::DefinitionRegistry,
    },
    network::WireId,
};
use std::{hint::black_box, time::Duration};

const BUILD_SIZES: &[usize] = &[16, 64, 256, 1_024];

const WIRE_SIZES: &[usize] = &[32, 128, 512, 2_048];

const ISLAND_SIZES: &[usize] = &[4, 16, 64, 256];

#[inline]
fn wire(raw: usize) -> WireId {
    WireId::try_from(u32::try_from(raw).expect("benchmark WireId must fit u32"))
        .expect("benchmark WireIds are one-based")
}

#[inline]
fn device(raw: usize) -> DeviceId {
    DeviceId::try_from(u32::try_from(raw).expect("benchmark DeviceId must fit u32"))
        .expect("benchmark DeviceIds are one-based")
}

#[inline]
fn admittance() -> DefinitionId {
    PrimitiveElementKind::Admittance.into()
}

fn line_world(wire_count: usize) -> World {
    assert!(wire_count >= 2);

    let mut world = World::default();

    for id in 1..=wire_count {
        world.add_wire(wire(id)).unwrap();
    }

    for id in 1..wire_count {
        world.connect_wires(wire(id), wire(id + 1)).unwrap();
    }

    world
}

fn ring_world(wire_count: usize) -> World {
    assert!(wire_count >= 3);

    let mut world = line_world(wire_count);

    world.connect_wires(wire(1), wire(wire_count)).unwrap();

    world
}

fn sparse_terminal_world(wire_count: usize) -> World {
    let definitions = DefinitionRegistry::new();

    let mut world = line_world(wire_count);

    let device_id = device(1);
    let target_wire = wire(wire_count / 2);

    world
        .add_device(&definitions, device_id, admittance())
        .unwrap();

    world
        .attach_terminal(target_wire, device_id, TerminalId::new(0))
        .unwrap();

    world
}

fn bus_world(device_count: usize) -> World {
    let definitions = DefinitionRegistry::new();

    let mut world = World::default();
    let bus = wire(1);

    world.add_wire(bus).unwrap();

    for id in 1..=device_count {
        let device_id = device(id);

        world
            .add_device(&definitions, device_id, admittance())
            .unwrap();

        world
            .attach_terminal(bus, device_id, TerminalId::new(0))
            .unwrap();
    }

    world
}

fn two_island_world(devices_per_side: usize) -> World {
    let definitions = DefinitionRegistry::new();

    let mut world = World::default();

    let left_wire = wire(1);
    let right_wire = wire(2);

    world.add_wire(left_wire).unwrap();
    world.add_wire(right_wire).unwrap();

    for index in 0..devices_per_side {
        let left_device = device(index + 1);
        let right_device = device(devices_per_side + index + 1);

        world
            .add_device(&definitions, left_device, admittance())
            .unwrap();

        world
            .attach_terminal(left_wire, left_device, TerminalId::new(0))
            .unwrap();

        world
            .add_device(&definitions, right_device, admittance())
            .unwrap();

        world
            .attach_terminal(right_wire, right_device, TerminalId::new(0))
            .unwrap();
    }

    world
}

fn articulation_world(branch_count: usize) -> World {
    let definitions = DefinitionRegistry::new();

    let mut world = World::default();

    let center = wire(1);

    world.add_wire(center).unwrap();

    for branch in 0..branch_count {
        let branch_wire = wire(branch + 2);

        world.add_wire(branch_wire).unwrap();
        world.connect_wires(center, branch_wire).unwrap();
    }

    for branch in 0..branch_count {
        let branch_wire = wire(branch + 2);
        let device_id = device(branch + 1);

        world
            .add_device(&definitions, device_id, admittance())
            .unwrap();

        world
            .attach_terminal(branch_wire, device_id, TerminalId::new(0))
            .unwrap();
    }

    world
}

fn wire_chain_commands(wire_count: usize) -> Vec<WorldCommand> {
    let mut commands = Vec::with_capacity(wire_count.saturating_mul(2).saturating_sub(1));

    for id in 1..=wire_count {
        commands.push(WorldCommand::AddWire { wire: wire(id) });
    }

    for id in 1..wire_count {
        commands.push(WorldCommand::ConnectWires {
            wire_a: wire(id),
            wire_b: wire(id + 1),
        });
    }

    commands
}

fn mixed_world_commands(size: usize) -> Vec<WorldCommand> {
    let mut commands = Vec::with_capacity(size.saturating_mul(4).saturating_sub(1));

    for id in 1..=size {
        commands.push(WorldCommand::AddWire { wire: wire(id) });
    }

    for id in 1..size {
        commands.push(WorldCommand::ConnectWires {
            wire_a: wire(id),
            wire_b: wire(id + 1),
        });
    }

    for id in 1..=size {
        commands.push(WorldCommand::AddDevice {
            device: device(id),
            definition: admittance(),
        });

        commands.push(WorldCommand::AttachTerminal {
            wire: wire(id),
            device: device(id),
            terminal: TerminalId::new(0),
        });
    }

    commands
}

fn bench_command_stream(
    c: &mut Criterion,
    name: &str,
    command_builder: fn(usize) -> Vec<WorldCommand>,
) {
    let mut group = c.benchmark_group(format!("engine_commands/{name}"));

    for &size in BUILD_SIZES {
        let commands = command_builder(size);

        group.throughput(Throughput::Elements(commands.len() as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter_batched_ref(
                || {
                    let mut engine = Engine::new();
                    let world_id = engine.new_world().unwrap();

                    (engine, world_id)
                },
                |state| {
                    let (engine, world_id) = state;

                    for &command in &commands {
                        engine.apply_world_command(*world_id, command).unwrap();
                    }

                    black_box(engine.world(*world_id).unwrap().network());
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_engine_command_streams(c: &mut Criterion) {
    bench_command_stream(c, "wire_chain_build", wire_chain_commands);

    bench_command_stream(c, "mixed_world_build", mixed_world_commands);
}

fn bench_disconnect_bridge(c: &mut Criterion) {
    let mut group = c.benchmark_group("topology/disconnect_bridge");

    for &size in WIRE_SIZES {
        let baseline = line_world(size);

        let left = wire(size / 2);
        let right = wire(size / 2 + 1);

        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(BenchmarkId::new("wires", size), &size, move |b, _| {
            b.iter_batched_ref(
                || baseline.clone(),
                |world| {
                    world.disconnect_wires(left, right).unwrap();

                    black_box(world.network());
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}

fn bench_disconnect_redundant_edge(c: &mut Criterion) {
    let mut group = c.benchmark_group("topology/disconnect_redundant_edge");

    for &size in WIRE_SIZES {
        let baseline = ring_world(size);

        let first = wire(1);
        let last = wire(size);

        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(BenchmarkId::new("wires", size), &size, move |b, _| {
            b.iter_batched_ref(
                || baseline.clone(),
                |world| {
                    world.disconnect_wires(first, last).unwrap();

                    black_box(world.network());
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}

fn bench_detach_terminal_vs_physical_wires(c: &mut Criterion) {
    let mut group = c.benchmark_group("topology/detach_terminal_physical_wires");

    for &size in WIRE_SIZES {
        let baseline = sparse_terminal_world(size);

        let target_wire = wire(size / 2);
        let target_device = device(1);

        group.bench_with_input(BenchmarkId::new("wires", size), &size, move |b, _| {
            b.iter_batched_ref(
                || baseline.clone(),
                |world| {
                    world
                        .detach_terminal(target_wire, target_device, TerminalId::new(0))
                        .unwrap();

                    black_box(world.network());
                },
                BatchSize::PerIteration,
            );
        });
    }

    group.finish();
}

fn bench_detach_terminal_vs_incidence_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("topology/detach_terminal_incidence_count");

    for &size in ISLAND_SIZES {
        let baseline = bus_world(size);

        let bus = wire(1);
        let target = device(size);

        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("terminal_incidences", size),
            &size,
            move |b, _| {
                b.iter_batched_ref(
                    || baseline.clone(),
                    |world| {
                        world
                            .detach_terminal(bus, target, TerminalId::new(0))
                            .unwrap();

                        black_box(world.network());
                    },
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

fn bench_merge_device_backed_islands(c: &mut Criterion) {
    let mut group = c.benchmark_group("topology/merge_device_backed_islands");

    for &size in ISLAND_SIZES {
        let baseline = two_island_world(size);

        let left = wire(1);
        let right = wire(2);

        // The merge heuristic will rewrite roughly one side.
        group.throughput(Throughput::Elements(size as u64));

        group.bench_with_input(
            BenchmarkId::new("devices_per_side", size),
            &size,
            move |b, _| {
                b.iter_batched_ref(
                    || baseline.clone(),
                    |world| {
                        world.connect_wires(left, right).unwrap();

                        black_box(world.network());
                    },
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

fn bench_remove_articulation_wire(c: &mut Criterion) {
    let mut group = c.benchmark_group("topology/remove_articulation_wire");

    for &branches in ISLAND_SIZES {
        let baseline = articulation_world(branches);

        let center = wire(1);

        group.throughput(Throughput::Elements(branches as u64));

        group.bench_with_input(
            BenchmarkId::new("branches", branches),
            &branches,
            move |b, _| {
                b.iter_batched_ref(
                    || baseline.clone(),
                    |world| {
                        world.remove_wire(center).unwrap();

                        black_box(world.network());
                    },
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets =
        bench_engine_command_streams,
        bench_disconnect_bridge,
        bench_disconnect_redundant_edge,
        bench_detach_terminal_vs_physical_wires,
        bench_detach_terminal_vs_incidence_count,
        bench_merge_device_backed_islands,
        bench_remove_articulation_wire
}

criterion_main!(benches);
