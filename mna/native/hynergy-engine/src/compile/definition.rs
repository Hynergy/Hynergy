use crate::compile::template::{
    CompiledDefinitionTemplate, DefinitionTemplateBuildError, DefinitionTemplateBuilder,
    LocalStateId, LocalUnknownId, LocalValueId,
};
use hynergy_ids::define_id;
use hynergy_model::circuit::{Circuit, NodeId, ValueRef};
use hynergy_model::device::definition::{
    DeviceBody, DeviceDefinition, DevicePartitionId, PrimitiveElementKind, TerminalId,
};
use hynergy_model::device::registry::DefinitionRegistry;
use hynergy_model::parameter::ParameterId;
use smallvec::SmallVec;
use thiserror::Error;

define_id!(DefinitionStateId: u32);

#[derive(Debug)]
pub(crate) struct CompiledDefinition {
    partitions: Box<[CompiledPartitionTemplate]>,
    state_count: usize,
}

impl CompiledDefinition {
    pub(crate) fn compile(
        registry: &DefinitionRegistry,
        definition: &DeviceDefinition,
    ) -> Result<Self, DefinitionCompileError> {
        let partitions = match definition.body() {
            DeviceBody::Primitive(PrimitiveElementKind::TickDelay) => {
                compile_tick_delay_partitions(definition)?
            }

            DeviceBody::Primitive(_) => {
                let template = CompiledDefinitionTemplate::compile(definition)?;

                debug_assert_eq!(
                    definition.partition_count(),
                    1,
                    "supported primitive must have one partition",
                );

                let partition = DevicePartitionId::new(0);

                let definition_parameters = definition_parameters(definition);

                let definition_states = definition_states(definition);

                vec![CompiledPartitionTemplate {
                    definition_terminals: definition_terminals_for_partition(definition, partition),

                    definition_parameters,

                    definition_state_reads: definition_states.clone(),

                    definition_state_writes: definition_states.clone(),

                    definition_states,

                    template,
                }]
                .into_boxed_slice()
            }

            DeviceBody::Composite(circuit) => {
                compile_composite_partitions(registry, definition, circuit)?
            }
        };

        debug_assert_eq!(
            partitions.len(),
            definition.partition_count(),
            "compiled partition count must match definition",
        );

        Ok(Self {
            partitions,
            state_count: definition.state_count(),
        })
    }

    #[inline]
    pub(crate) fn partition_count(&self) -> usize {
        self.partitions.len()
    }

    #[inline]
    pub(crate) const fn state_count(&self) -> usize {
        self.state_count
    }

    #[inline]
    pub(crate) fn partition(
        &self,
        partition: DevicePartitionId,
    ) -> Option<&CompiledPartitionTemplate> {
        self.partitions.get(partition.index())
    }
}

#[derive(Debug)]
pub(crate) struct CompiledPartitionTemplate {
    definition_terminals: SmallVec<[TerminalId; 8]>,
    definition_parameters: SmallVec<[ParameterId; 1]>,
    definition_states: SmallVec<[DefinitionStateId; 4]>,
    definition_state_reads: SmallVec<[DefinitionStateId; 4]>,
    definition_state_writes: SmallVec<[DefinitionStateId; 4]>,
    template: CompiledDefinitionTemplate,
}

impl CompiledPartitionTemplate {
    #[inline]
    pub(crate) fn definition_terminals(&self) -> &[TerminalId] {
        &self.definition_terminals
    }

    #[inline]
    pub(crate) fn definition_states(&self) -> &[DefinitionStateId] {
        &self.definition_states
    }

    #[inline]
    pub(crate) fn definition_state_reads(&self) -> &[DefinitionStateId] {
        &self.definition_state_reads
    }

    #[inline]
    pub(crate) fn definition_state_writes(&self) -> &[DefinitionStateId] {
        &self.definition_state_writes
    }

    #[inline]
    pub(crate) const fn template(&self) -> &CompiledDefinitionTemplate {
        &self.template
    }

    #[inline]
    pub(crate) fn definition_parameters(&self) -> &[ParameterId] {
        &self.definition_parameters
    }
}

fn definition_parameters(definition: &DeviceDefinition) -> SmallVec<[ParameterId; 1]> {
    (0..definition.parameters().len())
        .map(|index| {
            ParameterId::new(u32::try_from(index).expect("parameter index must fit ParameterId"))
        })
        .collect()
}

fn definition_states(definition: &DeviceDefinition) -> SmallVec<[DefinitionStateId; 4]> {
    (0..definition.state_count())
        .map(|index| {
            DefinitionStateId::new(
                u32::try_from(index).expect("state index must fit DefinitionStateId"),
            )
        })
        .collect()
}

fn compile_tick_delay_partitions(
    definition: &DeviceDefinition,
) -> Result<Box<[CompiledPartitionTemplate]>, DefinitionCompileError> {
    debug_assert_eq!(definition.partition_count(), 2);
    debug_assert_eq!(definition.state_count(), 1);

    let state = DefinitionStateId::new(0);

    let mut partitions = Vec::with_capacity(definition.partition_count());

    for partition_index in 0..definition.partition_count() {
        let partition = DevicePartitionId::new(
            u16::try_from(partition_index).expect("partition index must fit DevicePartitionId"),
        );

        let definition_terminals = definition_terminals_for_partition(definition, partition);

        let mut builder = DefinitionTemplateBuilder::default();

        match partition_index {
            0 => {
                debug_assert_eq!(definition_terminals.len(), 2);

                let positive = builder.terminal_voltage()?;
                let negative = builder.terminal_voltage()?;

                let next_state = builder.write_only_state()?;

                let positive_voltage = builder.unknown_value(positive)?;
                let negative_voltage = builder.unknown_value(negative)?;

                let voltage = builder.sub(positive_voltage, negative_voltage)?;

                builder.write_state_id(next_state, voltage)?;
            }

            1 => {
                debug_assert_eq!(definition_terminals.len(), 2);

                let positive = builder.terminal_voltage()?;
                let negative = builder.terminal_voltage()?;

                let branch_current = builder.branch_current_unknown()?;

                let previous = builder.read_state()?;

                stamp_voltage_source(
                    &mut builder,
                    positive,
                    negative,
                    branch_current,
                    previous.value(),
                )?;
            }

            _ => unreachable!("TickDelay must have exactly two partitions"),
        }

        let (definition_state_reads, definition_state_writes) = match partition_index {
            0 => (SmallVec::new(), SmallVec::from_slice(&[state])),

            1 => (SmallVec::from_slice(&[state]), SmallVec::new()),

            _ => unreachable!("TickDelay must have exactly two partitions"),
        };

        partitions.push(CompiledPartitionTemplate {
            definition_terminals,
            definition_parameters: SmallVec::new(),
            definition_states: SmallVec::from_slice(&[state]),
            definition_state_reads,
            definition_state_writes,
            template: builder.finish()?,
        });
    }

    Ok(partitions.into_boxed_slice())
}

fn definition_terminals_for_partition(
    definition: &DeviceDefinition,
    partition: DevicePartitionId,
) -> SmallVec<[TerminalId; 8]> {
    definition
        .terminal_partitions()
        .iter()
        .enumerate()
        .filter_map(|(index, &terminal_partition)| {
            if terminal_partition != partition {
                return None;
            }

            Some(TerminalId::new(
                u32::try_from(index).expect("terminal index must fit TerminalId"),
            ))
        })
        .collect()
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionCompileError {
    #[error("primitive {kind:?} is not yet supported by the compiler")]
    UnsupportedPrimitive { kind: PrimitiveElementKind },

    #[error("composite definitions are not yet supported by the compiler")]
    CompositeNotYetSupported,

    #[error(transparent)]
    Template(#[from] DefinitionTemplateBuildError),
}

impl CompiledDefinitionTemplate {
    pub(crate) fn compile(definition: &DeviceDefinition) -> Result<Self, DefinitionCompileError> {
        match definition.body() {
            DeviceBody::Primitive(kind) => compile_primitive(*kind),

            DeviceBody::Composite(_) => Err(DefinitionCompileError::CompositeNotYetSupported),
        }
    }
}

fn compile_primitive(
    kind: PrimitiveElementKind,
) -> Result<CompiledDefinitionTemplate, DefinitionCompileError> {
    let mut builder = DefinitionTemplateBuilder::default();

    match kind {
        PrimitiveElementKind::Conductance => {
            let a = builder.terminal_voltage()?;
            let b = builder.terminal_voltage()?;

            let conductance = builder.parameter()?;

            stamp_conductance(&mut builder, a, b, conductance);
        }

        PrimitiveElementKind::Resistance => {
            let a = builder.terminal_voltage()?;
            let b = builder.terminal_voltage()?;

            let resistance = builder.parameter()?;
            let one = builder.constant(1.0)?;

            let conductance = builder.div(one, resistance)?;

            stamp_conductance(&mut builder, a, b, conductance);
        }

        PrimitiveElementKind::CurrentSource => {
            let from = builder.terminal_voltage()?;
            let to = builder.terminal_voltage()?;

            let current = builder.parameter()?;

            stamp_current_source(&mut builder, from, to, current);
        }

        PrimitiveElementKind::VoltageSource => {
            let positive = builder.terminal_voltage()?;
            let negative = builder.terminal_voltage()?;

            let branch_current = builder.branch_current_unknown()?;

            let voltage = builder.parameter()?;

            stamp_voltage_source(&mut builder, positive, negative, branch_current, voltage)?;
        }
        PrimitiveElementKind::VoltageControlledCurrentSource => {
            let output_positive = builder.terminal_voltage()?;
            let output_negative = builder.terminal_voltage()?;
            let control_positive = builder.terminal_voltage()?;
            let control_negative = builder.terminal_voltage()?;

            let transconductance = builder.parameter()?;

            stamp_vccs(
                &mut builder,
                output_positive,
                output_negative,
                control_positive,
                control_negative,
                transconductance,
            );
        }

        PrimitiveElementKind::VoltageControlledVoltageSource => {
            let output_positive = builder.terminal_voltage()?;
            let output_negative = builder.terminal_voltage()?;
            let control_positive = builder.terminal_voltage()?;
            let control_negative = builder.terminal_voltage()?;

            let branch_current = builder.branch_current_unknown()?;

            let gain = builder.parameter()?;

            stamp_vcvs(
                &mut builder,
                output_positive,
                output_negative,
                control_positive,
                control_negative,
                branch_current,
                gain,
            )?;
        }

        PrimitiveElementKind::Capacitor => {
            let positive = builder.terminal_voltage()?;
            let negative = builder.terminal_voltage()?;

            let capacitance = builder.parameter()?;
            let timestep = builder.timestep()?;

            let previous_voltage = builder.state()?;

            let conductance = builder.div(capacitance, timestep)?;

            stamp_conductance(&mut builder, positive, negative, conductance);

            let history = builder.mul(conductance, previous_voltage.value())?;

            builder.add_rhs(positive, history, 1.0);
            builder.add_rhs(negative, history, -1.0);

            let positive_voltage = builder.unknown_value(positive)?;
            let negative_voltage = builder.unknown_value(negative)?;
            let next_voltage = builder.sub(positive_voltage, negative_voltage)?;

            builder.write_state(previous_voltage, next_voltage)?;
        }

        PrimitiveElementKind::Inductor => {
            let positive = builder.terminal_voltage()?;
            let negative = builder.terminal_voltage()?;

            let inductance = builder.parameter()?;
            let timestep = builder.timestep()?;

            let previous_current = builder.state()?;

            let conductance = builder.div(timestep, inductance)?;

            stamp_conductance(&mut builder, positive, negative, conductance);

            stamp_current_source(&mut builder, positive, negative, previous_current.value());

            let positive_voltage = builder.unknown_value(positive)?;
            let negative_voltage = builder.unknown_value(negative)?;
            let voltage = builder.sub(positive_voltage, negative_voltage)?;
            let current_delta = builder.mul(conductance, voltage)?;
            let next_current = builder.add(previous_current.value(), current_delta)?;

            builder.write_state(previous_current, next_current)?;
        }

        PrimitiveElementKind::VoltageControlledConductance => {
            let output_positive = builder.terminal_voltage()?;
            let output_negative = builder.terminal_voltage()?;
            let control = builder.terminal_voltage()?;

            let parameters = ControlledConductanceParameters {
                threshold: builder.parameter()?,
                transition: builder.parameter()?,
                g_min: builder.parameter()?,
                g_max: builder.parameter()?,
            };

            stamp_voltage_controlled_conductance(
                &mut builder,
                output_positive,
                output_negative,
                control,
                parameters,
            )?;
        }

        PrimitiveElementKind::VoltageControlledSwitch => {
            let output_positive = builder.terminal_voltage()?;

            let output_negative = builder.terminal_voltage()?;

            let control_positive = builder.terminal_voltage()?;

            let control_negative = builder.terminal_voltage()?;

            let parameters = ControlledSwitchParameters {
                threshold: builder.parameter()?,
                hysteresis: builder.parameter()?,
                g_max: builder.parameter()?,
                g_min: builder.parameter()?,
            };

            let state = builder.state()?;

            let next_mode = stamp_voltage_controlled_switch(
                &mut builder,
                output_positive,
                output_negative,
                control_positive,
                control_negative,
                state.value(),
                parameters,
            )?;

            builder.write_state(state, next_mode)?;
        }

        _ => {
            return Err(DefinitionCompileError::UnsupportedPrimitive { kind });
        }
    }

    Ok(builder.finish()?)
}

fn compile_composite_partitions(
    registry: &DefinitionRegistry,
    definition: &DeviceDefinition,
    circuit: &Circuit,
) -> Result<Box<[CompiledPartitionTemplate]>, DefinitionCompileError> {
    let mut compiled_children = Vec::with_capacity(circuit.elements().len());

    for element in circuit.elements() {
        let child_definition = registry
            .get(element.definition())
            .expect("registered composite child definition must exist");

        compiled_children.push(CompiledDefinition::compile(registry, child_definition)?);
    }

    let mut element_state_offsets = Vec::with_capacity(compiled_children.len());

    let mut next_state = 0usize;

    for child in &compiled_children {
        element_state_offsets.push(next_state);

        next_state = next_state
            .checked_add(child.state_count())
            .expect("validated composite state count must not overflow");
    }

    debug_assert_eq!(
        next_state,
        definition.state_count(),
        "compiled child states must cover composite state space",
    );

    let mut partitions = Vec::with_capacity(definition.partition_count());

    for parent_partition_index in 0..definition.partition_count() {
        let parent_partition =
            DevicePartitionId::new(u16::try_from(parent_partition_index).expect(
                "definition partition index must fit \
                         DevicePartitionId",
            ));

        partitions.push(compile_composite_partition(
            definition,
            circuit,
            parent_partition,
            &compiled_children,
            &element_state_offsets,
        )?);
    }

    Ok(partitions.into_boxed_slice())
}

fn compile_composite_partition(
    definition: &DeviceDefinition,
    circuit: &Circuit,
    parent_partition: DevicePartitionId,
    compiled_children: &[CompiledDefinition],
    element_state_offsets: &[usize],
) -> Result<CompiledPartitionTemplate, DefinitionCompileError> {
    debug_assert_eq!(circuit.elements().len(), compiled_children.len());
    debug_assert_eq!(compiled_children.len(), element_state_offsets.len());

    let definition_terminals = definition_terminals_for_partition(definition, parent_partition);

    let mut builder = DefinitionTemplateBuilder::default();
    let mut node_unknowns = vec![None; circuit.node_count() as usize];

    for &terminal in &definition_terminals {
        let node = definition.terminals()[terminal.index()];
        let unknown = builder.terminal_voltage()?;

        debug_assert!(
            node_unknowns[node.index()].replace(unknown).is_none(),
            "composite terminal nodes must be unique",
        );
    }

    let mut parameter_values = vec![None; definition.parameters().len()];
    let mut definition_parameters = SmallVec::<[ParameterId; 1]>::new();

    let state_count = definition.state_count();

    let mut state_used = vec![false; state_count];
    let mut state_reads = vec![false; state_count];
    let mut state_writes = vec![false; state_count];

    let element_partitions = definition.element_partitions();

    let mut child_partition_offset = 0usize;

    for (element_index, child) in compiled_children.iter().enumerate() {
        let state_offset = element_state_offsets[element_index];

        for child_partition_index in 0..child.partition_count() {
            let mapped_parent = element_partitions[child_partition_offset + child_partition_index];

            if mapped_parent != parent_partition {
                continue;
            }

            let child_partition = child
                .partition(DevicePartitionId::new(
                    u16::try_from(child_partition_index)
                        .expect("child partition index must fit DevicePartitionId"),
                ))
                .expect("compiled child partition must exist");

            for &state in child_partition.definition_states() {
                let index = state_offset + state.index();
                state_used[index] = true;
            }

            for &state in child_partition.definition_state_reads() {
                let index = state_offset + state.index();

                debug_assert!(state_used[index]);

                state_reads[index] = true;
            }

            for &state in child_partition.definition_state_writes() {
                let index = state_offset + state.index();

                debug_assert!(state_used[index]);

                state_writes[index] = true;
            }
        }

        child_partition_offset += child.partition_count();
    }

    debug_assert_eq!(
        child_partition_offset,
        element_partitions.len(),
        "element partition layout must cover every child partition",
    );

    let mut local_states = vec![None; state_count];

    let mut definition_states = SmallVec::<[DefinitionStateId; 4]>::new();
    let mut definition_state_reads = SmallVec::<[DefinitionStateId; 4]>::new();
    let mut definition_state_writes = SmallVec::<[DefinitionStateId; 4]>::new();

    for index in 0..state_count {
        if !state_used[index] {
            continue;
        }

        let state = DefinitionStateId::new(
            u32::try_from(index)
                .expect("validated composite state index must fit DefinitionStateId"),
        );

        let local = builder.allocate_state_slot(state_writes[index])?;

        local_states[index] = Some(local);
        definition_states.push(state);

        if state_reads[index] {
            definition_state_reads.push(state);
        }

        if state_writes[index] {
            definition_state_writes.push(state);
        }
    }

    child_partition_offset = 0;

    for (element_index, element) in circuit.elements().iter().enumerate() {
        let child = &compiled_children[element_index];
        let state_offset = element_state_offsets[element_index];

        for child_partition_index in 0..child.partition_count() {
            let mapped_parent = element_partitions[child_partition_offset + child_partition_index];

            if mapped_parent != parent_partition {
                continue;
            }

            let child_partition_id = DevicePartitionId::new(
                u16::try_from(child_partition_index)
                    .expect("child partition index must fit DevicePartitionId"),
            );

            let child_partition = child
                .partition(child_partition_id)
                .expect("compiled child partition must exist");

            let mut terminals = SmallVec::<[LocalUnknownId; 8]>::new();

            for &terminal in child_partition.definition_terminals() {
                let node = element.terminals()[terminal.index()];

                terminals.push(composite_node_unknown(
                    &mut builder,
                    &mut node_unknowns,
                    node,
                )?);
            }

            let mut parameters = SmallVec::<[LocalValueId; 4]>::new();

            for &parameter in child_partition.definition_parameters() {
                let value = element.parameters()[parameter.index()];

                parameters.push(composite_parameter_value(
                    &mut builder,
                    &mut parameter_values,
                    &mut definition_parameters,
                    value,
                )?);
            }

            let mut states = SmallVec::<[LocalStateId; 4]>::new();

            for &child_state in child_partition.definition_states() {
                let global_index = state_offset + child_state.index();

                states.push(
                    local_states[global_index]
                        .expect("used child state must have a parent partition state slot"),
                );
            }

            child_partition.template().instantiate_into(
                &mut builder,
                &terminals,
                &parameters,
                &states,
            )?;
        }

        child_partition_offset += child.partition_count();
    }

    debug_assert_eq!(
        child_partition_offset,
        element_partitions.len(),
        "element partition layout must cover every child partition",
    );

    Ok(CompiledPartitionTemplate {
        definition_terminals,
        definition_parameters,
        definition_states,
        definition_state_reads,
        definition_state_writes,
        template: builder.finish()?,
    })
}

fn composite_node_unknown(
    builder: &mut DefinitionTemplateBuilder,
    node_unknowns: &mut [Option<LocalUnknownId>],
    node: NodeId,
) -> Result<LocalUnknownId, DefinitionTemplateBuildError> {
    if let Some(unknown) = node_unknowns[node.index()] {
        return Ok(unknown);
    }

    let unknown = builder.allocated_voltage_unknown()?;

    node_unknowns[node.index()] = Some(unknown);

    Ok(unknown)
}

fn composite_parameter_value(
    builder: &mut DefinitionTemplateBuilder,
    parameter_values: &mut [Option<LocalValueId>],
    definition_parameters: &mut SmallVec<[ParameterId; 1]>,
    value: ValueRef,
) -> Result<LocalValueId, DefinitionTemplateBuildError> {
    match value {
        ValueRef::Literal(value) => builder.constant(value),

        ValueRef::Parameter(parameter) => {
            if let Some(value) = parameter_values[parameter.index()] {
                return Ok(value);
            }

            let value = builder.parameter()?;

            parameter_values[parameter.index()] = Some(value);

            definition_parameters.push(parameter);

            Ok(value)
        }
    }
}

#[inline]
fn stamp_conductance(
    builder: &mut DefinitionTemplateBuilder,
    a: LocalUnknownId,
    b: LocalUnknownId,
    conductance: LocalValueId,
) {
    builder.add_matrix(a, a, conductance, 1.0);
    builder.add_matrix(b, a, conductance, -1.0);
    builder.add_matrix(a, b, conductance, -1.0);
    builder.add_matrix(b, b, conductance, 1.0);
}

#[inline]
fn stamp_current_source(
    builder: &mut DefinitionTemplateBuilder,
    from: LocalUnknownId,
    to: LocalUnknownId,
    current: LocalValueId,
) {
    builder.add_rhs(from, current, -1.0);
    builder.add_rhs(to, current, 1.0);
}

fn stamp_voltage_source(
    builder: &mut DefinitionTemplateBuilder,
    positive: LocalUnknownId,
    negative: LocalUnknownId,
    branch_current: LocalUnknownId,
    voltage: LocalValueId,
) -> Result<(), DefinitionTemplateBuildError> {
    let one = builder.constant(1.0)?;

    builder.add_matrix(positive, branch_current, one, 1.0);
    builder.add_matrix(negative, branch_current, one, -1.0);
    builder.add_matrix(branch_current, positive, one, 1.0);
    builder.add_matrix(branch_current, negative, one, -1.0);
    builder.add_rhs(branch_current, voltage, 1.0);

    Ok(())
}

#[inline]
fn stamp_vccs(
    builder: &mut DefinitionTemplateBuilder,
    output_positive: LocalUnknownId,
    output_negative: LocalUnknownId,
    control_positive: LocalUnknownId,
    control_negative: LocalUnknownId,
    transconductance: LocalValueId,
) {
    builder.add_matrix(output_positive, control_positive, transconductance, 1.0);
    builder.add_matrix(output_positive, control_negative, transconductance, -1.0);
    builder.add_matrix(output_negative, control_positive, transconductance, -1.0);
    builder.add_matrix(output_negative, control_negative, transconductance, 1.0);
}

fn stamp_vcvs(
    builder: &mut DefinitionTemplateBuilder,
    output_positive: LocalUnknownId,
    output_negative: LocalUnknownId,
    control_positive: LocalUnknownId,
    control_negative: LocalUnknownId,
    branch_current: LocalUnknownId,
    gain: LocalValueId,
) -> Result<(), DefinitionTemplateBuildError> {
    let one = builder.constant(1.0)?;

    builder.add_matrix(output_positive, branch_current, one, 1.0);
    builder.add_matrix(output_negative, branch_current, one, -1.0);
    builder.add_matrix(branch_current, output_positive, one, 1.0);
    builder.add_matrix(branch_current, output_negative, one, -1.0);
    builder.add_matrix(branch_current, control_positive, gain, -1.0);
    builder.add_matrix(branch_current, control_negative, gain, 1.0);

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct ControlledSwitchParameters {
    threshold: LocalValueId,
    hysteresis: LocalValueId,
    g_max: LocalValueId,
    g_min: LocalValueId,
}

#[derive(Debug, Clone, Copy)]
struct ControlledConductanceParameters {
    threshold: LocalValueId,
    transition: LocalValueId,
    g_min: LocalValueId,
    g_max: LocalValueId,
}

fn stamp_voltage_controlled_conductance(
    builder: &mut DefinitionTemplateBuilder,
    output_positive: LocalUnknownId,
    output_negative: LocalUnknownId,
    control: LocalUnknownId,
    parameters: ControlledConductanceParameters,
) -> Result<(), DefinitionTemplateBuildError> {
    let ControlledConductanceParameters {
        threshold,
        transition,
        g_min,
        g_max,
    } = parameters;

    let half = builder.constant(0.5)?;
    let one = builder.constant(1.0)?;

    let half_transition = builder.mul(transition, half)?;

    let low = builder.sub(threshold, half_transition)?;
    let high = builder.add(threshold, half_transition)?;

    let conductance_range = builder.sub(g_max, g_min)?;
    let transition_slope = builder.div(conductance_range, transition)?;

    let output_positive_voltage = builder.unknown_value(output_positive)?;
    let output_negative_voltage = builder.unknown_value(output_negative)?;
    let control_node_voltage = builder.unknown_value(control)?;

    let output_voltage = builder.sub(output_positive_voltage, output_negative_voltage)?;

    let control_voltage = builder.sub(control_node_voltage, output_negative_voltage)?;

    let below = builder.less_equal(control_voltage, low)?;
    let above = builder.less_equal(high, control_voltage)?;

    let inside = builder.sub(one, below)?;
    let inside = builder.sub(inside, above)?;

    let active_slope = builder.mul(transition_slope, inside)?;

    let control_offset = builder.sub(control_voltage, low)?;
    let transition_delta = builder.mul(active_slope, control_offset)?;
    let above_delta = builder.mul(conductance_range, above)?;

    let conductance = builder.add(g_min, above_delta)?;
    let conductance = builder.add(conductance, transition_delta)?;

    let transconductance = builder.mul(active_slope, output_voltage)?;

    let correction = builder.mul(transconductance, control_voltage)?;

    stamp_conductance(builder, output_positive, output_negative, conductance);

    stamp_vccs(
        builder,
        output_positive,
        output_negative,
        control,
        output_negative,
        transconductance,
    );

    builder.add_rhs(output_positive, correction, 1.0);
    builder.add_rhs(output_negative, correction, -1.0);

    Ok(())
}

fn stamp_voltage_controlled_switch(
    builder: &mut DefinitionTemplateBuilder,
    output_positive: LocalUnknownId,
    output_negative: LocalUnknownId,
    control_positive: LocalUnknownId,
    control_negative: LocalUnknownId,
    initial_mode: LocalValueId,
    parameters: ControlledSwitchParameters,
) -> Result<LocalValueId, DefinitionTemplateBuildError> {
    let ControlledSwitchParameters {
        threshold,
        hysteresis,
        g_max,
        g_min,
    } = parameters;

    let zero = builder.constant(0.0)?;
    let half = builder.constant(0.5)?;
    let one = builder.constant(1.0)?;

    let half_hysteresis = builder.mul(hysteresis, half)?;

    let lower = builder.sub(threshold, half_hysteresis)?;
    let upper = builder.add(threshold, half_hysteresis)?;

    let control_positive_voltage = builder.unknown_value(control_positive)?;
    let control_negative_voltage = builder.unknown_value(control_negative)?;

    let control_voltage = builder.sub(control_positive_voltage, control_negative_voltage)?;

    let mode = builder.iteration_latch(initial_mode)?;

    let current_mode = mode.value();

    let turn_on = builder.less_equal(upper, control_voltage)?;
    let turn_off = builder.less_equal(control_voltage, lower)?;

    let off = builder.sub(one, current_mode)?;

    let remain_on = builder.sub(one, turn_off)?;

    let off_to_on = builder.mul(off, turn_on)?;
    let on_to_on = builder.mul(current_mode, remain_on)?;

    let hysteretic_mode = builder.add(off_to_on, on_to_on)?;
    let zero_hysteresis = builder.less_equal(hysteresis, zero)?;
    let direct_mode = builder.less_equal(threshold, control_voltage)?;
    let use_hysteresis = builder.sub(one, zero_hysteresis)?;
    let hysteretic_part = builder.mul(use_hysteresis, hysteretic_mode)?;
    let direct_part = builder.mul(zero_hysteresis, direct_mode)?;
    let next_mode = builder.add(hysteretic_part, direct_part)?;

    builder.update_iteration_latch(mode, next_mode);
    builder.require_iteration_stability(next_mode);

    let conductance_range = builder.sub(g_max, g_min)?;
    let mode_conductance = builder.mul(next_mode, conductance_range)?;
    let conductance = builder.add(g_min, mode_conductance)?;

    stamp_conductance(builder, output_positive, output_negative, conductance);

    Ok(next_mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hynergy_ir::StateSlot;

    use crate::compile::{island_ir::IslandIrBuilder, unknown::UnknownAllocator};

    use crate::compile::state::BoundStateSlots;
    use crate::compile::template::CompiledDefinitionTemplate;
    use hynergy_mna::{
        pattern::{PatternBuilder, UnknownIndex},
        system::MnaSystem,
    };
    use hynergy_model::circuit::Element;
    use hynergy_model::device::builder::DeviceDefinitionBuilder;
    use hynergy_model::device::{
        definition::{DefinitionId, PrimitiveElementKind},
        registry::DefinitionRegistry,
    };
    use hynergy_model::parameter::ParameterId;

    fn state_slots(indices: &[u32]) -> BoundStateSlots {
        BoundStateSlots::new(indices.iter().copied().map(StateSlot::new).collect())
    }

    fn template(kind: PrimitiveElementKind) -> CompiledDefinitionTemplate {
        let registry = DefinitionRegistry::new();

        let definition = registry.get(DefinitionId::from(kind)).unwrap();

        CompiledDefinitionTemplate::compile(definition).unwrap()
    }

    #[test]
    fn voltage_source_allocates_branch_unknown_and_solves() {
        let source = template(PrimitiveElementKind::VoltageSource);

        assert_eq!(source.terminal_count(), 2);
        assert_eq!(source.allocated_unknown_count(), 1,);
        assert_eq!(source.state_count(), 0);

        let node = UnknownIndex::new(0);

        let mut unknown_allocator = UnknownAllocator::new(1).unwrap();

        let allocated = unknown_allocator
            .allocate(source.allocated_unknown_count())
            .unwrap();

        let branch_current = allocated.get(0).unwrap();

        let unknowns = source
            .bind_unknowns(&[Some(node), None], allocated)
            .unwrap();

        let mut pattern_builder = PatternBuilder::with_capacity(
            unknown_allocator.dimension(),
            source.matrix_entry_count(),
        )
        .unwrap();

        source
            .request_pattern(&unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        let states = state_slots(&[]);
        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let inputs = source.bind(&unknowns, &states, &mut ir_builder).unwrap();

        let ir = ir_builder.finish().unwrap();

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(inputs.parameter(0).unwrap(), 5.0);

        ir.value_program().execute_static(&mut workspace);

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut matrix = system.values_mut();

            ir.matrix_program().execute(&mut matrix, workspace.values());
        }

        system.factorize().unwrap();

        let mut solution = [0.0, 0.0];

        ir.rhs_program().execute(&mut solution, workspace.values());
        system.solve_in_place(&mut solution).unwrap();

        assert!((solution[node.index()] - 5.0).abs() < 1.0e-12);
        assert!(solution[branch_current.index()].abs() < 1.0e-12);
    }

    #[test]
    fn voltage_source_branch_unknown_exposes_source_current() {
        let source = template(PrimitiveElementKind::VoltageSource);
        let conductance = template(PrimitiveElementKind::Conductance);

        let node = UnknownIndex::new(0);

        let mut unknown_allocator = UnknownAllocator::new(1).unwrap();

        let conductance_allocated = unknown_allocator
            .allocate(conductance.allocated_unknown_count())
            .unwrap();

        let source_allocated = unknown_allocator
            .allocate(source.allocated_unknown_count())
            .unwrap();

        let branch_current = source_allocated.get(0).unwrap();

        let conductance_unknowns = conductance
            .bind_unknowns(&[Some(node), None], conductance_allocated)
            .unwrap();

        let source_unknowns = source
            .bind_unknowns(&[Some(node), None], source_allocated)
            .unwrap();

        let mut pattern_builder = PatternBuilder::with_capacity(
            unknown_allocator.dimension(),
            conductance.matrix_entry_count() + source.matrix_entry_count(),
        )
        .unwrap();

        conductance
            .request_pattern(&conductance_unknowns, &mut pattern_builder)
            .unwrap();

        source
            .request_pattern(&source_unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        let conductance_states = state_slots(&[]);
        let source_states = state_slots(&[]);

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let conductance_inputs = conductance
            .bind(&conductance_unknowns, &conductance_states, &mut ir_builder)
            .unwrap();

        let source_inputs = source
            .bind(&source_unknowns, &source_states, &mut ir_builder)
            .unwrap();

        let ir = ir_builder.finish().unwrap();

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(conductance_inputs.parameter(0).unwrap(), 2.0);
        workspace.set_input(source_inputs.parameter(0).unwrap(), 5.0);

        ir.value_program().execute_static(&mut workspace);

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut matrix = system.values_mut();

            ir.matrix_program().execute(&mut matrix, workspace.values());
        }

        system.factorize().unwrap();

        let mut solution = vec![0.0; unknown_allocator.dimension()];

        ir.rhs_program().execute(&mut solution, workspace.values());
        system.solve_in_place(&mut solution).unwrap();

        assert!((solution[node.index()] - 5.0).abs() < 1.0e-12);
        assert!((solution[branch_current.index()] + 10.0).abs() < 1.0e-12);
    }

    #[test]
    fn vccs_has_no_auxiliary_unknown() {
        let vccs = template(PrimitiveElementKind::VoltageControlledCurrentSource);

        assert_eq!(vccs.terminal_count(), 4);
        assert_eq!(vccs.allocated_unknown_count(), 0,);
        assert_eq!(vccs.parameter_count(), 1);
        assert_eq!(vccs.state_count(), 0);
    }

    #[test]
    fn vcvs_allocates_branch_current_unknown() {
        let vcvs = template(PrimitiveElementKind::VoltageControlledVoltageSource);

        assert_eq!(vcvs.terminal_count(), 4);
        assert_eq!(vcvs.allocated_unknown_count(), 1,);
        assert_eq!(vcvs.parameter_count(), 1);
        assert_eq!(vcvs.state_count(), 0);
    }

    #[test]
    fn vccs_uses_control_voltage_with_expected_polarity() {
        let voltage_source = template(PrimitiveElementKind::VoltageSource);
        let conductance = template(PrimitiveElementKind::Conductance);
        let vccs = template(PrimitiveElementKind::VoltageControlledCurrentSource);

        let output = UnknownIndex::new(0);
        let control = UnknownIndex::new(1);

        let mut unknown_allocator = UnknownAllocator::new(2).unwrap();

        let voltage_source_aux = unknown_allocator
            .allocate(voltage_source.allocated_unknown_count())
            .unwrap();

        let conductance_aux = unknown_allocator
            .allocate(conductance.allocated_unknown_count())
            .unwrap();

        let vccs_aux = unknown_allocator
            .allocate(vccs.allocated_unknown_count())
            .unwrap();

        let voltage_source_unknowns = voltage_source
            .bind_unknowns(&[Some(control), None], voltage_source_aux)
            .unwrap();

        let conductance_unknowns = conductance
            .bind_unknowns(&[Some(output), None], conductance_aux)
            .unwrap();

        let vccs_unknowns = vccs
            .bind_unknowns(&[Some(output), None, Some(control), None], vccs_aux)
            .unwrap();

        let mut pattern_builder = PatternBuilder::with_capacity(
            unknown_allocator.dimension(),
            voltage_source.matrix_entry_count()
                + conductance.matrix_entry_count()
                + vccs.matrix_entry_count(),
        )
        .unwrap();

        voltage_source
            .request_pattern(&voltage_source_unknowns, &mut pattern_builder)
            .unwrap();

        conductance
            .request_pattern(&conductance_unknowns, &mut pattern_builder)
            .unwrap();

        vccs.request_pattern(&vccs_unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        let states = state_slots(&[]);

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let voltage_source_inputs = voltage_source
            .bind(&voltage_source_unknowns, &states, &mut ir_builder)
            .unwrap();

        let conductance_inputs = conductance
            .bind(&conductance_unknowns, &states, &mut ir_builder)
            .unwrap();

        let vccs_inputs = vccs.bind(&vccs_unknowns, &states, &mut ir_builder).unwrap();

        let ir = ir_builder.finish().unwrap();

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(voltage_source_inputs.parameter(0).unwrap(), 2.0);
        workspace.set_input(conductance_inputs.parameter(0).unwrap(), 2.0);
        workspace.set_input(vccs_inputs.parameter(0).unwrap(), 3.0);

        ir.value_program().execute_static(&mut workspace);

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut matrix = system.values_mut();

            ir.matrix_program().execute(&mut matrix, workspace.values());
        }

        system.factorize().unwrap();

        let mut solution = vec![0.0; unknown_allocator.dimension()];

        ir.rhs_program().execute(&mut solution, workspace.values());
        system.solve_in_place(&mut solution).unwrap();

        assert!((solution[control.index()] - 2.0).abs() < 1.0e-12);
        assert!((solution[output.index()] + 3.0).abs() < 1.0e-12);
    }

    #[test]
    fn vcvs_enforces_controlled_output_voltage() {
        let voltage_source = template(PrimitiveElementKind::VoltageSource);
        let vcvs = template(PrimitiveElementKind::VoltageControlledVoltageSource);

        let control = UnknownIndex::new(0);
        let output = UnknownIndex::new(1);

        let mut unknown_allocator = UnknownAllocator::new(2).unwrap();

        let control_source_aux = unknown_allocator
            .allocate(voltage_source.allocated_unknown_count())
            .unwrap();

        let vcvs_aux = unknown_allocator
            .allocate(vcvs.allocated_unknown_count())
            .unwrap();

        let control_source_unknowns = voltage_source
            .bind_unknowns(&[Some(control), None], control_source_aux)
            .unwrap();

        let vcvs_unknowns = vcvs
            .bind_unknowns(&[Some(output), None, Some(control), None], vcvs_aux)
            .unwrap();

        let mut pattern_builder = PatternBuilder::with_capacity(
            unknown_allocator.dimension(),
            voltage_source.matrix_entry_count() + vcvs.matrix_entry_count(),
        )
        .unwrap();

        voltage_source
            .request_pattern(&control_source_unknowns, &mut pattern_builder)
            .unwrap();

        vcvs.request_pattern(&vcvs_unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        let states = state_slots(&[]);

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let control_source_inputs = voltage_source
            .bind(&control_source_unknowns, &states, &mut ir_builder)
            .unwrap();

        let vcvs_inputs = vcvs.bind(&vcvs_unknowns, &states, &mut ir_builder).unwrap();

        let ir = ir_builder.finish().unwrap();

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(control_source_inputs.parameter(0).unwrap(), 2.0);
        workspace.set_input(vcvs_inputs.parameter(0).unwrap(), 3.0);

        ir.value_program().execute_static(&mut workspace);

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut matrix = system.values_mut();

            ir.matrix_program().execute(&mut matrix, workspace.values());
        }

        system.factorize().unwrap();

        let mut solution = vec![0.0; unknown_allocator.dimension()];

        ir.rhs_program().execute(&mut solution, workspace.values());
        system.solve_in_place(&mut solution).unwrap();

        assert!((solution[control.index()] - 2.0).abs() < 1.0e-12);
        assert!((solution[output.index()] - 6.0).abs() < 1.0e-12);
    }

    #[test]
    fn capacitor_uses_backward_euler_history_across_ticks() {
        let capacitor = template(PrimitiveElementKind::Capacitor);
        let source = template(PrimitiveElementKind::CurrentSource);

        assert_eq!(capacitor.terminal_count(), 2);
        assert_eq!(capacitor.allocated_unknown_count(), 0);
        assert_eq!(capacitor.parameter_count(), 1);
        assert_eq!(capacitor.state_count(), 1);

        let node = UnknownIndex::new(0);

        let mut unknown_allocator = UnknownAllocator::new(1).unwrap();

        let capacitor_allocated = unknown_allocator
            .allocate(capacitor.allocated_unknown_count())
            .unwrap();

        let source_allocated = unknown_allocator
            .allocate(source.allocated_unknown_count())
            .unwrap();

        let capacitor_unknowns = capacitor
            .bind_unknowns(&[Some(node), None], capacitor_allocated)
            .unwrap();

        let source_unknowns = source
            .bind_unknowns(&[None, Some(node)], source_allocated)
            .unwrap();

        let mut pattern_builder = PatternBuilder::with_capacity(
            unknown_allocator.dimension(),
            capacitor.matrix_entry_count() + source.matrix_entry_count(),
        )
        .unwrap();

        capacitor
            .request_pattern(&capacitor_unknowns, &mut pattern_builder)
            .unwrap();

        source
            .request_pattern(&source_unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        let source_states = state_slots(&[]);

        let capacitor_states = state_slots(&[0]);
        let capacitor_state = capacitor_states.get(0).unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let capacitor_inputs = capacitor
            .bind(&capacitor_unknowns, &capacitor_states, &mut ir_builder)
            .unwrap();

        let source_inputs = source
            .bind(&source_unknowns, &source_states, &mut ir_builder)
            .unwrap();

        let ir = ir_builder.finish().unwrap();

        assert_eq!(ir.state_inputs().len(), 1);
        assert_eq!(ir.state_inputs()[0].0, capacitor_state);
        assert_eq!(ir.solution_inputs().len(), 1);
        assert_eq!(ir.solution_inputs()[0].0, node);

        let timestep_input = ir.timestep_input().unwrap();
        let state_input = ir.state_inputs()[0].1;
        let solution_input = ir.solution_inputs()[0].1;

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(capacitor_inputs.parameter(0).unwrap(), 1.0);
        workspace.set_input(source_inputs.parameter(0).unwrap(), 1.0);
        workspace.set_input(timestep_input, 1.0);

        ir.value_program().execute_static(&mut workspace);

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut matrix = system.values_mut();

            ir.matrix_program().execute(&mut matrix, workspace.values());
        }

        system.factorize().unwrap();

        workspace.set_input(state_input, 0.0);

        ir.value_program().execute_tick(&mut workspace);

        let mut solution = [0.0];

        ir.rhs_program().execute(&mut solution, workspace.values());

        system.solve_in_place(&mut solution).unwrap();

        assert!((solution[node.index()] - 1.0).abs() < 1.0e-12);

        workspace.set_input(solution_input, solution[node.index()]);

        ir.value_program().execute_iteration(&mut workspace);

        let mut next_state = [0.0];

        ir.state_transition()
            .execute(&mut next_state, workspace.values());

        assert!((next_state[capacitor_state.index()] - 1.0).abs() < 1.0e-12);

        workspace.set_input(state_input, next_state[capacitor_state.index()]);

        ir.value_program().execute_tick(&mut workspace);

        let mut solution = [0.0];

        ir.rhs_program().execute(&mut solution, workspace.values());
        system.solve_in_place(&mut solution).unwrap();

        assert!((solution[node.index()] - 2.0).abs() < 1.0e-12);

        workspace.set_input(solution_input, solution[node.index()]);
        ir.value_program().execute_iteration(&mut workspace);

        let mut next_state = [0.0];

        ir.state_transition()
            .execute(&mut next_state, workspace.values());

        assert!((next_state[capacitor_state.index()] - 2.0).abs() < 1.0e-12);
    }

    #[test]
    fn inductor_uses_backward_euler_history_across_ticks() {
        let source = template(PrimitiveElementKind::VoltageSource);
        let inductor = template(PrimitiveElementKind::Inductor);

        assert_eq!(inductor.terminal_count(), 2);
        assert_eq!(inductor.allocated_unknown_count(), 0);
        assert_eq!(inductor.parameter_count(), 1);
        assert_eq!(inductor.state_count(), 1);

        let node = UnknownIndex::new(0);

        let mut unknown_allocator = UnknownAllocator::new(1).unwrap();

        let source_allocated = unknown_allocator
            .allocate(source.allocated_unknown_count())
            .unwrap();

        let inductor_allocated = unknown_allocator
            .allocate(inductor.allocated_unknown_count())
            .unwrap();

        let source_branch = source_allocated.get(0).unwrap();

        let source_unknowns = source
            .bind_unknowns(&[Some(node), None], source_allocated)
            .unwrap();

        let inductor_unknowns = inductor
            .bind_unknowns(&[Some(node), None], inductor_allocated)
            .unwrap();

        let mut pattern_builder = PatternBuilder::with_capacity(
            unknown_allocator.dimension(),
            source.matrix_entry_count() + inductor.matrix_entry_count(),
        )
        .unwrap();

        source
            .request_pattern(&source_unknowns, &mut pattern_builder)
            .unwrap();

        inductor
            .request_pattern(&inductor_unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        let source_states = state_slots(&[]);
        let inductor_states = state_slots(&[0]);
        let inductor_state = inductor_states.get(0).unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let source_inputs = source
            .bind(&source_unknowns, &source_states, &mut ir_builder)
            .unwrap();

        let inductor_inputs = inductor
            .bind(&inductor_unknowns, &inductor_states, &mut ir_builder)
            .unwrap();

        let ir = ir_builder.finish().unwrap();

        assert_eq!(ir.state_inputs().len(), 1);
        assert_eq!(ir.state_inputs()[0].0, inductor_state);
        assert_eq!(ir.solution_inputs().len(), 1);
        assert_eq!(ir.solution_inputs()[0].0, node);

        let timestep_input = ir.timestep_input().unwrap();
        let state_input = ir.state_inputs()[0].1;
        let solution_input = ir.solution_inputs()[0].1;

        let mut workspace = ir.value_program().new_workspace();

        workspace.set_input(source_inputs.parameter(0).unwrap(), 2.0);
        workspace.set_input(inductor_inputs.parameter(0).unwrap(), 2.0);
        workspace.set_input(timestep_input, 0.5);

        ir.value_program().execute_static(&mut workspace);

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut matrix = system.values_mut();

            ir.matrix_program().execute(&mut matrix, workspace.values());
        }

        system.factorize().unwrap();
        workspace.set_input(state_input, 0.0);

        ir.value_program().execute_tick(&mut workspace);

        let mut solution = vec![0.0; unknown_allocator.dimension()];

        ir.rhs_program().execute(&mut solution, workspace.values());

        system.solve_in_place(&mut solution).unwrap();

        assert!((solution[node.index()] - 2.0).abs() < 1.0e-12);
        assert!((solution[source_branch.index()] + 0.5).abs() < 1.0e-12);

        workspace.set_input(solution_input, solution[node.index()]);

        ir.value_program().execute_iteration(&mut workspace);

        let mut next_state = [0.0];

        ir.state_transition()
            .execute(&mut next_state, workspace.values());

        assert!((next_state[inductor_state.index()] - 0.5).abs() < 1.0e-12);

        workspace.set_input(state_input, next_state[inductor_state.index()]);
        ir.value_program().execute_tick(&mut workspace);

        let mut solution = vec![0.0; unknown_allocator.dimension()];

        ir.rhs_program().execute(&mut solution, workspace.values());

        system.solve_in_place(&mut solution).unwrap();

        assert!((solution[node.index()] - 2.0).abs() < 1.0e-12);
        assert!((solution[source_branch.index()] + 1.0).abs() < 1.0e-12);

        workspace.set_input(solution_input, solution[node.index()]);
        ir.value_program().execute_iteration(&mut workspace);

        let mut next_state = [0.0];

        ir.state_transition()
            .execute(&mut next_state, workspace.values());

        assert!((next_state[inductor_state.index()] - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn conductance_compiles_one_device_partition() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::Conductance))
            .unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        assert_eq!(compiled.partition_count(), 1);
        assert_eq!(compiled.state_count(), 0);

        let partition = compiled.partition(DevicePartitionId::new(0)).unwrap();

        assert_eq!(
            partition.definition_terminals(),
            &[TerminalId::new(0), TerminalId::new(1)],
        );

        assert_eq!(partition.template().terminal_count(), 2);
    }

    #[test]
    fn tick_delay_compiles_two_device_partitions() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::TickDelay))
            .unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        assert_eq!(compiled.partition_count(), 2);
        assert_eq!(compiled.state_count(), 1);

        let input = compiled.partition(DevicePartitionId::new(0)).unwrap();

        let output = compiled.partition(DevicePartitionId::new(1)).unwrap();

        assert_eq!(
            input.definition_terminals(),
            &[TerminalId::new(0), TerminalId::new(1)],
        );

        assert_eq!(
            output.definition_terminals(),
            &[TerminalId::new(2), TerminalId::new(3)],
        );

        assert_eq!(input.template().terminal_count(), 2);
        assert_eq!(output.template().terminal_count(), 2);
    }

    #[test]
    fn tick_delay_partitions_share_one_definition_state() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::TickDelay))
            .unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        let input = compiled.partition(DevicePartitionId::new(0)).unwrap();

        let output = compiled.partition(DevicePartitionId::new(1)).unwrap();

        assert_eq!(input.definition_states(), &[DefinitionStateId::new(0)],);
        assert_eq!(output.definition_states(), &[DefinitionStateId::new(0)],);
    }

    #[test]
    fn tick_delay_partitions_split_state_read_and_write() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::TickDelay))
            .unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        let input = compiled.partition(DevicePartitionId::new(0)).unwrap();

        let output = compiled.partition(DevicePartitionId::new(1)).unwrap();

        assert_eq!(input.definition_state_reads(), &[]);
        assert_eq!(
            input.definition_state_writes(),
            &[DefinitionStateId::new(0)],
        );

        assert_eq!(
            output.definition_state_reads(),
            &[DefinitionStateId::new(0)],
        );
        assert_eq!(output.definition_state_writes(), &[]);
    }

    #[test]
    fn tick_delay_output_template_reads_shared_state() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::TickDelay))
            .unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        let output = compiled.partition(DevicePartitionId::new(1)).unwrap();

        assert_eq!(
            output.definition_state_reads(),
            &[DefinitionStateId::new(0)],
        );

        assert_eq!(output.template().state_count(), 1);
        assert_eq!(output.template().allocated_unknown_count(), 1);
    }

    #[test]
    fn tick_delay_input_writes_terminal_voltage_to_shared_state() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::TickDelay))
            .unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        let input = compiled.partition(DevicePartitionId::new(0)).unwrap();

        let template = input.template();

        assert_eq!(template.state_count(), 1);
        assert_eq!(template.allocated_unknown_count(), 0);

        let positive = UnknownIndex::new(0);
        let negative = UnknownIndex::new(1);

        let mut unknown_allocator = UnknownAllocator::new(2).unwrap();

        let allocated = unknown_allocator
            .allocate(template.allocated_unknown_count())
            .unwrap();

        let unknowns = template
            .bind_unknowns(&[Some(positive), Some(negative)], allocated)
            .unwrap();

        let pattern = PatternBuilder::with_capacity(
            unknown_allocator.dimension(),
            template.matrix_entry_count(),
        )
        .unwrap()
        .finish()
        .unwrap();

        let states = state_slots(&[0]);
        let state = states.get(0).unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        template.bind(&unknowns, &states, &mut ir_builder).unwrap();

        let ir = ir_builder.finish().unwrap();

        assert_eq!(ir.solution_inputs().len(), 2);

        let mut workspace = ir.value_program().new_workspace();

        for &(unknown, input) in ir.solution_inputs() {
            let value = if unknown == positive {
                5.0
            } else if unknown == negative {
                2.0
            } else {
                panic!("TickDelay input uses an unexpected unknown");
            };

            workspace.set_input(input, value);
        }

        ir.value_program().execute_iteration(&mut workspace);

        let mut next_state = [0.0];

        ir.state_transition()
            .execute(&mut next_state, workspace.values());

        assert_eq!(state.index(), 0);
        assert!((next_state[state.index()] - 3.0).abs() < 1.0e-12);
    }

    #[test]
    fn conductance_partition_maps_definition_parameter() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::Conductance))
            .unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        let partition = compiled.partition(DevicePartitionId::new(0)).unwrap();

        assert_eq!(partition.definition_parameters(), &[ParameterId::new(0)],);
    }

    #[test]
    fn tick_delay_partition_templates_have_no_runtime_parameters() {
        let registry = DefinitionRegistry::new();

        let definition = registry
            .get(DefinitionId::from(PrimitiveElementKind::TickDelay))
            .unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        assert_eq!(compiled.state_count(), 1);

        assert!(
            compiled
                .partition(DevicePartitionId::new(0))
                .unwrap()
                .definition_parameters()
                .is_empty()
        );

        assert!(
            compiled
                .partition(DevicePartitionId::new(1))
                .unwrap()
                .definition_parameters()
                .is_empty()
        );
    }
    #[test]
    fn stateless_composite_inlines_child_templates() {
        let mut registry = DefinitionRegistry::new();

        let resistance = DefinitionId::from(PrimitiveElementKind::Resistance);

        let resistance_constraint = registry.get(resistance).unwrap().parameters()[0];

        let definition = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);

            let a = builder.add_terminal().unwrap();
            let b = builder.add_terminal().unwrap();
            let middle = builder.add_node().unwrap();

            let r2 = builder.add_parameter(resistance_constraint).unwrap();

            // R1 = 2 ohm literal.
            builder
                .add_element(Element::new(
                    resistance,
                    vec![a, middle],
                    vec![ValueRef::Literal(2.0)],
                ))
                .unwrap();

            // R2 = parent parameter.
            builder
                .add_element(Element::new(
                    resistance,
                    vec![middle, b],
                    vec![ValueRef::Parameter(r2)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        let definition_id = registry.register(definition).unwrap();

        let definition = registry.get(definition_id).unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        assert_eq!(compiled.partition_count(), 1);
        assert_eq!(compiled.state_count(), 0);

        let partition = compiled.partition(DevicePartitionId::new(0)).unwrap();

        assert_eq!(
            partition.definition_terminals(),
            &[TerminalId::new(0), TerminalId::new(1)],
        );

        assert_eq!(partition.definition_parameters(), &[ParameterId::new(0)],);

        let template = partition.template();

        // The internal series node becomes one allocated voltage unknown.
        assert_eq!(template.terminal_count(), 2);
        assert_eq!(template.allocated_unknown_count(), 1);
        assert_eq!(template.parameter_count(), 1);
        assert_eq!(template.state_count(), 0);

        let a_unknown = UnknownIndex::new(0);

        let mut unknown_allocator = UnknownAllocator::new(1).unwrap();

        let allocated = unknown_allocator
            .allocate(template.allocated_unknown_count())
            .unwrap();

        let middle_unknown = allocated.get(0).unwrap();

        let unknowns = template
            .bind_unknowns(&[Some(a_unknown), None], allocated)
            .unwrap();

        let mut pattern_builder = PatternBuilder::new(unknown_allocator.dimension()).unwrap();

        template
            .request_pattern(&unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        let aa = pattern.slot(a_unknown, a_unknown).unwrap();

        let am = pattern.slot(a_unknown, middle_unknown).unwrap();

        let ma = pattern.slot(middle_unknown, a_unknown).unwrap();

        let mm = pattern.slot(middle_unknown, middle_unknown).unwrap();

        let states = state_slots(&[]);

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let inputs = template.bind(&unknowns, &states, &mut ir_builder).unwrap();

        let ir = ir_builder.finish().unwrap();

        let mut workspace = ir.value_program().new_workspace();

        // R2 = 3 ohm.
        workspace.set_input(inputs.parameter(0).unwrap(), 3.0);

        ir.value_program().execute_static(&mut workspace);

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut matrix = system.values_mut();

            ir.matrix_program().execute(&mut matrix, workspace.values());
        }

        let values = system.values();

        let g1 = 1.0 / 2.0;
        let g2 = 1.0 / 3.0;

        assert!((values[aa.index()] - g1).abs() < 1.0e-12);
        assert!((values[am.index()] + g1).abs() < 1.0e-12);
        assert!((values[ma.index()] + g1).abs() < 1.0e-12);

        assert!((values[mm.index()] - (g1 + g2)).abs() < 1.0e-12);
    }

    #[test]
    fn composite_flattens_child_states_in_element_order() {
        let mut registry = DefinitionRegistry::new();

        let definition = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);

            let a = builder.add_terminal().unwrap();
            let b = builder.add_terminal().unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::Resistance.into(),
                    vec![a, b],
                    vec![ValueRef::Literal(1.0)],
                ))
                .unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::Capacitor.into(),
                    vec![a, b],
                    vec![ValueRef::Literal(2.0)],
                ))
                .unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::Inductor.into(),
                    vec![a, b],
                    vec![ValueRef::Literal(3.0)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        assert_eq!(definition.state_count(), 2);

        let id = registry.register(definition).unwrap();

        let compiled = CompiledDefinition::compile(&registry, registry.get(id).unwrap()).unwrap();

        assert_eq!(compiled.state_count(), 2);
        assert_eq!(compiled.partition_count(), 1);

        let partition = compiled.partition(DevicePartitionId::new(0)).unwrap();

        assert_eq!(
            partition.definition_states(),
            &[DefinitionStateId::new(0), DefinitionStateId::new(1),],
        );

        assert_eq!(
            partition.definition_state_reads(),
            &[DefinitionStateId::new(0), DefinitionStateId::new(1),],
        );

        assert_eq!(
            partition.definition_state_writes(),
            &[DefinitionStateId::new(0), DefinitionStateId::new(1),],
        );

        assert_eq!(partition.template().state_count(), 2);
    }

    #[test]
    fn composite_preserves_split_state_access_across_partitions() {
        let mut registry = DefinitionRegistry::new();

        let definition = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);

            let input_positive = builder.add_terminal().unwrap();

            let input_negative = builder.add_terminal().unwrap();

            let output_positive = builder.add_terminal().unwrap();

            let output_negative = builder.add_terminal().unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::TickDelay.into(),
                    vec![
                        input_positive,
                        input_negative,
                        output_positive,
                        output_negative,
                    ],
                    vec![ValueRef::Literal(1.25)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        assert_eq!(definition.state_count(), 1);
        assert_eq!(definition.partition_count(), 2);

        let id = registry.register(definition).unwrap();

        let compiled = CompiledDefinition::compile(&registry, registry.get(id).unwrap()).unwrap();

        assert_eq!(compiled.state_count(), 1);
        assert_eq!(compiled.partition_count(), 2);

        let input = compiled.partition(DevicePartitionId::new(0)).unwrap();

        let output = compiled.partition(DevicePartitionId::new(1)).unwrap();

        assert_eq!(input.definition_states(), &[DefinitionStateId::new(0)],);

        assert!(input.definition_state_reads().is_empty());

        assert_eq!(
            input.definition_state_writes(),
            &[DefinitionStateId::new(0)],
        );

        assert_eq!(output.definition_states(), &[DefinitionStateId::new(0)],);

        assert_eq!(
            output.definition_state_reads(),
            &[DefinitionStateId::new(0)],
        );

        assert!(output.definition_state_writes().is_empty());

        assert_eq!(input.template().state_count(), 1);
        assert_eq!(output.template().state_count(), 1);
    }
}
