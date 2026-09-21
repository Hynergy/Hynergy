use crate::compile::template::{
    CompiledDefinitionTemplate, DefinitionTemplateBuildError, DefinitionTemplateBuilder,
    LocalStateId, LocalUnknownId, LocalValueId,
};
use hynergy_ids::define_id;
use hynergy_model::circuit::{Circuit, NodeId, ValueRef};
use hynergy_model::device::definition::{
    DefinitionObserverId, DefinitionObserverSource, DeviceBody, DeviceDefinition,
    DevicePartitionId, PrimitiveElementKind, TerminalId,
};
use hynergy_model::device::registry::DefinitionRegistry;
use hynergy_model::parameter::ParameterId;
use smallvec::SmallVec;
use thiserror::Error;

define_id!(DefinitionStateId: u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DefinitionStateInitializer {
    Literal(f64),
    Parameter(ParameterId),
}

#[derive(Debug)]
pub(crate) struct CompiledDefinition {
    partitions: Box<[CompiledPartitionTemplate]>,
    state_initializers: Box<[DefinitionStateInitializer]>,
}

impl CompiledDefinition {
    fn new(
        partitions: Box<[CompiledPartitionTemplate]>,
        state_initializers: Box<[DefinitionStateInitializer]>,
    ) -> Result<Self, DefinitionCompileError> {
        let state_count = state_initializers.len();

        let mut state_writers = vec![None; state_count];

        for (partition_index, partition) in partitions.iter().enumerate() {
            let partition_id = DevicePartitionId::new(
                u16::try_from(partition_index)
                    .expect("compiled partition index must fit DevicePartitionId"),
            );

            for &state in partition.definition_state_writes() {
                let owner = state_writers
                    .get_mut(state.index())
                    .expect("compiled state write must reference a definition state");

                match *owner {
                    None => {
                        *owner = Some(partition_id);
                    }

                    Some(first) if first == partition_id => {}

                    Some(first) => {
                        return Err(DefinitionCompileError::MultipleStateWriterPartitions {
                            state,
                            first,
                            second: partition_id,
                        });
                    }
                }
            }
        }

        if let Some(index) = state_writers.iter().position(Option::is_none) {
            return Err(DefinitionCompileError::MissingStateWriter {
                state: DefinitionStateId::new(
                    u32::try_from(index)
                        .expect("definition state index must fit DefinitionStateId"),
                ),
            });
        }

        Ok(Self {
            partitions,
            state_initializers,
        })
    }

    pub(crate) fn compile(
        registry: &DefinitionRegistry,
        definition: &DeviceDefinition,
    ) -> Result<Self, DefinitionCompileError> {
        let (partitions, state_initializers) = match definition.body() {
            DeviceBody::Primitive(PrimitiveElementKind::TickDelay) => (
                compile_tick_delay_partitions(definition)?,
                primitive_state_initializers(PrimitiveElementKind::TickDelay),
            ),

            DeviceBody::Primitive(kind) => {
                let template = CompiledDefinitionTemplate::compile(definition)?;

                debug_assert_eq!(
                    definition.partition_count(),
                    1,
                    "supported primitive must have one partition",
                );

                let partition = DevicePartitionId::new(0);

                let definition_parameters = definition_parameters(definition);

                let definition_states = definition_states(definition);

                let definition_observers =
                    definition_observers_for_partition(definition, partition);

                debug_assert_eq!(
                    template.output_count(),
                    definition_observers.len(),
                    "primitive template outputs must match primitive observers",
                );

                let partitions = vec![CompiledPartitionTemplate {
                    definition_terminals: definition_terminals_for_partition(definition, partition),
                    definition_parameters,
                    definition_state_reads: definition_states.clone(),
                    definition_state_writes: definition_states.clone(),
                    definition_states,
                    definition_observers,
                    template,
                }]
                .into_boxed_slice();

                (partitions, primitive_state_initializers(*kind))
            }

            DeviceBody::Composite(circuit) => compile_composite(registry, definition, circuit)?,
        };

        debug_assert_eq!(
            partitions.len(),
            definition.partition_count(),
            "compiled partition count must match definition",
        );

        debug_assert_eq!(
            state_initializers.len(),
            definition.state_count(),
            "compiled state initializers must cover the definition state space",
        );

        Self::new(partitions, state_initializers)
    }

    #[inline]
    pub(crate) fn partition(
        &self,
        partition: DevicePartitionId,
    ) -> Option<&CompiledPartitionTemplate> {
        self.partitions.get(partition.index())
    }

    #[inline]
    pub(crate) fn partition_count(&self) -> usize {
        self.partitions.len()
    }

    #[inline]
    pub(crate) fn state_count(&self) -> usize {
        self.state_initializers.len()
    }

    #[inline]
    pub(crate) fn state_initializers(&self) -> &[DefinitionStateInitializer] {
        &self.state_initializers
    }
}

fn primitive_state_initializers(kind: PrimitiveElementKind) -> Box<[DefinitionStateInitializer]> {
    match kind {
        PrimitiveElementKind::Capacitor
        | PrimitiveElementKind::Inductor
        | PrimitiveElementKind::SchmittBuffer => {
            vec![DefinitionStateInitializer::Literal(0.0)].into_boxed_slice()
        }

        PrimitiveElementKind::TickDelay => {
            vec![DefinitionStateInitializer::Parameter(ParameterId::new(0))].into_boxed_slice()
        }

        _ => Vec::new().into_boxed_slice(),
    }
}

fn voltage_difference(
    builder: &mut DefinitionTemplateBuilder,
    positive: LocalUnknownId,
    negative: LocalUnknownId,
) -> Result<LocalValueId, DefinitionTemplateBuildError> {
    let positive = builder.unknown_value(positive)?;
    let negative = builder.unknown_value(negative)?;

    builder.sub(positive, negative)
}

#[derive(Debug)]
pub(crate) struct CompiledPartitionTemplate {
    definition_terminals: SmallVec<[TerminalId; 4]>,
    definition_parameters: SmallVec<[ParameterId; 4]>,
    definition_states: SmallVec<[DefinitionStateId; 4]>,
    definition_state_reads: SmallVec<[DefinitionStateId; 4]>,
    definition_state_writes: SmallVec<[DefinitionStateId; 4]>,
    definition_observers: SmallVec<[DefinitionObserverId; 4]>,
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

    #[inline]
    pub(crate) fn definition_observers(&self) -> &[DefinitionObserverId] {
        &self.definition_observers
    }
}

fn definition_parameters(definition: &DeviceDefinition) -> SmallVec<[ParameterId; 4]> {
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

fn definition_observers_for_partition(
    definition: &DeviceDefinition,
    partition: DevicePartitionId,
) -> SmallVec<[DefinitionObserverId; 4]> {
    definition
        .observers()
        .iter()
        .enumerate()
        .filter(|&(_, observer)| observer.partition() == partition)
        .map(|(index, _)| {
            DefinitionObserverId::new(
                u32::try_from(index).expect("observer index must fit DefinitionObserverId"),
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
        let definition_observers = definition_observers_for_partition(definition, partition);

        let mut builder = DefinitionTemplateBuilder::default();

        match partition_index {
            0 => {
                debug_assert_eq!(definition_terminals.len(), 2,);

                let positive = builder.terminal_voltage()?;
                let negative = builder.terminal_voltage()?;
                let next_state = builder.write_only_state()?;
                let voltage = voltage_difference(&mut builder, positive, negative)?;

                builder.write_state_id(next_state, voltage)?;
                builder.output(voltage)?;
            }
            1 => {
                debug_assert_eq!(definition_terminals.len(), 2,);

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

                let voltage = voltage_difference(&mut builder, positive, negative)?;
                let current = builder.unknown_value(branch_current)?;

                builder.output(voltage)?;
                builder.output(current)?;
            }
            _ => {
                unreachable!("TickDelay must have exactly two partitions",)
            }
        }

        debug_assert_eq!(
            builder.output_count(),
            definition_observers.len(),
            "TickDelay partition outputs must match observers",
        );

        let (definition_state_reads, definition_state_writes) = match partition_index {
            0 => (SmallVec::new(), SmallVec::from_slice(&[state])),
            1 => (SmallVec::from_slice(&[state]), SmallVec::new()),
            _ => {
                unreachable!("TickDelay must have exactly two partitions",)
            }
        };

        partitions.push(CompiledPartitionTemplate {
            definition_terminals,
            definition_parameters: SmallVec::new(),
            definition_states: SmallVec::from_slice(&[state]),
            definition_state_reads,
            definition_state_writes,
            definition_observers,
            template: builder.finish()?,
        });
    }

    Ok(partitions.into_boxed_slice())
}

fn definition_terminals_for_partition(
    definition: &DeviceDefinition,
    partition: DevicePartitionId,
) -> SmallVec<[TerminalId; 4]> {
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

#[derive(Debug, Clone, Copy)]
struct ControlledConductanceValues {
    output_voltage: LocalValueId,
    control_voltage: LocalValueId,
    output_current: LocalValueId,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionCompileError {
    #[error("primitive {kind:?} is not yet supported by the compiler")]
    UnsupportedPrimitive { kind: PrimitiveElementKind },

    #[error("composite definitions are not yet supported by the compiler")]
    CompositeNotYetSupported,

    #[error(transparent)]
    Template(#[from] DefinitionTemplateBuildError),

    #[error("definition state {state:?} has multiple writer partitions: {first:?} and {second:?}")]
    MultipleStateWriterPartitions {
        state: DefinitionStateId,
        first: DevicePartitionId,
        second: DevicePartitionId,
    },

    #[error("definition state {state:?} has no next-state writer")]
    MissingStateWriter { state: DefinitionStateId },
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
            let positive = builder.terminal_voltage()?;
            let negative = builder.terminal_voltage()?;

            let conductance = builder.parameter()?;

            stamp_conductance(&mut builder, positive, negative, conductance);

            let voltage = voltage_difference(&mut builder, positive, negative)?;
            let current = builder.mul(conductance, voltage)?;

            builder.output(voltage)?;
            builder.output(current)?;
        }

        PrimitiveElementKind::Diode => {
            let anode = builder.terminal_voltage()?;
            let cathode = builder.terminal_voltage()?;

            let g_max = builder.parameter()?;
            let g_min = builder.parameter()?;

            let voltage = voltage_difference(&mut builder, anode, cathode)?;
            let zero = builder.constant(0.0)?;
            let one = builder.constant(1.0)?;

            let reverse_or_zero = builder.less_equal(voltage, zero)?;
            let forward = builder.sub(one, reverse_or_zero)?;

            builder.require_iteration_stability(forward);

            let conductance = switched_conductance(&mut builder, forward, g_min, g_max)?;

            stamp_conductance(&mut builder, anode, cathode, conductance);

            let current = builder.mul(conductance, voltage)?;

            builder.output(voltage)?;
            builder.output(current)?;
        }

        PrimitiveElementKind::SchmittBuffer => {
            compile_schmitt_buffer(&mut builder)?;
        }

        PrimitiveElementKind::Not
        | PrimitiveElementKind::And
        | PrimitiveElementKind::Nand
        | PrimitiveElementKind::Or
        | PrimitiveElementKind::Nor => {
            compile_logic_gate(&mut builder, kind)?;
        }

        PrimitiveElementKind::Resistance => {
            let positive = builder.terminal_voltage()?;
            let negative = builder.terminal_voltage()?;

            let resistance = builder.parameter()?;
            let one = builder.constant(1.0)?;

            let conductance = builder.div(one, resistance)?;

            stamp_conductance(&mut builder, positive, negative, conductance);

            let voltage = voltage_difference(&mut builder, positive, negative)?;
            let current = builder.mul(conductance, voltage)?;

            builder.output(voltage)?;
            builder.output(current)?;
        }

        PrimitiveElementKind::CurrentSource => {
            let from = builder.terminal_voltage()?;
            let to = builder.terminal_voltage()?;

            let current = builder.parameter()?;

            stamp_current_source(&mut builder, from, to, current);

            let voltage = voltage_difference(&mut builder, from, to)?;

            builder.output(voltage)?;
            builder.output(current)?;
        }

        PrimitiveElementKind::VoltageSource => {
            let positive = builder.terminal_voltage()?;
            let negative = builder.terminal_voltage()?;

            let branch_current = builder.branch_current_unknown()?;

            let voltage_parameter = builder.parameter()?;

            stamp_voltage_source(
                &mut builder,
                positive,
                negative,
                branch_current,
                voltage_parameter,
            )?;

            let voltage = voltage_difference(&mut builder, positive, negative)?;

            let current = builder.unknown_value(branch_current)?;

            builder.output(voltage)?;
            builder.output(current)?;
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

            let output_voltage =
                voltage_difference(&mut builder, output_positive, output_negative)?;

            let control_voltage =
                voltage_difference(&mut builder, control_positive, control_negative)?;

            let output_current = builder.mul(transconductance, control_voltage)?;

            builder.output(output_voltage)?;
            builder.output(control_voltage)?;
            builder.output(output_current)?;
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

            let output_voltage =
                voltage_difference(&mut builder, output_positive, output_negative)?;

            let control_voltage =
                voltage_difference(&mut builder, control_positive, control_negative)?;

            let output_current = builder.unknown_value(branch_current)?;

            builder.output(output_voltage)?;
            builder.output(control_voltage)?;
            builder.output(output_current)?;
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

            let voltage = voltage_difference(&mut builder, positive, negative)?;
            let voltage_delta = builder.sub(voltage, previous_voltage.value())?;
            let current = builder.mul(conductance, voltage_delta)?;

            builder.write_state(previous_voltage, voltage)?;

            builder.output(voltage)?;
            builder.output(current)?;
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

            let voltage = voltage_difference(&mut builder, positive, negative)?;

            let current_delta = builder.mul(conductance, voltage)?;

            let current = builder.add(previous_current.value(), current_delta)?;

            builder.write_state(previous_current, current)?;

            builder.output(voltage)?;
            builder.output(current)?;
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

            let values = stamp_voltage_controlled_conductance(
                &mut builder,
                output_positive,
                output_negative,
                control,
                parameters,
            )?;

            builder.output(values.output_voltage)?;
            builder.output(values.control_voltage)?;
            builder.output(values.output_current)?;
        }

        PrimitiveElementKind::VoltageControlledSwitch => {
            let output_positive = builder.terminal_voltage()?;

            let output_negative = builder.terminal_voltage()?;

            let control_positive = builder.terminal_voltage()?;

            let control_negative = builder.terminal_voltage()?;

            let parameters = ControlledSwitchParameters {
                threshold: builder.parameter()?,
                g_max: builder.parameter()?,
                g_min: builder.parameter()?,
            };

            let values = stamp_voltage_controlled_switch(
                &mut builder,
                output_positive,
                output_negative,
                control_positive,
                control_negative,
                parameters,
            )?;

            builder.output(values.output_voltage)?;
            builder.output(values.control_voltage)?;
            builder.output(values.output_current)?;
        }

        PrimitiveElementKind::TickDelay => {
            return Err(DefinitionCompileError::UnsupportedPrimitive { kind });
        }
    }

    Ok(builder.finish()?)
}

type CompiledComposite = (
    Box<[CompiledPartitionTemplate]>,
    Box<[DefinitionStateInitializer]>,
);

fn compile_composite(
    registry: &DefinitionRegistry,
    definition: &DeviceDefinition,
    circuit: &Circuit,
) -> Result<CompiledComposite, DefinitionCompileError> {
    let mut compiled_children = Vec::with_capacity(circuit.elements().len());

    for element in circuit.elements() {
        let child_definition = registry
            .get(element.definition())
            .expect("registered composite child definition must exist");

        compiled_children.push(CompiledDefinition::compile(registry, child_definition)?);
    }

    let mut state_initializers = Vec::with_capacity(definition.state_count());

    for (element, child) in circuit.elements().iter().zip(&compiled_children) {
        for &initializer in child.state_initializers() {
            let initializer = match initializer {
                DefinitionStateInitializer::Literal(value) => {
                    DefinitionStateInitializer::Literal(value)
                }

                DefinitionStateInitializer::Parameter(parameter) => {
                    match element.parameters()[parameter.index()] {
                        ValueRef::Literal(value) => DefinitionStateInitializer::Literal(value),

                        ValueRef::Parameter(parameter) => {
                            DefinitionStateInitializer::Parameter(parameter)
                        }
                    }
                }
            };

            state_initializers.push(initializer);
        }
    }

    debug_assert_eq!(
        state_initializers.len(),
        definition.state_count(),
        "flattened child state initializers must cover composite state space",
    );

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

    Ok((
        partitions.into_boxed_slice(),
        state_initializers.into_boxed_slice(),
    ))
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

        let previous = node_unknowns[node.index()].replace(unknown);
        debug_assert!(
            previous.is_none(),
            "composite terminal nodes must be unique",
        );
    }

    let mut parameter_values = vec![None; definition.parameters().len()];
    let mut definition_parameters = SmallVec::<[ParameterId; 4]>::new();

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

    let mut forwarded_observer_values = vec![None; definition.observers().len()];

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

            let mut terminals = SmallVec::<[LocalUnknownId; 4]>::new();

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

            let child_outputs = child_partition.template().instantiate_into(
                &mut builder,
                &terminals,
                &parameters,
                &states,
            )?;

            debug_assert_eq!(
                child_outputs.len(),
                child_partition.definition_observers().len(),
            );

            for (&child_observer, &output) in child_partition
                .definition_observers()
                .iter()
                .zip(&child_outputs)
            {
                for (parent_index, parent_observer) in definition.observers().iter().enumerate() {
                    if parent_observer.partition() != parent_partition {
                        continue;
                    }

                    let DefinitionObserverSource::Child { element, observer } =
                        parent_observer.source()
                    else {
                        continue;
                    };

                    if element.index() != element_index || observer != child_observer {
                        continue;
                    }

                    let previous = forwarded_observer_values[parent_index].replace(output);
                    debug_assert!(
                        previous.is_none(),
                        "forwarded observer must have exactly one child output",
                    );
                }
            }
        }

        child_partition_offset += child.partition_count();
    }

    debug_assert_eq!(
        child_partition_offset,
        element_partitions.len(),
        "element partition layout must cover every child partition",
    );

    let definition_observers = compile_partition_observers(
        definition,
        parent_partition,
        &node_unknowns,
        &forwarded_observer_values,
        &mut builder,
    )?;

    Ok(CompiledPartitionTemplate {
        definition_terminals,
        definition_parameters,
        definition_states,
        definition_state_reads,
        definition_state_writes,
        definition_observers,
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
    definition_parameters: &mut SmallVec<[ParameterId; 4]>,
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
fn switched_conductance(
    builder: &mut DefinitionTemplateBuilder,
    mode: LocalValueId,
    g_min: LocalValueId,
    g_max: LocalValueId,
) -> Result<LocalValueId, DefinitionTemplateBuildError> {
    let conductance_range = builder.sub(g_max, g_min)?;
    let mode_conductance = builder.mul(mode, conductance_range)?;

    builder.add(g_min, mode_conductance)
}

fn compile_schmitt_buffer(
    builder: &mut DefinitionTemplateBuilder,
) -> Result<(), DefinitionTemplateBuildError> {
    let output = builder.terminal_voltage()?;
    let vdd = builder.terminal_voltage()?;
    let vss = builder.terminal_voltage()?;
    let input = builder.terminal_voltage()?;

    let threshold = builder.parameter()?;
    let hysteresis = builder.parameter()?;
    let g_max = builder.parameter()?;
    let g_min = builder.parameter()?;

    let state = builder.state()?;
    let input_voltage = voltage_difference(builder, input, vss)?;
    let output_high =
        hysteretic_binary_mode(builder, input_voltage, state.value(), threshold, hysteresis)?;

    builder.write_state(state, output_high)?;

    let conductance_range = builder.sub(g_max, g_min)?;
    let high_delta = builder.mul(output_high, conductance_range)?;
    let pull_up = builder.add(g_min, high_delta)?;
    let pull_down = builder.sub(g_max, high_delta)?;

    stamp_conductance(builder, vdd, output, pull_up);
    stamp_conductance(builder, output, vss, pull_down);

    let output_voltage = voltage_difference(builder, output, vss)?;
    let supply_branch_voltage = voltage_difference(builder, vdd, output)?;
    let supply_current = builder.mul(pull_up, supply_branch_voltage)?;

    builder.output(output_voltage)?;
    builder.output(input_voltage)?;
    builder.output(supply_current)?;

    Ok(())
}

fn hysteretic_binary_mode(
    builder: &mut DefinitionTemplateBuilder,
    input_voltage: LocalValueId,
    initial_mode: LocalValueId,
    threshold: LocalValueId,
    hysteresis: LocalValueId,
) -> Result<LocalValueId, DefinitionTemplateBuildError> {
    let zero = builder.constant(0.0)?;
    let half = builder.constant(0.5)?;
    let one = builder.constant(1.0)?;

    let half_hysteresis = builder.mul(hysteresis, half)?;
    let lower = builder.sub(threshold, half_hysteresis)?;
    let upper = builder.add(threshold, half_hysteresis)?;

    let mode = builder.iteration_latch(initial_mode)?;
    let current_mode = mode.value();

    let turn_on = builder.less_equal(upper, input_voltage)?;
    let turn_off = builder.less_equal(input_voltage, lower)?;
    let off = builder.sub(one, current_mode)?;
    let remain_on = builder.sub(one, turn_off)?;
    let off_to_on = builder.mul(off, turn_on)?;
    let on_to_on = builder.mul(current_mode, remain_on)?;
    let hysteretic_mode = builder.add(off_to_on, on_to_on)?;

    let zero_hysteresis = builder.less_equal(hysteresis, zero)?;
    let direct_mode = builder.less_equal(threshold, input_voltage)?;
    let use_hysteresis = builder.sub(one, zero_hysteresis)?;
    let hysteretic_part = builder.mul(use_hysteresis, hysteretic_mode)?;
    let direct_part = builder.mul(zero_hysteresis, direct_mode)?;
    let next_mode = builder.add(hysteretic_part, direct_part)?;

    builder.update_iteration_latch(mode, next_mode);
    builder.require_iteration_stability(next_mode);

    Ok(next_mode)
}

fn compile_logic_gate(
    builder: &mut DefinitionTemplateBuilder,
    kind: PrimitiveElementKind,
) -> Result<(), DefinitionTemplateBuildError> {
    let output = builder.terminal_voltage()?;
    let vdd = builder.terminal_voltage()?;
    let vss = builder.terminal_voltage()?;
    let input_a = builder.terminal_voltage()?;

    let input_b = match kind {
        PrimitiveElementKind::Not => None,
        PrimitiveElementKind::And
        | PrimitiveElementKind::Nand
        | PrimitiveElementKind::Or
        | PrimitiveElementKind::Nor => Some(builder.terminal_voltage()?),
        _ => unreachable!("compile_logic_gate called for a non-logic primitive"),
    };

    let threshold = builder.parameter()?;
    let g_max = builder.parameter()?;
    let g_min = builder.parameter()?;

    let one = builder.constant(1.0)?;

    let input_a_voltage = voltage_difference(builder, input_a, vss)?;
    let input_a_high = builder.less_equal(threshold, input_a_voltage)?;

    let (input_b_voltage, input_b_high) = if let Some(input_b) = input_b {
        let voltage = voltage_difference(builder, input_b, vss)?;
        let high = builder.less_equal(threshold, voltage)?;

        (Some(voltage), Some(high))
    } else {
        (None, None)
    };

    let output_high = match kind {
        PrimitiveElementKind::Not => builder.sub(one, input_a_high)?,

        PrimitiveElementKind::And => builder.mul(
            input_a_high,
            input_b_high.expect("binary gate must have input B"),
        )?,

        PrimitiveElementKind::Nand => {
            let both = builder.mul(
                input_a_high,
                input_b_high.expect("binary gate must have input B"),
            )?;

            builder.sub(one, both)?
        }

        PrimitiveElementKind::Or => {
            let input_b_high = input_b_high.expect("binary gate must have input B");
            let either_sum = builder.add(input_a_high, input_b_high)?;
            let both = builder.mul(input_a_high, input_b_high)?;

            builder.sub(either_sum, both)?
        }

        PrimitiveElementKind::Nor => {
            let input_b_high = input_b_high.expect("binary gate must have input B");
            let input_a_low = builder.sub(one, input_a_high)?;
            let input_b_low = builder.sub(one, input_b_high)?;

            builder.mul(input_a_low, input_b_low)?
        }

        _ => unreachable!("compile_logic_gate called for a non-logic primitive"),
    };

    builder.require_iteration_stability(output_high);

    // Complementary finite-conductance output stage:
    // HIGH -> pull-up = G_max, pull-down = G_min
    // LOW  -> pull-up = G_min, pull-down = G_max
    let conductance_range = builder.sub(g_max, g_min)?;
    let high_delta = builder.mul(output_high, conductance_range)?;
    let pull_up = builder.add(g_min, high_delta)?;
    let pull_down = builder.sub(g_max, high_delta)?;

    stamp_conductance(builder, vdd, output, pull_up);
    stamp_conductance(builder, output, vss, pull_down);

    let output_voltage = voltage_difference(builder, output, vss)?;
    let supply_branch_voltage = voltage_difference(builder, vdd, output)?;
    let supply_current = builder.mul(pull_up, supply_branch_voltage)?;

    builder.output(output_voltage)?;
    builder.output(input_a_voltage)?;

    if let Some(input_b_voltage) = input_b_voltage {
        builder.output(input_b_voltage)?;
    }

    builder.output(supply_current)?;

    Ok(())
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
) -> Result<ControlledConductanceValues, DefinitionTemplateBuildError> {
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

    let output_current = builder.mul(conductance, output_voltage)?;

    Ok(ControlledConductanceValues {
        output_voltage,
        control_voltage,
        output_current,
    })
}

fn stamp_voltage_controlled_switch(
    builder: &mut DefinitionTemplateBuilder,
    output_positive: LocalUnknownId,
    output_negative: LocalUnknownId,
    control_positive: LocalUnknownId,
    control_negative: LocalUnknownId,
    parameters: ControlledSwitchParameters,
) -> Result<ControlledSwitchValues, DefinitionTemplateBuildError> {
    let ControlledSwitchParameters {
        threshold,
        g_max,
        g_min,
    } = parameters;

    let control_positive_voltage = builder.unknown_value(control_positive)?;
    let control_negative_voltage = builder.unknown_value(control_negative)?;
    let control_voltage = builder.sub(control_positive_voltage, control_negative_voltage)?;
    let mode = builder.less_equal(threshold, control_voltage)?;

    builder.require_iteration_stability(mode);

    let conductance = switched_conductance(builder, mode, g_min, g_max)?;

    stamp_conductance(builder, output_positive, output_negative, conductance);

    let output_voltage = voltage_difference(builder, output_positive, output_negative)?;
    let output_current = builder.mul(conductance, output_voltage)?;

    Ok(ControlledSwitchValues {
        output_voltage,
        control_voltage,
        output_current,
    })
}

fn compile_partition_observers(
    definition: &DeviceDefinition,
    partition: DevicePartitionId,
    node_unknowns: &[Option<LocalUnknownId>],
    forwarded_values: &[Option<LocalValueId>],
    builder: &mut DefinitionTemplateBuilder,
) -> Result<SmallVec<[DefinitionObserverId; 4]>, DefinitionTemplateBuildError> {
    debug_assert_eq!(forwarded_values.len(), definition.observers().len());

    let mut compiled = SmallVec::new();

    for (index, observer) in definition.observers().iter().enumerate() {
        if observer.partition() != partition {
            continue;
        }

        let value = match observer.source() {
            DefinitionObserverSource::Voltage { positive, negative } => {
                let positive = node_unknowns[positive.index()]
                    .expect("observer node must belong to its compiled partition");
                let negative = node_unknowns[negative.index()]
                    .expect("observer node must belong to its compiled partition");

                let positive = builder.unknown_value(positive)?;
                let negative = builder.unknown_value(negative)?;

                builder.sub(positive, negative)?
            }

            DefinitionObserverSource::Current { .. } => {
                unreachable!("direct current observers are only emitted by primitive compilation",)
            }

            DefinitionObserverSource::Child { .. } => forwarded_values[index]
                .expect("forwarded child observer must have a compiled child output"),
        };

        builder.output(value)?;
        compiled.push(DefinitionObserverId::new(
            u32::try_from(index).expect("validated observer index must fit DefinitionObserverId"),
        ));
    }

    Ok(compiled)
}

#[derive(Debug, Clone, Copy)]
struct ControlledSwitchValues {
    output_voltage: LocalValueId,
    control_voltage: LocalValueId,
    output_current: LocalValueId,
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
    use hynergy_model::parameter::{ParameterConstraints, ParameterId};

    fn state_slots(indices: &[u32]) -> BoundStateSlots {
        BoundStateSlots::new(indices.iter().copied().map(StateSlot::new).collect())
    }

    fn template(kind: PrimitiveElementKind) -> CompiledDefinitionTemplate {
        let registry = DefinitionRegistry::new();

        let definition = registry.get(DefinitionId::from(kind)).unwrap();

        CompiledDefinitionTemplate::compile(definition).unwrap()
    }

    fn assert_compiled_primitive_observers(kind: PrimitiveElementKind, expected: &[&[u32]]) {
        let registry = DefinitionRegistry::new();
        let definition = registry.get(DefinitionId::from(kind)).unwrap();
        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

        assert_eq!(compiled.partition_count(), expected.len(), "{kind:?}",);

        for (partition_index, &expected_observers) in expected.iter().enumerate() {
            let partition = compiled
                .partition(DevicePartitionId::new(partition_index as u16))
                .unwrap();

            let actual = partition
                .definition_observers()
                .iter()
                .map(|observer| observer.id())
                .collect::<Vec<_>>();

            assert_eq!(
                actual, expected_observers,
                "{kind:?} partition {partition_index}",
            );

            assert_eq!(
                partition.template().output_count(),
                expected_observers.len(),
                "{kind:?} partition {partition_index}",
            );
        }
    }

    #[test]
    fn stateful_primitives_have_complete_state_writer_coverage() {
        let registry = DefinitionRegistry::new();

        for kind in [
            PrimitiveElementKind::Capacitor,
            PrimitiveElementKind::Inductor,
            PrimitiveElementKind::TickDelay,
            PrimitiveElementKind::SchmittBuffer,
        ] {
            let definition = registry.get(DefinitionId::from(kind)).unwrap();

            assert!(
                definition.state_count() > 0,
                "{kind:?} should remain stateful",
            );

            let compiled = CompiledDefinition::compile(&registry, definition)
                .unwrap_or_else(|error| panic!("{kind:?} failed to compile: {error}"));

            assert_eq!(compiled.state_count(), definition.state_count(), "{kind:?}",);
        }
    }

    #[test]
    fn composite_state_flattening_preserves_complete_writer_coverage() {
        let registry = DefinitionRegistry::new();

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
                    vec![ValueRef::Literal(0.0)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        assert_eq!(definition.state_count(), 1);

        let compiled = CompiledDefinition::compile(&registry, &definition).unwrap();

        assert_eq!(compiled.state_count(), 1);
    }

    fn partition_with_state_writes(states: &[DefinitionStateId]) -> CompiledPartitionTemplate {
        CompiledPartitionTemplate {
            definition_terminals: SmallVec::new(),
            definition_parameters: SmallVec::new(),
            definition_states: SmallVec::from_slice(states),
            definition_state_reads: SmallVec::new(),
            definition_state_writes: SmallVec::from_slice(states),
            definition_observers: SmallVec::new(),
            template: DefinitionTemplateBuilder::default().finish().unwrap(),
        }
    }

    #[test]
    fn compiled_definition_rejects_multiple_writer_partitions_for_state() {
        let state = DefinitionStateId::new(0);

        let error = CompiledDefinition::new(
            vec![
                partition_with_state_writes(&[state]),
                partition_with_state_writes(&[state]),
            ]
            .into_boxed_slice(),
            vec![DefinitionStateInitializer::Literal(0.0)].into_boxed_slice(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            DefinitionCompileError::MultipleStateWriterPartitions {
                state,
                first: DevicePartitionId::new(0),
                second: DevicePartitionId::new(1),
            },
        );
    }

    #[test]
    fn compiled_definition_rejects_missing_state_writer() {
        let state = DefinitionStateId::new(0);

        let error = CompiledDefinition::new(
            Vec::new().into_boxed_slice(),
            vec![DefinitionStateInitializer::Literal(0.0)].into_boxed_slice(),
        )
        .unwrap_err();

        assert_eq!(error, DefinitionCompileError::MissingStateWriter { state },);
    }

    #[test]
    fn primitive_observers_become_partition_template_outputs() {
        for kind in [
            PrimitiveElementKind::Resistance,
            PrimitiveElementKind::Conductance,
            PrimitiveElementKind::VoltageSource,
            PrimitiveElementKind::CurrentSource,
            PrimitiveElementKind::Capacitor,
            PrimitiveElementKind::Inductor,
            PrimitiveElementKind::Diode,
        ] {
            assert_compiled_primitive_observers(kind, &[&[0, 1]]);
        }

        for kind in [
            PrimitiveElementKind::VoltageControlledCurrentSource,
            PrimitiveElementKind::VoltageControlledVoltageSource,
            PrimitiveElementKind::VoltageControlledSwitch,
            PrimitiveElementKind::VoltageControlledConductance,
            PrimitiveElementKind::Not,
            PrimitiveElementKind::SchmittBuffer,
        ] {
            assert_compiled_primitive_observers(kind, &[&[0, 1, 2]]);
        }

        for kind in [
            PrimitiveElementKind::And,
            PrimitiveElementKind::Nand,
            PrimitiveElementKind::Or,
            PrimitiveElementKind::Nor,
        ] {
            assert_compiled_primitive_observers(kind, &[&[0, 1, 2, 3]]);
        }

        assert_compiled_primitive_observers(PrimitiveElementKind::TickDelay, &[&[0], &[1, 2]]);
    }

    #[test]
    fn logic_gate_templates_are_stateless_and_need_no_auxiliary_unknowns() {
        for kind in [
            PrimitiveElementKind::Not,
            PrimitiveElementKind::And,
            PrimitiveElementKind::Nand,
            PrimitiveElementKind::Or,
            PrimitiveElementKind::Nor,
        ] {
            let gate = template(kind);

            assert_eq!(
                gate.terminal_count(),
                if kind == PrimitiveElementKind::Not {
                    4
                } else {
                    5
                },
            );
            assert_eq!(gate.allocated_unknown_count(), 0, "{kind:?}");
            assert_eq!(gate.parameter_count(), 3, "{kind:?}");
            assert_eq!(gate.state_count(), 0, "{kind:?}");
        }
    }

    #[test]
    fn voltage_controlled_switch_template_is_stateless() {
        let switch = template(PrimitiveElementKind::VoltageControlledSwitch);

        assert_eq!(switch.terminal_count(), 4);
        assert_eq!(switch.allocated_unknown_count(), 0);
        assert_eq!(switch.parameter_count(), 3);
        assert_eq!(switch.state_count(), 0);
    }

    #[test]
    fn schmitt_buffer_template_has_one_persistent_state() {
        let buffer = template(PrimitiveElementKind::SchmittBuffer);

        assert_eq!(buffer.terminal_count(), 4);
        assert_eq!(buffer.allocated_unknown_count(), 0);
        assert_eq!(buffer.parameter_count(), 4);
        assert_eq!(buffer.state_count(), 1);
    }

    #[test]
    fn composite_direct_voltage_observer_becomes_template_output() {
        let mut registry = DefinitionRegistry::new();

        let definition = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);

            let positive = builder.add_terminal().unwrap();
            let negative = builder.add_terminal().unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::Conductance.into(),
                    vec![positive, negative],
                    vec![ValueRef::Literal(1.0)],
                ))
                .unwrap();

            builder.add_voltage_observer(positive, negative).unwrap();

            builder.build_definition().unwrap()
        };

        assert_eq!(definition.observers().len(), 1);
        assert_eq!(
            definition.observers()[0].partition(),
            DevicePartitionId::new(0),
        );

        let definition_id = registry.register(definition).unwrap();
        let definition = registry.get(definition_id).unwrap();

        let compiled = CompiledDefinition::compile(&registry, definition).unwrap();
        let partition = compiled.partition(DevicePartitionId::new(0)).unwrap();

        assert_eq!(partition.template().output_count(), 1);

        assert_eq!(
            partition.definition_observers(),
            &[DefinitionObserverId::new(0)],
        );

        assert_eq!(partition.template().output_count(), 1);
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
        assert_eq!(ir.solution_inputs().len(), 2);

        assert!(
            ir.solution_inputs()
                .iter()
                .any(|&(unknown, _)| unknown == node),
        );

        assert!(
            ir.solution_inputs()
                .iter()
                .any(|&(unknown, _)| unknown == source_branch),
        );

        let timestep_input = ir.timestep_input().unwrap();
        let state_input = ir.state_inputs()[0].1;

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

        for &(unknown, input) in ir.solution_inputs() {
            workspace.set_input(input, solution[unknown.index()]);
        }

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

        for &(unknown, input) in ir.solution_inputs() {
            workspace.set_input(input, solution[unknown.index()]);
        }

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

    #[test]
    fn composite_forwards_child_observer_output() {
        let mut registry = DefinitionRegistry::new();

        let child = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);

            let positive = builder.add_terminal().unwrap();
            let negative = builder.add_terminal().unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::Conductance.into(),
                    vec![positive, negative],
                    vec![ValueRef::Literal(1.0)],
                ))
                .unwrap();

            let observer = builder.add_voltage_observer(positive, negative).unwrap();

            assert_eq!(observer, DefinitionObserverId::new(0));

            builder.build_definition().unwrap()
        };

        let child = registry.register(child).unwrap();

        let parent = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);

            let positive = builder.add_terminal().unwrap();
            let negative = builder.add_terminal().unwrap();

            let child = builder
                .add_element(Element::new(
                    child,
                    vec![positive, negative],
                    Vec::<ValueRef>::new(),
                ))
                .unwrap();

            let observer = builder
                .add_child_observer(child, DefinitionObserverId::new(0))
                .unwrap();

            assert_eq!(observer, DefinitionObserverId::new(0));

            builder.build_definition().unwrap()
        };

        let parent = registry.register(parent).unwrap();

        let compiled =
            CompiledDefinition::compile(&registry, registry.get(parent).unwrap()).unwrap();

        let partition = compiled.partition(DevicePartitionId::new(0)).unwrap();

        assert_eq!(
            partition.definition_observers(),
            &[DefinitionObserverId::new(0)],
        );

        assert_eq!(partition.template().output_count(), 1);
    }

    #[test]
    fn stateful_primitives_compile_logical_state_initializers() {
        let registry = DefinitionRegistry::new();

        let cases = [
            (
                PrimitiveElementKind::Capacitor,
                DefinitionStateInitializer::Literal(0.0),
            ),
            (
                PrimitiveElementKind::Inductor,
                DefinitionStateInitializer::Literal(0.0),
            ),
            (
                PrimitiveElementKind::TickDelay,
                DefinitionStateInitializer::Parameter(ParameterId::new(0)),
            ),
            (
                PrimitiveElementKind::SchmittBuffer,
                DefinitionStateInitializer::Literal(0.0),
            ),
        ];

        for (kind, expected) in cases {
            let definition = registry.get(DefinitionId::from(kind)).unwrap();

            let compiled = CompiledDefinition::compile(&registry, definition).unwrap();

            assert_eq!(compiled.state_initializers(), &[expected], "{kind:?}",);
        }
    }

    #[test]
    fn composite_state_initializers_flatten_child_parameter_mappings() {
        let registry = DefinitionRegistry::new();

        let definition = {
            let mut builder = DeviceDefinitionBuilder::new(&registry);

            let initial = builder
                .add_parameter(ParameterConstraints::default())
                .unwrap();

            let first_terminals = [
                builder.add_terminal().unwrap(),
                builder.add_terminal().unwrap(),
                builder.add_terminal().unwrap(),
                builder.add_terminal().unwrap(),
            ];

            let second_terminals = [
                builder.add_terminal().unwrap(),
                builder.add_terminal().unwrap(),
                builder.add_terminal().unwrap(),
                builder.add_terminal().unwrap(),
            ];

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::TickDelay.into(),
                    first_terminals.to_vec(),
                    vec![ValueRef::Parameter(initial)],
                ))
                .unwrap();

            builder
                .add_element(Element::new(
                    PrimitiveElementKind::TickDelay.into(),
                    second_terminals.to_vec(),
                    vec![ValueRef::Literal(4.25)],
                ))
                .unwrap();

            builder.build_definition().unwrap()
        };

        assert_eq!(definition.state_count(), 2);

        let compiled = CompiledDefinition::compile(&registry, &definition).unwrap();

        assert_eq!(
            compiled.state_initializers(),
            &[
                DefinitionStateInitializer::Parameter(ParameterId::new(0),),
                DefinitionStateInitializer::Literal(4.25),
            ],
        );
    }
}
