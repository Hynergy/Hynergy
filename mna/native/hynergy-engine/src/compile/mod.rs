mod template;
mod unknown;

pub(crate) mod definition;

pub(crate) use template::{
    CompiledDefinitionTemplate, DefinitionTemplateBuildError, DefinitionTemplateBuilder,
    LocalUnknownId, LocalValueId,
};

pub(crate) use unknown::UnknownRange;
