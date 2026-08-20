pub const ABI_VERSION: u32 = 1;

#[unsafe(no_mangle)]
pub extern "C" fn hynergy_abi_version() -> u32 {
    ABI_VERSION
}
