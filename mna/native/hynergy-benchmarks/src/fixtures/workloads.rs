use super::{CircuitFamily, CpuScenario, IdAllocator, ScenarioMetadata, WorldScenario, circuits};
use hynergy_engine::{Engine, EngineConfig, EngineTickError, WorldCommand, WorldConfig};
use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixedProfile {
    Representative,
    Typical,
    Active,
    Stress,
}
impl MixedProfile {
    pub const ALL: [Self; 3] = [Self::Typical, Self::Active, Self::Stress];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Representative => "representative",
            Self::Typical => "typical",
            Self::Active => "active",
            Self::Stress => "stress",
        }
    }

    pub const fn counts(self) -> (usize, usize, usize) {
        match self {
            Self::Representative => (128, 16, 1),
            Self::Typical => (1024, 64, 4),
            Self::Active => (256, 128, 8),
            Self::Stress => (1024, 256, 16),
        }
    }

    pub const fn has_subscriptions(self) -> bool {
        matches!(self, Self::Stress)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionProfile {
    Sparse,
    DenseSleeping,
    DenseChanging,
}
impl SubscriptionProfile {
    pub const ALL: [Self; 3] = [Self::Sparse, Self::DenseSleeping, Self::DenseChanging];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Sparse => "sparse",
            Self::DenseSleeping => "dense_sleeping",
            Self::DenseChanging => "dense_changing",
        }
    }

    pub const fn changes_each_tick(self) -> bool {
        matches!(self, Self::DenseChanging)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologyMutation {
    Merge,
    Split,
}
impl TopologyMutation {
    pub const ALL: [Self; 2] = [Self::Merge, Self::Split];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Merge => "merge_then_tick",
            Self::Split => "split_then_tick",
        }
    }
}

pub struct TopologyScenario {
    scenario: WorldScenario,
    mutation: TopologyMutation,
    wire_a: hynergy_model::network::WireId,
    wire_b: hynergy_model::network::WireId,
}

impl TopologyScenario {
    pub fn new(mutation: TopologyMutation) -> Self {
        let mut scenario = new_world();
        add_family(&mut scenario, CircuitFamily::Simple, 2);

        let wire_a = scenario.circuits[0].ground;
        let wire_b = scenario.circuits[1].ground;

        if mutation == TopologyMutation::Split {
            scenario
                .engine
                .apply_world_command(
                    scenario.world_id,
                    WorldCommand::ConnectWires { wire_a, wire_b },
                )
                .unwrap();
        }

        scenario.tick().unwrap();

        Self {
            scenario,
            mutation,
            wire_a,
            wire_b,
        }
    }

    pub fn run_once(&mut self) -> Result<(), EngineTickError> {
        let command = match self.mutation {
            TopologyMutation::Merge => WorldCommand::ConnectWires {
                wire_a: self.wire_a,
                wire_b: self.wire_b,
            },
            TopologyMutation::Split => WorldCommand::DisconnectWires {
                wire_a: self.wire_a,
                wire_b: self.wire_b,
            },
        };

        self.scenario
            .engine
            .apply_world_command(self.scenario.world_id, command)
            .expect("topology benchmark mutation must be valid");

        self.scenario.tick()
    }

    pub const fn circuits(&self) -> usize {
        self.scenario.metadata.circuits()
    }
}

pub(crate) fn build_cpu(logic_nodes: usize) -> CpuScenario {
    let mut engine = Engine::new(EngineConfig::new(1));
    let world_id = engine
        .new_world(WorldConfig::new(NonZeroU32::new(30).unwrap()))
        .unwrap();
    let mut ids = IdAllocator::new();

    let fixture = circuits::build_cpu_workload(&mut engine, world_id, &mut ids, logic_nodes);

    CpuScenario {
        engine,
        world_id,
        clock: fixture.clock,
        logic_nodes,
        device_count: fixture.device_count,
        nonlinear_device_count: fixture.nonlinear_device_count,
        stateful_device_count: fixture.stateful_device_count,
        clock_high: false,
    }
}

pub(crate) fn build_uniform(family: CircuitFamily, count: usize) -> WorldScenario {
    let mut scenario = new_world();
    add_family(&mut scenario, family, count);
    scenario
}

pub(crate) fn build_mixed(profile: MixedProfile) -> WorldScenario {
    let (simple, medium, complex) = profile.counts();
    build_mixed_with_counts(simple, medium, complex, profile.has_subscriptions())
}

pub fn build_mixed_with_counts(
    simple: usize,
    medium: usize,
    complex: usize,
    subscribe_subset: bool,
) -> WorldScenario {
    let mut scenario = new_world();
    add_family(&mut scenario, CircuitFamily::Simple, simple);
    add_family(&mut scenario, CircuitFamily::Medium, medium);
    add_family(&mut scenario, CircuitFamily::Complex, complex);

    if subscribe_subset && !scenario.circuits.is_empty() {
        scenario.subscribe_every(16);
    }

    scenario
}

pub fn build_subscription_case(profile: SubscriptionProfile, count: usize) -> WorldScenario {
    assert!(count > 0);

    let mut scenario = build_uniform(CircuitFamily::Simple, count);

    match profile {
        SubscriptionProfile::Sparse => scenario.subscribe_every(16.min(count)),
        SubscriptionProfile::DenseSleeping | SubscriptionProfile::DenseChanging => {
            scenario.subscribe_every(1)
        }
    }

    scenario
}

fn new_world() -> WorldScenario {
    let mut engine = Engine::new(EngineConfig::new(1));
    let world_id = engine
        .new_world(WorldConfig::new(NonZeroU32::new(30).unwrap()))
        .unwrap();

    WorldScenario {
        engine,
        world_id,
        ids: IdAllocator::new(),
        circuits: Vec::new(),
        metadata: ScenarioMetadata::default(),
        rhs_high: false,
        matrix_high: false,
    }
}

fn add_family(scenario: &mut WorldScenario, family: CircuitFamily, count: usize) {
    scenario.circuits.reserve(count);

    for _ in 0..count {
        let before_devices = scenario.ids.allocated_devices();

        let circuit = circuits::build_circuit(
            &mut scenario.engine,
            scenario.world_id,
            &mut scenario.ids,
            family,
        );

        let devices = scenario.ids.allocated_devices() - before_devices;

        assert_eq!(
            devices,
            family.devices_per_circuit(),
            "benchmark circuit device count drifted; update the family metadata",
        );
        debug_assert_eq!(circuit.family, family);

        scenario.metadata.record_circuit(family, devices);
        scenario.circuits.push(circuit);
    }
}
