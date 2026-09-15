use super::{
    CompiledDefinitionTemplate, DefinitionTemplateBuildError, DefinitionTemplateBuilder,
    LocalUnknownId, LocalValueId,
};
use thiserror::Error;

use hynergy_model::device::definition::{DeviceBody, DeviceDefinition, PrimitiveElementKind};

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
    let mut builder = DefinitionTemplateBuilder::new();

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

        _ => {
            return Err(DefinitionCompileError::UnsupportedPrimitive { kind });
        }
    }

    Ok(builder.finish()?)
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::compile::{
        island_ir::IslandIrBuilder, state::StateAllocator, unknown::UnknownAllocator,
    };

    use hynergy_mna::{
        pattern::{PatternBuilder, UnknownIndex},
        system::MnaSystem,
    };

    use hynergy_model::device::{
        definition::{DefinitionId, PrimitiveElementKind},
        registry::DefinitionRegistry,
    };

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

        let mut state_allocator = StateAllocator::new();

        let states = state_allocator.allocate(source.state_count()).unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let inputs = source.bind(&unknowns, states, &mut ir_builder).unwrap();

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

        let mut state_allocator = StateAllocator::new();

        let conductance_states = state_allocator.allocate(conductance.state_count()).unwrap();
        let source_states = state_allocator.allocate(source.state_count()).unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let conductance_inputs = conductance
            .bind(&conductance_unknowns, conductance_states, &mut ir_builder)
            .unwrap();

        let source_inputs = source
            .bind(&source_unknowns, source_states, &mut ir_builder)
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

        let mut state_allocator = StateAllocator::new();

        let voltage_source_states = state_allocator
            .allocate(voltage_source.state_count())
            .unwrap();

        let conductance_states = state_allocator.allocate(conductance.state_count()).unwrap();
        let vccs_states = state_allocator.allocate(vccs.state_count()).unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let voltage_source_inputs = voltage_source
            .bind(
                &voltage_source_unknowns,
                voltage_source_states,
                &mut ir_builder,
            )
            .unwrap();

        let conductance_inputs = conductance
            .bind(&conductance_unknowns, conductance_states, &mut ir_builder)
            .unwrap();

        let vccs_inputs = vccs
            .bind(&vccs_unknowns, vccs_states, &mut ir_builder)
            .unwrap();

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

        let mut state_allocator = StateAllocator::new();

        let control_source_states = state_allocator
            .allocate(voltage_source.state_count())
            .unwrap();

        let vcvs_states = state_allocator.allocate(vcvs.state_count()).unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let control_source_inputs = voltage_source
            .bind(
                &control_source_unknowns,
                control_source_states,
                &mut ir_builder,
            )
            .unwrap();

        let vcvs_inputs = vcvs
            .bind(&vcvs_unknowns, vcvs_states, &mut ir_builder)
            .unwrap();

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

        let mut state_allocator = StateAllocator::new();

        let capacitor_states = state_allocator.allocate(capacitor.state_count()).unwrap();
        let source_states = state_allocator.allocate(source.state_count()).unwrap();
        let capacitor_state = capacitor_states.get(0).unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let capacitor_inputs = capacitor
            .bind(&capacitor_unknowns, capacitor_states, &mut ir_builder)
            .unwrap();

        let source_inputs = source
            .bind(&source_unknowns, source_states, &mut ir_builder)
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

        let mut state_allocator = StateAllocator::new();

        let source_states = state_allocator.allocate(source.state_count()).unwrap();
        let inductor_states = state_allocator.allocate(inductor.state_count()).unwrap();
        let inductor_state = inductor_states.get(0).unwrap();

        let mut ir_builder = IslandIrBuilder::new(&pattern);

        let source_inputs = source
            .bind(&source_unknowns, source_states, &mut ir_builder)
            .unwrap();

        let inductor_inputs = inductor
            .bind(&inductor_unknowns, inductor_states, &mut ir_builder)
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
}
