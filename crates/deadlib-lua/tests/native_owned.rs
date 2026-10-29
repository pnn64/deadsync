#![cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
use deadlib_lua::NativeCAbiLoader;
use std::{path::PathBuf, process::Command};

#[test]
fn c_programs_own_default_states_between_entries_and_close_every_heap() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            for sdk in [false, true] {
                run(sdk);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
fn build(sdk: bool) -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let folder = manifest.join("../../target/lua-native-owned").join(format!(
        "{}-{}-{}",
        std::process::id(),
        std::env::consts::ARCH,
        sdk
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let image = folder.join("native-owned.dll");
    let headers = manifest.join("../../vendor/lua-src/lua-5.1.5");
    let source = manifest.join("tests/native-default/native-owned.c");
    let boundary = manifest.join("src/native_abi/boundary.c");
    let version = format!(
        "DEADLUA_ABI_VERSION=0x{}ULL",
        env!("DEADLIB_LUA_NATIVE_ABI_VERSION")
    );
    let expected = include_str!("native-boundary/native-owned.expected")
        .lines()
        .map(|line| format!("\"{}\\n\"\n", line.escape_default()))
        .collect::<String>();
    std::fs::write(folder.join("native-owned-expected.h"), expected).unwrap();
    let output = if cfg!(windows) {
        let finder = PathBuf::from(std::env::var_os("ProgramFiles(x86)").unwrap())
            .join("Microsoft Visual Studio/Installer/vswhere.exe");
        let found = Command::new(finder)
            .args([
                "-latest",
                "-products",
                "*",
                "-find",
                "VC/Auxiliary/Build/vcvars64.bat",
            ])
            .output()
            .unwrap();
        assert!(found.status.success());
        let setup = String::from_utf8(found.stdout).unwrap().trim().to_string();
        let mut args = vec![
            "/nologo".into(),
            "/LD".into(),
            "/O2".into(),
            "/MT".into(),
            "/DLUA_BUILD_AS_DLL".into(),
            "/DLUA_CORE".into(),
            "/DNATIVE_OWNED_VERIFY".into(),
            format!("/I{}", folder.display()),
            format!("/D{version}"),
            format!("/I{}", headers.display()),
            source.to_string_lossy().into_owned(),
            boundary.to_string_lossy().into_owned(),
            "/link".into(),
            format!("/OUT:{}", image.display()),
        ];
        if sdk {
            args.retain(|arg| arg != "/DLUA_CORE" && arg != &boundary.to_string_lossy());
            args.insert(
                args.len() - 2,
                PathBuf::from(env!("OUT_DIR"))
                    .join("lua51.lib")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        let response = folder.join("build.rsp");
        std::fs::write(
            &response,
            args.iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(" "),
        )
        .unwrap();
        let script = folder.join("build.cmd");
        std::fs::write(
            &script,
            format!(
                "@call \"{setup}\" >nul\n@cl @\"{}\"\n@exit /b %errorlevel%\n",
                response.display()
            ),
        )
        .unwrap();
        Command::new("cmd")
            .args(["/d", "/c"])
            .arg(script)
            .current_dir(&folder)
            .output()
            .unwrap()
    } else {
        let mut cc = Command::new("cc");
        cc.args(["-shared", "-fPIC", "-O2", "-std=c11"]);
        if cfg!(target_arch = "x86") {
            cc.args(["-m32", "-msse2", "-mfpmath=sse"]);
        }
        cc.arg("-DNATIVE_OWNED_VERIFY").arg("-I").arg(&folder);
        if !sdk {
            cc.arg(&boundary);
        }
        cc.arg(format!("-D{version}"))
            .arg("-I")
            .arg(headers)
            .arg(source)
            .arg("-o")
            .arg(&image)
            .output()
            .unwrap()
    };
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    image
}
fn run(sdk: bool) {
    let image = build(sdk);
    let path = image.to_string_lossy();
    // SAFETY: both int(void) programs and the C shim are built above.
    let loader = if sdk {
        NativeCAbiLoader::default()
    } else {
        unsafe { NativeCAbiLoader::from_library(path.as_bytes()) }.unwrap()
    };
    let first = unsafe { loader.program(path.as_bytes(), b"owned_first") }.unwrap();
    let second = unsafe { loader.program(path.as_bytes(), b"owned_second") }.unwrap();
    let verify = unsafe { loader.program(path.as_bytes(), b"owned_verify") }.unwrap();
    drop(loader);
    assert_eq!(first.run(), 123);
    drop(first);
    assert_eq!(second.run(), -51);
    assert_eq!(verify.run(), 0);
    drop(second);
    drop(verify);
    let stats = NativeCAbiLoader::statistics();
    assert_eq!(stats.operations, stats.returned);
    assert_eq!(stats.states_created, stats.states_destroyed);
    assert_eq!(stats.groups_created, stats.groups_destroyed);
    assert_eq!(stats.libraries_opened, stats.libraries_closed);
}
