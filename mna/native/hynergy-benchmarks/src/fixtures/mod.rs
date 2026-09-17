pub mod circuits;
pub mod digital;
pub mod workloads;

use hynergy_engine::{Engine, EngineTickError, WorldCommand, WorldCommandApplyError};
use hynergy_model::device::definition::{DefinitionObserverId, DeviceId};
use hynergy_model::network::WireId;
use hynergy_model::parameter::ParameterId;
use std::time::{Duration, Instant};

pub use digital::{
    FULL_CPU_DEVICE_COUNT, FULL_CPU_NONLINEAR_DEVICE_COUNT, FULL_CPU_STATEFUL_DEVICE_COUNT,
    FullCpuScenario, ValidatedCpuScenario,
};
pub use workloads::{MixedProfile, SubscriptionProfile, TopologyMutation, TopologyScenario};

pub const SIMPLE_COUNTS: &[usize] = &[16, 256, 4096];
pub const MEDIUM_COUNTS: &[usize] = &[4, 32, 256];
pub const COMPLEX_COUNTS: &[usize] = &[1, 4, 16];

const CORE_SIMPLE_COUNTS: &[usize] = &[256];
const CORE_MEDIUM_COUNTS: &[usize] = &[32];
const CORE_COMPLEX_COUNTS: &[usize] = &[4];

const CORE_CPU_SIZES: &[CpuWorkloadSize] = &[CpuWorkloadSize::Small];
const FULL_CPU_SIZES: &[CpuWorkloadSize] = &[
    CpuWorkloadSize::Tiny,
    CpuWorkloadSize::Small,
    CpuWorkloadSize::Stress,
];
const CORE_PROTOCOL_SIZES: &[usize] = &[128];
const FULL_PROTOCOL_SIZES: &[usize] = &[8, 32, 128, 512];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchSuite {
    Core,
    Full,
}

impl BenchSuite {
    pub fn from_env() -> Self {
        let value = std::env::var("HYNERGY_BENCH_SUITE")
            .or_else(|_| std::env::var("SUITE"))
            .unwrap_or_else(|_| "core".to_owned());

        match value.as_str() {
            "core" => Self::Core,
            "full" => Self::Full,
            other => panic!("invalid benchmark suite {other:?}; expected core or full"),
        }
    }

    pub const fn is_full(self) -> bool {
        matches!(self, Self::Full)
    }

    pub fn counts(self, family: CircuitFamily) -> &'static [usize] {
        match (self, family) {
            (Self::Core, CircuitFamily::Simple) => CORE_SIMPLE_COUNTS,
            (Self::Core, CircuitFamily::Medium) => CORE_MEDIUM_COUNTS,
            (Self::Core, CircuitFamily::Complex) => CORE_COMPLEX_COUNTS,
            (Self::Full, CircuitFamily::Simple) => SIMPLE_COUNTS,
            (Self::Full, CircuitFamily::Medium) => MEDIUM_COUNTS,
            (Self::Full, CircuitFamily::Complex) => COMPLEX_COUNTS,
        }
    }

    pub const fn cpu_sizes(self) -> &'static [CpuWorkloadSize] {
        match self {
            Self::Core => CORE_CPU_SIZES,
            Self::Full => FULL_CPU_SIZES,
        }
    }

    pub const fn protocol_sizes(self) -> &'static [usize] {
        match self {
            Self::Core => CORE_PROTOCOL_SIZES,
            Self::Full => FULL_PROTOCOL_SIZES,
        }
    }

    pub const fn mixed_profiles(self) -> &'static [MixedProfile] {
        match self {
            Self::Core => &[MixedProfile::Representative],
            Self::Full => &MixedProfile::ALL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuWorkloadSize {
    Tiny,
    Small,
    Stress,
}

impl CpuWorkloadSize {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Small => "small",
            Self::Stress => "stress",
        }
    }

    pub const fn logic_nodes(self) -> usize {
        match self {
            Self::Tiny => 128,
            Self::Small => 512,
            Self::Stress => 2048,
        }
    }

    pub const fn device_count(self) -> usize {
        cpu_device_count(self.logic_nodes())
    }

    pub const fn nonlinear_device_count(self) -> usize {
        self.logic_nodes() * 2
    }

    pub const fn stateful_device_count(self) -> usize {
        self.logic_nodes() + self.logic_nodes().div_ceil(2)
    }

    pub const fn validated_lanes(self) -> usize {
        match self {
            Self::Tiny => 3,
            Self::Small => 12,
            Self::Stress => 48,
        }
    }

    pub const fn validated_device_count(self) -> usize {
        digital::validated_cpu_device_count(self.validated_lanes())
    }

    pub const fn validated_nonlinear_device_count(self) -> usize {
        digital::validated_cpu_nonlinear_device_count(self.validated_lanes())
    }

    pub const fn validated_stateful_device_count(self) -> usize {
        digital::validated_cpu_stateful_device_count(self.validated_lanes())
    }
}

const fn cpu_device_count(logic_nodes: usize) -> usize {
    2 + logic_nodes * 5 + logic_nodes.div_ceil(2)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitFamily {
    Simple,
    Medium,
    Complex,
}

impl CircuitFamily {
    pub const ALL: [Self; 3] = [Self::Simple, Self::Medium, Self::Complex];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Medium => "medium",
            Self::Complex => "complex",
        }
    }

    pub const fn is_stateful(self) -> bool {
        !matches!(self, Self::Simple)
    }

    pub const fn devices_per_circuit(self) -> usize {
        match self {
            Self::Simple => 7,
            Self::Medium => 36,
            Self::Complex => 146,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScenarioMetadata {
    circuits: usize,
    devices: usize,
    simple: usize,
    medium: usize,
    complex: usize,
    subscriptions: usize,
}

impl ScenarioMetadata {
    pub const fn circuits(self) -> usize {
        self.circuits
    }

    pub const fn devices(self) -> usize {
        self.devices
    }

    pub const fn subscriptions(self) -> usize {
        self.subscriptions
    }

    pub const fn family_count(self, family: CircuitFamily) -> usize {
        match family {
            CircuitFamily::Simple => self.simple,
            CircuitFamily::Medium => self.medium,
            CircuitFamily::Complex => self.complex,
        }
    }

    fn record_circuit(&mut self, family: CircuitFamily, devices: usize) {
        self.circuits += 1;
        self.devices += devices;

        match family {
            CircuitFamily::Simple => self.simple += 1,
            CircuitFamily::Medium => self.medium += 1,
            CircuitFamily::Complex => self.complex += 1,
        }
    }

    fn record_subscription(&mut self) {
        self.subscriptions += 1;
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CircuitHandle {
    pub(crate) family: CircuitFamily,
    pub(crate) source: DeviceId,
    pub(crate) matrix_device: DeviceId,
    pub(crate) observer_device: DeviceId,
    pub(crate) ground: WireId,
}

pub struct CpuScenario {
    engine: Engine,
    world_id: u32,
    clock: DeviceId,
    logic_nodes: usize,
    device_count: usize,
    nonlinear_device_count: usize,
    stateful_device_count: usize,
    clock_high: bool,
}

impl CpuScenario {
    pub fn new(size: CpuWorkloadSize) -> Self {
        workloads::build_cpu(size.logic_nodes())
    }

    pub fn for_test(logic_nodes: usize) -> Self {
        workloads::build_cpu(logic_nodes)
    }

    #[inline]
    pub const fn logic_nodes(&self) -> usize {
        self.logic_nodes
    }

    #[inline]
    pub const fn device_count(&self) -> usize {
        self.device_count
    }

    #[inline]
    pub const fn nonlinear_device_count(&self) -> usize {
        self.nonlinear_device_count
    }

    #[inline]
    pub const fn stateful_device_count(&self) -> usize {
        self.stateful_device_count
    }

    #[inline]
    pub fn tick(&mut self) -> Result<(), EngineTickError> {
        self.engine.tick_world(self.world_id)
    }

    pub fn warm(&mut self, ticks: usize) -> Result<(), EngineTickError> {
        for _ in 0..ticks {
            self.tick()?;
        }

        Ok(())
    }

    pub fn toggle_clock(&mut self) -> Result<(), WorldCommandApplyError> {
        self.clock_high = !self.clock_high;
        let value = if self.clock_high { 4.5 } else { 0.5 };

        self.engine.apply_world_command(
            self.world_id,
            WorldCommand::SetDeviceParameter {
                device: self.clock,
                parameter: ParameterId::new(0),
                value,
            },
        )
    }

    pub fn measure_switching_ticks(&mut self, iterations: u64) -> Duration {
        let mut total = Duration::ZERO;

        for _ in 0..iterations {
            self.toggle_clock().unwrap();

            let start = Instant::now();
            self.tick().unwrap();
            total += start.elapsed();
        }

        total
    }
}

pub struct WorldScenario {
    pub(crate) engine: Engine,
    pub(crate) world_id: u32,
    pub(crate) ids: IdAllocator,
    pub(crate) circuits: Vec<CircuitHandle>,
    pub(crate) metadata: ScenarioMetadata,
    rhs_high: bool,
    matrix_high: bool,
}

impl WorldScenario {
    pub fn uniform(family: CircuitFamily, count: usize) -> Self {
        workloads::build_uniform(family, count)
    }
    pub fn mixed(profile: MixedProfile) -> Self {
        workloads::build_mixed(profile)
    }

    #[inline]
    pub const fn metadata(&self) -> ScenarioMetadata {
        self.metadata
    }

    #[inline]
    pub fn tick(&mut self) -> Result<(), EngineTickError> {
        self.engine.tick_world(self.world_id)
    }
    pub fn warm(&mut self, ticks: usize) -> Result<(), EngineTickError> {
        for _ in 0..ticks {
            self.tick()?;
        }

        Ok(())
    }

    pub fn toggle_rhs(&mut self) -> Result<(), WorldCommandApplyError> {
        self.rhs_high = !self.rhs_high;
        let value = if self.rhs_high { 5.25 } else { 5.0 };

        for circuit in &self.circuits {
            self.engine.apply_world_command(
                self.world_id,
                WorldCommand::SetDeviceParameter {
                    device: circuit.source,
                    parameter: ParameterId::new(0),
                    value,
                },
            )?;
        }

        Ok(())
    }

    pub fn toggle_matrix(&mut self) -> Result<(), WorldCommandApplyError> {
        self.matrix_high = !self.matrix_high;
        let value = if self.matrix_high { 0.125 } else { 0.1 };

        for circuit in &self.circuits {
            self.engine.apply_world_command(
                self.world_id,
                WorldCommand::SetDeviceParameter {
                    device: circuit.matrix_device,
                    parameter: ParameterId::new(0),
                    value,
                },
            )?;
        }

        Ok(())
    }

    pub fn subscribe_every(&mut self, stride: usize) {
        assert!(stride > 0);

        for (index, circuit) in self.circuits.iter().enumerate() {
            if index % stride != 0 {
                continue;
            }

            self.engine
                .subscribe_observer(
                    self.world_id,
                    circuit.observer_device,
                    DefinitionObserverId::new(0),
                )
                .unwrap();

            self.metadata.record_subscription();
        }
    }

    pub fn subscription_count(&self) -> usize {
        self.engine.subscription_count(self.world_id).unwrap()
    }

    pub fn subscription_update_count(&self) -> usize {
        self.engine
            .subscription_updates(self.world_id)
            .unwrap()
            .len()
    }
    pub fn measure_rhs_dirty_ticks(&mut self, iterations: u64) -> Duration {
        let mut total = Duration::ZERO;

        for _ in 0..iterations {
            self.toggle_rhs().unwrap();

            let start = Instant::now();
            self.tick().unwrap();
            total += start.elapsed();
        }

        total
    }
    pub fn measure_matrix_dirty_ticks(&mut self, iterations: u64) -> Duration {
        let mut total = Duration::ZERO;

        for _ in 0..iterations {
            self.toggle_matrix().unwrap();

            let start = Instant::now();
            self.tick().unwrap();
            total += start.elapsed();
        }

        total
    }
}

#[derive(Debug, Default)]
pub(crate) struct IdAllocator {
    next_wire: u32,
    next_device: u32,
}

impl IdAllocator {
    pub(crate) fn new() -> Self {
        Self {
            next_wire: 1,
            next_device: 1,
        }
    }

    pub(crate) fn wire(&mut self) -> WireId {
        let raw = self.next_wire;
        self.next_wire = self
            .next_wire
            .checked_add(1)
            .expect("benchmark wire ID exhausted");
        WireId::try_from(raw).expect("benchmark wire IDs start at one")
    }

    pub(crate) fn device(&mut self) -> DeviceId {
        let raw = self.next_device;
        self.next_device = self
            .next_device
            .checked_add(1)
            .expect("benchmark device ID exhausted");
        DeviceId::try_from(raw).expect("benchmark device IDs start at one")
    }

    pub(crate) fn allocated_devices(&self) -> usize {
        usize::try_from(self.next_device - 1).expect("benchmark device count must fit usize")
    }
}
