use std::{collections::HashSet, env, error::Error, iter::FromIterator, process::Command, str};

fn parse_version() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=.git/HEAD");

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

    parse_version()
}
