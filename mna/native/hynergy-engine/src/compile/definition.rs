use super::{
    CompiledDefinitionTemplate, DefinitionTemplateBuildError, DefinitionTemplateBuilder,
    LocalUnknownId, LocalValueId,
};

use hynergy_model::device::definition::{DeviceBody, DeviceDefinition, PrimitiveElementKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionCompileError {
    UnsupportedPrimitive { kind: PrimitiveElementKind },

    CompositeNotYetSupported,

    Template(DefinitionTemplateBuildError),
}

impl From<DefinitionTemplateBuildError> for DefinitionCompileError {
    #[inline]
    fn from(error: DefinitionTemplateBuildError) -> Self {
        Self::Template(error)
    }
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
            let negaitve = builder.terminal_voltage()?;

            let branch_current = builder.branch_current_unknown()?;

            let voltage = builder.parameter()?;

            stamp_voltage_source(&mut builder, positive, negaitve, branch_current, voltage);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::unknown::UnknownAllocator;
    use hynergy_ir::{MatrixProgram, RhsProgram, ValueProgramBuilder};
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

        let definition = registry
            .get(DefinitionId::from(kind))
            .expect("primitive definition must be registered");

        CompiledDefinitionTemplate::compile(definition).expect("primitive must compile")
    }

    #[test]
    fn voltage_source_allocates_branch_unknown_and_solves() {
        let source = template(PrimitiveElementKind::VoltageSource);

        assert_eq!(source.terminal_count(), 2);
        assert_eq!(source.allocated_unknown_count(), 1);

        let node = UnknownIndex::new(0);

        let mut allocator = UnknownAllocator::new(1).unwrap();

        let allocated = allocator
            .allocate(source.allocated_unknown_count())
            .unwrap();

        let branch_current = allocated.get(0).unwrap();

        assert_eq!(branch_current, UnknownIndex::new(1),);

        let unknowns = source
            .bind_unknowns(&[Some(node), None], allocated)
            .unwrap();

        let mut pattern_builder =
            PatternBuilder::with_capacity(allocator.dimension(), source.matrix_entry_count())
                .unwrap();

        source
            .request_pattern(&unknowns, &mut pattern_builder)
            .unwrap();

        let pattern = pattern_builder.finish().unwrap();

        assert_eq!(pattern.dimension(), 2);

        assert_eq!(pattern.nnz(), 2);

        let mut value_builder = ValueProgramBuilder::new();

        let mut matrix_ops = Vec::new();
        let mut rhs_ops = Vec::new();

        let inputs = source
            .bind(
                &unknowns,
                &pattern,
                &mut value_builder,
                &mut matrix_ops,
                &mut rhs_ops,
            )
            .unwrap();

        let matrix_program = MatrixProgram::new(matrix_ops);

        let rhs_program = RhsProgram::new(rhs_ops);

        let value_program = value_builder.finish();

        let mut workspace = value_program.new_workspace();

        workspace.set_input(inputs.parameter(0).unwrap(), 5.0);

        value_program.execute_static(&mut workspace);

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut matrix = system.values_mut();

            matrix_program.execute(&mut matrix, workspace.values());
        }

        system.factorize().unwrap();

        let mut solution = [0.0, 0.0];

        rhs_program.execute(&mut solution, workspace.values());

        system.solve_in_place(&mut solution).unwrap();

        assert!((solution[node.index()] - 5.0).abs() < 1.0e-12);

        assert!(solution[branch_current.index()].abs() < 1.0e-12);
    }

    #[test]
    fn voltage_source_branch_unknown_exposes_source_current() {
        let source = template(PrimitiveElementKind::VoltageSource);

        let conductance = template(PrimitiveElementKind::Conductance);

        let node = UnknownIndex::new(0);

        let mut allocator = UnknownAllocator::new(1).unwrap();

        let conductance_allocated = allocator
            .allocate(conductance.allocated_unknown_count())
            .unwrap();

        let source_allocated = allocator
            .allocate(source.allocated_unknown_count())
            .unwrap();

        let branch_current = source_allocated.get(0).unwrap();

        assert_eq!(branch_current, UnknownIndex::new(1),);

        let conductance_unknowns = conductance
            .bind_unknowns(&[Some(node), None], conductance_allocated)
            .unwrap();

        let source_unknowns = source
            .bind_unknowns(&[Some(node), None], source_allocated)
            .unwrap();

        let mut pattern_builder = PatternBuilder::with_capacity(
            allocator.dimension(),
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

        let mut value_builder = ValueProgramBuilder::new();

        let mut matrix_ops = Vec::new();
        let mut rhs_ops = Vec::new();

        let conductance_inputs = conductance
            .bind(
                &conductance_unknowns,
                &pattern,
                &mut value_builder,
                &mut matrix_ops,
                &mut rhs_ops,
            )
            .unwrap();

        let source_inputs = source
            .bind(
                &source_unknowns,
                &pattern,
                &mut value_builder,
                &mut matrix_ops,
                &mut rhs_ops,
            )
            .unwrap();

        let matrix_program = MatrixProgram::new(matrix_ops);

        let rhs_program = RhsProgram::new(rhs_ops);

        let value_program = value_builder.finish();

        let mut workspace = value_program.new_workspace();

        workspace.set_input(conductance_inputs.parameter(0).unwrap(), 2.0);

        workspace.set_input(source_inputs.parameter(0).unwrap(), 5.0);

        value_program.execute_static(&mut workspace);

        let mut system = MnaSystem::new(pattern).unwrap();

        {
            let mut matrix = system.values_mut();

            matrix_program.execute(&mut matrix, workspace.values());
        }

        system.factorize().unwrap();

        let mut solution = vec![0.0; allocator.dimension()];

        rhs_program.execute(&mut solution, workspace.values());

        system.solve_in_place(&mut solution).unwrap();

        assert!((solution[node.index()] - 5.0).abs() < 1.0e-12);

        assert!((solution[branch_current.index()] + 10.0).abs() < 1.0e-12);
    }
}
