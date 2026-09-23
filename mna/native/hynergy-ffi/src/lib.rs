use hynergy_engine::{
    Engine, EngineConfig, EngineTickError, SubscriptionError, SubscriptionId, WorldConfig,
    WorldManagementError,
};
use hynergy_model::device::definition::{DefinitionObserverId, DeviceId};
use hynergy_protocol::{
    DefinitionRegistrationError, DefinitionRegistrationErrorKind, WorldCommandError,
    WorldCommandErrorKind,
};
use std::num::NonZeroU32;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub const ABI_VERSION: u32 = 1;
pub const ABI_REVISION: u32 = 2;

/// Returns the ABI major version implemented by this library.
///
/// The major version changes only when compatibility with the existing ABI is
/// broken. The caller must check this value before it calls other ABI
/// functions.
#[unsafe(no_mangle)]
pub extern "C" fn hynergy_abi_version() -> u32 {
    ABI_VERSION
}

/// Returns the additive revision of ABI major version 1.
///
/// The ABI revision increases only when backward-compatible ABI features are
/// added. Existing callers may continue using an ABI with a newer revision.
#[unsafe(no_mangle)]
pub extern "C" fn hynergy_abi_revision() -> u32 {
    ABI_REVISION
}

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

/// Creates an engine.
///
/// The returned pointer owns the engine. The caller must pass the pointer to
/// [`hynergy_engine_destroy`] when the engine is no longer necessary.
#[unsafe(no_mangle)]
pub extern "C" fn hynergy_engine_create(max_worker_threads: u32) -> *mut Engine {
    Box::into_raw(Box::new(Engine::new(EngineConfig::new(max_worker_threads))))
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
    input_len: u32,
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
    } else {
        let registration = catch_unwind(AssertUnwindSafe(|| {
            let engine = unsafe { &mut *engine };
            let input = unsafe { std::slice::from_raw_parts(input, input_len as usize) };
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
    InvalidTickFrequency = 5,
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
    tick_frequency_hz: u32,
    result: *mut WorldCreationResult,
) -> u32 {
    if result.is_null() {
        return WorldCode::NullResult as u32;
    }

    let Some(tick_frequency_hz) = NonZeroU32::new(tick_frequency_hz) else {
        let output = WorldCreationResult::failure(WorldCode::InvalidTickFrequency);

        let code = output.code;

        unsafe {
            result.write(output);
        }

        return code;
    };

    let output = if engine.is_null() {
        WorldCreationResult::failure(WorldCode::NullEngine)
    } else {
        let creation = catch_unwind(AssertUnwindSafe(|| {
            let engine = unsafe { &mut *engine };
            engine.new_world(WorldConfig::new(tick_frequency_hz))
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
    ResourceExhausted = 32,

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
    input_len: u32,
    result: *mut CommandResult,
) -> u32 {
    if result.is_null() {
        return CommandCode::NullResult as u32;
    }

    let output = if engine.is_null() {
        CommandResult::failure(CommandCode::NullEngine, u32::MAX, u32::MAX)
    } else if input.is_null() {
        CommandResult::failure(CommandCode::NullInput, u32::MAX, u32::MAX)
    } else {
        let application = catch_unwind(AssertUnwindSafe(|| {
            let engine = unsafe { &mut *engine };
            let input = unsafe { std::slice::from_raw_parts(input, input_len as usize) };

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
        WorldCommandErrorKind::ResourceExhausted => CommandCode::ResourceExhausted,
    };

    CommandResult::failure(code, error.command_index(), error.byte_offset())
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickCode {
    Success = 0,

    NullEngine = 1,
    NullResult = 2,
    NullOutput = 3,
    UnknownWorld = 4,
    BufferTooSmall = 5,

    MissingParameter = 20,
    Singular = 21,
    NonlinearDidNotConverge = 22,
    NonFiniteMatrix = 23,
    NonFiniteSolution = 24,
    ResourceExhausted = 25,
    BackendFailure = 26,
    CompilationFailed = 27,
    InternalInvariant = 28,

    InternalPanic = u32::MAX,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickResult {
    pub code: u32,

    pub record_count: u32,
    pub required_capacity: u32,

    pub device_id: u32,
    pub parameter_id: u32,
    pub iterations: u32,
}

impl TickResult {
    const fn success(record_count: u32, required_capacity: u32) -> Self {
        Self {
            code: TickCode::Success as u32,
            record_count,
            required_capacity,
            device_id: u32::MAX,
            parameter_id: u32::MAX,
            iterations: u32::MAX,
        }
    }

    const fn failure(code: TickCode, required_capacity: u32) -> Self {
        Self {
            code: code as u32,
            record_count: 0,
            required_capacity,
            device_id: u32::MAX,
            parameter_id: u32::MAX,
            iterations: u32::MAX,
        }
    }
}

fn map_tick_error(error: EngineTickError, required_capacity: u32) -> TickResult {
    match error {
        EngineTickError::UnknownWorld => {
            TickResult::failure(TickCode::UnknownWorld, required_capacity)
        }

        EngineTickError::MissingParameter { device, parameter } => TickResult {
            code: TickCode::MissingParameter as u32,
            record_count: 0,
            required_capacity,
            device_id: device.get(),
            parameter_id: parameter.id(),
            iterations: u32::MAX,
        },

        EngineTickError::Singular => TickResult::failure(TickCode::Singular, required_capacity),

        EngineTickError::NonlinearDidNotConverge { iterations } => TickResult {
            code: TickCode::NonlinearDidNotConverge as u32,
            record_count: 0,
            required_capacity,
            device_id: u32::MAX,
            parameter_id: u32::MAX,
            iterations: u32::try_from(iterations).unwrap_or(u32::MAX),
        },

        EngineTickError::NonFiniteMatrix => {
            TickResult::failure(TickCode::NonFiniteMatrix, required_capacity)
        }

        EngineTickError::NonFiniteSolution => {
            TickResult::failure(TickCode::NonFiniteSolution, required_capacity)
        }

        EngineTickError::ResourceExhausted => {
            TickResult::failure(TickCode::ResourceExhausted, required_capacity)
        }

        EngineTickError::BackendFailure => {
            TickResult::failure(TickCode::BackendFailure, required_capacity)
        }

        EngineTickError::CompilationFailed => {
            TickResult::failure(TickCode::CompilationFailed, required_capacity)
        }

        EngineTickError::InternalInvariant => {
            TickResult::failure(TickCode::InternalInvariant, required_capacity)
        }
    }
}

/// Advances a world by one tick and writes subscription updates to `records`.
///
/// The first successful tick reports each subscribed observer. Later ticks
/// report an observer only when the bit pattern of its value changes.
/// On success, `record_count` gives the number of records written.
///
/// `record_capacity` must be at least the number of active subscriptions.
/// If the capacity is too small, the function returns [`TickCode::BufferTooSmall`]
/// and sets `required_capacity` to that number. The world does not advance.
/// `records` can be null only when no active subscriptions exist.
///
/// The function returns a [`TickCode`] value. If `result` is not null, the
/// function also writes that code and the tick details to `result`.
///
/// # Safety
///
/// If `engine` is not null, it must point to a live [`Engine`] with exclusive
/// access for this call. If `records` is not null, it must point to aligned,
/// writable storage for `record_capacity` consecutive [`SubscriptionRecord`]
/// values. If `result` is not null, it must point to aligned, writable storage
/// for one [`TickResult`]. The records, result, and engine storage must not
/// overlap. The caller must prevent concurrent use of the same engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_world_tick(
    engine: *mut Engine,
    world_id: u32,
    records: *mut SubscriptionRecord,
    record_capacity: u32,
    result: *mut TickResult,
) -> u32 {
    if result.is_null() {
        return TickCode::NullResult as u32;
    }

    let output = if engine.is_null() {
        TickResult::failure(TickCode::NullEngine, 0)
    } else {
        let execution = catch_unwind(AssertUnwindSafe(|| {
            let engine = unsafe { &mut *engine };

            let required = match engine.subscription_count(world_id) {
                Ok(required) => required,

                Err(SubscriptionError::UnknownWorld) => {
                    return TickResult::failure(TickCode::UnknownWorld, 0);
                }

                Err(_) => {
                    return TickResult::failure(TickCode::InternalInvariant, 0);
                }
            };

            let required = u32::try_from(required).expect("active subscription count must fit u32");

            if record_capacity < required {
                return TickResult::failure(TickCode::BufferTooSmall, required);
            }

            if required != 0 && records.is_null() {
                return TickResult::failure(TickCode::NullOutput, required);
            }

            if let Err(error) = engine.tick_world(world_id) {
                return map_tick_error(error, required);
            }

            let updates = engine
                .subscription_updates(world_id)
                .expect("world validated before successful tick");

            debug_assert!(updates.len() <= required as usize);

            for (index, update) in updates.iter().enumerate() {
                let record = SubscriptionRecord {
                    subscription_id: update.subscription().get(),

                    status: SubscriptionStatusCode::Available as u32,

                    value: update.value(),
                };

                unsafe {
                    records.add(index).write(record);
                }
            }

            TickResult::success(
                u32::try_from(updates.len()).expect("update count must fit u32"),
                required,
            )
        }));

        execution.unwrap_or_else(|_| TickResult::failure(TickCode::InternalPanic, 0))
    };

    let code = output.code;

    unsafe {
        result.write(output);
    }

    code
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionCode {
    Success = 0,

    NullEngine = 1,
    NullResult = 2,
    UnknownWorld = 3,
    InvalidDeviceId = 4,
    InvalidSubscriptionId = 5,

    UnknownDevice = 20,
    UnknownObserver = 21,
    IdExhausted = 22,
    UnknownSubscription = 23,

    InternalPanic = u32::MAX,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscriptionCreationResult {
    pub code: u32,
    pub subscription_id: u32,
}

impl SubscriptionCreationResult {
    const fn success(subscription_id: u32) -> Self {
        Self {
            code: SubscriptionCode::Success as u32,
            subscription_id,
        }
    }

    const fn failure(code: SubscriptionCode) -> Self {
        Self {
            code: code as u32,
            subscription_id: u32::MAX,
        }
    }
}

fn map_subscription_error(error: SubscriptionError) -> SubscriptionCode {
    match error {
        SubscriptionError::UnknownWorld => SubscriptionCode::UnknownWorld,
        SubscriptionError::UnknownDevice { .. } => SubscriptionCode::UnknownDevice,
        SubscriptionError::UnknownObserver { .. } => SubscriptionCode::UnknownObserver,
        SubscriptionError::IdExhausted => SubscriptionCode::IdExhausted,
        SubscriptionError::UnknownSubscription { .. } => SubscriptionCode::UnknownSubscription,
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionStatusCode {
    Available = 0,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubscriptionRecord {
    pub subscription_id: u32,
    pub status: u32,
    pub value: f64,
}

/// Creates a subscription to a device observer in a world.
///
/// `device_id` identifies a device in `world_id`. `observer_id` identifies an
/// observer in that device's definition. On success, `subscription_id` in
/// `result` contains the new subscription ID. On failure, it contains `u32::MAX`.
///
/// The caller receives observer values through [`hynergy_world_tick`]. The
/// caller can pass the subscription ID to [`hynergy_world_unsubscribe`] to
/// remove the subscription.
///
/// The function returns a [`SubscriptionCode`] value. If `result` is not null,
/// the function also writes that code and the subscription ID to `result`.
///
/// # Safety
///
/// If `engine` is not null, it must point to a live [`Engine`] with exclusive
/// access for this call. If `result` is not null, it must point to aligned,
/// writable storage for one [`SubscriptionCreationResult`]. The result and
/// engine storage must not overlap. The caller must prevent concurrent use
/// of the same engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_world_subscribe_observer(
    engine: *mut Engine,
    world_id: u32,
    device_id: u32,
    observer_id: u32,
    result: *mut SubscriptionCreationResult,
) -> u32 {
    if result.is_null() {
        return SubscriptionCode::NullResult as u32;
    }

    let output = if engine.is_null() {
        SubscriptionCreationResult::failure(SubscriptionCode::NullEngine)
    } else {
        let Ok(device) = DeviceId::try_from(device_id) else {
            let output = SubscriptionCreationResult::failure(SubscriptionCode::InvalidDeviceId);

            let code = output.code;

            unsafe {
                result.write(output);
            }

            return code;
        };

        let observer = DefinitionObserverId::new(observer_id);

        let subscription = catch_unwind(AssertUnwindSafe(|| {
            let engine = unsafe { &mut *engine };

            engine.subscribe_observer(world_id, device, observer)
        }));

        match subscription {
            Ok(Ok(subscription)) => SubscriptionCreationResult::success(subscription.get()),

            Ok(Err(error)) => SubscriptionCreationResult::failure(map_subscription_error(error)),

            Err(_) => SubscriptionCreationResult::failure(SubscriptionCode::InternalPanic),
        }
    };

    let code = output.code;

    unsafe {
        result.write(output);
    }

    code
}

/// Removes a subscription from a world.
///
/// `subscription_id` identifies a subscription in `world_id`. After this call
/// succeeds, later ticks do not report updates for the removed subscription.
/// The function returns a [`SubscriptionCode`] value.
///
/// # Safety
///
/// If `engine` is not null, it must point to a live [`Engine`] with exclusive
/// access for this call. The caller must prevent concurrent use of the same
/// engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_world_unsubscribe(
    engine: *mut Engine,
    world_id: u32,
    subscription_id: u32,
) -> u32 {
    if engine.is_null() {
        return SubscriptionCode::NullEngine as u32;
    }

    let Ok(subscription) = SubscriptionId::try_from(subscription_id) else {
        return SubscriptionCode::InvalidSubscriptionId as u32;
    };

    let removal = catch_unwind(AssertUnwindSafe(|| {
        let engine = unsafe { &mut *engine };

        engine.unsubscribe(world_id, subscription)
    }));

    match removal {
        Ok(Ok(())) => SubscriptionCode::Success as u32,

        Ok(Err(error)) => map_subscription_error(error) as u32,

        Err(_) => SubscriptionCode::InternalPanic as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hynergy_model::device::definition::{DefinitionId, PrimitiveElementKind};
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

    fn subscription_result_sentinel() -> SubscriptionCreationResult {
        SubscriptionCreationResult {
            code: 0xaaaa_aaaa,
            subscription_id: 0xbbbb_bbbb,
        }
    }

    fn subscribe_observer(engine: *mut Engine, world: u32, device: u32, observer: u32) -> u32 {
        let mut result = subscription_result_sentinel();

        assert_eq!(
            unsafe {
                hynergy_world_subscribe_observer(engine, world, device, observer, &mut result)
            },
            SubscriptionCode::Success as u32,
        );

        result.subscription_id
    }

    fn assert_close(actual: f64, expected: f64) {
        let tolerance = 1.0e-10 * expected.abs().max(1.0);

        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual}",
        );
    }

    fn record_value(records: &[SubscriptionRecord], subscription: u32) -> f64 {
        records
            .iter()
            .find(|record| record.subscription_id == subscription)
            .expect("subscription update must exist")
            .value
    }

    #[test]
    fn primitive_voltage_and_current_observers_preserve_current_direction() {
        const ADD_WIRE: u16 = 1;
        const ADD_DEVICE: u16 = 5;
        const ATTACH_TERMINAL: u16 = 7;
        const SET_DEVICE_PARAMETER: u16 = 9;

        let engine = hynergy_engine_create(1);
        let world = create_world(engine).world_id;

        let conductance = 1;
        let source = 2;

        let negative = 1;
        let positive = 2;

        let commands = [
            world_command(ADD_WIRE, &u32_payload(&[negative])),
            world_command(ADD_WIRE, &u32_payload(&[positive])),
            world_command(
                ADD_DEVICE,
                &u32_payload(&[
                    conductance,
                    DefinitionId::from(PrimitiveElementKind::Conductance).get(),
                ]),
            ),
            world_command(
                ADD_DEVICE,
                &u32_payload(&[
                    source,
                    DefinitionId::from(PrimitiveElementKind::VoltageSource).get(),
                ]),
            ),
            world_command(ATTACH_TERMINAL, &u32_payload(&[positive, conductance, 0])),
            world_command(ATTACH_TERMINAL, &u32_payload(&[negative, conductance, 1])),
            world_command(ATTACH_TERMINAL, &u32_payload(&[positive, source, 0])),
            world_command(ATTACH_TERMINAL, &u32_payload(&[negative, source, 1])),
            world_command(
                SET_DEVICE_PARAMETER,
                &set_parameter_payload(conductance, 0, 2.0),
            ),
            world_command(SET_DEVICE_PARAMETER, &set_parameter_payload(source, 0, 5.0)),
        ];

        let mut command_result = command_result_sentinel();

        assert_eq!(
            apply(engine, world, &world_buffer(&commands), &mut command_result,),
            CommandCode::Success as u32,
        );

        let conductance_voltage = subscribe_observer(engine, world, conductance, 0);
        let conductance_current = subscribe_observer(engine, world, conductance, 1);
        let source_current = subscribe_observer(engine, world, source, 1);

        let mut records = [
            subscription_record_sentinel(),
            subscription_record_sentinel(),
            subscription_record_sentinel(),
        ];

        let mut result = tick_result_sentinel();

        assert_eq!(
            unsafe {
                hynergy_world_tick(
                    engine,
                    world,
                    records.as_mut_ptr(),
                    records.len() as u32,
                    &mut result,
                )
            },
            TickCode::Success as u32,
        );

        assert_eq!(result.record_count, 3);

        assert_close(record_value(&records, conductance_voltage), 5.0);
        assert_close(record_value(&records, conductance_current), 10.0);
        assert_close(record_value(&records, source_current), -10.0);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn vccs_observers_publish_output_control_and_current() {
        const ADD_WIRE: u16 = 1;
        const ADD_DEVICE: u16 = 5;
        const ATTACH_TERMINAL: u16 = 7;
        const SET_DEVICE_PARAMETER: u16 = 9;

        let engine = hynergy_engine_create(1);
        let world = create_world(engine).world_id;

        let reference = 1;
        let output = 2;
        let control = 3;

        let vccs = 1;
        let output_source = 2;
        let control_source = 3;

        let commands = [
            world_command(ADD_WIRE, &u32_payload(&[reference])),
            world_command(ADD_WIRE, &u32_payload(&[output])),
            world_command(ADD_WIRE, &u32_payload(&[control])),
            world_command(
                ADD_DEVICE,
                &u32_payload(&[
                    vccs,
                    DefinitionId::from(PrimitiveElementKind::VoltageControlledCurrentSource).get(),
                ]),
            ),
            world_command(
                ADD_DEVICE,
                &u32_payload(&[
                    output_source,
                    DefinitionId::from(PrimitiveElementKind::VoltageSource).get(),
                ]),
            ),
            world_command(
                ADD_DEVICE,
                &u32_payload(&[
                    control_source,
                    DefinitionId::from(PrimitiveElementKind::VoltageSource).get(),
                ]),
            ),
            world_command(ATTACH_TERMINAL, &u32_payload(&[output, vccs, 0])),
            world_command(ATTACH_TERMINAL, &u32_payload(&[reference, vccs, 1])),
            world_command(ATTACH_TERMINAL, &u32_payload(&[control, vccs, 2])),
            world_command(ATTACH_TERMINAL, &u32_payload(&[reference, vccs, 3])),
            world_command(ATTACH_TERMINAL, &u32_payload(&[output, output_source, 0])),
            world_command(
                ATTACH_TERMINAL,
                &u32_payload(&[reference, output_source, 1]),
            ),
            world_command(ATTACH_TERMINAL, &u32_payload(&[control, control_source, 0])),
            world_command(
                ATTACH_TERMINAL,
                &u32_payload(&[reference, control_source, 1]),
            ),
            world_command(SET_DEVICE_PARAMETER, &set_parameter_payload(vccs, 0, 2.0)),
            world_command(
                SET_DEVICE_PARAMETER,
                &set_parameter_payload(output_source, 0, 4.0),
            ),
            world_command(
                SET_DEVICE_PARAMETER,
                &set_parameter_payload(control_source, 0, 3.0),
            ),
        ];

        let mut command_result = command_result_sentinel();

        assert_eq!(
            apply(engine, world, &world_buffer(&commands), &mut command_result,),
            CommandCode::Success as u32,
        );

        let output_voltage = subscribe_observer(engine, world, vccs, 0);
        let control_voltage = subscribe_observer(engine, world, vccs, 1);
        let output_current = subscribe_observer(engine, world, vccs, 2);

        let mut records = [
            subscription_record_sentinel(),
            subscription_record_sentinel(),
            subscription_record_sentinel(),
        ];

        let mut result = tick_result_sentinel();

        assert_eq!(
            unsafe {
                hynergy_world_tick(
                    engine,
                    world,
                    records.as_mut_ptr(),
                    records.len() as u32,
                    &mut result,
                )
            },
            TickCode::Success as u32,
        );

        assert_eq!(result.record_count, 3);

        assert_close(record_value(&records, output_voltage), 4.0);
        assert_close(record_value(&records, control_voltage), 3.0);
        assert_close(record_value(&records, output_current), 6.0);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn tick_delay_observers_follow_partition_and_snapshot_semantics() {
        const ADD_WIRE: u16 = 1;
        const ADD_DEVICE: u16 = 5;
        const ATTACH_TERMINAL: u16 = 7;
        const SET_DEVICE_PARAMETER: u16 = 9;

        let engine = hynergy_engine_create(1);
        let world = create_world(engine).world_id;

        let input_negative = 1;
        let input_positive = 2;
        let output_negative = 3;
        let output_positive = 4;

        let delay = 1;
        let input_source = 2;
        let output_load = 3;

        let commands = [
            world_command(ADD_WIRE, &u32_payload(&[input_negative])),
            world_command(ADD_WIRE, &u32_payload(&[input_positive])),
            world_command(ADD_WIRE, &u32_payload(&[output_negative])),
            world_command(ADD_WIRE, &u32_payload(&[output_positive])),
            world_command(
                ADD_DEVICE,
                &u32_payload(&[
                    delay,
                    DefinitionId::from(PrimitiveElementKind::TickDelay).get(),
                ]),
            ),
            world_command(
                ADD_DEVICE,
                &u32_payload(&[
                    input_source,
                    DefinitionId::from(PrimitiveElementKind::VoltageSource).get(),
                ]),
            ),
            world_command(
                ADD_DEVICE,
                &u32_payload(&[
                    output_load,
                    DefinitionId::from(PrimitiveElementKind::Conductance).get(),
                ]),
            ),
            world_command(ATTACH_TERMINAL, &u32_payload(&[input_positive, delay, 0])),
            world_command(ATTACH_TERMINAL, &u32_payload(&[input_negative, delay, 1])),
            world_command(ATTACH_TERMINAL, &u32_payload(&[output_positive, delay, 2])),
            world_command(ATTACH_TERMINAL, &u32_payload(&[output_negative, delay, 3])),
            world_command(
                ATTACH_TERMINAL,
                &u32_payload(&[input_positive, input_source, 0]),
            ),
            world_command(
                ATTACH_TERMINAL,
                &u32_payload(&[input_negative, input_source, 1]),
            ),
            world_command(
                ATTACH_TERMINAL,
                &u32_payload(&[output_positive, output_load, 0]),
            ),
            world_command(
                ATTACH_TERMINAL,
                &u32_payload(&[output_negative, output_load, 1]),
            ),
            world_command(
                SET_DEVICE_PARAMETER,
                &set_parameter_payload(input_source, 0, 5.0),
            ),
            world_command(
                SET_DEVICE_PARAMETER,
                &set_parameter_payload(output_load, 0, 2.0),
            ),
        ];

        let mut command_result = command_result_sentinel();

        assert_eq!(
            apply(engine, world, &world_buffer(&commands), &mut command_result,),
            CommandCode::Success as u32,
        );

        let input_voltage = subscribe_observer(engine, world, delay, 0);
        let output_voltage = subscribe_observer(engine, world, delay, 1);
        let output_current = subscribe_observer(engine, world, delay, 2);

        let mut records = [
            subscription_record_sentinel(),
            subscription_record_sentinel(),
            subscription_record_sentinel(),
        ];

        let mut result = tick_result_sentinel();

        assert_eq!(
            unsafe {
                hynergy_world_tick(
                    engine,
                    world,
                    records.as_mut_ptr(),
                    records.len() as u32,
                    &mut result,
                )
            },
            TickCode::Success as u32,
        );

        assert_eq!(result.record_count, 3);

        assert_close(record_value(&records, input_voltage), 5.0);
        assert_close(record_value(&records, output_voltage), 0.0);
        assert_close(record_value(&records, output_current), 0.0);

        records.fill(subscription_record_sentinel());
        result = tick_result_sentinel();

        assert_eq!(
            unsafe {
                hynergy_world_tick(
                    engine,
                    world,
                    records.as_mut_ptr(),
                    records.len() as u32,
                    &mut result,
                )
            },
            TickCode::Success as u32,
        );

        assert_eq!(result.record_count, 2);

        assert_close(
            record_value(&records[..result.record_count as usize], output_voltage),
            5.0,
        );

        assert_close(
            record_value(&records[..result.record_count as usize], output_current),
            -10.0,
        );

        assert!(
            records[..result.record_count as usize]
                .iter()
                .all(|record| { record.subscription_id != input_voltage }),
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn definition_registration_preserves_command_error_location() {
        const ADD_TERMINAL: u16 = 1;
        const ADD_ELEMENT: u16 = 4;

        const VOLTAGE_CONTROLLED_SWITCH: u32 = 9;

        let engine = hynergy_engine_create(1);

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
                    1.0, // G_max
                    1.0, // G_min
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

        let engine = hynergy_engine_create(1);

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
        unsafe {
            hynergy_engine_register_definition(engine, bytes.as_ptr(), bytes.len() as u32, result)
        }
    }

    fn create_world(engine: *mut Engine) -> WorldCreationResult {
        let mut result = world_result_sentinel();

        assert_eq!(
            unsafe { hynergy_engine_create_world(engine, 30, &mut result) },
            WorldCode::Success as u32
        );

        result
    }

    fn apply(engine: *mut Engine, world: u32, bytes: &[u8], result: *mut CommandResult) -> u32 {
        unsafe {
            hynergy_world_apply_commands(engine, world, bytes.as_ptr(), bytes.len() as u32, result)
        }
    }

    fn set_parameter_payload(device_id: u32, parameter_id: u32, value: f64) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16);

        bytes.extend_from_slice(&device_id.to_le_bytes());
        bytes.extend_from_slice(&parameter_id.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());

        bytes
    }

    fn tick_result_sentinel() -> TickResult {
        TickResult {
            code: 0xaaaa_aaaa,
            record_count: 0xbbbb_bbbb,
            required_capacity: 0xcccc_cccc,
            device_id: 0xdddd_dddd,
            parameter_id: 0xeeee_eeee,
            iterations: 0xffff_ffff,
        }
    }

    fn subscription_record_sentinel() -> SubscriptionRecord {
        SubscriptionRecord {
            subscription_id: 0xaaaa_aaaa,
            status: 0xbbbb_bbbb,
            value: f64::from_bits(0xcccc_cccc_dddd_dddd),
        }
    }

    fn register_observed_conductance(engine: *mut Engine) -> u32 {
        const ADD_TERMINAL: u16 = 1;
        const ADD_ELEMENT: u16 = 4;
        const ADD_VOLTAGE_OBSERVER: u16 = 5;

        let commands = [
            definition_command(ADD_TERMINAL, &[]),
            definition_command(ADD_TERMINAL, &[]),
            definition_command(
                ADD_ELEMENT,
                &literal_element_payload(
                    DefinitionId::from(PrimitiveElementKind::Conductance).get(),
                    &[0, 1],
                    &[1.0],
                ),
            ),
            definition_command(ADD_VOLTAGE_OBSERVER, &u32_payload(&[0, 1])),
        ];

        let bytes = definition_buffer_with_commands(&commands);

        let mut result = definition_result_sentinel();

        assert_eq!(
            register(engine, &bytes, &mut result),
            DefinitionRegistrationCode::Success as u32,
        );

        assert_ne!(result.definition_id, u32::MAX);

        result.definition_id
    }

    fn create_observed_voltage_world(engine: *mut Engine, voltage: f64) -> (u32, u32, u32) {
        const ADD_WIRE: u16 = 1;
        const ADD_DEVICE: u16 = 5;
        const ATTACH_TERMINAL: u16 = 7;
        const SET_DEVICE_PARAMETER: u16 = 9;

        const NEGATIVE_WIRE: u32 = 1;
        const POSITIVE_WIRE: u32 = 2;

        const OBSERVED_DEVICE: u32 = 1;
        const SOURCE_DEVICE: u32 = 2;

        let observed_definition = register_observed_conductance(engine);

        let world = create_world(engine).world_id;

        let voltage_source_definition =
            DefinitionId::from(PrimitiveElementKind::VoltageSource).get();

        let commands = [
            world_command(ADD_WIRE, &u32_payload(&[NEGATIVE_WIRE])),
            world_command(ADD_WIRE, &u32_payload(&[POSITIVE_WIRE])),
            world_command(
                ADD_DEVICE,
                &u32_payload(&[OBSERVED_DEVICE, observed_definition]),
            ),
            world_command(
                ADD_DEVICE,
                &u32_payload(&[SOURCE_DEVICE, voltage_source_definition]),
            ),
            world_command(
                ATTACH_TERMINAL,
                &u32_payload(&[POSITIVE_WIRE, OBSERVED_DEVICE, 0]),
            ),
            world_command(
                ATTACH_TERMINAL,
                &u32_payload(&[NEGATIVE_WIRE, OBSERVED_DEVICE, 1]),
            ),
            world_command(
                ATTACH_TERMINAL,
                &u32_payload(&[POSITIVE_WIRE, SOURCE_DEVICE, 0]),
            ),
            world_command(
                ATTACH_TERMINAL,
                &u32_payload(&[NEGATIVE_WIRE, SOURCE_DEVICE, 1]),
            ),
            world_command(
                SET_DEVICE_PARAMETER,
                &set_parameter_payload(SOURCE_DEVICE, 0, voltage),
            ),
        ];

        let bytes = world_buffer(&commands);

        let mut result = command_result_sentinel();

        assert_eq!(
            apply(engine, world, &bytes, &mut result),
            CommandCode::Success as u32,
        );

        (world, OBSERVED_DEVICE, SOURCE_DEVICE)
    }

    fn subscribe_first_observer(engine: *mut Engine, world: u32, device: u32) -> u32 {
        let mut result = subscription_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_subscribe_observer(engine, world, device, 0, &mut result,) },
            SubscriptionCode::Success as u32,
        );

        assert_eq!(result.code, SubscriptionCode::Success as u32,);

        assert_ne!(result.subscription_id, 0);
        assert_ne!(result.subscription_id, u32::MAX);

        result.subscription_id
    }

    #[test]
    fn abi_function_signatures_are_stable() {
        let _: extern "C" fn() -> u32 = hynergy_abi_version;

        let _: extern "C" fn() -> u32 = hynergy_abi_revision;

        let _: extern "C" fn(u32) -> *mut Engine = hynergy_engine_create;

        let _: unsafe extern "C" fn(*mut Engine) = hynergy_engine_destroy;

        let _: unsafe extern "C" fn(
            *mut Engine,
            *const u8,
            u32,
            *mut DefinitionRegistrationResult,
        ) -> u32 = hynergy_engine_register_definition;

        let _: unsafe extern "C" fn(*mut Engine, u32, *mut WorldCreationResult) -> u32 =
            hynergy_engine_create_world;

        let _: unsafe extern "C" fn(*mut Engine, u32) -> u32 = hynergy_engine_destroy_world;

        let _: unsafe extern "C" fn(*mut Engine, u32, *const u8, u32, *mut CommandResult) -> u32 =
            hynergy_world_apply_commands;

        let _: unsafe extern "C" fn(
            *mut Engine,
            u32,
            *mut SubscriptionRecord,
            u32,
            *mut TickResult,
        ) -> u32 = hynergy_world_tick;

        let _: unsafe extern "C" fn(
            *mut Engine,
            u32,
            u32,
            u32,
            *mut SubscriptionCreationResult,
        ) -> u32 = hynergy_world_subscribe_observer;

        let _: unsafe extern "C" fn(*mut Engine, u32, u32) -> u32 = hynergy_world_unsubscribe;
    }

    #[test]
    fn subscription_creation_result_layout_is_stable() {
        assert_eq!(size_of::<SubscriptionCreationResult>(), 8,);

        assert_eq!(align_of::<SubscriptionCreationResult>(), align_of::<u32>(),);

        assert_eq!(offset_of!(SubscriptionCreationResult, code), 0,);

        assert_eq!(offset_of!(SubscriptionCreationResult, subscription_id), 4,);
    }

    #[test]
    fn subscription_record_layout_is_stable() {
        assert_eq!(size_of::<SubscriptionRecord>(), 16,);

        assert_eq!(align_of::<SubscriptionRecord>(), align_of::<f64>(),);

        assert_eq!(offset_of!(SubscriptionRecord, subscription_id), 0,);

        assert_eq!(offset_of!(SubscriptionRecord, status), 4,);

        assert_eq!(offset_of!(SubscriptionRecord, value), 8,);
    }

    #[test]
    fn tick_result_layout_is_stable() {
        assert_eq!(size_of::<TickResult>(), 24);

        assert_eq!(align_of::<TickResult>(), align_of::<u32>(),);

        assert_eq!(offset_of!(TickResult, code), 0);
        assert_eq!(offset_of!(TickResult, record_count), 4);

        assert_eq!(offset_of!(TickResult, required_capacity), 8,);

        assert_eq!(offset_of!(TickResult, device_id), 12);

        assert_eq!(offset_of!(TickResult, parameter_id), 16,);

        assert_eq!(offset_of!(TickResult, iterations), 20,);
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
            StateCountExhausted = 36,

            InternalPanic = u32::MAX,
        });

        assert_codes!(WorldCode {
            Success = 0,
            NullEngine = 1,
            NullResult = 2,
            UnknownWorld = 3,
            WorldIdExhausted = 4,
            InvalidTickFrequency = 5,
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

        assert_codes!(SubscriptionCode {
            Success = 0,

            NullEngine = 1,
            NullResult = 2,
            UnknownWorld = 3,
            InvalidDeviceId = 4,
            InvalidSubscriptionId = 5,

            UnknownDevice = 20,
            UnknownObserver = 21,
            IdExhausted = 22,
            UnknownSubscription = 23,

            InternalPanic = u32::MAX,
        });

        assert_codes!(SubscriptionStatusCode {
            Available = 0,
        });

        assert_codes!(TickCode {
            Success = 0,

            NullEngine = 1,
            NullResult = 2,
            NullOutput = 3,
            UnknownWorld = 4,
            BufferTooSmall = 5,

            MissingParameter = 20,
            Singular = 21,
            NonlinearDidNotConverge = 22,
            NonFiniteMatrix = 23,
            NonFiniteSolution = 24,
            ResourceExhausted = 25,
            BackendFailure = 26,
            CompilationFailed = 27,
            InternalInvariant = 28,

            InternalPanic = u32::MAX,
        });
    }

    #[test]
    fn definition_registration_result_layout_is_stable() {
        assert_eq!(size_of::<DefinitionRegistrationResult>(), 16,);

        assert_eq!(
            align_of::<DefinitionRegistrationResult>(),
            align_of::<u32>(),
        );

        assert_eq!(offset_of!(DefinitionRegistrationResult, code), 0,);
        assert_eq!(offset_of!(DefinitionRegistrationResult, command_index), 4,);
        assert_eq!(offset_of!(DefinitionRegistrationResult, byte_offset), 8,);
        assert_eq!(offset_of!(DefinitionRegistrationResult, definition_id), 12,);
    }
    #[test]
    fn world_creation_result_layout_is_stable() {
        assert_eq!(size_of::<WorldCreationResult>(), 8);
        assert_eq!(align_of::<WorldCreationResult>(), align_of::<u32>(),);
        assert_eq!(offset_of!(WorldCreationResult, code), 0,);
        assert_eq!(offset_of!(WorldCreationResult, world_id), 4,);
    }

    #[test]
    fn command_result_layout_is_stable() {
        assert_eq!(size_of::<CommandResult>(), 16);
        assert_eq!(align_of::<CommandResult>(), align_of::<u32>(),);
        assert_eq!(offset_of!(CommandResult, code), 0);
        assert_eq!(offset_of!(CommandResult, command_index), 4,);
        assert_eq!(offset_of!(CommandResult, byte_offset), 8,);
        assert_eq!(offset_of!(CommandResult, reserved), 12,);
    }

    #[test]
    fn abi_version_and_revision_are_stable() {
        assert_eq!(hynergy_abi_version(), ABI_VERSION);
        assert_eq!(ABI_VERSION, 1);

        assert_eq!(hynergy_abi_revision(), ABI_REVISION);
        assert_eq!(ABI_REVISION, 2);
    }

    #[test]
    fn observer_subscription_lifecycle_is_exposed_through_ffi() {
        let engine = hynergy_engine_create(1);

        let (world, observed, _) = create_observed_voltage_world(engine, 5.0);

        let mut first = subscription_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_subscribe_observer(engine, world, observed, 0, &mut first,) },
            SubscriptionCode::Success as u32,
        );

        assert_eq!(first.code, SubscriptionCode::Success as u32,);

        assert_eq!(first.subscription_id, 1);

        let mut second = subscription_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_subscribe_observer(engine, world, observed, 0, &mut second,) },
            SubscriptionCode::Success as u32,
        );

        assert_eq!(second.subscription_id, 2);

        assert_eq!(
            unsafe { hynergy_world_unsubscribe(engine, world, first.subscription_id,) },
            SubscriptionCode::Success as u32,
        );

        assert_eq!(
            unsafe { hynergy_world_unsubscribe(engine, world, first.subscription_id,) },
            SubscriptionCode::UnknownSubscription as u32,
        );

        let mut third = subscription_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_subscribe_observer(engine, world, observed, 0, &mut third,) },
            SubscriptionCode::Success as u32,
        );

        assert_eq!(third.subscription_id, 3);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn observer_subscription_validates_device_and_observer() {
        let engine = hynergy_engine_create(1);

        let (world, observed, _) = create_observed_voltage_world(engine, 5.0);

        let mut result = subscription_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_subscribe_observer(engine, world, 999, 0, &mut result,) },
            SubscriptionCode::UnknownDevice as u32,
        );

        assert_eq!(
            result,
            SubscriptionCreationResult {
                code: SubscriptionCode::UnknownDevice as u32,
                subscription_id: u32::MAX,
            },
        );

        result = subscription_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_subscribe_observer(engine, world, observed, 1, &mut result,) },
            SubscriptionCode::UnknownObserver as u32,
        );

        assert_eq!(
            result,
            SubscriptionCreationResult {
                code: SubscriptionCode::UnknownObserver as u32,
                subscription_id: u32::MAX,
            },
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn subscription_ffi_rejects_invalid_raw_ids() {
        let engine = hynergy_engine_create(1);

        let world = create_world(engine).world_id;

        let mut result = subscription_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_subscribe_observer(engine, world, 0, 0, &mut result,) },
            SubscriptionCode::InvalidDeviceId as u32,
        );

        assert_eq!(
            result,
            SubscriptionCreationResult {
                code: SubscriptionCode::InvalidDeviceId as u32,
                subscription_id: u32::MAX,
            },
        );

        assert_eq!(
            unsafe { hynergy_world_unsubscribe(engine, world, 0,) },
            SubscriptionCode::InvalidSubscriptionId as u32,
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn subscribe_rejects_null_result_without_subscribing() {
        let engine = hynergy_engine_create(1);

        let (world, observed, _) = create_observed_voltage_world(engine, 5.0);

        assert_eq!(
            unsafe {
                hynergy_world_subscribe_observer(engine, world, observed, 0, std::ptr::null_mut())
            },
            SubscriptionCode::NullResult as u32,
        );

        let mut result = subscription_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_subscribe_observer(engine, world, observed, 0, &mut result,) },
            SubscriptionCode::Success as u32,
        );

        assert_eq!(result.subscription_id, 1);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn tick_writes_subscription_updates_to_caller_buffer() {
        let engine = hynergy_engine_create(1);

        let (world, observed, _) = create_observed_voltage_world(engine, 5.0);

        let subscription = subscribe_first_observer(engine, world, observed);

        let mut result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, std::ptr::null_mut(), 0, &mut result,) },
            TickCode::BufferTooSmall as u32,
        );

        assert_eq!(
            result,
            TickResult {
                code: TickCode::BufferTooSmall as u32,
                record_count: 0,
                required_capacity: 1,
                device_id: u32::MAX,
                parameter_id: u32::MAX,
                iterations: u32::MAX,
            },
        );

        let mut record = subscription_record_sentinel();

        result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, &mut record, 1, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(
            result,
            TickResult {
                code: TickCode::Success as u32,
                record_count: 1,
                required_capacity: 1,
                device_id: u32::MAX,
                parameter_id: u32::MAX,
                iterations: u32::MAX,
            },
        );

        assert_eq!(record.subscription_id, subscription,);

        assert_eq!(record.status, SubscriptionStatusCode::Available as u32,);

        assert_eq!(record.value.to_bits(), 5.0f64.to_bits(),);

        record = subscription_record_sentinel();
        result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, &mut record, 1, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(result.record_count, 0);
        assert_eq!(result.required_capacity, 1);

        assert_eq!(record, subscription_record_sentinel(),);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn tick_publishes_changed_subscription_value() {
        const SET_DEVICE_PARAMETER: u16 = 9;

        let engine = hynergy_engine_create(1);
        let (world, observed, source) = create_observed_voltage_world(engine, 5.0);
        let subscription = subscribe_first_observer(engine, world, observed);

        let mut record = subscription_record_sentinel();
        let mut result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, &mut record, 1, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(result.record_count, 1);

        assert_eq!(record.value.to_bits(), 5.0f64.to_bits(),);

        let bytes = world_buffer(&[world_command(
            SET_DEVICE_PARAMETER,
            &set_parameter_payload(source, 0, 7.0),
        )]);

        let mut command_result = command_result_sentinel();

        assert_eq!(
            apply(engine, world, &bytes, &mut command_result,),
            CommandCode::Success as u32,
        );

        record = subscription_record_sentinel();
        result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, &mut record, 1, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(result.record_count, 1);
        assert_eq!(record.subscription_id, subscription,);
        assert_eq!(record.status, SubscriptionStatusCode::Available as u32,);
        assert_eq!(record.value.to_bits(), 7.0f64.to_bits(),);

        result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, &mut record, 1, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(result.record_count, 0);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn tick_rejects_null_output_before_executing() {
        let engine = hynergy_engine_create(1);
        let (world, observed, _) = create_observed_voltage_world(engine, 5.0);
        let subscription = subscribe_first_observer(engine, world, observed);

        let mut result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, std::ptr::null_mut(), 1, &mut result,) },
            TickCode::NullOutput as u32,
        );

        assert_eq!(result.record_count, 0);
        assert_eq!(result.required_capacity, 1);

        let mut record = subscription_record_sentinel();

        result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, &mut record, 1, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(result.record_count, 1);
        assert_eq!(record.subscription_id, subscription);
        assert_eq!(record.value.to_bits(), 5.0f64.to_bits(),);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn tick_allows_null_output_when_no_subscriptions_exist() {
        let engine = hynergy_engine_create(1);
        let (world, _, _) = create_observed_voltage_world(engine, 5.0);

        let mut result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, std::ptr::null_mut(), 0, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(
            result,
            TickResult {
                code: TickCode::Success as u32,
                record_count: 0,
                required_capacity: 0,
                device_id: u32::MAX,
                parameter_id: u32::MAX,
                iterations: u32::MAX,
            },
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn unsubscribe_reduces_tick_required_capacity() {
        let engine = hynergy_engine_create(1);

        let (world, observed, _) = create_observed_voltage_world(engine, 5.0);

        let first = subscribe_first_observer(engine, world, observed);
        let second = subscribe_first_observer(engine, world, observed);

        assert_ne!(first, second);

        let mut records = [
            subscription_record_sentinel(),
            subscription_record_sentinel(),
        ];

        let mut result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, records.as_mut_ptr(), 2, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(result.record_count, 2);
        assert_eq!(result.required_capacity, 2);

        assert_eq!(
            unsafe { hynergy_world_unsubscribe(engine, world, first,) },
            SubscriptionCode::Success as u32,
        );

        result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, records.as_mut_ptr(), 1, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(result.required_capacity, 1);
        assert_eq!(result.record_count, 0);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn removing_device_removes_dependent_subscription_from_tick_capacity() {
        const REMOVE_DEVICE: u16 = 6;

        let engine = hynergy_engine_create(1);
        let (world, observed, _) = create_observed_voltage_world(engine, 5.0);
        let subscription = subscribe_first_observer(engine, world, observed);

        let mut record = subscription_record_sentinel();
        let mut result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, &mut record, 1, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(result.required_capacity, 1);

        let bytes = world_buffer(&[world_command(REMOVE_DEVICE, &u32_payload(&[observed]))]);

        let mut command_result = command_result_sentinel();

        assert_eq!(
            apply(engine, world, &bytes, &mut command_result,),
            CommandCode::Success as u32,
        );

        result = tick_result_sentinel();

        assert_eq!(
            unsafe { hynergy_world_tick(engine, world, std::ptr::null_mut(), 0, &mut result,) },
            TickCode::Success as u32,
        );

        assert_eq!(result.required_capacity, 0);
        assert_eq!(result.record_count, 0);

        assert_eq!(
            unsafe { hynergy_world_unsubscribe(engine, world, subscription,) },
            SubscriptionCode::UnknownSubscription as u32,
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn engine_lifecycle_accepts_created_and_null_engines() {
        let engine = hynergy_engine_create(1);

        assert!(!engine.is_null());

        unsafe {
            hynergy_engine_destroy(engine);
            hynergy_engine_destroy(std::ptr::null_mut());
        }
    }

    #[test]
    fn definition_registration_returns_assigned_ids() {
        let engine = hynergy_engine_create(1);
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
        let engine = hynergy_engine_create(1);
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
        let engine = hynergy_engine_create(1);
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
        let engine = hynergy_engine_create(1);
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
        let engine = hynergy_engine_create(1);

        assert_eq!(
            unsafe { hynergy_engine_create_world(engine, 30, std::ptr::null_mut(),) },
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

        let code = unsafe { hynergy_engine_create_world(std::ptr::null_mut(), 30, &mut result) };

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
        let engine = hynergy_engine_create(1);

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
        let engine = hynergy_engine_create(1);
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
        let engine = hynergy_engine_create(1);
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
        let engine = hynergy_engine_create(1);
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
        let engine = hynergy_engine_create(1);
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
        let engine = hynergy_engine_create(1);
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
        let engine = hynergy_engine_create(1);
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
        let engine = hynergy_engine_create(1);
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
