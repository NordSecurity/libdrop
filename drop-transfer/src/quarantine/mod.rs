#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(all(not(target_os = "macos"), not(windows)), path = "dummy.rs")]
mod plat;

pub(crate) trait PathExt {
    fn quarantine(&self) -> std::io::Result<()>;
}
