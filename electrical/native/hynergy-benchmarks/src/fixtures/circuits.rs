use super::{CircuitFamily, CircuitHandle, IdAllocator};
use hynergy_engine::{Engine, WorldCommand};
use hynergy_model::device::definition::{DeviceId, PrimitiveElementKind, TerminalId};
use hynergy_model::network::WireId;
use hynergy_model::parameter::ParameterId;

pub(crate) fn build_circuit(
    engine: &mut Engine,
    world_id: u32,
    ids: &mut IdAllocator,
    family: CircuitFamily,
) -> CircuitHandle {
    match family {
        CircuitFamily::Simple => build_simple(engine, world_id, ids),
        CircuitFamily::Medium => build_medium(engine, world_id, ids),
        CircuitFamily::Complex => build_complex(engine, world_id, ids),
    }
}

fn add_wire(engine: &mut Engine, world_id: u32, ids: &mut IdAllocator) -> WireId {
    let wire = ids.wire();
    engine
        .apply_world_command(world_id, WorldCommand::AddWire { wire })
        .unwrap();
    wire
}

fn add_primitive(
    engine: &mut Engine,
    world_id: u32,
    ids: &mut IdAllocator,
    kind: PrimitiveElementKind,
    parameters: &[f64],
) -> DeviceId {
    debug_assert_eq!(kind.parameter_count(), parameters.len());

    let device = ids.device();

    engine
        .apply_world_command(
            world_id,
            WorldCommand::AddDevice {
                device,
                definition: kind.into(),
            },
        )
        .unwrap();

    for (index, &value) in parameters.iter().enumerate() {
        engine
            .apply_world_command(
                world_id,
                WorldCommand::SetDeviceParameter {
                    device,
                    parameter: ParameterId::new(index as u32),
                    value,
                },
            )
            .unwrap();
    }

    device
}

fn attach(engine: &mut Engine, world_id: u32, wire: WireId, device: DeviceId, terminal: u32) {
    engine
        .apply_world_command(
            world_id,
            WorldCommand::AttachTerminal {
                wire,
                device,
                terminal: TerminalId::new(terminal),
            },
        )
        .unwrap();
}

fn attach_all(engine: &mut Engine, world_id: u32, device: DeviceId, wires: &[WireId]) {
    for (terminal, &wire) in wires.iter().enumerate() {
        attach(
            engine,
            world_id,
            wire,
            device,
            u32::try_from(terminal).expect("benchmark terminal index must fit u16"),
        );
    }
}

fn two_terminal(
    engine: &mut Engine,
    world_id: u32,
    ids: &mut IdAllocator,
    kind: PrimitiveElementKind,
    a: WireId,
    b: WireId,
    parameters: &[f64],
) -> DeviceId {
    let device = add_primitive(engine, world_id, ids, kind, parameters);
    attach_all(engine, world_id, device, &[a, b]);
    device
}

fn build_simple(engine: &mut Engine, world_id: u32, ids: &mut IdAllocator) -> CircuitHandle {
    let ground = add_wire(engine, world_id, ids);
    let supply = add_wire(engine, world_id, ids);
    let middle = add_wire(engine, world_id, ids);
    let output = add_wire(engine, world_id, ids);

    let source = two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::VoltageSource,
        supply,
        ground,
        &[5.0],
    );

    two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::Resistance,
        supply,
        middle,
        &[10.0],
    );

    let matrix_device = two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::Conductance,
        middle,
        ground,
        &[0.1],
    );

    two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::Resistance,
        middle,
        output,
        &[20.0],
    );

    two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::Conductance,
        output,
        ground,
        &[0.2],
    );

    two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::Resistance,
        supply,
        output,
        &[50.0],
    );

    two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::CurrentSource,
        output,
        ground,
        &[0.01],
    );

    CircuitHandle {
        family: CircuitFamily::Simple,
        source,
        matrix_device,
        observer_device: source,
        ground,
    }
}

fn build_medium(engine: &mut Engine, world_id: u32, ids: &mut IdAllocator) -> CircuitHandle {
    const BRANCHES: usize = 8;

    let ground = add_wire(engine, world_id, ids);
    let supply = add_wire(engine, world_id, ids);
    let nodes = (0..BRANCHES)
        .map(|_| add_wire(engine, world_id, ids))
        .collect::<Vec<_>>();

    let source = two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::VoltageSource,
        supply,
        ground,
        &[5.0],
    );

    let mut matrix_device = None;
    let mut observer_device = None;

    for (index, &node) in nodes.iter().enumerate() {
        two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Resistance,
            supply,
            node,
            &[10.0 + index as f64],
        );

        let conductance = two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Conductance,
            node,
            ground,
            &[if index == 0 {
                0.1
            } else {
                0.08 + 0.005 * index as f64
            }],
        );

        if index == 0 {
            matrix_device = Some(conductance);
            observer_device = Some(conductance);
        }

        two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Capacitor,
            node,
            ground,
            &[1.0e-4 * (index + 1) as f64],
        );

        if index % 2 == 0 {
            two_terminal(
                engine,
                world_id,
                ids,
                PrimitiveElementKind::Inductor,
                node,
                ground,
                &[1.0e-3 * (index + 1) as f64],
            );
        }
    }

    for index in 0..BRANCHES - 1 {
        two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Resistance,
            nodes[index],
            nodes[index + 1],
            &[30.0 + index as f64],
        );
    }

    CircuitHandle {
        family: CircuitFamily::Medium,
        source,
        matrix_device: matrix_device.expect("medium fixture has a conductance"),
        observer_device: observer_device.expect("medium fixture has an observer device"),
        ground,
    }
}

fn build_complex(engine: &mut Engine, world_id: u32, ids: &mut IdAllocator) -> CircuitHandle {
    const BRANCHES: usize = 24;
    const CONTROLLED: usize = 6;

    let ground = add_wire(engine, world_id, ids);
    let supply = add_wire(engine, world_id, ids);
    let control = add_wire(engine, world_id, ids);
    let nodes = (0..BRANCHES)
        .map(|_| add_wire(engine, world_id, ids))
        .collect::<Vec<_>>();

    let source = two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::VoltageSource,
        supply,
        ground,
        &[5.0],
    );

    two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::VoltageSource,
        control,
        ground,
        &[3.0],
    );

    let mut matrix_device = None;
    let mut observer_device = None;

    for (index, &node) in nodes.iter().enumerate() {
        two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Resistance,
            supply,
            node,
            &[12.0 + (index % 7) as f64],
        );

        let conductance = two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Conductance,
            node,
            ground,
            &[if index == 0 {
                0.1
            } else {
                0.04 + 0.002 * (index % 9) as f64
            }],
        );

        if index == 0 {
            matrix_device = Some(conductance);
            observer_device = Some(conductance);
        }

        two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Capacitor,
            node,
            ground,
            &[5.0e-5 * (1 + index % 8) as f64],
        );

        two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::CurrentSource,
            node,
            ground,
            &[0.001 * (1 + index % 3) as f64],
        );

        if index % 2 == 0 {
            two_terminal(
                engine,
                world_id,
                ids,
                PrimitiveElementKind::Inductor,
                node,
                ground,
                &[5.0e-4 * (1 + index % 6) as f64],
            );
        }
    }

    for index in 0..BRANCHES {
        let next = (index + 1) % BRANCHES;
        two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Resistance,
            nodes[index],
            nodes[next],
            &[40.0 + (index % 11) as f64],
        );
    }

    for index in 0..CONTROLLED {
        let output = nodes[index * 3];
        let vccs = add_primitive(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::VoltageControlledCurrentSource,
            &[0.005],
        );
        attach_all(engine, world_id, vccs, &[output, ground, control, ground]);
    }

    for index in 0..CONTROLLED {
        let output = nodes[index * 3 + 1];
        let nonlinear = add_primitive(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::VoltageControlledConductance,
            &[2.5, 0.5, 0.001, 0.05],
        );
        attach_all(engine, world_id, nonlinear, &[output, ground, control]);
    }

    CircuitHandle {
        family: CircuitFamily::Complex,
        source,
        matrix_device: matrix_device.expect("complex fixture has a conductance"),
        observer_device: observer_device.expect("complex fixture has an observer device"),
        ground,
    }
}

pub(crate) struct CpuFixtureHandle {
    pub(crate) clock: DeviceId,
    pub(crate) device_count: usize,
    pub(crate) nonlinear_device_count: usize,
    pub(crate) stateful_device_count: usize,
}

pub(crate) fn build_cpu_workload(
    engine: &mut Engine,
    world_id: u32,
    ids: &mut IdAllocator,
    logic_nodes: usize,
) -> CpuFixtureHandle {
    assert!(
        logic_nodes >= 4,
        "CPU benchmark needs at least four logic nodes"
    );

    let before_devices = ids.allocated_devices();

    let ground = add_wire(engine, world_id, ids);
    let supply = add_wire(engine, world_id, ids);
    let clock_node = add_wire(engine, world_id, ids);
    let nodes = (0..logic_nodes)
        .map(|_| add_wire(engine, world_id, ids))
        .collect::<Vec<_>>();

    two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::VoltageSource,
        supply,
        ground,
        &[5.0],
    );

    let clock = two_terminal(
        engine,
        world_id,
        ids,
        PrimitiveElementKind::VoltageSource,
        clock_node,
        ground,
        &[0.5],
    );

    for (index, &node) in nodes.iter().enumerate() {
        // Pull-up and weak leakage keep every logic node numerically anchored.
        two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Resistance,
            supply,
            node,
            &[200.0 + (index % 8) as f64 * 5.0],
        );

        two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Conductance,
            node,
            ground,
            &[1.0e-5],
        );

        // Four-node logic slices are clock-seeded and then feed forward. This
        // resembles a banked CPU datapath without creating a deliberate
        // nonlinear oscillator that would make convergence the only workload.
        let slice_start = index - index % 4;
        let control = if index % 4 == 0 {
            clock_node
        } else {
            nodes[index - 1]
        };
        let slice_control = if index % 4 <= 1 {
            clock_node
        } else {
            nodes[slice_start + 1]
        };

        let switch = add_primitive(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::VoltageControlledSwitch,
            &[2.5, 0.05, 1.0e-6],
        );
        attach_all(engine, world_id, switch, &[node, ground, control, ground]);

        let nonlinear = add_primitive(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::VoltageControlledConductance,
            &[2.5, 0.40, 1.0e-6, 0.02],
        );
        attach_all(engine, world_id, nonlinear, &[node, ground, slice_control]);

        if index % 2 == 0 {
            two_terminal(
                engine,
                world_id,
                ids,
                PrimitiveElementKind::Capacitor,
                node,
                ground,
                &[1.0e-6 * (1 + index % 4) as f64],
            );
        }
    }

    // Weak bus/routing coupling keeps the workload a single interconnected
    // island while leaving the nonlinear logic slices as the dominant work.
    for index in 0..logic_nodes {
        let next = (index + 1) % logic_nodes;
        two_terminal(
            engine,
            world_id,
            ids,
            PrimitiveElementKind::Resistance,
            nodes[index],
            nodes[next],
            &[10_000.0 + (index % 16) as f64 * 100.0],
        );
    }

    let device_count = ids.allocated_devices() - before_devices;
    let nonlinear_device_count = logic_nodes * 2;
    let stateful_device_count = logic_nodes.div_ceil(2);
    let expected_devices = 2 + logic_nodes * 5 + logic_nodes.div_ceil(2);

    assert_eq!(
        device_count, expected_devices,
        "CPU benchmark device count drifted; update CpuWorkloadSize metadata",
    );

    CpuFixtureHandle {
        clock,
        device_count,
        nonlinear_device_count,
        stateful_device_count,
    }
}
