use crate::decoder::{Decoder, Truncated};
use hynergy_engine::Engine;
use hynergy_model::circuit::{Element, NodeId, ValueRef};
use hynergy_model::device::builder::{DeviceDefinitionBuilder, DeviceDefinitionBuilderError};
use hynergy_model::device::definition::{DefinitionId, DeviceDefinition};
use hynergy_model::device::registry::RegisterDeviceError;
use hynergy_model::parameter::{Bound, ParameterConstraints, ParameterId};

pub const DEFINITION_BUFFER_VERSION: u16 = 1;
pub const DEFINITION_BUFFER_MAGIC: [u8; 4] = *b"HYDF";
pub const DEFINITION_COMMAND_ADD_TERMINAL: u16 = 1;
pub const DEFINITION_COMMAND_ADD_NODE: u16 = 2;
pub const DEFINITION_COMMAND_ADD_PARAMETER: u16 = 3;
pub const DEFINITION_COMMAND_ADD_ELEMENT: u16 = 4;
pub const DEFINITION_VALUE_LITERAL: u8 = 0;
pub const DEFINITION_VALUE_PARAMETER: u8 = 1;

const DEFINITION_HEADER_LENGTH: usize = 16;

const CONSTRAINT_LOWER: u16 = 1 << 0;
const CONSTRAINT_LOWER_INCLUSIVE: u16 = 1 << 1;

const CONSTRAINT_UPPER: u16 = 1 << 2;
const CONSTRAINT_UPPER_INCLUSIVE: u16 = 1 << 3;

const CONSTRAINT_NON_ZERO: u16 = 1 << 4;

const CONSTRAINT_RECIPROCAL_RANGE: u16 = 1 << 5;

const CONSTRAINT_RECIPROCAL_LOWER: u16 = 1 << 6;
const CONSTRAINT_RECIPROCAL_LOWER_INCLUSIVE: u16 = 1 << 7;

const CONSTRAINT_RECIPROCAL_UPPER: u16 = 1 << 8;
const CONSTRAINT_RECIPROCAL_UPPER_INCLUSIVE: u16 = 1 << 9;

const CONSTRAINT_KNOWN_FLAGS: u16 = CONSTRAINT_LOWER
    | CONSTRAINT_LOWER_INCLUSIVE
    | CONSTRAINT_UPPER
    | CONSTRAINT_UPPER_INCLUSIVE
    | CONSTRAINT_NON_ZERO
    | CONSTRAINT_RECIPROCAL_RANGE
    | CONSTRAINT_RECIPROCAL_LOWER
    | CONSTRAINT_RECIPROCAL_LOWER_INCLUSIVE
    | CONSTRAINT_RECIPROCAL_UPPER
    | CONSTRAINT_RECIPROCAL_UPPER_INCLUSIVE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionRegistrationErrorKind {
    InvalidMagic,
    UnsupportedVersion,
    InvalidFlags,
    InvalidReserved,
    TruncatedInput,
    UnknownCommand,
    InvalidCommandLength,
    InvalidCount,
    InvalidDefinitionId,
    UnknownValueKind,
    TrailingBytes,
    UnknownDefinition,
    TerminalCountMismatch,
    ParameterCountMismatch,
    NodeOutOfRange,
    ParameterOutOfRange,
    ParameterConstraintViolation,
    NodeIdExhausted,
    ParameterIdExhausted,
    DefinitionIdExhausted,
    InvalidDefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionRegistrationError {
    kind: DefinitionRegistrationErrorKind,
    command_index: u32,
    byte_offset: u32,
}

impl DefinitionRegistrationError {
    const fn new(
        kind: DefinitionRegistrationErrorKind,
        command_index: u32,
        byte_offset: usize,
    ) -> Self {
        Self {
            kind,
            command_index,
            byte_offset: byte_offset as u32,
        }
    }

    #[inline]
    const fn header(kind: DefinitionRegistrationErrorKind, byte_offset: usize) -> Self {
        Self::new(kind, u32::MAX, byte_offset)
    }

    #[inline]
    const fn command(
        kind: DefinitionRegistrationErrorKind,
        command_index: u32,
        byte_offset: usize,
    ) -> Self {
        Self::new(kind, command_index, byte_offset)
    }

    #[inline]
    pub const fn kind(self) -> DefinitionRegistrationErrorKind {
        self.kind
    }

    #[inline]
    pub const fn command_index(self) -> u32 {
        self.command_index
    }

    #[inline]
    pub const fn byte_offset(self) -> u32 {
        self.byte_offset
    }
}

#[inline]
pub fn register_definition_buffer(
    engine: &mut Engine,
    input: &[u8],
) -> Result<DefinitionId, DefinitionRegistrationError> {
    let definition = decode_definition(engine, input)?;

    engine
        .register_definition(definition)
        .map_err(map_registry_error)
}

fn decode_definition(
    engine: &Engine,
    input: &[u8],
) -> Result<DeviceDefinition, DefinitionRegistrationError> {
    let mut decoder = Decoder::new(input, 0);
    let magic_offset = decoder.offset();
    let magic = decoder.read_array::<4>().map_err(truncated_header)?;
    if magic != DEFINITION_BUFFER_MAGIC {
        return Err(DefinitionRegistrationError::header(
            DefinitionRegistrationErrorKind::InvalidMagic,
            magic_offset,
        ));
    }

    let version_offset = decoder.offset();
    let version = decoder.read_u16().map_err(truncated_header)?;
    if version != DEFINITION_BUFFER_VERSION {
        return Err(DefinitionRegistrationError::header(
            DefinitionRegistrationErrorKind::UnsupportedVersion,
            version_offset,
        ));
    }

    let flags_offset = decoder.offset();
    let flags = decoder.read_u16().map_err(truncated_header)?;
    if flags != 0 {
        return Err(DefinitionRegistrationError::header(
            DefinitionRegistrationErrorKind::InvalidFlags,
            flags_offset,
        ));
    }

    let reserved_offset = decoder.offset();
    let reserved = decoder.read_u32().map_err(truncated_header)?;

    if reserved != 0 {
        return Err(DefinitionRegistrationError::header(
            DefinitionRegistrationErrorKind::InvalidReserved,
            reserved_offset,
        ));
    }

    let command_count = decoder.read_u32().map_err(truncated_header)?;
    debug_assert_eq!(decoder.offset(), DEFINITION_HEADER_LENGTH);

    let mut builder = DeviceDefinitionBuilder::new(engine.definitions());
    for command_index in 0..command_count {
        let command_offset = decoder.offset();

        let tag = decoder
            .read_u16()
            .map_err(|e| truncated(e, command_index))?;
        let payload_length = decoder
            .read_u32()
            .map_err(|e| truncated(e, command_index))? as usize;
        let payload_offset = decoder.offset();
        let payload = decoder
            .read_bytes(payload_length)
            .map_err(|e| truncated(e, command_index))?;
        let mut payload_decoder = Decoder::new(payload, payload_offset);

        match tag {
            DEFINITION_COMMAND_ADD_TERMINAL => {
                require_empty_payload(payload, command_index, command_offset)?;
                builder
                    .add_terminal()
                    .map_err(|error| map_builder_error(error, command_index, command_offset))?;
            }
            DEFINITION_COMMAND_ADD_NODE => {
                require_empty_payload(payload, command_index, command_offset)?;
                builder
                    .add_node()
                    .map_err(|error| map_builder_error(error, command_index, command_offset))?;
            }
            DEFINITION_COMMAND_ADD_PARAMETER => {
                let constraints = decode_constraints(&mut payload_decoder, command_index)?;

                if !payload_decoder.is_empty() {
                    return Err(DefinitionRegistrationError::command(
                        DefinitionRegistrationErrorKind::InvalidCommandLength,
                        command_index,
                        command_offset,
                    ));
                }

                builder
                    .add_parameter(constraints)
                    .map_err(|error| map_builder_error(error, command_index, command_offset))?;
            }
            DEFINITION_COMMAND_ADD_ELEMENT => {
                let element = decode_element(&mut payload_decoder, command_index)?;
                if !payload_decoder.is_empty() {
                    return Err(DefinitionRegistrationError::command(
                        DefinitionRegistrationErrorKind::InvalidCommandLength,
                        command_index,
                        command_offset,
                    ));
                }
                builder
                    .add_element(element)
                    .map_err(|error| map_builder_error(error, command_index, command_offset))?;
            }
            _ => {
                return Err(DefinitionRegistrationError::command(
                    DefinitionRegistrationErrorKind::UnknownCommand,
                    command_index,
                    command_offset,
                ));
            }
        }
    }

    if !decoder.is_empty() {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::TrailingBytes,
            command_count,
            decoder.offset(),
        ));
    }

    Ok(builder.build_definition())
}

fn decode_element(
    decoder: &mut Decoder<'_>,
    command_index: u32,
) -> Result<Element, DefinitionRegistrationError> {
    let id_offset = decoder.offset();
    let raw_definition = decoder
        .read_u32()
        .map_err(|e| truncated(e, command_index))?;

    let definition = DefinitionId::try_from(raw_definition).map_err(|_| {
        DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidDefinitionId,
            command_index,
            id_offset,
        )
    })?;

    let terminal_count_offset = decoder.offset();
    let terminal_count = decoder
        .read_u32()
        .map_err(|e| truncated(e, command_index))? as usize;
    if terminal_count > decoder.remaining() / size_of::<u32>() {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidCount,
            command_index,
            terminal_count_offset,
        ));
    }
    let mut terminals = Vec::with_capacity(terminal_count);
    for _ in 0..terminal_count {
        terminals.push(NodeId::from(
            decoder
                .read_u32()
                .map_err(|e| truncated(e, command_index))?,
        ));
    }

    let parameter_count_offset = decoder.offset();
    let parameter_count = decoder
        .read_u32()
        .map_err(|e| truncated(e, command_index))? as usize;
    if parameter_count > decoder.remaining() / (size_of::<u8>() + size_of::<u32>()) {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidCount,
            command_index,
            parameter_count_offset,
        ));
    }
    let mut parameters = Vec::with_capacity(parameter_count);
    for _ in 0..parameter_count {
        let kind_offset = decoder.offset();
        let kind = decoder.read_u8().map_err(|e| truncated(e, command_index))?;
        let value = match kind {
            DEFINITION_VALUE_LITERAL => ValueRef::Literal(
                decoder
                    .read_f64()
                    .map_err(|e| truncated(e, command_index))?,
            ),
            DEFINITION_VALUE_PARAMETER => ValueRef::Parameter(ParameterId::from(
                decoder
                    .read_u32()
                    .map_err(|e| truncated(e, command_index))?,
            )),
            _ => {
                return Err(DefinitionRegistrationError::command(
                    DefinitionRegistrationErrorKind::UnknownValueKind,
                    command_index,
                    kind_offset,
                ));
            }
        };
        parameters.push(value);
    }

    Ok(Element::new(definition, terminals, parameters))
}

fn decode_constraints(
    decoder: &mut Decoder<'_>,
    command_index: u32,
) -> Result<ParameterConstraints, DefinitionRegistrationError> {
    let flags_offset = decoder.offset();
    let flags = decoder
        .read_u16()
        .map_err(|e| truncated(e, command_index))?;

    if flags & !CONSTRAINT_KNOWN_FLAGS != 0 {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidFlags,
            command_index,
            flags_offset,
        ));
    }

    let invalid = (flags & CONSTRAINT_LOWER_INCLUSIVE != 0 && flags & CONSTRAINT_LOWER == 0)
        || (flags & CONSTRAINT_UPPER_INCLUSIVE != 0 && flags & CONSTRAINT_UPPER == 0)
        || (flags & CONSTRAINT_RECIPROCAL_LOWER_INCLUSIVE != 0
            && flags & CONSTRAINT_RECIPROCAL_LOWER == 0)
        || (flags & CONSTRAINT_RECIPROCAL_UPPER_INCLUSIVE != 0
            && flags & CONSTRAINT_RECIPROCAL_UPPER == 0);

    if invalid {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidFlags,
            command_index,
            flags_offset,
        ));
    }

    let reciprocal_bits = CONSTRAINT_RECIPROCAL_LOWER
        | CONSTRAINT_RECIPROCAL_LOWER_INCLUSIVE
        | CONSTRAINT_RECIPROCAL_UPPER
        | CONSTRAINT_RECIPROCAL_UPPER_INCLUSIVE;

    if flags & reciprocal_bits != 0 && flags & CONSTRAINT_RECIPROCAL_RANGE == 0 {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidFlags,
            command_index,
            flags_offset,
        ));
    }

    let lower = decode_bound(decoder, flags, CONSTRAINT_LOWER, CONSTRAINT_LOWER_INCLUSIVE)
        .map_err(|e| truncated(e, command_index))?;

    let upper = decode_bound(decoder, flags, CONSTRAINT_UPPER, CONSTRAINT_UPPER_INCLUSIVE)
        .map_err(|e| truncated(e, command_index))?;

    let reciprocal_range = if flags & CONSTRAINT_RECIPROCAL_RANGE != 0 {
        let lower = decode_bound(
            decoder,
            flags,
            CONSTRAINT_RECIPROCAL_LOWER,
            CONSTRAINT_RECIPROCAL_LOWER_INCLUSIVE,
        )
        .map_err(|e| truncated(e, command_index))?;

        let upper = decode_bound(
            decoder,
            flags,
            CONSTRAINT_RECIPROCAL_UPPER,
            CONSTRAINT_RECIPROCAL_UPPER_INCLUSIVE,
        )
        .map_err(|e| truncated(e, command_index))?;

        Some((lower, upper))
    } else {
        None
    };

    Ok(ParameterConstraints::new(
        lower,
        upper,
        flags & CONSTRAINT_NON_ZERO != 0,
        reciprocal_range,
    ))
}

#[inline]
fn decode_bound(
    decoder: &mut Decoder<'_>,
    flags: u16,
    present: u16,
    inclusive: u16,
) -> Result<Option<Bound>, Truncated> {
    if flags & present == 0 {
        return Ok(None);
    }

    Ok(Some(Bound {
        value: decoder.read_f64()?,
        inclusive: flags & inclusive != 0,
    }))
}

#[inline]
fn require_empty_payload(
    payload: &[u8],
    command_index: u32,
    command_offset: usize,
) -> Result<(), DefinitionRegistrationError> {
    if payload.is_empty() {
        Ok(())
    } else {
        Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidCommandLength,
            command_index,
            command_offset,
        ))
    }
}

fn map_builder_error(
    error: DeviceDefinitionBuilderError,
    command_index: u32,
    command_offset: usize,
) -> DefinitionRegistrationError {
    let kind = match error {
        DeviceDefinitionBuilderError::UnknownDefinition { .. } => {
            DefinitionRegistrationErrorKind::UnknownDefinition
        }
        DeviceDefinitionBuilderError::TerminalCountMismatch { .. } => {
            DefinitionRegistrationErrorKind::TerminalCountMismatch
        }
        DeviceDefinitionBuilderError::ParameterCountMismatch { .. } => {
            DefinitionRegistrationErrorKind::ParameterCountMismatch
        }
        DeviceDefinitionBuilderError::NodeOutOfRange { .. } => {
            DefinitionRegistrationErrorKind::NodeOutOfRange
        }
        DeviceDefinitionBuilderError::ParameterOutOfRange { .. } => {
            DefinitionRegistrationErrorKind::ParameterOutOfRange
        }
        DeviceDefinitionBuilderError::ParameterConstraint { .. } => {
            DefinitionRegistrationErrorKind::ParameterConstraintViolation
        }
        DeviceDefinitionBuilderError::NodeIdExhausted => {
            DefinitionRegistrationErrorKind::NodeIdExhausted
        }
        DeviceDefinitionBuilderError::ParameterIdExhausted => {
            DefinitionRegistrationErrorKind::ParameterIdExhausted
        }
        DeviceDefinitionBuilderError::ElementIdExhausted => {
            DefinitionRegistrationErrorKind::InvalidDefinition
        }
    };
    DefinitionRegistrationError::command(kind, command_index, command_offset)
}

fn map_registry_error(error: RegisterDeviceError) -> DefinitionRegistrationError {
    let kind = match error {
        RegisterDeviceError::DefinitionIdExhausted => {
            DefinitionRegistrationErrorKind::DefinitionIdExhausted
        }

        RegisterDeviceError::PrimitiveRegistrationForbidden { .. }
        | RegisterDeviceError::TerminalCountExceedsNodeCount { .. }
        | RegisterDeviceError::UnknownDefinition { .. } => {
            DefinitionRegistrationErrorKind::InvalidDefinition
        }
    };

    DefinitionRegistrationError::header(kind, u32::MAX as usize)
}

#[inline]
fn truncated(error: Truncated, command_index: u32) -> DefinitionRegistrationError {
    DefinitionRegistrationError::new(
        DefinitionRegistrationErrorKind::TruncatedInput,
        command_index,
        error.offset,
    )
}

#[inline]
fn truncated_header(error: Truncated) -> DefinitionRegistrationError {
    DefinitionRegistrationError::header(
        DefinitionRegistrationErrorKind::TruncatedInput,
        error.offset,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hynergy_model::device::definition::DeviceBody;

    #[derive(Clone, Copy)]
    enum TestValue {
        Literal(f64),
        Parameter(u32),
    }

    fn command(tag: u16, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(6 + payload.len());
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn framed_command(tag: u16, payload_length: u32, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(6 + payload.len());
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&payload_length.to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn definition_buffer(commands: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&DEFINITION_BUFFER_MAGIC);
        bytes.extend_from_slice(&DEFINITION_BUFFER_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes()); // reserved
        bytes.extend_from_slice(&(commands.len() as u32).to_le_bytes());

        for command in commands {
            bytes.extend_from_slice(command);
        }

        bytes
    }

    fn element_payload(device: u32, terminals: &[u32], parameters: &[TestValue]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&device.to_le_bytes());
        bytes.extend_from_slice(&(terminals.len() as u32).to_le_bytes());

        for terminal in terminals {
            bytes.extend_from_slice(&terminal.to_le_bytes());
        }

        bytes.extend_from_slice(&(parameters.len() as u32).to_le_bytes());

        for parameter in parameters {
            match parameter {
                TestValue::Literal(value) => {
                    bytes.push(DEFINITION_VALUE_LITERAL);
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                TestValue::Parameter(parameter) => {
                    bytes.push(DEFINITION_VALUE_PARAMETER);
                    bytes.extend_from_slice(&parameter.to_le_bytes());
                }
            }
        }

        bytes
    }

    fn constraints_payload(
        lower: Option<Bound>,
        upper: Option<Bound>,
        non_zero: bool,
        reciprocal_range: Option<(Option<Bound>, Option<Bound>)>,
    ) -> Vec<u8> {
        let mut flags = 0_u16;
        let mut values = Vec::new();

        if let Some(bound) = lower {
            flags |= CONSTRAINT_LOWER;
            if bound.inclusive {
                flags |= CONSTRAINT_LOWER_INCLUSIVE;
            }
            values.extend_from_slice(&bound.value.to_le_bytes());
        }

        if let Some(bound) = upper {
            flags |= CONSTRAINT_UPPER;
            if bound.inclusive {
                flags |= CONSTRAINT_UPPER_INCLUSIVE;
            }
            values.extend_from_slice(&bound.value.to_le_bytes());
        }

        if non_zero {
            flags |= CONSTRAINT_NON_ZERO;
        }

        if let Some((lower, upper)) = reciprocal_range {
            flags |= CONSTRAINT_RECIPROCAL_RANGE;

            if let Some(bound) = lower {
                flags |= CONSTRAINT_RECIPROCAL_LOWER;
                if bound.inclusive {
                    flags |= CONSTRAINT_RECIPROCAL_LOWER_INCLUSIVE;
                }
                values.extend_from_slice(&bound.value.to_le_bytes());
            }

            if let Some(bound) = upper {
                flags |= CONSTRAINT_RECIPROCAL_UPPER;
                if bound.inclusive {
                    flags |= CONSTRAINT_RECIPROCAL_UPPER_INCLUSIVE;
                }
                values.extend_from_slice(&bound.value.to_le_bytes());
            }
        }

        let mut bytes = Vec::with_capacity(2 + values.len());
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&values);
        bytes
    }

    fn registered_definition(engine: &Engine) -> &DeviceDefinition {
        engine
            .definitions()
            .get(DefinitionId::try_from(Engine::COMPOSITE_DEFINITION_ID_BASE).unwrap())
            .unwrap()
    }

    fn assert_error(
        bytes: &[u8],
        kind: DefinitionRegistrationErrorKind,
        command_index: u32,
        byte_offset: u32,
    ) {
        let error = register_definition_buffer(&mut Engine::new(), bytes).unwrap_err();

        assert_eq!(error.kind(), kind);
        assert_eq!(error.command_index(), command_index);
        assert_eq!(error.byte_offset(), byte_offset);
    }

    #[test]
    fn empty_definition_registers() {
        let mut engine = Engine::new();

        let definition_id =
            register_definition_buffer(&mut engine, &definition_buffer(&[])).unwrap();

        assert_eq!(definition_id.get(), Engine::COMPOSITE_DEFINITION_ID_BASE);

        let definition = engine.definitions().get(definition_id).unwrap();

        assert!(matches!(definition.body(), DeviceBody::Composite(_)));
        assert!(definition.terminals().is_empty());
        assert!(definition.parameters().is_empty());
    }

    #[test]
    fn complete_definition_registers_expected_contents() {
        let mut engine = Engine::new();

        let commands = [
            command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
            command(DEFINITION_COMMAND_ADD_NODE, &[]),
            command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
            command(
                DEFINITION_COMMAND_ADD_PARAMETER,
                &constraints_payload(None, None, false, None),
            ),
            command(
                DEFINITION_COMMAND_ADD_ELEMENT,
                &element_payload(1, &[0, 2], &[TestValue::Parameter(0)]),
            ),
        ];

        register_definition_buffer(&mut engine, &definition_buffer(&commands)).unwrap();

        let definition = registered_definition(&engine);

        assert_eq!(definition.terminals().len(), 2);
        assert_eq!(definition.parameters().len(), 1);

        let DeviceBody::Composite(circuit) = definition.body() else {
            panic!("expected composite definition");
        };

        assert_eq!(circuit.node_count(), 3);
        assert_eq!(circuit.elements().len(), 1);
        assert_eq!(circuit.elements()[0].definition().get(), 1);
        assert_eq!(circuit.elements()[0].terminals()[0].id(), 0);
        assert_eq!(circuit.elements()[0].terminals()[1].id(), 2);
    }

    #[test]
    fn truncated_header_reports_input_end() {
        let bytes = definition_buffer(&[]);

        for length in [0, 1, 3, 5, 7, 11, 15] {
            assert_error(
                &bytes[..length],
                DefinitionRegistrationErrorKind::TruncatedInput,
                u32::MAX,
                length as u32,
            );
        }
    }

    #[test]
    fn invalid_header_fields_report_their_offsets() {
        let valid = definition_buffer(&[]);

        let mut invalid_magic = valid.clone();
        invalid_magic[0] = b'X';

        let mut invalid_version = valid.clone();
        invalid_version[4..6].copy_from_slice(&(DEFINITION_BUFFER_VERSION + 1).to_le_bytes());

        let mut invalid_flags = valid.clone();
        invalid_flags[6..8].copy_from_slice(&1_u16.to_le_bytes());

        let mut invalid_reserved = valid;
        invalid_reserved[8..12].copy_from_slice(&1_u32.to_le_bytes());

        for (bytes, kind, offset) in [
            (
                invalid_magic,
                DefinitionRegistrationErrorKind::InvalidMagic,
                0,
            ),
            (
                invalid_version,
                DefinitionRegistrationErrorKind::UnsupportedVersion,
                4,
            ),
            (
                invalid_flags,
                DefinitionRegistrationErrorKind::InvalidFlags,
                6,
            ),
            (
                invalid_reserved,
                DefinitionRegistrationErrorKind::InvalidReserved,
                8,
            ),
        ] {
            assert_error(&bytes, kind, u32::MAX, offset);
        }
    }

    #[test]
    fn truncated_command_frame_reports_command_index() {
        let mut bytes = definition_buffer(&[]);
        bytes[12..16].copy_from_slice(&1_u32.to_le_bytes());

        assert_error(
            &bytes,
            DefinitionRegistrationErrorKind::TruncatedInput,
            0,
            DEFINITION_HEADER_LENGTH as u32,
        );
    }

    #[test]
    fn truncated_command_payload_reports_input_end() {
        let bytes = definition_buffer(&[framed_command(
            DEFINITION_COMMAND_ADD_ELEMENT,
            8,
            &1_u32.to_le_bytes(),
        )]);

        assert_error(
            &bytes,
            DefinitionRegistrationErrorKind::TruncatedInput,
            0,
            26,
        );
    }

    #[test]
    fn unknown_command_reports_command_start() {
        let bytes = definition_buffer(&[command(99, &[])]);

        assert_error(
            &bytes,
            DefinitionRegistrationErrorKind::UnknownCommand,
            0,
            DEFINITION_HEADER_LENGTH as u32,
        );
    }

    #[test]
    fn fixed_empty_payload_commands_reject_nonempty_payloads() {
        for tag in [DEFINITION_COMMAND_ADD_TERMINAL, DEFINITION_COMMAND_ADD_NODE] {
            let bytes = definition_buffer(&[command(tag, &[0])]);

            assert_error(
                &bytes,
                DefinitionRegistrationErrorKind::InvalidCommandLength,
                0,
                DEFINITION_HEADER_LENGTH as u32,
            );
        }
    }

    #[test]
    fn parameter_command_rejects_trailing_payload_bytes() {
        let mut payload = constraints_payload(None, None, false, None);
        payload.push(0);

        let bytes = definition_buffer(&[command(DEFINITION_COMMAND_ADD_PARAMETER, &payload)]);

        assert_error(
            &bytes,
            DefinitionRegistrationErrorKind::InvalidCommandLength,
            0,
            DEFINITION_HEADER_LENGTH as u32,
        );
    }

    #[test]
    fn element_command_rejects_trailing_payload_bytes() {
        let mut payload = element_payload(1, &[], &[]);
        payload.push(0);

        let bytes = definition_buffer(&[command(DEFINITION_COMMAND_ADD_ELEMENT, &payload)]);

        assert_error(
            &bytes,
            DefinitionRegistrationErrorKind::InvalidCommandLength,
            0,
            DEFINITION_HEADER_LENGTH as u32,
        );
    }

    #[test]
    fn parameter_constraints_round_trip() {
        let lower = Bound {
            value: 1.0,
            inclusive: false,
        };
        let upper = Bound {
            value: 10.0,
            inclusive: true,
        };
        let reciprocal_lower = Bound {
            value: 0.1,
            inclusive: true,
        };
        let reciprocal_upper = Bound {
            value: 1.0,
            inclusive: false,
        };

        let expected = ParameterConstraints::new(
            Some(lower),
            Some(upper),
            true,
            Some((Some(reciprocal_lower), Some(reciprocal_upper))),
        );

        let commands = [command(
            DEFINITION_COMMAND_ADD_PARAMETER,
            &constraints_payload(
                Some(lower),
                Some(upper),
                true,
                Some((Some(reciprocal_lower), Some(reciprocal_upper))),
            ),
        )];

        let mut engine = Engine::new();

        register_definition_buffer(&mut engine, &definition_buffer(&commands)).unwrap();

        assert_eq!(registered_definition(&engine).parameters(), &[expected]);
    }

    #[test]
    fn invalid_constraint_flags_are_rejected() {
        for flags in [
            1 << 15,
            CONSTRAINT_LOWER_INCLUSIVE,
            CONSTRAINT_UPPER_INCLUSIVE,
            CONSTRAINT_RECIPROCAL_LOWER,
            CONSTRAINT_RECIPROCAL_UPPER,
        ] {
            let bytes = definition_buffer(&[command(
                DEFINITION_COMMAND_ADD_PARAMETER,
                &flags.to_le_bytes(),
            )]);

            assert_error(&bytes, DefinitionRegistrationErrorKind::InvalidFlags, 0, 22);
        }
    }

    #[test]
    fn truncated_constraint_bound_reports_payload_end() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&CONSTRAINT_LOWER.to_le_bytes());
        payload.extend_from_slice(&1_u32.to_le_bytes());

        let bytes = definition_buffer(&[command(DEFINITION_COMMAND_ADD_PARAMETER, &payload)]);

        assert_error(
            &bytes,
            DefinitionRegistrationErrorKind::TruncatedInput,
            0,
            28,
        );
    }

    #[test]
    fn zero_element_definition_id_is_rejected() {
        let bytes = definition_buffer(&[command(
            DEFINITION_COMMAND_ADD_ELEMENT,
            &element_payload(0, &[], &[]),
        )]);

        assert_error(
            &bytes,
            DefinitionRegistrationErrorKind::InvalidDefinitionId,
            0,
            22,
        );
    }

    #[test]
    fn terminal_count_cannot_exceed_command_payload() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1_u32.to_le_bytes());
        payload.extend_from_slice(&u32::MAX.to_le_bytes());

        let bytes = definition_buffer(&[command(DEFINITION_COMMAND_ADD_ELEMENT, &payload)]);

        assert_error(&bytes, DefinitionRegistrationErrorKind::InvalidCount, 0, 26);
    }

    #[test]
    fn parameter_count_cannot_exceed_command_payload() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1_u32.to_le_bytes());
        payload.extend_from_slice(&0_u32.to_le_bytes());
        payload.extend_from_slice(&u32::MAX.to_le_bytes());

        let bytes = definition_buffer(&[command(DEFINITION_COMMAND_ADD_ELEMENT, &payload)]);

        assert_error(&bytes, DefinitionRegistrationErrorKind::InvalidCount, 0, 30);
    }

    #[test]
    fn unknown_element_value_kind_is_rejected() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1_u32.to_le_bytes());
        payload.extend_from_slice(&0_u32.to_le_bytes());
        payload.extend_from_slice(&1_u32.to_le_bytes());
        payload.push(99);
        payload.extend_from_slice(&0_u32.to_le_bytes());

        let bytes = definition_buffer(&[command(DEFINITION_COMMAND_ADD_ELEMENT, &payload)]);

        assert_error(
            &bytes,
            DefinitionRegistrationErrorKind::UnknownValueKind,
            0,
            34,
        );
    }

    #[test]
    fn builder_errors_report_stable_command_locations() {
        let cases = [
            (
                vec![command(
                    DEFINITION_COMMAND_ADD_ELEMENT,
                    &element_payload(99, &[], &[]),
                )],
                DefinitionRegistrationErrorKind::UnknownDefinition,
            ),
            (
                vec![
                    command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
                    command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
                    command(
                        DEFINITION_COMMAND_ADD_ELEMENT,
                        &element_payload(1, &[0], &[TestValue::Literal(1.0)]),
                    ),
                ],
                DefinitionRegistrationErrorKind::TerminalCountMismatch,
            ),
            (
                vec![
                    command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
                    command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
                    command(
                        DEFINITION_COMMAND_ADD_ELEMENT,
                        &element_payload(1, &[0, 1], &[]),
                    ),
                ],
                DefinitionRegistrationErrorKind::ParameterCountMismatch,
            ),
            (
                vec![
                    command(DEFINITION_COMMAND_ADD_NODE, &[]),
                    command(
                        DEFINITION_COMMAND_ADD_ELEMENT,
                        &element_payload(1, &[0, 1], &[TestValue::Literal(1.0)]),
                    ),
                ],
                DefinitionRegistrationErrorKind::NodeOutOfRange,
            ),
            (
                vec![
                    command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
                    command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
                    command(
                        DEFINITION_COMMAND_ADD_ELEMENT,
                        &element_payload(1, &[0, 1], &[TestValue::Parameter(0)]),
                    ),
                ],
                DefinitionRegistrationErrorKind::ParameterOutOfRange,
            ),
            (
                vec![
                    command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
                    command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
                    command(
                        DEFINITION_COMMAND_ADD_ELEMENT,
                        &element_payload(1, &[0, 1], &[TestValue::Literal(f64::NAN)]),
                    ),
                ],
                DefinitionRegistrationErrorKind::ParameterConstraintViolation,
            ),
        ];

        for (commands, expected_kind) in cases {
            let command_index = (commands.len() - 1) as u32;
            let byte_offset = DEFINITION_HEADER_LENGTH
                + commands[..commands.len() - 1]
                    .iter()
                    .map(Vec::len)
                    .sum::<usize>();

            let error =
                register_definition_buffer(&mut Engine::new(), &definition_buffer(&commands))
                    .unwrap_err();

            assert_eq!(error.kind(), expected_kind);
            assert_eq!(error.command_index(), command_index);
            assert_eq!(error.byte_offset(), byte_offset as u32);
        }
    }

    #[test]
    fn failed_buffer_does_not_register_definition() {
        let mut engine = Engine::new();

        let bytes = definition_buffer(&[
            command(DEFINITION_COMMAND_ADD_TERMINAL, &[]),
            command(99, &[]),
        ]);

        assert_eq!(
            register_definition_buffer(&mut engine, &bytes)
                .unwrap_err()
                .kind(),
            DefinitionRegistrationErrorKind::UnknownCommand
        );

        assert!(
            engine
                .definitions()
                .get(DefinitionId::try_from(Engine::COMPOSITE_DEFINITION_ID_BASE).unwrap())
                .is_none()
        );
    }

    #[test]
    fn command_count_larger_than_available_commands_is_rejected() {
        let mut bytes = definition_buffer(&[command(DEFINITION_COMMAND_ADD_NODE, &[])]);
        bytes[12..16].copy_from_slice(&2_u32.to_le_bytes());

        assert_error(
            &bytes,
            DefinitionRegistrationErrorKind::TruncatedInput,
            1,
            22,
        );
    }

    #[test]
    fn bytes_after_declared_commands_are_rejected() {
        let mut bytes = definition_buffer(&[]);
        bytes.extend_from_slice(&command(DEFINITION_COMMAND_ADD_NODE, &[]));

        assert_error(
            &bytes,
            DefinitionRegistrationErrorKind::TrailingBytes,
            0,
            DEFINITION_HEADER_LENGTH as u32,
        );
    }

    #[test]
    fn registration_returns_engine_assigned_definition_ids() {
        let mut engine = Engine::new();

        let first = register_definition_buffer(&mut engine, &definition_buffer(&[])).unwrap();

        let second = register_definition_buffer(&mut engine, &definition_buffer(&[])).unwrap();

        assert_eq!(first.get(), Engine::COMPOSITE_DEFINITION_ID_BASE);

        assert_eq!(second.get(), Engine::COMPOSITE_DEFINITION_ID_BASE + 1);

        assert!(engine.definitions().get(first).is_some());
        assert!(engine.definitions().get(second).is_some());
    }
}
