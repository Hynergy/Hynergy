use hynergy_engine::Engine;
use hynergy_protocol::{DefinitionRegistrationError, DefinitionRegistrationErrorKind};
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
    UnknownDevice = 20,
    TerminalCountMismatch = 21,
    ParameterCountMismatch = 22,
    NodeOutOfRange = 23,
    ParameterOutOfRange = 24,
    ParameterConstraintViolation = 25,
    NodeIdExhausted = 26,
    ParameterIdExhausted = 27,
    DeviceIdExhausted = 28,
    InvalidDefinition = 29,
    InternalPanic = u32::MAX,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionRegistrationResult {
    pub code: u32,
    pub command_index: u32,
    pub byte_offset: u32,
}

impl DefinitionRegistrationResult {
    const fn success() -> Self {
        Self {
            code: DefinitionRegistrationCode::Success as u32,
            command_index: u32::MAX,
            byte_offset: u32::MAX,
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
/// The function writes the operation result to `result` and returns the same
/// status code. A successful result uses `u32::MAX` for `command_index` and
/// `byte_offset`.
///
/// The function returns `NullResult` without processing the input if `result`
/// is null. It reports other invalid pointers through `result`. It catches a
/// Rust panic and reports `InternalPanic`.
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
            Ok(Ok(())) => DefinitionRegistrationResult::success(),
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
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WorldCreationResult {
    pub code: u32,
    pub world_id: u32,
}

/// Creates a world and returns its engine-assigned ID in `result`.
///
/// The engine owns the new world. The caller must use the returned world ID
/// for later world operations.
///
/// This function is not implemented. The caller must not call it.
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
    todo!("world creation is not implemented")
}

/// Destroys a world and all resources that the world owns.
///
/// `world_id` must identify a live world that belongs to `engine`. The ID is
/// invalid after this call succeeds.
///
/// This function is not implemented. The caller must not call it.
///
/// # Safety
///
/// `engine` must point to a live [`Engine`] with exclusive access for this
/// call. The caller must prevent concurrent use of the same engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_engine_destroy_world(engine: *mut Engine, world_id: u32) -> u32 {
    todo!("world destruction is not implemented")
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CommandResult {
    pub code: u32,
    pub command_index: u32,
    pub byte_offset: u32,
    pub applied_command_count: u32,
}

/// Applies a world command buffer in command order.
///
/// The operation is not atomic. If a command fails, all earlier successful
/// commands remain applied. The function stops at the first failure. It writes
/// the failure location and the applied command count to `result`.
///
/// This function is not implemented. The caller must not call it.
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
    todo!("world command buffers are not implemented")
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
    engine: *mut Engine,
    world_id: u32,
    publications: *mut Publication,
    publication_capacity: u32,
    result: *mut SolveResult,
) -> u32 {
    todo!("world solving is not implemented")
}

fn map_registration_error(error: DefinitionRegistrationError) -> DefinitionRegistrationResult {
    let code = match error.kind() {
        DefinitionRegistrationErrorKind::InvalidMagic => DefinitionRegistrationCode::InvalidMagic,
        DefinitionRegistrationErrorKind::UnsupportedVersion => {
            DefinitionRegistrationCode::UnsupportedVersion
        }
        DefinitionRegistrationErrorKind::InvalidFlags => DefinitionRegistrationCode::InvalidFlags,
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
        DefinitionRegistrationErrorKind::UnknownValueKind => {
            DefinitionRegistrationCode::UnknownValueKind
        }
        DefinitionRegistrationErrorKind::TrailingBytes => DefinitionRegistrationCode::TrailingBytes,
        DefinitionRegistrationErrorKind::UnknownDevice => DefinitionRegistrationCode::UnknownDevice,
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
        DefinitionRegistrationErrorKind::DeviceIdExhausted => {
            DefinitionRegistrationCode::DeviceIdExhausted
        }
        DefinitionRegistrationErrorKind::InvalidDefinition => {
            DefinitionRegistrationCode::InvalidDefinition
        }
        _ => DefinitionRegistrationCode::InvalidDefinition,
    };
    DefinitionRegistrationResult::failure(code, error.command_index(), error.byte_offset())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY_DEFINITION_BUFFER: [u8; 16] = [
        b'H',
        b'Y',
        b'D',
        b'F',
        1,
        0,
        0,
        0,
        Engine::COMPOSITE_DEFINITION_ID_BASE as u8,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];

    fn sentinel_result() -> DefinitionRegistrationResult {
        DefinitionRegistrationResult {
            code: 0xaaaa_aaaa,
            command_index: 0xbbbb_bbbb,
            byte_offset: 0xcccc_cccc,
        }
    }

    fn register(
        engine: *mut Engine,
        bytes: &[u8],
        result: *mut DefinitionRegistrationResult,
    ) -> u32 {
        unsafe { hynergy_engine_register_definition(engine, bytes.as_ptr(), bytes.len(), result) }
    }

    #[test]
    fn abi_version_is_stable() {
        assert_eq!(hynergy_abi_version(), 1);
    }

    #[test]
    fn registration_result_has_stable_c_layout() {
        assert_eq!(size_of::<DefinitionRegistrationResult>(), 12);
        assert_eq!(
            align_of::<DefinitionRegistrationResult>(),
            align_of::<u32>()
        );
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
    fn registration_reports_success_through_the_abi() {
        let engine = hynergy_engine_create();
        let mut result = sentinel_result();

        let code = register(engine, &EMPTY_DEFINITION_BUFFER, &mut result);

        assert_eq!(code, DefinitionRegistrationCode::Success as u32);
        assert_eq!(
            result,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::Success as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn registration_maps_protocol_errors_to_stable_abi_results() {
        let engine = hynergy_engine_create();
        let mut bytes = EMPTY_DEFINITION_BUFFER;
        bytes[0] = b'X';
        let mut result = sentinel_result();

        let code = register(engine, &bytes, &mut result);

        assert_eq!(code, DefinitionRegistrationCode::InvalidMagic as u32);
        assert_eq!(
            result,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::InvalidMagic as u32,
                command_index: u32::MAX,
                byte_offset: 0,
            }
        );

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }

    #[test]
    fn abi_rejects_null_pointers_without_mutating_the_engine() {
        let engine = hynergy_engine_create();
        let mut result = sentinel_result();

        let null_result_code = register(engine, &EMPTY_DEFINITION_BUFFER, std::ptr::null_mut());
        assert_eq!(
            null_result_code,
            DefinitionRegistrationCode::NullResult as u32
        );
        assert_eq!(result, sentinel_result());

        let null_engine_code =
            register(std::ptr::null_mut(), &EMPTY_DEFINITION_BUFFER, &mut result);
        assert_eq!(
            null_engine_code,
            DefinitionRegistrationCode::NullEngine as u32
        );
        assert_eq!(
            result,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::NullEngine as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
            }
        );

        let null_input_code =
            unsafe { hynergy_engine_register_definition(engine, std::ptr::null(), 0, &mut result) };
        assert_eq!(
            null_input_code,
            DefinitionRegistrationCode::NullInput as u32
        );
        assert_eq!(
            result,
            DefinitionRegistrationResult {
                code: DefinitionRegistrationCode::NullInput as u32,
                command_index: u32::MAX,
                byte_offset: u32::MAX,
            }
        );

        let success_code = register(engine, &EMPTY_DEFINITION_BUFFER, &mut result);
        assert_eq!(success_code, DefinitionRegistrationCode::Success as u32);

        unsafe {
            hynergy_engine_destroy(engine);
        }
    }
}
