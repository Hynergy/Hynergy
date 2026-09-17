use super::{CpuWorkloadSize, IdAllocator};
use hynergy_engine::{
    Engine, EngineConfig, EngineTickError, SubscriptionId, WorldCommand, WorldCommandApplyError,
    WorldConfig,
};
use hynergy_model::device::definition::{DeviceId, PrimitiveElementKind, TerminalId};
use hynergy_model::network::WireId;
use hynergy_model::parameter::ParameterId;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

const LOGIC_LOW_VOLTAGE: f64 = 0.5;
const LOGIC_HIGH_VOLTAGE: f64 = 4.5;
const SUPPLY_VOLTAGE: f64 = 5.0;
const LOGIC_LOW_MAX: f64 = 1.0;
const LOGIC_HIGH_MIN: f64 = 4.0;

const NAND_PULL_UP_OHMS: f64 = 100.0;
const SWITCH_THRESHOLD: f64 = 2.5;
const SWITCH_HYSTERESIS: f64 = 0.20;
const SWITCH_G_MAX: f64 = 1.0;
const SWITCH_G_MIN: f64 = 1.0e-9;

const BITS_PER_LANE: usize = 8;
const NANDS_PER_FULL_ADDER: usize = 9;
const SWITCHES_PER_NAND: usize = 2;
const INPUT_SOURCES_PER_LANE: usize = BITS_PER_LANE * 2 + 1;
const REGISTER_DELAYS_PER_LANE: usize = BITS_PER_LANE + 1;
const DEVICES_PER_NAND: usize = 3;
const DEVICES_PER_LANE: usize = INPUT_SOURCES_PER_LANE
    + BITS_PER_LANE * NANDS_PER_FULL_ADDER * DEVICES_PER_NAND
    + REGISTER_DELAYS_PER_LANE;
const NONLINEAR_DEVICES_PER_LANE: usize = BITS_PER_LANE * NANDS_PER_FULL_ADDER * SWITCHES_PER_NAND;
const STATEFUL_DEVICES_PER_LANE: usize = NONLINEAR_DEVICES_PER_LANE + REGISTER_DELAYS_PER_LANE;

#[derive(Debug, Clone, Copy)]
struct InputPin {
    wire: WireId,
    source: DeviceId,
}

#[derive(Debug, Clone, Copy)]
struct GateOutput {
    wire: WireId,
    pull_up: DeviceId,
}

#[derive(Debug, Clone, Copy)]
struct RegisterBit {
    input: WireId,
    output: WireId,
    delay: DeviceId,
}

#[derive(Debug, Clone, Copy)]
enum ProbeTransform {
    Direct,
    SupplyMinus,
}

impl ProbeTransform {
    #[inline]
    fn apply(self, value: f64) -> f64 {
        match self {
            Self::Direct => value,
            Self::SupplyMinus => SUPPLY_VOLTAGE - value,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Probe {
    subscription: SubscriptionId,
    transform: ProbeTransform,
    value: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProbeId(usize);

struct DigitalHarness {
    engine: Engine,
    world_id: u32,
    ids: IdAllocator,
    ground: WireId,
    supply: WireId,
    probes: Vec<Probe>,
}

impl DigitalHarness {
    fn new() -> Self {
        let mut engine = Engine::new(EngineConfig::new(1));
        let world_id = engine
            .new_world(WorldConfig::new(NonZeroU32::new(30).unwrap()))
            .unwrap();
        let mut ids = IdAllocator::new();

        let ground = add_wire(&mut engine, world_id, &mut ids);
        let supply = add_wire(&mut engine, world_id, &mut ids);

        two_terminal(
            &mut engine,
            world_id,
            &mut ids,
            PrimitiveElementKind::VoltageSource,
            supply,
            ground,
            &[SUPPLY_VOLTAGE],
        );

        Self {
            engine,
            world_id,
            ids,
            ground,
            supply,
            probes: Vec::new(),
        }
    }

    fn input(&mut self, high: bool) -> InputPin {
        let wire = add_wire(&mut self.engine, self.world_id, &mut self.ids);
        let source = two_terminal(
            &mut self.engine,
            self.world_id,
            &mut self.ids,
            PrimitiveElementKind::VoltageSource,
            wire,
            self.ground,
            &[logic_voltage(high)],
        );

        InputPin { wire, source }
    }

    fn set_input(&mut self, pin: InputPin, high: bool) -> Result<(), WorldCommandApplyError> {
        self.engine.apply_world_command(
            self.world_id,
            WorldCommand::SetDeviceParameter {
                device: pin.source,
                parameter: ParameterId::new(0),
                value: logic_voltage(high),
            },
        )
    }

    fn nand(&mut self, a: WireId, b: WireId) -> GateOutput {
        let output = add_wire(&mut self.engine, self.world_id, &mut self.ids);
        self.nand_into(output, a, b)
    }

    fn nand_into(&mut self, output: WireId, a: WireId, b: WireId) -> GateOutput {
        let middle = add_wire(&mut self.engine, self.world_id, &mut self.ids);

        let pull_up = two_terminal(
            &mut self.engine,
            self.world_id,
            &mut self.ids,
            PrimitiveElementKind::Resistance,
            self.supply,
            output,
            &[NAND_PULL_UP_OHMS],
        );

        let upper = add_primitive(
            &mut self.engine,
            self.world_id,
            &mut self.ids,
            PrimitiveElementKind::VoltageControlledSwitch,
            &[
                SWITCH_THRESHOLD,
                SWITCH_HYSTERESIS,
                SWITCH_G_MAX,
                SWITCH_G_MIN,
            ],
        );
        attach_all(
            &mut self.engine,
            self.world_id,
            upper,
            &[output, middle, a, self.ground],
        );

        let lower = add_primitive(
            &mut self.engine,
            self.world_id,
            &mut self.ids,
            PrimitiveElementKind::VoltageControlledSwitch,
            &[
                SWITCH_THRESHOLD,
                SWITCH_HYSTERESIS,
                SWITCH_G_MAX,
                SWITCH_G_MIN,
            ],
        );
        attach_all(
            &mut self.engine,
            self.world_id,
            lower,
            &[middle, self.ground, b, self.ground],
        );

        GateOutput {
            wire: output,
            pull_up,
        }
    }

    fn inverter(&mut self, input: WireId) -> GateOutput {
        self.nand(input, input)
    }

    fn inverter_into(&mut self, output: WireId, input: WireId) -> GateOutput {
        self.nand_into(output, input, input)
    }

    fn and_gate(&mut self, a: WireId, b: WireId) -> GateOutput {
        let nand = self.nand(a, b);
        self.inverter(nand.wire)
    }

    fn or_gate(&mut self, a: WireId, b: WireId) -> GateOutput {
        let not_a = self.inverter(a);
        let not_b = self.inverter(b);
        self.nand(not_a.wire, not_b.wire)
    }

    fn xor_gate(&mut self, a: WireId, b: WireId) -> GateOutput {
        let n1 = self.nand(a, b);
        let n2 = self.nand(a, n1.wire);
        let n3 = self.nand(b, n1.wire);
        self.nand(n2.wire, n3.wire)
    }

    fn nor(&mut self, a: WireId, b: WireId) -> GateOutput {
        let or = self.or_gate(a, b);
        self.inverter(or.wire)
    }

    fn mux(&mut self, a: WireId, b: WireId, select: WireId) -> GateOutput {
        let output = add_wire(&mut self.engine, self.world_id, &mut self.ids);
        self.mux_into(output, a, b, select)
    }

    fn mux_into(&mut self, output: WireId, a: WireId, b: WireId, select: WireId) -> GateOutput {
        let not_select = self.inverter(select);
        let not_a_selected = self.nand(a, not_select.wire);
        let not_b_selected = self.nand(b, select);
        self.nand_into(output, not_a_selected.wire, not_b_selected.wire)
    }

    fn full_adder(&mut self, a: WireId, b: WireId, carry_in: WireId) -> (GateOutput, GateOutput) {
        let n1 = self.nand(a, b);
        let n2 = self.nand(a, n1.wire);
        let n3 = self.nand(b, n1.wire);
        let xor_ab = self.nand(n2.wire, n3.wire);

        let n4 = self.nand(xor_ab.wire, carry_in);
        let n5 = self.nand(xor_ab.wire, n4.wire);
        let n6 = self.nand(carry_in, n4.wire);
        let sum = self.nand(n5.wire, n6.wire);

        // n1 = !(A & B), n4 = !(Cin & (A xor B)). NANDing those
        // complements gives (A & B) | (Cin & (A xor B)).
        let carry_out = self.nand(n1.wire, n4.wire);

        (sum, carry_out)
    }

    fn register_bit(&mut self, initial_high: bool) -> RegisterBit {
        let input = add_wire(&mut self.engine, self.world_id, &mut self.ids);
        let output = add_wire(&mut self.engine, self.world_id, &mut self.ids);
        let delay = add_primitive(
            &mut self.engine,
            self.world_id,
            &mut self.ids,
            PrimitiveElementKind::TickDelay,
            &[logic_voltage(initial_high)],
        );

        attach_all(
            &mut self.engine,
            self.world_id,
            delay,
            &[input, self.ground, output, self.ground],
        );

        RegisterBit {
            input,
            output,
            delay,
        }
    }

    fn tick_delay(&mut self, input: WireId, initial_high: bool) -> (WireId, DeviceId) {
        let output = add_wire(&mut self.engine, self.world_id, &mut self.ids);
        let delay = add_primitive(
            &mut self.engine,
            self.world_id,
            &mut self.ids,
            PrimitiveElementKind::TickDelay,
            &[logic_voltage(initial_high)],
        );

        attach_all(
            &mut self.engine,
            self.world_id,
            delay,
            &[input, self.ground, output, self.ground],
        );

        (output, delay)
    }

    fn observe_gate(&mut self, output: GateOutput) -> ProbeId {
        self.observe(output.pull_up, 0, ProbeTransform::SupplyMinus)
    }

    fn observe_delay(&mut self, delay: DeviceId) -> ProbeId {
        self.observe(delay, 1, ProbeTransform::Direct)
    }

    fn observe(&mut self, device: DeviceId, observer: u32, transform: ProbeTransform) -> ProbeId {
        let subscription = self
            .engine
            .subscribe_observer(
                self.world_id,
                device,
                hynergy_model::device::definition::DefinitionObserverId::new(observer),
            )
            .unwrap();

        let id = ProbeId(self.probes.len());
        self.probes.push(Probe {
            subscription,
            transform,
            value: None,
        });
        id
    }

    fn tick(&mut self) -> Result<(), EngineTickError> {
        self.engine.tick_world(self.world_id)?;

        for update in self.engine.subscription_updates(self.world_id).unwrap() {
            if let Some(probe) = self
                .probes
                .iter_mut()
                .find(|probe| probe.subscription == update.subscription())
            {
                probe.value = Some(probe.transform.apply(update.value()));
            }
        }

        Ok(())
    }

    fn voltage(&self, probe: ProbeId) -> f64 {
        self.probes[probe.0]
            .value
            .expect("observed digital signal must have a value after a successful tick")
    }
}

#[inline]
const fn logic_voltage(high: bool) -> f64 {
    if high {
        LOGIC_HIGH_VOLTAGE
    } else {
        LOGIC_LOW_VOLTAGE
    }
}

pub fn logic_level(voltage: f64) -> Option<bool> {
    if voltage <= LOGIC_LOW_MAX {
        Some(false)
    } else if voltage >= LOGIC_HIGH_MIN {
        Some(true)
    } else {
        None
    }
}

pub fn inverter_truth_table() -> Vec<(bool, f64)> {
    let mut harness = DigitalHarness::new();
    let input = harness.input(false);
    let output = harness.inverter(input.wire);
    let probe = harness.observe_gate(output);

    [false, true]
        .into_iter()
        .map(|value| {
            harness.set_input(input, value).unwrap();
            harness.tick().unwrap();
            (value, harness.voltage(probe))
        })
        .collect()
}

pub fn nand_truth_table() -> Vec<((bool, bool), f64)> {
    let mut harness = DigitalHarness::new();
    let a = harness.input(false);
    let b = harness.input(false);
    let output = harness.nand(a.wire, b.wire);
    let probe = harness.observe_gate(output);

    let mut rows = Vec::with_capacity(4);
    for av in [false, true] {
        for bv in [false, true] {
            harness.set_input(a, av).unwrap();
            harness.set_input(b, bv).unwrap();
            harness.tick().unwrap();
            rows.push(((av, bv), harness.voltage(probe)));
        }
    }
    rows
}

pub fn nor_truth_table() -> Vec<((bool, bool), f64)> {
    let mut harness = DigitalHarness::new();
    let a = harness.input(false);
    let b = harness.input(false);
    let output = harness.nor(a.wire, b.wire);
    let probe = harness.observe_gate(output);

    let mut rows = Vec::with_capacity(4);
    for av in [false, true] {
        for bv in [false, true] {
            harness.set_input(a, av).unwrap();
            harness.set_input(b, bv).unwrap();
            harness.tick().unwrap();
            rows.push(((av, bv), harness.voltage(probe)));
        }
    }
    rows
}

pub fn mux_truth_table() -> Vec<((bool, bool, bool), f64)> {
    let mut harness = DigitalHarness::new();
    let a = harness.input(false);
    let b = harness.input(false);
    let select = harness.input(false);
    let output = harness.mux(a.wire, b.wire, select.wire);
    let probe = harness.observe_gate(output);

    let mut rows = Vec::with_capacity(8);
    for av in [false, true] {
        for bv in [false, true] {
            for sv in [false, true] {
                harness.set_input(a, av).unwrap();
                harness.set_input(b, bv).unwrap();
                harness.set_input(select, sv).unwrap();
                harness.tick().unwrap();
                rows.push(((av, bv, sv), harness.voltage(probe)));
            }
        }
    }
    rows
}

pub fn full_adder_truth_table() -> Vec<((bool, bool, bool), f64, f64)> {
    let mut harness = DigitalHarness::new();
    let a = harness.input(false);
    let b = harness.input(false);
    let carry_in = harness.input(false);
    let (sum, carry_out) = harness.full_adder(a.wire, b.wire, carry_in.wire);
    let sum_probe = harness.observe_gate(sum);
    let carry_probe = harness.observe_gate(carry_out);

    let mut rows = Vec::with_capacity(8);
    for av in [false, true] {
        for bv in [false, true] {
            for cv in [false, true] {
                harness.set_input(a, av).unwrap();
                harness.set_input(b, bv).unwrap();
                harness.set_input(carry_in, cv).unwrap();
                harness.tick().unwrap();
                rows.push((
                    (av, bv, cv),
                    harness.voltage(sum_probe),
                    harness.voltage(carry_probe),
                ));
            }
        }
    }
    rows
}

pub fn tick_delay_trace() -> Vec<f64> {
    let mut harness = DigitalHarness::new();
    let input = harness.input(false);
    let (_, delay) = harness.tick_delay(input.wire, false);
    let probe = harness.observe_delay(delay);

    harness.tick().unwrap();
    let initial = harness.voltage(probe);

    harness.set_input(input, true).unwrap();
    harness.tick().unwrap();
    let before_high = harness.voltage(probe);

    harness.tick().unwrap();
    let high = harness.voltage(probe);

    harness.set_input(input, false).unwrap();
    harness.tick().unwrap();
    harness.tick().unwrap();
    let low_again = harness.voltage(probe);

    vec![initial, before_high, high, low_again]
}

#[derive(Debug)]
struct LaneInputs {
    a: [DeviceId; BITS_PER_LANE],
    b: [DeviceId; BITS_PER_LANE],
    carry_in: DeviceId,
    current_a: u8,
    current_b: u8,
    current_carry: bool,
}

#[derive(Debug)]
struct LaneProbes {
    result: [ProbeId; BITS_PER_LANE],
    carry: ProbeId,
}

pub struct ValidatedCpuScenario {
    harness: DigitalHarness,
    lanes: Vec<LaneInputs>,
    lane_probes: Vec<LaneProbes>,
    device_count: usize,
    nonlinear_device_count: usize,
    stateful_device_count: usize,
    pattern_step: u64,
}

impl ValidatedCpuScenario {
    pub fn new(size: CpuWorkloadSize) -> Self {
        Self::build(size.validated_lanes(), false)
    }

    pub fn for_test(lanes: usize) -> Self {
        Self::build(lanes, true)
    }

    fn build(lane_count: usize, observe_outputs: bool) -> Self {
        assert!(lane_count > 0);

        let mut harness = DigitalHarness::new();
        let mut lanes = Vec::with_capacity(lane_count);
        let mut lane_probes = Vec::with_capacity(if observe_outputs { lane_count } else { 0 });

        for lane_index in 0..lane_count {
            let initial_a = (lane_index as u8).wrapping_mul(37).wrapping_add(11);
            let initial_b = (lane_index as u8).wrapping_mul(19).wrapping_add(7);
            let initial_carry = lane_index % 2 != 0;

            let mut a_pins = Vec::with_capacity(BITS_PER_LANE);
            let mut b_pins = Vec::with_capacity(BITS_PER_LANE);

            for bit in 0..BITS_PER_LANE {
                a_pins.push(harness.input(((initial_a >> bit) & 1) != 0));
                b_pins.push(harness.input(((initial_b >> bit) & 1) != 0));
            }
            let carry_in = harness.input(initial_carry);

            let mut carry_wire = carry_in.wire;
            let mut result_delays = Vec::with_capacity(BITS_PER_LANE);

            for bit in 0..BITS_PER_LANE {
                let (sum, carry_out) =
                    harness.full_adder(a_pins[bit].wire, b_pins[bit].wire, carry_wire);
                let (_, delay) = harness.tick_delay(sum.wire, false);
                result_delays.push(delay);
                carry_wire = carry_out.wire;
            }

            let (_, carry_delay) = harness.tick_delay(carry_wire, false);

            if observe_outputs {
                let result = result_delays
                    .iter()
                    .copied()
                    .map(|delay| harness.observe_delay(delay))
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("8-bit lane must create exactly eight result probes");

                lane_probes.push(LaneProbes {
                    result,
                    carry: harness.observe_delay(carry_delay),
                });
            }

            lanes.push(LaneInputs {
                a: a_pins
                    .iter()
                    .map(|pin| pin.source)
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("8-bit lane must create exactly eight A sources"),
                b: b_pins
                    .iter()
                    .map(|pin| pin.source)
                    .collect::<Vec<_>>()
                    .try_into()
                    .expect("8-bit lane must create exactly eight B sources"),
                carry_in: carry_in.source,
                current_a: initial_a,
                current_b: initial_b,
                current_carry: initial_carry,
            });
        }

        let device_count = harness.ids.allocated_devices();
        let expected_device_count = validated_cpu_device_count(lane_count);
        assert_eq!(
            device_count, expected_device_count,
            "validated CPU device count drifted; update workload metadata",
        );

        Self {
            harness,
            lanes,
            lane_probes,
            device_count,
            nonlinear_device_count: lane_count * NONLINEAR_DEVICES_PER_LANE,
            stateful_device_count: lane_count * STATEFUL_DEVICES_PER_LANE,
            pattern_step: 0,
        }
    }

    #[inline]
    pub fn lane_count(&self) -> usize {
        self.lanes.len()
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
        self.harness.tick()
    }

    pub fn warm(&mut self, ticks: usize) -> Result<(), EngineTickError> {
        for _ in 0..ticks {
            self.tick()?;
        }
        Ok(())
    }

    pub fn set_lane_operands(
        &mut self,
        lane: usize,
        a: u8,
        b: u8,
        carry_in: bool,
    ) -> Result<(), WorldCommandApplyError> {
        let lane = self
            .lanes
            .get_mut(lane)
            .expect("validated CPU lane index must be in range");

        if lane.current_a != a {
            set_byte_sources(
                &mut self.harness.engine,
                self.harness.world_id,
                &lane.a,
                lane.current_a,
                a,
            )?;
            lane.current_a = a;
        }

        if lane.current_b != b {
            set_byte_sources(
                &mut self.harness.engine,
                self.harness.world_id,
                &lane.b,
                lane.current_b,
                b,
            )?;
            lane.current_b = b;
        }

        if lane.current_carry != carry_in {
            set_logic_source(
                &mut self.harness.engine,
                self.harness.world_id,
                lane.carry_in,
                carry_in,
            )?;
            lane.current_carry = carry_in;
        }

        Ok(())
    }

    pub fn registered_result(&self, lane: usize) -> Option<(u8, bool)> {
        let probes = self.lane_probes.get(lane)?;
        let mut result = 0u8;

        for (bit, &probe) in probes.result.iter().enumerate() {
            if logic_level(self.harness.voltage(probe))? {
                result |= 1 << bit;
            }
        }

        let carry = logic_level(self.harness.voltage(probes.carry))?;
        Some((result, carry))
    }

    fn apply_next_pattern(&mut self) -> Result<(), WorldCommandApplyError> {
        self.pattern_step = self.pattern_step.wrapping_add(1);
        let step = self.pattern_step;

        for lane_index in 0..self.lanes.len() {
            let lane_seed = lane_index as u64;
            let a = step
                .wrapping_mul(73)
                .wrapping_add(lane_seed.wrapping_mul(37)) as u8;
            let b = step
                .rotate_left(7)
                .wrapping_mul(29)
                .wrapping_add(lane_seed.wrapping_mul(53)) as u8;
            let carry = ((step ^ lane_seed) & 1) != 0;

            self.set_lane_operands(lane_index, a, b, carry)?;
        }

        Ok(())
    }

    pub fn measure_switching_ticks(&mut self, iterations: u64) -> Duration {
        let mut total = Duration::ZERO;

        for _ in 0..iterations {
            self.apply_next_pattern().unwrap();

            let start = Instant::now();
            self.tick().unwrap();
            total += start.elapsed();
        }

        total
    }
}

const FULL_CPU_PC_BITS: usize = 4;
const FULL_CPU_REGISTER_BITS: usize = 8;
const FULL_CPU_OPCODE_BITS: usize = 3;
const FULL_CPU_INSTRUCTION_BITS: usize = FULL_CPU_OPCODE_BITS + FULL_CPU_REGISTER_BITS;
const FULL_CPU_TICK_DELAYS: usize =
    FULL_CPU_PC_BITS + FULL_CPU_REGISTER_BITS * 2 + 1 + FULL_CPU_INSTRUCTION_BITS + 1;
const FULL_CPU_ROM_OR_GATES: usize = 40;
const FULL_CPU_NAND_GATES: usize = 1 // phase inverter
    + 4 + 16 * 6 + FULL_CPU_ROM_OR_GATES * 3 // 16x11 ROM
    + FULL_CPU_INSTRUCTION_BITS * 4 // IR hold/fetch muxes
    + 3 + 8 * 4 // opcode decoder
    + FULL_CPU_REGISTER_BITS * (9 + 4 + 2) // ADD, XOR, AND datapaths
    + 5 * 2 // phase-gated register write enables
    + FULL_CPU_REGISTER_BITS * 4 * 4 // four A-result mux stages
    + FULL_CPU_REGISTER_BITS * 4 // B write muxes
    + 4 // carry write mux
    + 7 * 3 + 1 // zero detector
    + FULL_CPU_PC_BITS * 9 // PC + 1 ripple adder
    + 2 + 3 // JZ condition + branch OR
    + FULL_CPU_PC_BITS * 4 // branch target muxes
    + FULL_CPU_PC_BITS * 4; // PC hold/execute muxes

pub const FULL_CPU_DEVICE_COUNT: usize =
    1 + FULL_CPU_NAND_GATES * DEVICES_PER_NAND + FULL_CPU_TICK_DELAYS;
pub const FULL_CPU_NONLINEAR_DEVICE_COUNT: usize = FULL_CPU_NAND_GATES * SWITCHES_PER_NAND;
pub const FULL_CPU_STATEFUL_DEVICE_COUNT: usize =
    FULL_CPU_NONLINEAR_DEVICE_COUNT + FULL_CPU_TICK_DELAYS;

const OP_NOP: u8 = 0;
const OP_LDA: u8 = 1;
const OP_LDB: u8 = 2;
const OP_ADD: u8 = 3;
const OP_XOR: u8 = 4;
const OP_AND: u8 = 5;
const OP_JZ: u8 = 6;
const OP_JMP: u8 = 7;

const fn full_cpu_instruction(opcode: u8, immediate: u8) -> u16 {
    (opcode as u16) | ((immediate as u16) << FULL_CPU_OPCODE_BITS)
}

const FULL_CPU_PROGRAM: [u16; 16] = [
    full_cpu_instruction(OP_LDA, 5),
    full_cpu_instruction(OP_LDB, 7),
    full_cpu_instruction(OP_ADD, 0),
    full_cpu_instruction(OP_LDB, 3),
    full_cpu_instruction(OP_XOR, 0),
    full_cpu_instruction(OP_LDB, 15),
    full_cpu_instruction(OP_AND, 0),
    full_cpu_instruction(OP_LDB, 241),
    full_cpu_instruction(OP_ADD, 0),
    full_cpu_instruction(OP_JZ, 11),
    full_cpu_instruction(OP_LDA, 0xee),
    full_cpu_instruction(OP_LDA, 42),
    full_cpu_instruction(OP_LDB, 1),
    full_cpu_instruction(OP_ADD, 0),
    full_cpu_instruction(OP_JMP, 0),
    full_cpu_instruction(OP_NOP, 0),
];

#[derive(Debug)]
struct FullCpuProbes {
    pc: [ProbeId; FULL_CPU_PC_BITS],
    a: [ProbeId; FULL_CPU_REGISTER_BITS],
    b: [ProbeId; FULL_CPU_REGISTER_BITS],
    carry: ProbeId,
    instruction: [ProbeId; FULL_CPU_INSTRUCTION_BITS],
    phase: ProbeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FullCpuSnapshot {
    pc: u8,
    a: u8,
    b: u8,
    carry: bool,
    opcode: u8,
    immediate: u8,
    execute_phase: bool,
}

impl FullCpuSnapshot {
    #[inline]
    pub const fn pc(self) -> u8 {
        self.pc
    }

    #[inline]
    pub const fn a(self) -> u8 {
        self.a
    }

    #[inline]
    pub const fn b(self) -> u8 {
        self.b
    }

    #[inline]
    pub const fn carry(self) -> bool {
        self.carry
    }

    #[inline]
    pub const fn opcode(self) -> u8 {
        self.opcode
    }

    #[inline]
    pub const fn immediate(self) -> u8 {
        self.immediate
    }

    #[inline]
    pub const fn execute_phase(self) -> bool {
        self.execute_phase
    }
}

pub struct FullCpuScenario {
    harness: DigitalHarness,
    probes: Option<FullCpuProbes>,
    device_count: usize,
    nonlinear_device_count: usize,
    stateful_device_count: usize,
}

impl Default for FullCpuScenario {
    fn default() -> Self {
        Self::new()
    }
}

impl FullCpuScenario {
    pub fn new() -> Self {
        Self::build(false)
    }

    pub fn for_test() -> Self {
        Self::build(true)
    }

    fn build(observe_architecture: bool) -> Self {
        let mut harness = DigitalHarness::new();

        let pc: [RegisterBit; FULL_CPU_PC_BITS] =
            std::array::from_fn(|_| harness.register_bit(false));
        let a: [RegisterBit; FULL_CPU_REGISTER_BITS] =
            std::array::from_fn(|_| harness.register_bit(false));
        let b: [RegisterBit; FULL_CPU_REGISTER_BITS] =
            std::array::from_fn(|_| harness.register_bit(false));
        let carry = harness.register_bit(false);
        let instruction: [RegisterBit; FULL_CPU_INSTRUCTION_BITS] =
            std::array::from_fn(|_| harness.register_bit(false));
        let phase = harness.register_bit(false);

        harness.inverter_into(phase.input, phase.output);

        let rom = build_full_cpu_rom(&mut harness, &pc);

        for bit in 0..FULL_CPU_INSTRUCTION_BITS {
            harness.mux_into(
                instruction[bit].input,
                rom[bit],
                instruction[bit].output,
                phase.output,
            );
        }

        let opcode: [WireId; FULL_CPU_OPCODE_BITS] =
            std::array::from_fn(|bit| instruction[bit].output);
        let immediate: [WireId; FULL_CPU_REGISTER_BITS] =
            std::array::from_fn(|bit| instruction[FULL_CPU_OPCODE_BITS + bit].output);

        let decoded = decode_opcode(&mut harness, opcode);

        let mut add_carry = harness.ground;
        let mut add_result = Vec::with_capacity(FULL_CPU_REGISTER_BITS);
        let mut xor_result = Vec::with_capacity(FULL_CPU_REGISTER_BITS);
        let mut and_result = Vec::with_capacity(FULL_CPU_REGISTER_BITS);

        for bit in 0..FULL_CPU_REGISTER_BITS {
            let (sum, carry_out) = harness.full_adder(a[bit].output, b[bit].output, add_carry);
            add_result.push(sum.wire);
            add_carry = carry_out.wire;

            xor_result.push(harness.xor_gate(a[bit].output, b[bit].output).wire);
            and_result.push(harness.and_gate(a[bit].output, b[bit].output).wire);
        }

        let exec_lda = harness
            .and_gate(phase.output, decoded[OP_LDA as usize])
            .wire;
        let exec_ldb = harness
            .and_gate(phase.output, decoded[OP_LDB as usize])
            .wire;
        let exec_add = harness
            .and_gate(phase.output, decoded[OP_ADD as usize])
            .wire;
        let exec_xor = harness
            .and_gate(phase.output, decoded[OP_XOR as usize])
            .wire;
        let exec_and = harness
            .and_gate(phase.output, decoded[OP_AND as usize])
            .wire;

        for bit in 0..FULL_CPU_REGISTER_BITS {
            let lda = harness.mux(a[bit].output, immediate[bit], exec_lda);
            let add = harness.mux(lda.wire, add_result[bit], exec_add);
            let xor = harness.mux(add.wire, xor_result[bit], exec_xor);
            harness.mux_into(a[bit].input, xor.wire, and_result[bit], exec_and);

            harness.mux_into(b[bit].input, b[bit].output, immediate[bit], exec_ldb);
        }

        harness.mux_into(carry.input, carry.output, add_carry, exec_add);

        let zero = zero_detector(&mut harness, &a);

        let mut pc_carry = harness.ground;
        let mut incremented_pc = Vec::with_capacity(FULL_CPU_PC_BITS);
        for (bit, register_bit) in pc.iter().enumerate() {
            let addend = if bit == 0 {
                harness.supply
            } else {
                harness.ground
            };
            let (sum, carry_out) = harness.full_adder(register_bit.output, addend, pc_carry);
            incremented_pc.push(sum.wire);
            pc_carry = carry_out.wire;
        }

        let jz_taken = harness.and_gate(decoded[OP_JZ as usize], zero).wire;
        let branch = harness.or_gate(jz_taken, decoded[OP_JMP as usize]).wire;

        for bit in 0..FULL_CPU_PC_BITS {
            let candidate = harness.mux(incremented_pc[bit], immediate[bit], branch);
            harness.mux_into(pc[bit].input, pc[bit].output, candidate.wire, phase.output);
        }

        let device_count = harness.ids.allocated_devices();
        assert_eq!(
            device_count, FULL_CPU_DEVICE_COUNT,
            "full CPU device count drifted; update benchmark metadata",
        );

        let probes = if observe_architecture {
            Some(FullCpuProbes {
                pc: std::array::from_fn(|bit| harness.observe_delay(pc[bit].delay)),
                a: std::array::from_fn(|bit| harness.observe_delay(a[bit].delay)),
                b: std::array::from_fn(|bit| harness.observe_delay(b[bit].delay)),
                carry: harness.observe_delay(carry.delay),
                instruction: std::array::from_fn(|bit| {
                    harness.observe_delay(instruction[bit].delay)
                }),
                phase: harness.observe_delay(phase.delay),
            })
        } else {
            None
        };

        Self {
            harness,
            probes,
            device_count,
            nonlinear_device_count: FULL_CPU_NONLINEAR_DEVICE_COUNT,
            stateful_device_count: FULL_CPU_STATEFUL_DEVICE_COUNT,
        }
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
        self.harness.tick()
    }

    pub fn warm(&mut self, ticks: usize) -> Result<(), EngineTickError> {
        for _ in 0..ticks {
            self.tick()?;
        }
        Ok(())
    }

    pub fn measure_instructions(&mut self, instructions: u64) -> Duration {
        let start = Instant::now();
        for _ in 0..instructions {
            self.tick().unwrap(); // FETCH
            self.tick().unwrap(); // EXEC
        }
        start.elapsed()
    }

    pub fn snapshot(&self) -> Option<FullCpuSnapshot> {
        let probes = self.probes.as_ref()?;
        let pc = read_probe_bits(&self.harness, &probes.pc)? as u8;
        let a = read_probe_bits(&self.harness, &probes.a)? as u8;
        let b = read_probe_bits(&self.harness, &probes.b)? as u8;
        let carry = logic_level(self.harness.voltage(probes.carry))?;
        let instruction = read_probe_bits(&self.harness, &probes.instruction)?;
        let execute_phase = logic_level(self.harness.voltage(probes.phase))?;

        Some(FullCpuSnapshot {
            pc,
            a,
            b,
            carry,
            opcode: (instruction & 0x7) as u8,
            immediate: ((instruction >> FULL_CPU_OPCODE_BITS) & 0xff) as u8,
            execute_phase,
        })
    }
}

fn build_full_cpu_rom(
    harness: &mut DigitalHarness,
    pc: &[RegisterBit; FULL_CPU_PC_BITS],
) -> [WireId; FULL_CPU_INSTRUCTION_BITS] {
    let inverted_pc: [WireId; FULL_CPU_PC_BITS] =
        std::array::from_fn(|bit| harness.inverter(pc[bit].output).wire);

    let address_lines: [WireId; 16] = std::array::from_fn(|address| {
        let terms: [WireId; FULL_CPU_PC_BITS] = std::array::from_fn(|bit| {
            if (address & (1 << bit)) != 0 {
                pc[bit].output
            } else {
                inverted_pc[bit]
            }
        });
        and4(harness, terms)
    });

    std::array::from_fn(|bit| {
        let mut selected = Vec::new();
        for (address, &word) in FULL_CPU_PROGRAM.iter().enumerate() {
            if (word & (1 << bit)) != 0 {
                selected.push(address_lines[address]);
            }
        }
        or_reduce(harness, &selected)
    })
}

fn decode_opcode(
    harness: &mut DigitalHarness,
    opcode: [WireId; FULL_CPU_OPCODE_BITS],
) -> [WireId; 8] {
    let inverted: [WireId; FULL_CPU_OPCODE_BITS] =
        std::array::from_fn(|bit| harness.inverter(opcode[bit]).wire);

    std::array::from_fn(|value| {
        let terms: [WireId; FULL_CPU_OPCODE_BITS] = std::array::from_fn(|bit| {
            if (value & (1 << bit)) != 0 {
                opcode[bit]
            } else {
                inverted[bit]
            }
        });
        and3(harness, terms)
    })
}

fn and3(harness: &mut DigitalHarness, inputs: [WireId; 3]) -> WireId {
    let first = harness.and_gate(inputs[0], inputs[1]);
    harness.and_gate(first.wire, inputs[2]).wire
}

fn and4(harness: &mut DigitalHarness, inputs: [WireId; 4]) -> WireId {
    let first = harness.and_gate(inputs[0], inputs[1]);
    let second = harness.and_gate(first.wire, inputs[2]);
    harness.and_gate(second.wire, inputs[3]).wire
}

fn or_reduce(harness: &mut DigitalHarness, inputs: &[WireId]) -> WireId {
    let Some((&first, rest)) = inputs.split_first() else {
        return harness.ground;
    };

    rest.iter()
        .copied()
        .fold(first, |acc, input| harness.or_gate(acc, input).wire)
}

fn zero_detector(
    harness: &mut DigitalHarness,
    register: &[RegisterBit; FULL_CPU_REGISTER_BITS],
) -> WireId {
    let wires: [WireId; FULL_CPU_REGISTER_BITS] = std::array::from_fn(|bit| register[bit].output);
    let any = or_reduce(harness, &wires);
    harness.inverter(any).wire
}

fn read_probe_bits<const N: usize>(harness: &DigitalHarness, probes: &[ProbeId; N]) -> Option<u16> {
    let mut value = 0u16;
    for (bit, &probe) in probes.iter().enumerate() {
        if logic_level(harness.voltage(probe))? {
            value |= 1 << bit;
        }
    }
    Some(value)
}

pub const fn validated_cpu_device_count(lanes: usize) -> usize {
    1 + lanes * DEVICES_PER_LANE
}

pub const fn validated_cpu_nonlinear_device_count(lanes: usize) -> usize {
    lanes * NONLINEAR_DEVICES_PER_LANE
}

pub const fn validated_cpu_stateful_device_count(lanes: usize) -> usize {
    lanes * STATEFUL_DEVICES_PER_LANE
}

fn set_byte_sources(
    engine: &mut Engine,
    world_id: u32,
    sources: &[DeviceId; BITS_PER_LANE],
    previous: u8,
    next: u8,
) -> Result<(), WorldCommandApplyError> {
    let changed = previous ^ next;

    for (bit, &source) in sources.iter().enumerate() {
        if (changed & (1 << bit)) == 0 {
            continue;
        }

        set_logic_source(engine, world_id, source, (next & (1 << bit)) != 0)?;
    }

    Ok(())
}

fn set_logic_source(
    engine: &mut Engine,
    world_id: u32,
    source: DeviceId,
    high: bool,
) -> Result<(), WorldCommandApplyError> {
    engine.apply_world_command(
        world_id,
        WorldCommand::SetDeviceParameter {
            device: source,
            parameter: ParameterId::new(0),
            value: logic_voltage(high),
        },
    )
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
            u32::try_from(terminal).expect("digital benchmark terminal index must fit u16"),
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
