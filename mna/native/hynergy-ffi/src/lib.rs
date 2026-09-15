use hynergy_engine::{Engine, WorldManagementError};
use hynergy_protocol::{
    DefinitionRegistrationError, DefinitionRegistrationErrorKind, WorldCommandError,
    WorldCommandErrorKind,
};
use std::panic::{AssertUnwindSafe, catch_unwind};

pub const ABI_VERSION: u32 = 1;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionRegistrationCode {
    Success = 0,
    NullEngine = 1,
    NullInput = 2,
    NullResult = 3,
    InputTooLarge = 4,

    InvalidMagic = 5,
    UnsupportedVersion = 6,
    InvalidFlags = 7,
    TruncatedInput = 8,
    UnknownCommand = 9,
    InvalidCommandLength = 10,
    InvalidCount = 11,
    UnknownValueKind = 12,
    TrailingBytes = 13,
    InvalidReserved = 14,
    InvalidDefinitionId = 15,

    UnknownDefinition = 20,
    TerminalCountMismatch = 21,
    ParameterCountMismatch = 22,
    NodeOutOfRange = 23,
    ParameterOutOfRange = 24,
    ParameterConstraintViolation = 25,
    NodeIdExhausted = 26,
    ParameterIdExhausted = 27,
    DefinitionIdExhausted = 28,
    InvalidDefinition = 29,
    InvalidPrimitiveParameters = 30,
    UnusedInternalNode = 31,
    DisconnectedInternalComponent = 32,
    IncompatibleParameterConstraints = 33,
    UnusedParameter = 34,
    DevicePartitionIdExhausted = 35,
    StateCountExhausted = 36,

    InternalPanic = u32::MAX,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionRegistrationResult {
    pub code: u32,
    pub command_index: u32,
    pub byte_offset: u32,
    pub definition_id: u32,
}

impl DefinitionRegistrationResult {
    const fn success(definition_id: u32) -> Self {
        Self {
            code: DefinitionRegistrationCode::Success as u32,
            command_index: u32::MAX,
            byte_offset: u32::MAX,
            definition_id,
        }
    }

    const fn failure(
        code: DefinitionRegistrationCode,
        command_index: u32,
        byte_offset: u32,
    ) -> Self {
        Self {
            code: code as u32,
            command_index,
            byte_offset,
            definition_id: u32::MAX,
        }
    }
}

/// Returns the ABI version that this library implements.
///
/// The caller must check this value before it calls other ABI functions.
#[unsafe(no_mangle)]
pub extern "C" fn hynergy_abi_version() -> u32 {
    ABI_VERSION
}

/// Creates an engine.
///
/// The returned pointer owns the engine. The caller must pass the pointer to
/// [`hynergy_engine_destroy`] when the engine is no longer necessary.
#[unsafe(no_mangle)]
pub extern "C" fn hynergy_engine_create() -> *mut Engine {
    Box::into_raw(Box::new(Engine::new()))
}

/// Destroys an engine and all resources that it owns.
///
/// This function has no effect if `engine` is null.
///
/// # Safety
///
/// `engine` must have been returned by `hynergy_engine_create`,
/// and it must not have been destroyed previously. The caller must prevent
/// concurrent access to the engine during this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_engine_destroy(engine: *mut Engine) {
    if !engine.is_null() {
        unsafe {
            drop(Box::from_raw(engine));
        }
    }
}

/// Registers the device definition in a definition command buffer.
///
/// On success, `definition_id` contains the engine-assigned definition ID.
/// The caller must use that ID in later operations that reference this
/// definition, including world `AddDevice` commands.
///
/// A successful result uses `u32::MAX` for `command_index` and `byte_offset`.
/// A failed result uses `u32::MAX` for `definition_id`.
///
/// # Safety
///
/// If `engine` is not null, it must point to a live [`Engine`] with exclusive
/// access for this call. If `input` is not null, it must point to `input_len`
/// readable bytes. If `result` is not null, it must point to writable storage
/// for one [`DefinitionRegistrationResult`]. The input, result, and engine
/// storage must not overlap. The caller must prevent concurrent use of the
/// same engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_engine_register_definition(
    engine: *mut Engine,
    input: *const u8,
    input_len: usize,
    result: *mut DefinitionRegistrationResult,
) -> u32 {
    if result.is_null() {
        return DefinitionRegistrationCode::NullResult as u32;
    }

    let output = if engine.is_null() {
        DefinitionRegistrationResult::failure(
            DefinitionRegistrationCode::NullEngine,
            u32::MAX,
            u32::MAX,
        )
    } else if input.is_null() {
        DefinitionRegistrationResult::failure(
            DefinitionRegistrationCode::NullInput,
            u32::MAX,
            u32::MAX,
        )
    } else if input_len > u32::MAX as usize {
        DefinitionRegistrationResult::failure(
            DefinitionRegistrationCode::InputTooLarge,
            u32::MAX,
            u32::MAX,
        )
    } else {
        let registration = catch_unwind(AssertUnwindSafe(|| {
            let engine = unsafe { &mut *engine };
            let input = unsafe { std::slice::from_raw_parts(input, input_len) };
            hynergy_protocol::register_definition_buffer(engine, input)
        }));
        match registration {
            Ok(Ok(definition_id)) => DefinitionRegistrationResult::success(definition_id.get()),
            Ok(Err(error)) => map_registration_error(error),
            Err(_) => DefinitionRegistrationResult::failure(
                DefinitionRegistrationCode::InternalPanic,
                u32::MAX,
                u32::MAX,
            ),
        }
    };

    let code = output.code;
    unsafe {
        result.write(output);
    }
    code
}

fn map_registration_error(error: DefinitionRegistrationError) -> DefinitionRegistrationResult {
    let code = match error.kind() {
        DefinitionRegistrationErrorKind::InvalidMagic => DefinitionRegistrationCode::InvalidMagic,
        DefinitionRegistrationErrorKind::UnsupportedVersion => {
            DefinitionRegistrationCode::UnsupportedVersion
        }
        DefinitionRegistrationErrorKind::InvalidFlags => DefinitionRegistrationCode::InvalidFlags,
        DefinitionRegistrationErrorKind::InvalidReserved => {
            DefinitionRegistrationCode::InvalidReserved
        }
        DefinitionRegistrationErrorKind::TruncatedInput => {
            DefinitionRegistrationCode::TruncatedInput
        }
        DefinitionRegistrationErrorKind::UnknownCommand => {
            DefinitionRegistrationCode::UnknownCommand
        }
        DefinitionRegistrationErrorKind::InvalidCommandLength => {
            DefinitionRegistrationCode::InvalidCommandLength
        }
        DefinitionRegistrationErrorKind::InvalidCount => DefinitionRegistrationCode::InvalidCount,
        DefinitionRegistrationErrorKind::InvalidDefinitionId => {
            DefinitionRegistrationCode::InvalidDefinitionId
        }
        DefinitionRegistrationErrorKind::UnknownValueKind => {
            DefinitionRegistrationCode::UnknownValueKind
        }
        DefinitionRegistrationErrorKind::TrailingBytes => DefinitionRegistrationCode::TrailingBytes,
        DefinitionRegistrationErrorKind::UnknownDefinition => {
            DefinitionRegistrationCode::UnknownDefinition
        }
        DefinitionRegistrationErrorKind::TerminalCountMismatch => {
            DefinitionRegistrationCode::TerminalCountMismatch
        }
        DefinitionRegistrationErrorKind::ParameterCountMismatch => {
            DefinitionRegistrationCode::ParameterCountMismatch
        }
        DefinitionRegistrationErrorKind::NodeOutOfRange => {
            DefinitionRegistrationCode::NodeOutOfRange
        }
        DefinitionRegistrationErrorKind::ParameterOutOfRange => {
            DefinitionRegistrationCode::ParameterOutOfRange
        }
        DefinitionRegistrationErrorKind::ParameterConstraintViolation => {
            DefinitionRegistrationCode::ParameterConstraintViolation
        }
        DefinitionRegistrationErrorKind::NodeIdExhausted => {
            DefinitionRegistrationCode::NodeIdExhausted
        }
        DefinitionRegistrationErrorKind::ParameterIdExhausted => {
            DefinitionRegistrationCode::ParameterIdExhausted
        }
        DefinitionRegistrationErrorKind::DefinitionIdExhausted => {
            DefinitionRegistrationCode::DefinitionIdExhausted
        }
        DefinitionRegistrationErrorKind::InvalidDefinition => {
            DefinitionRegistrationCode::InvalidDefinition
        }
        DefinitionRegistrationErrorKind::InvalidPrimitiveParameters => {
            DefinitionRegistrationCode::InvalidPrimitiveParameters
        }
        DefinitionRegistrationErrorKind::UnusedInternalNode => {
            DefinitionRegistrationCode::UnusedInternalNode
        }
        DefinitionRegistrationErrorKind::DisconnectedInternalComponent => {
            DefinitionRegistrationCode::DisconnectedInternalComponent
        }
        DefinitionRegistrationErrorKind::IncompatibleParameterConstraints => {
            DefinitionRegistrationCode::IncompatibleParameterConstraints
        }
        DefinitionRegistrationErrorKind::UnusedParameter => {
            DefinitionRegistrationCode::UnusedParameter
        }
        DefinitionRegistrationErrorKind::DevicePartitionIdExhausted => {
            DefinitionRegistrationCode::DevicePartitionIdExhausted
        }
        DefinitionRegistrationErrorKind::StateCountExhausted => {
            DefinitionRegistrationCode::StateCountExhausted
        }
    };

    DefinitionRegistrationResult::failure(code, error.command_index(), error.byte_offset())
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldCode {
    Success = 0,
    NullEngine = 1,
    NullResult = 2,
    UnknownWorld = 3,
    WorldIdExhausted = 4,
    InternalPanic = u32::MAX,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldCreationResult {
    pub code: u32,
    pub world_id: u32,
}

impl WorldCreationResult {
    const fn success(world_id: u32) -> Self {
        Self {
            code: WorldCode::Success as u32,
            world_id,
        }
    }

    const fn failure(code: WorldCode) -> Self {
        Self {
            code: code as u32,
            world_id: u32::MAX,
        }
    }
}

/// Creates a world and returns its engine-assigned ID in `result`.
///
/// The engine owns the new world. The caller must use the returned world ID
/// for later world operations.
///
/// # Safety
///
/// `engine` must point to a live [`Engine`] with exclusive access for this
/// call. `result` must point to writable storage for one
/// [`WorldCreationResult`]. The result and engine storage must not overlap.
/// The caller must prevent concurrent use of the same engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_engine_create_world(
    engine: *mut Engine,
    result: *mut WorldCreationResult,
) -> u32 {
    if result.is_null() {
        return WorldCode::NullResult as u32;
    }

    let output = if engine.is_null() {
        WorldCreationResult::failure(WorldCode::NullEngine)
    } else {
        let creation = catch_unwind(AssertUnwindSafe(|| {
            let engine = unsafe { &mut *engine };
            engine.new_world()
        }));

        match creation {
            Ok(Ok(world_id)) => WorldCreationResult::success(world_id),

            Ok(Err(WorldManagementError::WorldIdExhausted)) => {
                WorldCreationResult::failure(WorldCode::WorldIdExhausted)
            }

            // Creation cannot produce UnknownWorld.
            Ok(Err(WorldManagementError::UnknownWorld)) => {
                WorldCreationResult::failure(WorldCode::InternalPanic)
            }

            Err(_) => WorldCreationResult::failure(WorldCode::InternalPanic),
        }
    };

    let code = output.code;

    unsafe {
        result.write(output);
    }

    code
}

/// Destroys a world and all resources that the world owns.
///
/// `world_id` must identify a live world that belongs to `engine`. The ID is
/// invalid after this call succeeds.
///
/// # Safety
///
/// `engine` must point to a live [`Engine`] with exclusive access for this
/// call. The caller must prevent concurrent use of the same engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_engine_destroy_world(engine: *mut Engine, world_id: u32) -> u32 {
    if engine.is_null() {
        return WorldCode::NullEngine as u32;
    }

    let destruction = catch_unwind(AssertUnwindSafe(|| {
        let engine = unsafe { &mut *engine };
        engine.destroy_world(world_id)
    }));

    match destruction {
        Ok(Ok(())) => WorldCode::Success as u32,

        Ok(Err(WorldManagementError::UnknownWorld)) => WorldCode::UnknownWorld as u32,

        Ok(Err(WorldManagementError::WorldIdExhausted)) => WorldCode::InternalPanic as u32,

        Err(_) => WorldCode::InternalPanic as u32,
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCode {
    Success = 0,

    NullEngine = 1,
    NullInput = 2,
    NullResult = 3,
    InputTooLarge = 4,

    InvalidMagic = 5,
    UnsupportedVersion = 6,
    InvalidFlags = 7,
    InvalidReserved = 8,
    TruncatedInput = 9,
    UnknownCommand = 10,
    InvalidCommandLength = 11,
    InvalidId = 12,
    TrailingBytes = 13,
    UnknownWorld = 14,

    IdOutOfBound = 20,
    IdExceeds31Bit = 21,
    IdAlreadyAssigned = 22,
    IdNotAssigned = 23,
    WireConnectToSelf = 24,
    AlreadyConnected = 25,
    NotConnected = 26,
    TerminalAlreadyConnected = 27,
    InvalidTerminal = 28,
    InvalidParameter = 29,
    ParameterConstraintViolation = 30,
    UnknownDefinition = 31,

    InternalPanic = u32::MAX,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandResult {
    pub code: u32,
    pub command_index: u32,
    pub byte_offset: u32,
    pub reserved: u32,
}

impl CommandResult {
    const fn success() -> Self {
        Self {
            code: CommandCode::Success as u32,
            command_index: u32::MAX,
            byte_offset: u32::MAX,
            reserved: 0,
        }
    }

    const fn failure(code: CommandCode, command_index: u32, byte_offset: u32) -> Self {
        Self {
            code: code as u32,
            command_index,
            byte_offset,
            reserved: 0,
        }
    }
}

/// Applies a world command buffer in command order.
///
/// The operation is not atomic. If a command fails, all earlier successful
/// commands remain applied. The function stops at the first failure. On
/// failure, `command_index` identifies the failing command and `byte_offset`
/// identifies its location in the input buffer.
///
/// # Safety
///
/// `engine` must point to a live [`Engine`] with exclusive access for this
/// call. `input` must point to `input_len` readable bytes. `result` must point
/// to writable storage for one [`CommandResult`]. The input, result, and
/// engine storage must not overlap. The caller must prevent concurrent use of
/// the same engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_world_apply_commands(
    engine: *mut Engine,
    world_id: u32,
    input: *const u8,
    input_len: usize,
    result: *mut CommandResult,
) -> u32 {
    if result.is_null() {
        return CommandCode::NullResult as u32;
    }

    let output = if engine.is_null() {
        CommandResult::failure(CommandCode::NullEngine, u32::MAX, u32::MAX)
    } else if input.is_null() {
        CommandResult::failure(CommandCode::NullInput, u32::MAX, u32::MAX)
    } else if input_len > u32::MAX as usize {
        CommandResult::failure(CommandCode::InputTooLarge, u32::MAX, u32::MAX)
    } else {
        let application = catch_unwind(AssertUnwindSafe(|| {
            let engine = unsafe { &mut *engine };
            let input = unsafe { std::slice::from_raw_parts(input, input_len) };

            hynergy_protocol::apply_world_command_buffer(engine, world_id, input)
        }));

        match application {
            Ok(Ok(())) => CommandResult::success(),

            Ok(Err(error)) => map_world_command_error(error),

            Err(_) => CommandResult::failure(CommandCode::InternalPanic, u32::MAX, u32::MAX),
        }
    };

    let code = output.code;

    unsafe {
        result.write(output);
    }

    code
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Publication {
    pub subscription_id: u32,
    pub status: u32,
    pub value: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SolveResult {
    pub code: u32,
    pub publication_count: u32,
    pub required_capacity: u32,
    pub dirty_island_count: u32,
    pub solved_island_count: u32,
    pub failed_island_count: u32,
}

/// Solves the dirty islands in a world and writes subscription publications.
///
/// The function writes at most `publication_capacity` records to
/// `publications`. It writes the operation summary to `result`. It does not
/// publish records for unchanged islands.
///
/// If the publication capacity is too small, the function reports the
/// required capacity in `result`. It does not solve an island or publish a
/// record in this case.
///
/// This function is not implemented. The caller must not call it.
///
/// # Safety
///
/// `engine` must point to a live [`Engine`] with exclusive access for this
/// call. If `publication_capacity` is not zero, `publications` must point to
/// writable storage for that number of [`Publication`] records. `result` must
/// point to writable storage for one [`SolveResult`]. The publication, result,
/// and engine storage must not overlap. The caller must prevent concurrent use
/// of the same engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_world_solve(
    _engine: *mut Engine,
    _world_id: u32,
    _publications: *mut Publication,
    _publication_capacity: u32,
    _result: *mut SolveResult,
) -> u32 {
    todo!("world solving is not implemented")
}

fn map_world_command_error(error: WorldCommandError) -> CommandResult {
    let code = match error.kind() {
        WorldCommandErrorKind::InvalidMagic => CommandCode::InvalidMagic,
        WorldCommandErrorKind::UnsupportedVersion => CommandCode::UnsupportedVersion,
        WorldCommandErrorKind::InvalidFlags => CommandCode::InvalidFlags,
        WorldCommandErrorKind::InvalidReserved => CommandCode::InvalidReserved,
        WorldCommandErrorKind::TruncatedInput => CommandCode::TruncatedInput,
        WorldCommandErrorKind::UnknownCommand => CommandCode::UnknownCommand,
        WorldCommandErrorKind::InvalidCommandLength => CommandCode::InvalidCommandLength,
        WorldCommandErrorKind::InvalidId => CommandCode::InvalidId,
        WorldCommandErrorKind::TrailingBytes => CommandCode::TrailingBytes,
        WorldCommandErrorKind::UnknownWorld => CommandCode::UnknownWorld,

        WorldCommandErrorKind::IdOutOfBound => CommandCode::IdOutOfBound,
        WorldCommandErrorKind::IdExceeds31Bit => CommandCode::IdExceeds31Bit,
        WorldCommandErrorKind::IdAlreadyAssigned => CommandCode::IdAlreadyAssigned,
        WorldCommandErrorKind::IdNotAssigned => CommandCode::IdNotAssigned,
        WorldCommandErrorKind::WireConnectToSelf => CommandCode::WireConnectToSelf,
        WorldCommandErrorKind::AlreadyConnected => CommandCode::AlreadyConnected,
        WorldCommandErrorKind::NotConnected => CommandCode::NotConnected,
        WorldCommandErrorKind::TerminalAlreadyConnected => CommandCode::TerminalAlreadyConnected,
        WorldCommandErrorKind::InvalidTerminal => CommandCode::InvalidTerminal,
        WorldCommandErrorKind::InvalidParameter => CommandCode::InvalidParameter,
        WorldCommandErrorKind::ParameterConstraintViolation => {
            CommandCode::ParameterConstraintViolation
        }
        WorldCommandErrorKind::UnknownDefinition => CommandCode::UnknownDefinition,
    };

    CommandResult::failure(code, error.command_index(), error.byte_offset())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    fn definition_buffer() -> Vec<u8> {
        definition_buffer_with_commands(&[])
    }

    fn definition_command(tag: u16, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(6 + payload.len());
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn definition_buffer_with_commands(commands: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"HYDF");
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&(commands.len() as u32).to_le_bytes());

        for command in commands {
            bytes.extend_from_slice(command);
        }

        bytes
    }

    fn literal_element_payload(
        definition_id: u32,
        terminals: &[u32],
        parameters: &[f64],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&definition_id.to_le_bytes());

        bytes.extend_from_slice(&(terminals.len() as u32).to_le_bytes());
        for terminal in terminals {
            bytes.extend_from_slice(&terminal.to_le_bytes());
        }

        bytes.extend_from_slice(&(parameters.len() as u32).to_le_bytes());
        for parameter in parameters {
            bytes.push(0); // DEFINITION_VALUE_LITERAL
            bytes.extend_from_slice(&parameter.to_le_bytes());
        }

        bytes
    }

    #[test]
    fn definition_registration_preserves_command_error_location() {
        const ADD_TERMINAL: u16 = 1;
        const ADD_ELEMENT: u16 = 4;

        const VOLTAGE_CONTROLLED_SWITCH: u32 = 9;

        let engine = hynergy_engine_create();

        let mut commands = vec![
            definition_command(ADD_TERMINAL, &[]),
            definition_command(ADD_TERMINAL, &[]),
            definition_command(ADD_TERMINAL, &[]),
            definition_command(ADD_TERMINAL, &[]),
        ];

        let element_offset = 16 + commands.iter().map(Vec::len).sum::<usize>();

        commands.push(definition_command(
            ADD_ELEMENT,
            &literal_element_payload(
                VOLTAGE_CONTROLLED_SWITCH,
                &[0, 1, 2, 3],
                &[
                    0.0, // threshold
                    0.0, // hysteresis
                    1.0, // G_max
                    1.0, // G_min -- invalid because G_max must be > G_min
                ],
            ),
        ));

        let bytes = definition_buffer_with_commands(&commands);
        let mut result = definition_result_sentinel();

        let code = register(engine, &bytes, &mut result);

        assert_eq!(
            code,
            DefinitionRegistrationCode::InvalidPrimitiveParameters as u32
        );

        assert_eq!(
            result,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::InvalidPrimitiveParameters as u32,
                command_index: 4,
                byte_offset: element_offset as u32,
                definition_id: u32::MAX,
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn definition_registration_preserves_build_error_location() {
        const ADD_TERMINAL: u16 = 1;
        const ADD_NODE: u16 = 2;

        let engine = hynergy_engine_create();

        let commands = vec![
            definition_command(ADD_TERMINAL, &[]),
            definition_command(ADD_NODE, &[]),
        ];

        let bytes = definition_buffer_with_commands(&commands);
        let mut result = definition_result_sentinel();

        let code = register(engine, &bytes, &mut result);

        assert_eq!(code, DefinitionRegistrationCode::UnusedInternalNode as u32);

        assert_eq!(
            result,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::UnusedInternalNode as u32,
                command_index: commands.len() as u32,
                byte_offset: bytes.len() as u32,
                definition_id: u32::MAX,
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    fn world_command(tag: u16, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(6 + payload.len());
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn world_buffer(commands: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"HYWC");
        bytes.extend_from_slice(&1_u16.to_le_bytes());
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

    fn definition_result_sentinel() -> DefinitionRegistrationResult {
        DefinitionRegistrationResult {
            code: 0xaaaa_aaaa,
            command_index: 0xbbbb_bbbb,
            byte_offset: 0xcccc_cccc,
            definition_id: 0xdddd_dddd,
        }
    }

    fn world_result_sentinel() -> WorldCreationResult {
        WorldCreationResult {
            code: 0xaaaa_aaaa,
            world_id: 0xbbbb_bbbb,
        }
    }

    fn command_result_sentinel() -> CommandResult {
        CommandResult {
            code: 0xaaaa_aaaa,
            command_index: 0xbbbb_bbbb,
            byte_offset: 0xcccc_cccc,
            reserved: 0xdddd_dddd,
        }
    }

    fn register(
        engine: *mut Engine,
        bytes: &[u8],
        result: *mut DefinitionRegistrationResult,
    ) -> u32 {
        unsafe { hynergy_engine_register_definition(engine, bytes.as_ptr(), bytes.len(), result) }
    }

    fn create_world(engine: *mut Engine) -> WorldCreationResult {
        let mut result = world_result_sentinel();

        assert_eq!(
            unsafe { hynergy_engine_create_world(engine, &mut result) },
            WorldCode::Success as u32
        );

        result
    }

    fn apply(engine: *mut Engine, world: u32, bytes: &[u8], result: *mut CommandResult) -> u32 {
        unsafe { hynergy_world_apply_commands(engine, world, bytes.as_ptr(), bytes.len(), result) }
    }

    #[test]
    fn abi_version_is_stable() {
        assert_eq!(hynergy_abi_version(), ABI_VERSION);
        assert_eq!(ABI_VERSION, 1);
    }

    #[test]
    fn abi_code_values_are_stable() {
        macro_rules! assert_codes {
        ($enum:ident { $($variant:ident = $value:expr),* $(,)? }) => {
            $(
                assert_eq!(
                    $enum::$variant as u32,
                    $value,
                    concat!(stringify!($enum), "::", stringify!($variant)),
                );
            )*
        };
    }

        assert_codes!(DefinitionRegistrationCode {
            Success = 0,
            NullEngine = 1,
            NullInput = 2,
            NullResult = 3,
            InputTooLarge = 4,

            InvalidMagic = 5,
            UnsupportedVersion = 6,
            InvalidFlags = 7,
            TruncatedInput = 8,
            UnknownCommand = 9,
            InvalidCommandLength = 10,
            InvalidCount = 11,
            UnknownValueKind = 12,
            TrailingBytes = 13,
            InvalidReserved = 14,
            InvalidDefinitionId = 15,

            UnknownDefinition = 20,
            TerminalCountMismatch = 21,
            ParameterCountMismatch = 22,
            NodeOutOfRange = 23,
            ParameterOutOfRange = 24,
            ParameterConstraintViolation = 25,
            NodeIdExhausted = 26,
            ParameterIdExhausted = 27,
            DefinitionIdExhausted = 28,
            InvalidDefinition = 29,
            InvalidPrimitiveParameters = 30,
            UnusedInternalNode = 31,
            DisconnectedInternalComponent = 32,
            IncompatibleParameterConstraints = 33,
            UnusedParameter = 34,
            DevicePartitionIdExhausted = 35,

            InternalPanic = u32::MAX,
        });

        assert_codes!(WorldCode {
            Success = 0,
            NullEngine = 1,
            NullResult = 2,
            UnknownWorld = 3,
            WorldIdExhausted = 4,
            InternalPanic = u32::MAX,
        });

        assert_codes!(CommandCode {
            Success = 0,

            NullEngine = 1,
            NullInput = 2,
            NullResult = 3,
            InputTooLarge = 4,

            InvalidMagic = 5,
            UnsupportedVersion = 6,
            InvalidFlags = 7,
            InvalidReserved = 8,
            TruncatedInput = 9,
            UnknownCommand = 10,
            InvalidCommandLength = 11,
            InvalidId = 12,
            TrailingBytes = 13,
            UnknownWorld = 14,

            IdOutOfBound = 20,
            IdExceeds31Bit = 21,
            IdAlreadyAssigned = 22,
            IdNotAssigned = 23,
            WireConnectToSelf = 24,
            AlreadyConnected = 25,
            NotConnected = 26,
            TerminalAlreadyConnected = 27,
            InvalidTerminal = 28,
            InvalidParameter = 29,
            ParameterConstraintViolation = 30,
            UnknownDefinition = 31,

            InternalPanic = u32::MAX,
        });
    }

    #[test]
    fn result_layouts_are_stable() {
        assert_eq!(size_of::<DefinitionRegistrationResult>(), 16);
        assert_eq!(
            align_of::<DefinitionRegistrationResult>(),
            align_of::<u32>()
        );
        assert_eq!(offset_of!(DefinitionRegistrationResult, code), 0);
        assert_eq!(offset_of!(DefinitionRegistrationResult, command_index), 4);
        assert_eq!(offset_of!(DefinitionRegistrationResult, byte_offset), 8);
        assert_eq!(offset_of!(DefinitionRegistrationResult, definition_id), 12);

        assert_eq!(size_of::<WorldCreationResult>(), 8);
        assert_eq!(align_of::<WorldCreationResult>(), align_of::<u32>());
        assert_eq!(offset_of!(WorldCreationResult, code), 0);
        assert_eq!(offset_of!(WorldCreationResult, world_id), 4);

        assert_eq!(size_of::<CommandResult>(), 16);
        assert_eq!(align_of::<CommandResult>(), align_of::<u32>());
        assert_eq!(offset_of!(CommandResult, code), 0);
        assert_eq!(offset_of!(CommandResult, command_index), 4);
        assert_eq!(offset_of!(CommandResult, byte_offset), 8);
        assert_eq!(offset_of!(CommandResult, reserved), 12);
    }

    #[test]
    fn engine_lifecycle_accepts_created_and_null_engines() {
        let engine = hynergy_engine_create();

        assert!(!engine.is_null());

        unsafe {
            hynergy_engine_destroy(engine);
            hynergy_engine_destroy(std::ptr::null_mut());
        }
    }

    #[test]
    fn definition_registration_returns_assigned_ids() {
        let engine = hynergy_engine_create();
        let bytes = definition_buffer();

        let mut first = definition_result_sentinel();
        let mut second = definition_result_sentinel();

        assert_eq!(
            register(engine, &bytes, &mut first),
            DefinitionRegistrationCode::Success as u32
        );

        assert_eq!(
            register(engine, &bytes, &mut second),
            DefinitionRegistrationCode::Success as u32
        );

        assert_eq!(
            first,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::Success as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
                definition_id: Engine::COMPOSITE_DEFINITION_ID_BASE,
            }
        );

        assert_eq!(
            second,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::Success as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
                definition_id: Engine::COMPOSITE_DEFINITION_ID_BASE + 1,
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn definition_registration_maps_protocol_errors() {
        let engine = hynergy_engine_create();
        let mut bytes = definition_buffer();
        bytes[0] = b'X';

        let mut result = definition_result_sentinel();

        let code = register(engine, &bytes, &mut result);

        assert_eq!(code, DefinitionRegistrationCode::InvalidMagic as u32);
        assert_eq!(
            result,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::InvalidMagic as u32,
                command_index: u32::MAX,
                byte_offset: 0,
                definition_id: u32::MAX
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn definition_registration_rejects_null_result_without_processing() {
        let engine = hynergy_engine_create();
        let bytes = definition_buffer();

        assert_eq!(
            register(engine, &bytes, std::ptr::null_mut(),),
            DefinitionRegistrationCode::NullResult as u32
        );

        let mut result = definition_result_sentinel();

        assert_eq!(
            register(engine, &bytes, &mut result),
            DefinitionRegistrationCode::Success as u32
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn definition_registration_reports_null_engine() {
        let bytes = definition_buffer();
        let mut result = definition_result_sentinel();

        let code = register(std::ptr::null_mut(), &bytes, &mut result);

        assert_eq!(code, DefinitionRegistrationCode::NullEngine as u32);
        assert_eq!(
            result,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::NullEngine as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
                definition_id: u32::MAX
            }
        );
    }

    #[test]
    fn definition_registration_reports_null_input() {
        let engine = hynergy_engine_create();
        let mut result = definition_result_sentinel();

        let code =
            unsafe { hynergy_engine_register_definition(engine, std::ptr::null(), 0, &mut result) };

        assert_eq!(code, DefinitionRegistrationCode::NullInput as u32);
        assert_eq!(
            result,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::NullInput as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
                definition_id: u32::MAX
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn create_world_rejects_null_result_without_creating_world() {
        let engine = hynergy_engine_create();

        assert_eq!(
            unsafe { hynergy_engine_create_world(engine, std::ptr::null_mut(),) },
            WorldCode::NullResult as u32
        );

        let result = create_world(engine);

        assert_eq!(result.world_id, 0);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn create_world_reports_null_engine() {
        let mut result = world_result_sentinel();

        let code = unsafe { hynergy_engine_create_world(std::ptr::null_mut(), &mut result) };

        assert_eq!(code, WorldCode::NullEngine as u32);
        assert_eq!(
            result,
            WorldCreationResult {
                code: WorldCode::NullEngine as u32,
                world_id: u32::MAX,
            }
        );
    }

    #[test]
    fn world_lifecycle_is_exposed_through_ffi() {
        let engine = hynergy_engine_create();

        let first = create_world(engine);
        let second = create_world(engine);

        assert_eq!(first.world_id, 0);
        assert_eq!(second.world_id, 1);

        assert_eq!(
            unsafe { hynergy_engine_destroy_world(engine, first.world_id) },
            WorldCode::Success as u32
        );

        assert_eq!(
            unsafe { hynergy_engine_destroy_world(engine, first.world_id) },
            WorldCode::UnknownWorld as u32
        );

        let third = create_world(engine);
        assert_eq!(third.world_id, 2);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn destroy_world_reports_null_engine() {
        assert_eq!(
            unsafe { hynergy_engine_destroy_world(std::ptr::null_mut(), 0,) },
            WorldCode::NullEngine as u32
        );
    }

    #[test]
    fn apply_commands_rejects_null_result_without_applying() {
        let engine = hynergy_engine_create();
        let world = create_world(engine);

        let bytes = world_buffer(&[world_command(1, &u32_payload(&[1]))]);

        assert_eq!(
            apply(engine, world.world_id, &bytes, std::ptr::null_mut(),),
            CommandCode::NullResult as u32
        );

        let mut result = command_result_sentinel();

        assert_eq!(
            apply(engine, world.world_id, &bytes, &mut result,),
            CommandCode::Success as u32
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn apply_commands_reports_null_engine() {
        let bytes = world_buffer(&[]);
        let mut result = command_result_sentinel();

        let code = apply(std::ptr::null_mut(), 0, &bytes, &mut result);

        assert_eq!(code, CommandCode::NullEngine as u32);
        assert_eq!(
            result,
            CommandResult {
                code: CommandCode::NullEngine as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
                reserved: 0,
            }
        );
    }

    #[test]
    fn apply_commands_reports_null_input_without_applying() {
        let engine = hynergy_engine_create();
        let world = create_world(engine);
        let mut result = command_result_sentinel();

        let code = unsafe {
            hynergy_world_apply_commands(engine, world.world_id, std::ptr::null(), 0, &mut result)
        };

        assert_eq!(code, CommandCode::NullInput as u32);
        assert_eq!(
            result,
            CommandResult {
                code: CommandCode::NullInput as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
                reserved: 0,
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn command_buffer_applies_mutation_through_ffi() {
        let engine = hynergy_engine_create();
        let world = create_world(engine);

        let add = world_buffer(&[world_command(1, &u32_payload(&[1]))]);

        let mut result = command_result_sentinel();

        assert_eq!(
            apply(engine, world.world_id, &add, &mut result),
            CommandCode::Success as u32
        );

        assert_eq!(
            result,
            CommandResult {
                code: CommandCode::Success as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
                reserved: 0,
            }
        );

        // Proves the previous FFI call actually mutated the world.
        let remove = world_buffer(&[world_command(2, &u32_payload(&[1]))]);

        assert_eq!(
            apply(engine, world.world_id, &remove, &mut result),
            CommandCode::Success as u32
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn protocol_error_is_mapped_through_ffi() {
        let engine = hynergy_engine_create();
        let world = create_world(engine);

        let mut bytes = world_buffer(&[]);
        bytes[0] = b'X';

        let mut result = command_result_sentinel();

        let code = apply(engine, world.world_id, &bytes, &mut result);

        assert_eq!(code, CommandCode::InvalidMagic as u32);
        assert_eq!(
            result,
            CommandResult {
                code: CommandCode::InvalidMagic as u32,
                command_index: u32::MAX,
                byte_offset: 0,
                reserved: 0,
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn invalid_command_length_is_mapped_through_ffi() {
        let engine = hynergy_engine_create();
        let world = create_world(engine);

        let bytes = world_buffer(&[world_command(1, &[1, 0])]);

        let mut result = command_result_sentinel();

        let code = apply(engine, world.world_id, &bytes, &mut result);

        assert_eq!(code, CommandCode::InvalidCommandLength as u32);
        assert_eq!(
            result,
            CommandResult {
                code: CommandCode::InvalidCommandLength as u32,
                command_index: 0,
                byte_offset: 16,
                reserved: 0,
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn unknown_world_is_mapped_through_ffi() {
        let engine = hynergy_engine_create();
        let bytes = world_buffer(&[]);
        let mut result = command_result_sentinel();

        let code = apply(engine, 42, &bytes, &mut result);

        assert_eq!(code, CommandCode::UnknownWorld as u32);
        assert_eq!(
            result,
            CommandResult {
                code: CommandCode::UnknownWorld as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
                reserved: 0,
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn command_failure_preserves_location_and_prior_mutations() {
        let engine = hynergy_engine_create();
        let world = create_world(engine);

        let add = world_command(1, &u32_payload(&[1]));
        let bytes = world_buffer(&[add.clone(), add]);

        let mut result = command_result_sentinel();

        let code = apply(engine, world.world_id, &bytes, &mut result);

        assert_eq!(code, CommandCode::IdAlreadyAssigned as u32);

        assert_eq!(
            result,
            CommandResult {
                code: CommandCode::IdAlreadyAssigned as u32,
                command_index: 1,
                byte_offset: 26,
                reserved: 0,
            }
        );

        let remove = world_buffer(&[world_command(2, &u32_payload(&[1]))]);

        assert_eq!(
            apply(engine, world.world_id, &remove, &mut result),
            CommandCode::Success as u32
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }
}
