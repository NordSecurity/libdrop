use libc::c_char;

static VERSION: &[u8] = concat!(env!("DROP_VERSION"), "\0").as_bytes();

#[no_mangle]
pub extern "C" fn norddrop_version() -> *const c_char {
    VERSION.as_ptr() as *const _
}
