use libc::c_char;

static VERSION: &[u8] = concat!(env!("DROP_VERSION"), "\0").as_bytes();

/// @brief Get the version of the library
///
/// @return const char*
#[no_mangle]
pub extern "C" fn norddrop_version() -> *const c_char {
    VERSION.as_ptr() as *const _
}
