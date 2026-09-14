use crate::decoder::{Decoder, Truncated};
use hynergy_engine::{Engine, WorldCommand, WorldCommandApplyError};
use hynergy_model::device::definition::{DefinitionId, DeviceId, TerminalId};
use hynergy_model::network::{NetworkModelError, WireId};
use hynergy_model::parameter::ParameterId;

pub const WORLD_COMMAND_BUFFER_VERSION: u16 = 1;
pub const WORLD_COMMAND_BUFFER_MAGIC: [u8; 4] = *b"HYWC";

pub const WORLD_COMMAND_ADD_WIRE: u16 = 1;
pub const WORLD_COMMAND_REMOVE_WIRE: u16 = 2;
pub const WORLD_COMMAND_CONNECT_WIRES: u16 = 3;
pub const WORLD_COMMAND_DISCONNECT_WIRES: u16 = 4;
pub const WORLD_COMMAND_ADD_DEVICE: u16 = 5;
pub const WORLD_COMMAND_REMOVE_DEVICE: u16 = 6;
pub const WORLD_COMMAND_ATTACH_TERMINAL: u16 = 7;
pub const WORLD_COMMAND_DETACH_TERMINAL: u16 = 8;
pub const WORLD_COMMAND_SET_DEVICE_PARAMETER: u16 = 9;

const WORLD_COMMAND_HEADER_LENGTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldCommandErrorKind {
    InvalidMagic,
    UnsupportedVersion,
    InvalidFlags,
    InvalidReserved,
    TruncatedInput,
    UnknownCommand,
    InvalidCommandLength,
    InvalidId,
    TrailingBytes,
    UnknownWorld,

    IdOutOfBound,
    IdExceeds31Bit,
    IdAlreadyAssigned,
    IdNotAssigned,
    WireConnectToSelf,
    AlreadyConnected,
    NotConnected,
    TerminalAlreadyConnected,
    InvalidTerminal,
    InvalidParameter,
    ParameterConstraintViolation,
    UnknownDefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldCommandError {
    kind: WorldCommandErrorKind,
    command_index: u32,
    byte_offset: u32,
}

impl WorldCommandError {
    const fn new(kind: WorldCommandErrorKind, command_index: u32, byte_offset: usize) -> Self {
        Self {
            kind,
            command_index,
            byte_offset: byte_offset as u32,
        }
    }

    #[inline]
    const fn header(kind: WorldCommandErrorKind, byte_offset: usize) -> Self {
        Self::new(kind, u32::MAX, byte_offset)
    }

    #[inline]
    const fn world(kind: WorldCommandErrorKind) -> Self {
        Self {
            kind,
            command_index: u32::MAX,
            byte_offset: u32::MAX,
        }
    }

    #[inline]
    pub const fn kind(self) -> WorldCommandErrorKind {
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

pub fn apply_world_command_buffer(
    engine: &mut Engine,
    world_id: u32,
    input: &[u8],
) -> Result<(), WorldCommandError> {
    let mut decoder = Decoder::new(input, 0);

    let magic_offset = decoder.offset();
    let magic = decoder.read_array::<4>().map_err(truncated_header)?;
    if magic != WORLD_COMMAND_BUFFER_MAGIC {
        return Err(WorldCommandError::header(
            WorldCommandErrorKind::InvalidMagic,
            magic_offset,
        ));
    }

    let version_offset = decoder.offset();
    let version = decoder.read_u16().map_err(truncated_header)?;
    if version != WORLD_COMMAND_BUFFER_VERSION {
        return Err(WorldCommandError::header(
            WorldCommandErrorKind::UnsupportedVersion,
            version_offset,
        ));
    }

    let flags_offset = decoder.offset();
    let flags = decoder.read_u16().map_err(truncated_header)?;
    if flags != 0 {
        return Err(WorldCommandError::header(
            WorldCommandErrorKind::InvalidFlags,
            flags_offset,
        ));
    }

    let reserved_offset = decoder.offset();
    let reserved = decoder.read_u32().map_err(truncated_header)?;
    if reserved != 0 {
        return Err(WorldCommandError::header(
            WorldCommandErrorKind::InvalidReserved,
            reserved_offset,
        ));
    }

    let command_count = decoder.read_u32().map_err(truncated_header)?;

    debug_assert_eq!(decoder.offset(), WORLD_COMMAND_HEADER_LENGTH);

    if !engine.contains_world(world_id) {
        return Err(WorldCommandError::world(
            WorldCommandErrorKind::UnknownWorld,
        ));
    }

    for command_index in 0..command_count {
        let command_offset = decoder.offset();

        let tag = decoder
            .read_u16()
            .map_err(|e| truncated(e, command_index))?;
        let payload_length = decoder
            .read_u32()
            .map_err(|e| truncated(e, command_index))? as usize;

        let Some(expected_length) = expected_world_payload_length(tag) else {
            return Err(WorldCommandError::new(
                WorldCommandErrorKind::UnknownCommand,
                command_index,
                command_offset,
            ));
        };

        if payload_length != expected_length {
            return Err(WorldCommandError::new(
                WorldCommandErrorKind::InvalidCommandLength,
                command_index,
                command_offset,
            ));
        }

        let payload_offset = decoder.offset();
        let payload = decoder
            .read_bytes(payload_length)
            .map_err(|e| truncated(e, command_index))?;

        let mut payload_decoder = Decoder::new(payload, payload_offset);

        let command =
            decode_world_command(tag, &mut payload_decoder, command_index, command_offset)?;

        if !payload_decoder.is_empty() {
            return Err(WorldCommandError::new(
                WorldCommandErrorKind::InvalidCommandLength,
                command_index,
                command_offset,
            ));
        }

        if let Err(error) = engine.apply_world_command(world_id, command) {
            return Err(map_world_apply_error(error, command_index, command_offset));
        }
    }

    if !decoder.is_empty() {
        return Err(WorldCommandError::new(
            WorldCommandErrorKind::TrailingBytes,
            command_count,
            decoder.offset(),
        ));
    }

    Ok(())
}

fn decode_world_command(
    tag: u16,
    decoder: &mut Decoder<'_>,
    command_index: u32,
    command_offset: usize,
) -> Result<WorldCommand, WorldCommandError> {
    let command = match tag {
        WORLD_COMMAND_ADD_WIRE => {
            let wire = decode_non_zero_id::<WireId>(decoder, command_index)?;
            WorldCommand::AddWire { wire }
        }

        WORLD_COMMAND_REMOVE_WIRE => {
            let wire = decode_non_zero_id::<WireId>(decoder, command_index)?;
            WorldCommand::RemoveWire { wire }
        }

        WORLD_COMMAND_CONNECT_WIRES => {
            let wire_a = decode_non_zero_id::<WireId>(decoder, command_index)?;
            let wire_b = decode_non_zero_id::<WireId>(decoder, command_index)?;

            WorldCommand::ConnectWires { wire_a, wire_b }
        }

        WORLD_COMMAND_DISCONNECT_WIRES => {
            let wire_a = decode_non_zero_id::<WireId>(decoder, command_index)?;
            let wire_b = decode_non_zero_id::<WireId>(decoder, command_index)?;

            WorldCommand::DisconnectWires { wire_a, wire_b }
        }

        WORLD_COMMAND_ADD_DEVICE => {
            let device = decode_non_zero_id::<DeviceId>(decoder, command_index)?;
            let definition = decode_non_zero_id::<DefinitionId>(decoder, command_index)?;

            WorldCommand::AddDevice { device, definition }
        }

        WORLD_COMMAND_REMOVE_DEVICE => {
            let device = decode_non_zero_id::<DeviceId>(decoder, command_index)?;

            WorldCommand::RemoveDevice { device }
        }

        WORLD_COMMAND_ATTACH_TERMINAL => {
            let wire = decode_non_zero_id::<WireId>(decoder, command_index)?;
            let device = decode_non_zero_id::<DeviceId>(decoder, command_index)?;
            let terminal = TerminalId::from(
                decoder
                    .read_u32()
                    .map_err(|e| truncated(e, command_index))?,
            );

            WorldCommand::AttachTerminal {
                wire,
                device,
                terminal,
            }
        }

        WORLD_COMMAND_DETACH_TERMINAL => {
            let wire = decode_non_zero_id::<WireId>(decoder, command_index)?;
            let device = decode_non_zero_id::<DeviceId>(decoder, command_index)?;
            let terminal = TerminalId::from(
                decoder
                    .read_u32()
                    .map_err(|e| truncated(e, command_index))?,
            );

            WorldCommand::DetachTerminal {
                wire,
                device,
                terminal,
            }
        }

        WORLD_COMMAND_SET_DEVICE_PARAMETER => {
            let device = decode_non_zero_id::<DeviceId>(decoder, command_index)?;
            let parameter = ParameterId::from(
                decoder
                    .read_u32()
                    .map_err(|e| truncated(e, command_index))?,
            );
            let value = decoder
                .read_f64()
                .map_err(|e| truncated(e, command_index))?;

            WorldCommand::SetDeviceParameter {
                device,
                parameter,
                value,
            }
        }

        _ => {
            return Err(WorldCommandError::new(
                WorldCommandErrorKind::UnknownCommand,
                command_index,
                command_offset,
            ));
        }
    };

    Ok(command)
}

#[inline]
fn decode_non_zero_id<T>(
    decoder: &mut Decoder<'_>,
    command_index: u32,
) -> Result<T, WorldCommandError>
where
    T: TryFrom<u32>,
{
    let offset = decoder.offset();
    let raw = decoder
        .read_u32()
        .map_err(|e| truncated(e, command_index))?;

    T::try_from(raw).map_err(|_| {
        WorldCommandError::new(WorldCommandErrorKind::InvalidId, command_index, offset)
    })
}

fn map_world_apply_error(
    error: WorldCommandApplyError,
    command_index: u32,
    command_offset: usize,
) -> WorldCommandError {
    let kind = match error {
        WorldCommandApplyError::UnknownWorld => WorldCommandErrorKind::UnknownWorld,

        WorldCommandApplyError::Model(error) => match error {
            NetworkModelError::IdOutOfBound { .. } => WorldCommandErrorKind::IdOutOfBound,
            NetworkModelError::IdExceeds31Bit { .. } => WorldCommandErrorKind::IdExceeds31Bit,
            NetworkModelError::IdAlreadyAssigned { .. } => WorldCommandErrorKind::IdAlreadyAssigned,
            NetworkModelError::IdNotAssigned { .. } => WorldCommandErrorKind::IdNotAssigned,
            NetworkModelError::WireConnectToSelf => WorldCommandErrorKind::WireConnectToSelf,
            NetworkModelError::AlreadyConnected => WorldCommandErrorKind::AlreadyConnected,
            NetworkModelError::NotConnected => WorldCommandErrorKind::NotConnected,
            NetworkModelError::TerminalAlreadyConnected => {
                WorldCommandErrorKind::TerminalAlreadyConnected
            }
            NetworkModelError::InvalidTerminal => WorldCommandErrorKind::InvalidTerminal,
            NetworkModelError::InvalidParameter { .. } => WorldCommandErrorKind::InvalidParameter,
            NetworkModelError::ParameterConstraint { .. } => {
                WorldCommandErrorKind::ParameterConstraintViolation
            }
            NetworkModelError::UnknownDefinition { .. } => WorldCommandErrorKind::UnknownDefinition,
        },
    };

    WorldCommandError::new(kind, command_index, command_offset)
}

fn expected_world_payload_length(tag: u16) -> Option<usize> {
    match tag {
        WORLD_COMMAND_ADD_WIRE | WORLD_COMMAND_REMOVE_WIRE | WORLD_COMMAND_REMOVE_DEVICE => Some(4),

        WORLD_COMMAND_CONNECT_WIRES | WORLD_COMMAND_DISCONNECT_WIRES | WORLD_COMMAND_ADD_DEVICE => {
            Some(8)
        }

        WORLD_COMMAND_ATTACH_TERMINAL | WORLD_COMMAND_DETACH_TERMINAL => Some(12),

        WORLD_COMMAND_SET_DEVICE_PARAMETER => Some(16),

        _ => None,
    }
}

#[inline]
fn truncated(error: Truncated, command_index: u32) -> WorldCommandError {
    WorldCommandError::new(
        WorldCommandErrorKind::TruncatedInput,
        command_index,
        error.offset,
    )
}

#[inline]
fn truncated_header(error: Truncated) -> WorldCommandError {
    WorldCommandError::header(WorldCommandErrorKind::TruncatedInput, error.offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hynergy_model::device::definition::PrimitiveElementKind;
    use hynergy_model::network::ConnectionType;
    use hynergy_model::parameter::{ParameterConstraintError, ParameterId};
    use std::num::NonZeroU32;

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

    fn buffer(commands: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&WORLD_COMMAND_BUFFER_MAGIC);
        bytes.extend_from_slice(&WORLD_COMMAND_BUFFER_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&(commands.len() as u32).to_le_bytes());

        for command in commands {
            bytes.extend_from_slice(command);
        }

        bytes
    }

    fn u32_payload(values: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(values.len() * 4);

        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        bytes
    }

    fn parameter_payload(device: u32, parameter: u32, value: f64) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16);
        bytes.extend_from_slice(&device.to_le_bytes());
        bytes.extend_from_slice(&parameter.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
        bytes
    }

    fn assert_error(
        engine: &mut Engine,
        world: u32,
        bytes: &[u8],
        kind: WorldCommandErrorKind,
        command_index: u32,
        byte_offset: u32,
    ) {
        let error = apply_world_command_buffer(engine, world, bytes).unwrap_err();

        assert_eq!(error.kind(), kind);
        assert_eq!(error.command_index(), command_index);
        assert_eq!(error.byte_offset(), byte_offset);
    }

    #[test]
    fn empty_buffer_succeeds() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();

        apply_world_command_buffer(&mut engine, world, &buffer(&[])).unwrap();
    }

    #[test]
    fn every_world_command_is_decoded_and_applied() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();
        let definition = DefinitionId::from(PrimitiveElementKind::Conductance).get();

        let commands = [
            command(WORLD_COMMAND_ADD_WIRE, &u32_payload(&[1])),
            command(WORLD_COMMAND_ADD_WIRE, &u32_payload(&[2])),
            command(WORLD_COMMAND_CONNECT_WIRES, &u32_payload(&[1, 2])),
            command(WORLD_COMMAND_DISCONNECT_WIRES, &u32_payload(&[1, 2])),
            command(WORLD_COMMAND_ADD_DEVICE, &u32_payload(&[1, definition])),
            command(WORLD_COMMAND_ATTACH_TERMINAL, &u32_payload(&[1, 1, 0])),
            command(WORLD_COMMAND_DETACH_TERMINAL, &u32_payload(&[1, 1, 0])),
            command(
                WORLD_COMMAND_SET_DEVICE_PARAMETER,
                &parameter_payload(1, 0, 1.0),
            ),
            command(WORLD_COMMAND_REMOVE_DEVICE, &u32_payload(&[1])),
            command(WORLD_COMMAND_REMOVE_WIRE, &u32_payload(&[2])),
            command(WORLD_COMMAND_REMOVE_WIRE, &u32_payload(&[1])),
        ];

        apply_world_command_buffer(&mut engine, world, &buffer(&commands)).unwrap();

        let network = engine.world(world).unwrap().network();

        assert!(network.wires().iter().all(Option::is_none));
        assert!(network.devices().iter().all(Option::is_none));
    }

    #[test]
    fn wire_and_terminal_commands_create_expected_connections() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();
        let definition = DefinitionId::from(PrimitiveElementKind::Conductance).get();

        let commands = [
            command(WORLD_COMMAND_ADD_WIRE, &u32_payload(&[1])),
            command(WORLD_COMMAND_ADD_WIRE, &u32_payload(&[2])),
            command(WORLD_COMMAND_CONNECT_WIRES, &u32_payload(&[1, 2])),
            command(WORLD_COMMAND_ADD_DEVICE, &u32_payload(&[1, definition])),
            command(WORLD_COMMAND_ATTACH_TERMINAL, &u32_payload(&[1, 1, 0])),
        ];

        apply_world_command_buffer(&mut engine, world, &buffer(&commands)).unwrap();

        let network = engine.world(world).unwrap().network();
        let wire_a = WireId::try_from(1).unwrap();
        let wire_b = WireId::try_from(2).unwrap();
        let device = DeviceId::try_from(1).unwrap();
        let terminal = TerminalId::new(0);

        let connections = network.wire_connections(wire_a).unwrap();

        assert!(
            connections
                .iter()
                .any(|connection| connection.as_wire() == Some(wire_b))
        );
        assert!(
            connections
                .iter()
                .any(|connection| { connection.as_terminal() == Some((device, terminal)) })
        );
    }

    #[test]
    fn truncated_header_reports_input_end() {
        let bytes = buffer(&[]);

        for length in [0, 1, 3, 5, 7, 11, 15] {
            assert_error(
                &mut Engine::new(),
                0,
                &bytes[..length],
                WorldCommandErrorKind::TruncatedInput,
                u32::MAX,
                length as u32,
            );
        }
    }

    #[test]
    fn invalid_header_fields_report_their_offsets() {
        let valid = buffer(&[]);

        let mut invalid_magic = valid.clone();
        invalid_magic[0] = b'X';

        let mut invalid_version = valid.clone();
        invalid_version[4..6].copy_from_slice(&(WORLD_COMMAND_BUFFER_VERSION + 1).to_le_bytes());

        let mut invalid_flags = valid.clone();
        invalid_flags[6..8].copy_from_slice(&1_u16.to_le_bytes());

        let mut invalid_reserved = valid;
        invalid_reserved[8..12].copy_from_slice(&1_u32.to_le_bytes());

        for (bytes, kind, offset) in [
            (invalid_magic, WorldCommandErrorKind::InvalidMagic, 0),
            (
                invalid_version,
                WorldCommandErrorKind::UnsupportedVersion,
                4,
            ),
            (invalid_flags, WorldCommandErrorKind::InvalidFlags, 6),
            (invalid_reserved, WorldCommandErrorKind::InvalidReserved, 8),
        ] {
            assert_error(&mut Engine::new(), 0, &bytes, kind, u32::MAX, offset);
        }
    }

    #[test]
    fn unknown_world_is_reported_before_commands() {
        let mut engine = Engine::new();

        assert_error(
            &mut engine,
            42,
            &buffer(&[]),
            WorldCommandErrorKind::UnknownWorld,
            u32::MAX,
            u32::MAX,
        );
    }

    #[test]
    fn truncated_command_frame_reports_command_index() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();

        let mut bytes = buffer(&[]);
        bytes[12..16].copy_from_slice(&1_u32.to_le_bytes());

        assert_error(
            &mut engine,
            world,
            &bytes,
            WorldCommandErrorKind::TruncatedInput,
            0,
            WORLD_COMMAND_HEADER_LENGTH as u32,
        );
    }

    #[test]
    fn truncated_payload_reports_input_end() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();

        let bytes = buffer(&[framed_command(WORLD_COMMAND_ADD_WIRE, 4, &[1, 0])]);

        assert_error(
            &mut engine,
            world,
            &bytes,
            WorldCommandErrorKind::TruncatedInput,
            0,
            24,
        );
    }

    #[test]
    fn unknown_command_is_rejected_at_command_start() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();

        assert_error(
            &mut engine,
            world,
            &buffer(&[command(99, &[])]),
            WorldCommandErrorKind::UnknownCommand,
            0,
            WORLD_COMMAND_HEADER_LENGTH as u32,
        );
    }

    #[test]
    fn every_fixed_payload_length_is_enforced() {
        let cases = [
            (WORLD_COMMAND_ADD_WIRE, 4),
            (WORLD_COMMAND_REMOVE_WIRE, 4),
            (WORLD_COMMAND_CONNECT_WIRES, 8),
            (WORLD_COMMAND_DISCONNECT_WIRES, 8),
            (WORLD_COMMAND_ADD_DEVICE, 8),
            (WORLD_COMMAND_REMOVE_DEVICE, 4),
            (WORLD_COMMAND_ATTACH_TERMINAL, 12),
            (WORLD_COMMAND_DETACH_TERMINAL, 12),
            (WORLD_COMMAND_SET_DEVICE_PARAMETER, 16),
        ];

        for (tag, expected_length) in cases {
            let mut engine = Engine::new();
            let world = engine.new_world().unwrap();
            let payload = vec![0; expected_length - 1];

            assert_error(
                &mut engine,
                world,
                &buffer(&[command(tag, &payload)]),
                WorldCommandErrorKind::InvalidCommandLength,
                0,
                WORLD_COMMAND_HEADER_LENGTH as u32,
            );
        }
    }

    #[test]
    fn expected_payload_lengths_are_stable() {
        assert_eq!(
            expected_world_payload_length(WORLD_COMMAND_ADD_WIRE),
            Some(4)
        );
        assert_eq!(
            expected_world_payload_length(WORLD_COMMAND_REMOVE_WIRE),
            Some(4)
        );
        assert_eq!(
            expected_world_payload_length(WORLD_COMMAND_CONNECT_WIRES),
            Some(8)
        );
        assert_eq!(
            expected_world_payload_length(WORLD_COMMAND_DISCONNECT_WIRES),
            Some(8)
        );
        assert_eq!(
            expected_world_payload_length(WORLD_COMMAND_ADD_DEVICE),
            Some(8)
        );
        assert_eq!(
            expected_world_payload_length(WORLD_COMMAND_REMOVE_DEVICE),
            Some(4)
        );
        assert_eq!(
            expected_world_payload_length(WORLD_COMMAND_ATTACH_TERMINAL),
            Some(12)
        );
        assert_eq!(
            expected_world_payload_length(WORLD_COMMAND_DETACH_TERMINAL),
            Some(12)
        );
        assert_eq!(
            expected_world_payload_length(WORLD_COMMAND_SET_DEVICE_PARAMETER),
            Some(16)
        );
        assert_eq!(expected_world_payload_length(0), None);
        assert_eq!(expected_world_payload_length(u16::MAX), None);
    }

    #[test]
    fn zero_non_zero_id_is_rejected() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();

        assert_error(
            &mut engine,
            world,
            &buffer(&[command(WORLD_COMMAND_ADD_WIRE, &u32_payload(&[0]))]),
            WorldCommandErrorKind::InvalidId,
            0,
            22,
        );
    }

    #[test]
    fn zero_definition_id_is_rejected() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();

        assert_error(
            &mut engine,
            world,
            &buffer(&[command(WORLD_COMMAND_ADD_DEVICE, &u32_payload(&[1, 0]))]),
            WorldCommandErrorKind::InvalidId,
            0,
            26,
        );
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();
        let mut bytes = buffer(&[]);
        bytes.push(0);

        assert_error(
            &mut engine,
            world,
            &bytes,
            WorldCommandErrorKind::TrailingBytes,
            0,
            WORLD_COMMAND_HEADER_LENGTH as u32,
        );
    }

    #[test]
    fn earlier_commands_remain_applied_after_parse_failure() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();

        let first = command(WORLD_COMMAND_ADD_WIRE, &u32_payload(&[1]));
        let malformed = framed_command(WORLD_COMMAND_ADD_WIRE, 4, &[2, 0]);

        let error = apply_world_command_buffer(&mut engine, world, &buffer(&[first, malformed]))
            .unwrap_err();

        assert_eq!(error.kind(), WorldCommandErrorKind::TruncatedInput);
        assert_eq!(error.command_index(), 1);

        assert!(
            engine
                .world(world)
                .unwrap()
                .network()
                .wire_connections(WireId::try_from(1).unwrap())
                .is_ok()
        );
    }

    #[test]
    fn earlier_commands_remain_applied_after_model_failure() {
        let mut engine = Engine::new();
        let world = engine.new_world().unwrap();

        let add = command(WORLD_COMMAND_ADD_WIRE, &u32_payload(&[1]));

        let error = apply_world_command_buffer(&mut engine, world, &buffer(&[add.clone(), add]))
            .unwrap_err();

        assert_eq!(error.kind(), WorldCommandErrorKind::IdAlreadyAssigned);
        assert_eq!(error.command_index(), 1);

        assert!(
            engine
                .world(world)
                .unwrap()
                .network()
                .wire_connections(WireId::try_from(1).unwrap())
                .is_ok()
        );
    }

    #[test]
    fn model_errors_map_to_stable_protocol_errors() {
        let id = NonZeroU32::new(1).unwrap();
        let parameter = ParameterId::new(0);

        let cases = [
            (
                NetworkModelError::IdOutOfBound { id, upper_bound: 0 },
                WorldCommandErrorKind::IdOutOfBound,
            ),
            (
                NetworkModelError::IdExceeds31Bit { id },
                WorldCommandErrorKind::IdExceeds31Bit,
            ),
            (
                NetworkModelError::IdAlreadyAssigned { id },
                WorldCommandErrorKind::IdAlreadyAssigned,
            ),
            (
                NetworkModelError::IdNotAssigned {
                    ty: ConnectionType::Wire,
                    id,
                },
                WorldCommandErrorKind::IdNotAssigned,
            ),
            (
                NetworkModelError::WireConnectToSelf,
                WorldCommandErrorKind::WireConnectToSelf,
            ),
            (
                NetworkModelError::AlreadyConnected,
                WorldCommandErrorKind::AlreadyConnected,
            ),
            (
                NetworkModelError::NotConnected,
                WorldCommandErrorKind::NotConnected,
            ),
            (
                NetworkModelError::TerminalAlreadyConnected,
                WorldCommandErrorKind::TerminalAlreadyConnected,
            ),
            (
                NetworkModelError::InvalidTerminal,
                WorldCommandErrorKind::InvalidTerminal,
            ),
            (
                NetworkModelError::InvalidParameter { parameter },
                WorldCommandErrorKind::InvalidParameter,
            ),
            (
                NetworkModelError::ParameterConstraint {
                    parameter,
                    source: ParameterConstraintError::OutOfRange,
                },
                WorldCommandErrorKind::ParameterConstraintViolation,
            ),
            (
                NetworkModelError::UnknownDefinition {
                    definition: DefinitionId::try_from(99).unwrap(),
                },
                WorldCommandErrorKind::UnknownDefinition,
            ),
        ];

        for (error, expected_kind) in cases {
            let error = map_world_apply_error(WorldCommandApplyError::Model(error), 3, 42);

            assert_eq!(error.kind(), expected_kind);
            assert_eq!(error.command_index(), 3);
            assert_eq!(error.byte_offset(), 42);
        }
    }

    #[test]
    fn engine_unknown_world_error_maps_to_protocol_error() {
        let error = map_world_apply_error(WorldCommandApplyError::UnknownWorld, 3, 42);

        assert_eq!(error.kind(), WorldCommandErrorKind::UnknownWorld);
        assert_eq!(error.command_index(), 3);
        assert_eq!(error.byte_offset(), 42);
    }
}
