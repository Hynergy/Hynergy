use network_engine::Engine;
use network_protocol::{DefinitionRegistrationError, DefinitionRegistrationErrorKind};
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

#[unsafe(no_mangle)]
pub extern "C" fn hynergy_abi_version() -> u32 {
    ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn hynergy_engine_create() -> *mut Engine {
    Box::into_raw(Box::new(Engine::new()))
}

/// # Safety
///
/// `engine` must have been returned by `hynergy_engine_create`,
/// and it must not have been destroyed previously.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hynergy_engine_destroy(engine: *mut Engine) {
    if !engine.is_null() {
        unsafe {
            drop(Box::from_raw(engine));
        }
    }
}

/// # Safety
///
/// `engine` must be null or point to a live [`Engine`] with exclusive access
/// for the call. `input` must point to `input_len` readable bytes. `result`
/// must point to writable, non-overlapping storage for one
/// [`DefinitionRegistrationResult`]. The caller must prevent concurrent use
/// of the same engine.
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
            network_protocol::register_definition_buffer(engine, input)
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
