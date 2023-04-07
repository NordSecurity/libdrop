use std::{collections::HashSet, env, error::Error, iter::FromIterator, process::Command, str};

fn parse_version() -> Result<String, Box<dyn Error>> {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=migrations");

    let version = match option_env!("LIBDROP_RELEASE_NAME") {
        Some(v) => v.to_string(),
        None => format!(
            "dev-{}",
            str::from_utf8(
                &Command::new("git")
                    .arg("rev-parse")
                    .arg("HEAD")
                    .output()?
                    .stdout
            )?
        ),
    };

    println!("cargo:rustc-env=DROP_VERSION={}", &version);

    Ok(version)
}

fn create_winres(version: &str) -> Result<(), Box<dyn Error>> {
    fn parse_ver(parse: &str) -> Option<[u16; 3]> {
        let (major, parse) = parse.split_once('.')?;
        let major: u16 = major.parse().ok()?;

        let (minor, parse) = parse.split_once('.')?;
        let minor: u16 = minor.parse().ok()?;

        let patch: u16 = parse.parse().ok()?;

        Some([major, minor, patch])
    }

    let version = version.strip_prefix('v').unwrap_or(version);

    let ver_uint = if let Some([major, minor, patch]) = parse_ver(version) {
        // Win version is of the form: MAJOR << 48 | MINOR << 32 | PATCH << 16 | RELEASE
        (major as u64) << 48 | (minor as u64) << 32 | (patch as u64) << 16
    } else {
        0
    };

    winresource::WindowsResource::new()
        .set_version_info(winresource::VersionInfo::FILEVERSION, ver_uint)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, ver_uint)
        .set("ProductVersion", version)
        .set("FileVersion", version)
        .compile()?;

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let langs: HashSet<&str> = HashSet::from_iter(["GO", "JAVA", "CS"].iter().copied());
    let ffis = env::var("FFI").unwrap_or_default();

    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS defined");
    let mut ffi: HashSet<&str> = ffis.split(',').filter(|c| langs.contains(c)).collect();

    if ffi.is_empty() {
        match &*target_os {
            "linux" => {
                ffi.insert("GO");
            }
            "android" => {
                ffi.insert("JAVA");
            }
            "windows" => {
                ffi.insert("CS");
            }
            _ => (),
        };
    }

    if ffi.contains(&"GO") {
        let path = format!("ffi/bindings/{target_os}/wrap/go_wrap.c");
        println!("cargo:rerun-if-changed={}", &path);
        cc::Build::new()
            .file(&path)
            .flag("-D_FORTIFY_SOURCE=2")
            .compile("go_wrap");
    }
    if ffi.contains(&"JAVA") {
        let path = format!("ffi/bindings/{target_os}/wrap/java_wrap.c",);
        println!("cargo:rerun-if-changed={}", &path);
        cc::Build::new()
            .file(&path)
            .flag("-D_FORTIFY_SOURCE=2")
            .link_lib_modifier("-bundle")
            .link_lib_modifier("+whole-archive")
            .compile("java_wrap");
    }
    if ffi.contains(&"CS") {
        let path = format!("ffi/bindings/{target_os}/wrap/csharp_wrap.c",);
        println!("cargo:rerun-if-changed={}", &path);
        cc::Build::new()
            .file(&path)
            .flag("-D_FORTIFY_SOURCE=2")
            .link_lib_modifier("-bundle")
            .link_lib_modifier("+whole-archive")
            .compile("cs_wrap");
    }

    {
        let path = "suppress_source_fortification_check.c";
        println!("cargo:rerun-if-changed={}", &path);
        let mut build = cc::Build::new();
        build.file(path);
        build.warnings_into_errors(true);

        if target_os != "windows" {
            build
                .flag("-fstack-protector-strong")
                .define("_FORTIFY_SOURCE", "2");
        }

        build.compile("suppressSourceFortificationCheck")
    }

    let version = parse_version()?;

    if target_os == "windows" {
        create_winres(&version)?;
    }

    Ok(())
}
