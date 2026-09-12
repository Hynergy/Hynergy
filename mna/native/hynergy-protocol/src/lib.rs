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
    InvalidDeviceId,
    TruncatedInput,
    UnknownCommand,
    InvalidCommandLength,
    InvalidCount,
    UnknownValueKind,
    TrailingBytes,
    UnknownDevice,
    TerminalCountMismatch,
    ParameterCountMismatch,
    NodeOutOfRange,
    ParameterOutOfRange,
    ParameterConstraintViolation,
    NodeIdExhausted,
    ParameterIdExhausted,
    DeviceIdExhausted,
    InvalidDefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionRegistrationError {
    kind: DefinitionRegistrationErrorKind,
    command_index: u32,
    byte_offset: u32,
}

impl DefinitionRegistrationError {
    const fn header(kind: DefinitionRegistrationErrorKind, byte_offset: usize) -> Self {
        Self::new(kind, u32::MAX, byte_offset)
    }

    const fn command(
        kind: DefinitionRegistrationErrorKind,
        command_index: u32,
        byte_offset: usize,
    ) -> Self {
        Self::new(kind, command_index, byte_offset)
    }

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

    pub const fn kind(self) -> DefinitionRegistrationErrorKind {
        self.kind
    }

    pub const fn command_index(self) -> u32 {
        self.command_index
    }

    pub const fn byte_offset(self) -> u32 {
        self.byte_offset
    }
}

pub fn register_definition_buffer(
    engine: &mut Engine,
    input: &[u8],
) -> Result<(), DefinitionRegistrationError> {
    let definition = decode_definition(engine, input)?;
    engine
        .register_definition(definition)
        .map(|_| ())
        .map_err(map_registry_error)
}

fn decode_definition(
    engine: &Engine,
    input: &[u8],
) -> Result<DeviceDefinition, DefinitionRegistrationError> {
    let mut decoder = Decoder::new(input, 0, u32::MAX);
    let magic_offset = decoder.offset();
    let magic = decoder.read_array::<4>()?;
    if magic != DEFINITION_BUFFER_MAGIC {
        return Err(DefinitionRegistrationError::header(
            DefinitionRegistrationErrorKind::InvalidMagic,
            magic_offset,
        ));
    }

    let version_offset = decoder.offset();
    let version = decoder.read_u16()?;
    if version != DEFINITION_BUFFER_VERSION {
        return Err(DefinitionRegistrationError::header(
            DefinitionRegistrationErrorKind::UnsupportedVersion,
            version_offset,
        ));
    }

    let flags_offset = decoder.offset();
    let flags = decoder.read_u16()?;
    if flags != 0 {
        return Err(DefinitionRegistrationError::header(
            DefinitionRegistrationErrorKind::InvalidFlags,
            flags_offset,
        ));
    }

    let id_offset = decoder.offset();
    let id = decoder.read_u32()?;
    if id == 0 {
        return Err(DefinitionRegistrationError::header(
            DefinitionRegistrationErrorKind::InvalidDeviceId,
            id_offset,
        ));
    }

    let command_count = decoder.read_u32()?;
    debug_assert_eq!(decoder.offset(), DEFINITION_HEADER_LENGTH);

    let mut builder = DeviceDefinitionBuilder::new(engine.definitions());
    for command_index in 0..command_count {
        let command_offset = decoder.offset();
        decoder.command_index = command_index;
        let tag = decoder.read_u16()?;
        let payload_length = decoder.read_u32()? as usize;
        let payload_offset = decoder.offset();
        let payload = decoder.read_bytes(payload_length)?;
        let mut payload_decoder = Decoder::new(payload, payload_offset, command_index);

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
                let constraints = decode_constraints(&mut payload_decoder)?;

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
                let element = decode_element(&mut payload_decoder)?;
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

fn decode_element(decoder: &mut Decoder<'_>) -> Result<Element, DefinitionRegistrationError> {
    let id_offset = decoder.offset();
    let id = decoder.read_u32()?;

    if id == 0 {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidDeviceId,
            decoder.command_index,
            id_offset,
        ));
    }

    let device = DefinitionId::try_from(id).expect("definition ID was validated as non-zero");

    let terminal_count_offset = decoder.offset();
    let terminal_count = decoder.read_u32()? as usize;
    if terminal_count > decoder.remaining() / size_of::<u32>() {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidCount,
            decoder.command_index,
            terminal_count_offset,
        ));
    }
    let mut terminals = Vec::with_capacity(terminal_count);
    for _ in 0..terminal_count {
        terminals.push(NodeId::from(decoder.read_u32()?));
    }

    let parameter_count_offset = decoder.offset();
    let parameter_count = decoder.read_u32()? as usize;
    if parameter_count > decoder.remaining() / (size_of::<u8>() + size_of::<u32>()) {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidCount,
            decoder.command_index,
            parameter_count_offset,
        ));
    }
    let mut parameters = Vec::with_capacity(parameter_count);
    for _ in 0..parameter_count {
        let kind_offset = decoder.offset();
        let kind = decoder.read_u8()?;
        let value = match kind {
            DEFINITION_VALUE_LITERAL => ValueRef::Literal(decoder.read_f64()?),
            DEFINITION_VALUE_PARAMETER => {
                ValueRef::Parameter(ParameterId::from(decoder.read_u32()?))
            }
            _ => {
                return Err(DefinitionRegistrationError::command(
                    DefinitionRegistrationErrorKind::UnknownValueKind,
                    decoder.command_index,
                    kind_offset,
                ));
            }
        };
        parameters.push(value);
    }

    Ok(Element::new(device, terminals, parameters))
}

fn decode_constraints(
    decoder: &mut Decoder<'_>,
) -> Result<ParameterConstraints, DefinitionRegistrationError> {
    let flags_offset = decoder.offset();
    let flags = decoder.read_u16()?;

    if flags & !CONSTRAINT_KNOWN_FLAGS != 0 {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidFlags,
            decoder.command_index,
            flags_offset,
        ));
    }

    // Inclusive makes no sense unless the corresponding bound exists.
    let invalid = (flags & CONSTRAINT_LOWER_INCLUSIVE != 0 && flags & CONSTRAINT_LOWER == 0)
        || (flags & CONSTRAINT_UPPER_INCLUSIVE != 0 && flags & CONSTRAINT_UPPER == 0)
        || (flags & CONSTRAINT_RECIPROCAL_LOWER_INCLUSIVE != 0
            && flags & CONSTRAINT_RECIPROCAL_LOWER == 0)
        || (flags & CONSTRAINT_RECIPROCAL_UPPER_INCLUSIVE != 0
            && flags & CONSTRAINT_RECIPROCAL_UPPER == 0);

    if invalid {
        return Err(DefinitionRegistrationError::command(
            DefinitionRegistrationErrorKind::InvalidFlags,
            decoder.command_index,
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
            decoder.command_index,
            flags_offset,
        ));
    }

    let lower = decode_bound(decoder, flags, CONSTRAINT_LOWER, CONSTRAINT_LOWER_INCLUSIVE)?;

    let upper = decode_bound(decoder, flags, CONSTRAINT_UPPER, CONSTRAINT_UPPER_INCLUSIVE)?;

    let reciprocal_range = if flags & CONSTRAINT_RECIPROCAL_RANGE != 0 {
        let lower = decode_bound(
            decoder,
            flags,
            CONSTRAINT_RECIPROCAL_LOWER,
            CONSTRAINT_RECIPROCAL_LOWER_INCLUSIVE,
        )?;

        let upper = decode_bound(
            decoder,
            flags,
            CONSTRAINT_RECIPROCAL_UPPER,
            CONSTRAINT_RECIPROCAL_UPPER_INCLUSIVE,
        )?;

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

fn decode_bound(
    decoder: &mut Decoder<'_>,
    flags: u16,
    present: u16,
    inclusive: u16,
) -> Result<Option<Bound>, DefinitionRegistrationError> {
    if flags & present == 0 {
        return Ok(None);
    }

    Ok(Some(Bound {
        value: decoder.read_f64()?,
        inclusive: flags & inclusive != 0,
    }))
}

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
            DefinitionRegistrationErrorKind::UnknownDevice
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
            DefinitionRegistrationErrorKind::DeviceIdExhausted
        }
        RegisterDeviceError::PrimitiveRegistrationForbidden { .. }
        | RegisterDeviceError::TerminalCountExceedsNodeCount { .. } => {
            DefinitionRegistrationErrorKind::InvalidDefinition
        }
        _ => DefinitionRegistrationErrorKind::InvalidDefinition,
    };
    DefinitionRegistrationError::header(kind, u32::MAX as usize)
}

struct Decoder<'a> {
    input: &'a [u8],
    position: usize,
    base_offset: usize,
    command_index: u32,
}

impl<'a> Decoder<'a> {
    const fn new(input: &'a [u8], base_offset: usize, command_index: u32) -> Self {
        Self {
            input,
            position: 0,
            base_offset,
            command_index,
        }
    }

    const fn offset(&self) -> usize {
        self.base_offset + self.position
    }

    const fn remaining(&self) -> usize {
        self.input.len() - self.position
    }

    const fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    fn read_u8(&mut self) -> Result<u8, DefinitionRegistrationError> {
        Ok(self.read_array::<1>()?[0])
    }

    fn read_u16(&mut self) -> Result<u16, DefinitionRegistrationError> {
        Ok(u16::from_le_bytes(self.read_array()?))
    }

    fn read_u32(&mut self) -> Result<u32, DefinitionRegistrationError> {
        Ok(u32::from_le_bytes(self.read_array()?))
    }

    fn read_f64(&mut self) -> Result<f64, DefinitionRegistrationError> {
        Ok(f64::from_le_bytes(self.read_array()?))
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], DefinitionRegistrationError> {
        let bytes = self.read_bytes(N)?;
        Ok(bytes.try_into().expect("slice length was checked"))
    }

    fn read_bytes(&mut self, length: usize) -> Result<&'a [u8], DefinitionRegistrationError> {
        let Some(end) = self.position.checked_add(length) else {
            return Err(self.truncated());
        };
        if end > self.input.len() {
            return Err(self.truncated());
        }
        let bytes = &self.input[self.position..end];
        self.position = end;
        Ok(bytes)
    }

    const fn truncated(&self) -> DefinitionRegistrationError {
        DefinitionRegistrationError::new(
            DefinitionRegistrationErrorKind::TruncatedInput,
            self.command_index,
            self.base_offset + self.input.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hynergy_model::device::definition::DeviceBody;
    use hynergy_model::parameter::Bound;

    const BUFFER_MAGIC: [u8; 4] = *b"HYDF";
    const BUFFER_VERSION: u16 = 1;
    const COMMAND_ADD_TERMINAL: u16 = 1;
    const COMMAND_ADD_NODE: u16 = 2;
    const COMMAND_ADD_PARAMETER: u16 = 3;
    const COMMAND_ADD_ELEMENT: u16 = 4;
    const VALUE_LITERAL: u8 = 0;
    const VALUE_PARAMETER: u8 = 1;

    const FLAG_LOWER: u16 = 1 << 0;
    const FLAG_LOWER_INCLUSIVE: u16 = 1 << 1;
    const FLAG_UPPER: u16 = 1 << 2;
    const FLAG_UPPER_INCLUSIVE: u16 = 1 << 3;
    const FLAG_NON_ZERO: u16 = 1 << 4;
    const FLAG_RECIPROCAL_RANGE: u16 = 1 << 5;
    const FLAG_RECIPROCAL_LOWER: u16 = 1 << 6;
    const FLAG_RECIPROCAL_LOWER_INCLUSIVE: u16 = 1 << 7;
    const FLAG_RECIPROCAL_UPPER: u16 = 1 << 8;
    const FLAG_RECIPROCAL_UPPER_INCLUSIVE: u16 = 1 << 9;

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
                    bytes.push(VALUE_LITERAL);
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                TestValue::Parameter(index) => {
                    bytes.push(VALUE_PARAMETER);
                    bytes.extend_from_slice(&index.to_le_bytes());
                }
            }
        }
        bytes
    }

    fn definition_buffer(id: u32, commands: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&BUFFER_MAGIC);
        bytes.extend_from_slice(&BUFFER_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
        bytes.extend_from_slice(&(commands.len() as u32).to_le_bytes());
        for command in commands {
            bytes.extend_from_slice(command);
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
            flags |= FLAG_LOWER;
            if bound.inclusive {
                flags |= FLAG_LOWER_INCLUSIVE;
            }
            values.extend_from_slice(&bound.value.to_le_bytes());
        }

        if let Some(bound) = upper {
            flags |= FLAG_UPPER;
            if bound.inclusive {
                flags |= FLAG_UPPER_INCLUSIVE;
            }
            values.extend_from_slice(&bound.value.to_le_bytes());
        }

        if non_zero {
            flags |= FLAG_NON_ZERO;
        }

        if let Some((lower, upper)) = reciprocal_range {
            flags |= FLAG_RECIPROCAL_RANGE;

            if let Some(bound) = lower {
                flags |= FLAG_RECIPROCAL_LOWER;
                if bound.inclusive {
                    flags |= FLAG_RECIPROCAL_LOWER_INCLUSIVE;
                }
                values.extend_from_slice(&bound.value.to_le_bytes());
            }

            if let Some(bound) = upper {
                flags |= FLAG_RECIPROCAL_UPPER;
                if bound.inclusive {
                    flags |= FLAG_RECIPROCAL_UPPER_INCLUSIVE;
                }
                values.extend_from_slice(&bound.value.to_le_bytes());
            }
        }

        let mut bytes = Vec::with_capacity(2 + values.len());
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&values);
        bytes
    }

    #[test]
    fn empty_definition_registers_and_returns_its_engine_scoped_id() {
        let mut engine = Engine::new();

        register_definition_buffer(&mut engine, &definition_buffer(1, &[])).unwrap();

        let definition = engine
            .definitions()
            .get(DefinitionId::try_from(Engine::COMPOSITE_DEFINITION_ID_BASE).unwrap())
            .unwrap();
        assert_eq!(definition.terminals().len(), 0);
        assert_eq!(definition.parameters().len(), 0);
    }

    #[test]
    fn complete_builder_queue_registers_the_expected_definition() {
        let mut engine = Engine::new();
        let commands = [
            command(COMMAND_ADD_TERMINAL, &[]),
            command(COMMAND_ADD_NODE, &[]),
            command(COMMAND_ADD_TERMINAL, &[]),
            command(
                COMMAND_ADD_PARAMETER,
                &constraints_payload(None, None, false, None),
            ),
            command(
                COMMAND_ADD_ELEMENT,
                &element_payload(1, &[0, 2], &[TestValue::Parameter(0)]),
            ),
        ];

        register_definition_buffer(
            &mut engine,
            &definition_buffer(Engine::COMPOSITE_DEFINITION_ID_BASE, &commands),
        )
        .unwrap();

        let definition = engine
            .definitions()
            .get(DefinitionId::try_from(Engine::COMPOSITE_DEFINITION_ID_BASE).unwrap())
            .unwrap();
        assert_eq!(definition.terminals().len(), 2);
        assert_eq!(definition.parameters().len(), 1);
        let DeviceBody::Composite(circuit) = definition.body() else {
            panic!("registered definition was not composite");
        };
        assert_eq!(circuit.node_count(), 3);
        assert_eq!(circuit.elements().len(), 1);
        assert_eq!(circuit.elements()[0].definition().index(), 0);
        assert_eq!(circuit.elements()[0].terminals()[1].id(), 2);
    }

    #[test]
    fn malformed_headers_are_rejected_with_header_offsets() {
        let valid = definition_buffer(6, &[]);
        let mut wrong_magic = valid.clone();
        wrong_magic[0] = b'X';
        let mut wrong_version = valid.clone();
        wrong_version[4..6].copy_from_slice(&(BUFFER_VERSION + 1).to_le_bytes());
        let mut nonzero_flags = valid.clone();
        nonzero_flags[6..8].copy_from_slice(&1_u16.to_le_bytes());
        let cases = [
            (
                &valid[..5],
                DefinitionRegistrationErrorKind::TruncatedInput,
                5,
            ),
            (
                &wrong_magic[..],
                DefinitionRegistrationErrorKind::InvalidMagic,
                0,
            ),
            (
                &wrong_version[..],
                DefinitionRegistrationErrorKind::UnsupportedVersion,
                4,
            ),
            (
                &nonzero_flags[..],
                DefinitionRegistrationErrorKind::InvalidFlags,
                6,
            ),
        ];

        for (bytes, expected_kind, expected_offset) in cases {
            let error = register_definition_buffer(&mut Engine::new(), bytes).unwrap_err();
            assert_eq!(error.kind(), expected_kind);
            assert_eq!(error.command_index(), u32::MAX);
            assert_eq!(error.byte_offset(), expected_offset);
        }
    }

    #[test]
    fn truncated_unknown_and_mis_sized_commands_are_rejected_at_the_command() {
        let truncated = definition_buffer(Engine::COMPOSITE_DEFINITION_ID_BASE, &[vec![]]);
        let unknown = definition_buffer(Engine::COMPOSITE_DEFINITION_ID_BASE, &[command(99, &[])]);
        let wrong_fixed_length = definition_buffer(
            Engine::COMPOSITE_DEFINITION_ID_BASE,
            &[command(COMMAND_ADD_NODE, &[0])],
        );
        let cases = [
            (
                truncated,
                DefinitionRegistrationErrorKind::TruncatedInput,
                0,
                DEFINITION_HEADER_LENGTH,
            ),
            (
                unknown,
                DefinitionRegistrationErrorKind::UnknownCommand,
                0,
                DEFINITION_HEADER_LENGTH,
            ),
            (
                wrong_fixed_length,
                DefinitionRegistrationErrorKind::InvalidCommandLength,
                0,
                DEFINITION_HEADER_LENGTH,
            ),
        ];

        for (bytes, expected_kind, expected_index, expected_offset) in cases {
            let error = register_definition_buffer(&mut Engine::new(), &bytes).unwrap_err();
            assert_eq!(error.kind(), expected_kind);
            assert_eq!(error.command_index(), expected_index);
            assert_eq!(error.byte_offset(), expected_offset as u32);
        }
    }

    #[test]
    fn element_vector_count_cannot_read_beyond_its_command() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1_u32.to_le_bytes());
        payload.extend_from_slice(&u32::MAX.to_le_bytes());
        let bytes = definition_buffer(
            Engine::COMPOSITE_DEFINITION_ID_BASE,
            &[command(COMMAND_ADD_ELEMENT, &payload)],
        );

        let error = register_definition_buffer(&mut Engine::new(), &bytes).unwrap_err();

        assert_eq!(error.kind(), DefinitionRegistrationErrorKind::InvalidCount);
        assert_eq!(error.command_index(), 0);
        assert_eq!(error.byte_offset(), DEFINITION_HEADER_LENGTH as u32 + 10);
    }

    #[test]
    fn builder_validation_errors_have_stable_kinds_and_command_locations() {
        let cases = [
            (
                vec![command(COMMAND_ADD_ELEMENT, &element_payload(99, &[], &[]))],
                DefinitionRegistrationErrorKind::UnknownDevice,
            ),
            (
                vec![
                    command(COMMAND_ADD_TERMINAL, &[]),
                    command(COMMAND_ADD_TERMINAL, &[]),
                    command(
                        COMMAND_ADD_ELEMENT,
                        &element_payload(1, &[0], &[TestValue::Literal(1.0)]),
                    ),
                ],
                DefinitionRegistrationErrorKind::TerminalCountMismatch,
            ),
            (
                vec![
                    command(COMMAND_ADD_TERMINAL, &[]),
                    command(COMMAND_ADD_TERMINAL, &[]),
                    command(COMMAND_ADD_ELEMENT, &element_payload(1, &[0, 1], &[])),
                ],
                DefinitionRegistrationErrorKind::ParameterCountMismatch,
            ),
            (
                vec![
                    command(COMMAND_ADD_NODE, &[]),
                    command(
                        COMMAND_ADD_ELEMENT,
                        &element_payload(1, &[0, 1], &[TestValue::Literal(1.0)]),
                    ),
                ],
                DefinitionRegistrationErrorKind::NodeOutOfRange,
            ),
            (
                vec![
                    command(COMMAND_ADD_TERMINAL, &[]),
                    command(COMMAND_ADD_TERMINAL, &[]),
                    command(
                        COMMAND_ADD_ELEMENT,
                        &element_payload(1, &[0, 1], &[TestValue::Parameter(0)]),
                    ),
                ],
                DefinitionRegistrationErrorKind::ParameterOutOfRange,
            ),
            (
                vec![
                    command(COMMAND_ADD_TERMINAL, &[]),
                    command(COMMAND_ADD_TERMINAL, &[]),
                    command(
                        COMMAND_ADD_ELEMENT,
                        &element_payload(1, &[0, 1], &[TestValue::Literal(f64::NAN)]),
                    ),
                ],
                DefinitionRegistrationErrorKind::ParameterConstraintViolation,
            ),
        ];

        for (commands, expected_kind) in cases {
            let expected_index = (commands.len() - 1) as u32;
            let expected_offset = DEFINITION_HEADER_LENGTH as u32
                + commands[..commands.len() - 1]
                    .iter()
                    .map(Vec::len)
                    .sum::<usize>() as u32;

            let error = register_definition_buffer(
                &mut Engine::new(),
                &definition_buffer(Engine::COMPOSITE_DEFINITION_ID_BASE, &commands),
            )
            .unwrap_err();

            assert_eq!(error.kind(), expected_kind);
            assert_eq!(error.command_index(), expected_index);
            assert_eq!(error.byte_offset(), expected_offset);
        }
    }

    #[test]
    fn failed_buffer_does_not_publish_or_consume_a_device_id() {
        let mut engine = Engine::new();
        let invalid = definition_buffer(
            Engine::COMPOSITE_DEFINITION_ID_BASE,
            &[command(COMMAND_ADD_TERMINAL, &[]), command(99, &[])],
        );

        let failure = register_definition_buffer(&mut engine, &invalid).unwrap_err();
        assert_eq!(
            failure.kind(),
            DefinitionRegistrationErrorKind::UnknownCommand
        );

        assert!(
            engine
                .definitions()
                .get(DefinitionId::try_from(Engine::COMPOSITE_DEFINITION_ID_BASE,).unwrap())
                .is_none()
        );
    }

    #[test]
    fn incorrect_command_count_and_trailing_bytes_are_rejected() {
        let mut missing_command = definition_buffer(
            Engine::COMPOSITE_DEFINITION_ID_BASE,
            &[command(COMMAND_ADD_NODE, &[])],
        );
        missing_command[12..16].copy_from_slice(&2_u32.to_le_bytes());
        let mut trailing_command = definition_buffer(Engine::COMPOSITE_DEFINITION_ID_BASE, &[]);
        trailing_command.extend_from_slice(&command(COMMAND_ADD_NODE, &[]));
        let cases = [
            (
                missing_command,
                DefinitionRegistrationErrorKind::TruncatedInput,
                1,
                22,
            ),
            (
                trailing_command,
                DefinitionRegistrationErrorKind::TrailingBytes,
                0,
                16,
            ),
        ];

        for (bytes, expected_kind, expected_index, expected_offset) in cases {
            let mut engine = Engine::new();
            let error = register_definition_buffer(&mut engine, &bytes).unwrap_err();
            assert_eq!(error.kind(), expected_kind);
            assert_eq!(error.command_index(), expected_index);
            assert_eq!(error.byte_offset(), expected_offset);
            assert!(
                engine
                    .definitions()
                    .get(DefinitionId::try_from(Engine::COMPOSITE_DEFINITION_ID_BASE).unwrap())
                    .is_none()
            );
        }
    }
}
