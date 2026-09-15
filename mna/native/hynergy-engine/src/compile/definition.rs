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
            let a = builder.voltage_unknown()?;
            let b = builder.voltage_unknown()?;

            let conductance = builder.parameter()?;

            stamp_conductance(&mut builder, a, b, conductance);
        }

        PrimitiveElementKind::Resistance => {
            let a = builder.voltage_unknown()?;
            let b = builder.voltage_unknown()?;

            let resistance = builder.parameter()?;
            let one = builder.constant(1.0)?;

            let conductance = builder.div(one, resistance)?;

            stamp_conductance(&mut builder, a, b, conductance);
        }

        PrimitiveElementKind::CurrentSource => {
            let from = builder.voltage_unknown()?;
            let to = builder.voltage_unknown()?;

            let current = builder.parameter()?;

            stamp_current_source(&mut builder, from, to, current);
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

/// Independent current source whose positive current direction is
///
/// `from -> to`.
///
/// With the MNA convention `A x = b`, that means:
///
/// `b[from] -= I`
/// `b[to]   += I`
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
