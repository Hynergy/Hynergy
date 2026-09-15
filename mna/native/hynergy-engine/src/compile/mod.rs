mod template;

pub(crate) mod definition;

pub(crate) use template::{
    BoundDefinitionInputs, CompiledDefinitionTemplate, DefinitionLinkError,
    DefinitionTemplateBuildError, DefinitionTemplateBuilder, LocalUnknownId, LocalValueId,
};
