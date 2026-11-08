#![cfg(all(
    any(windows, target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")
))]
use std::{path::PathBuf, process::Command};

#[test]
fn c_host_and_module_use_the_standalone_library_without_rust_bootstrap() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = env!("DEADLUA_CAPI_TARGET");
    let build = std::env::temp_dir()
        .join("deadlua-capi-test-build")
        .join(target);
    let compiled = Command::new(env!("CARGO"))
        .args([
            "build",
            "--locked",
            "-p",
            "deadlib-lua-capi",
            "--lib",
            "--target",
            target,
            "--target-dir",
        ])
        .arg(&build)
        .current_dir(manifest.join("../../.."))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}{}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    let profile = build.join(target).join("debug");
    let parent = if cfg!(windows) {
        manifest.join("../../../target/lua-capi-tests")
    } else {
        std::env::temp_dir().join("deadlua-capi-tests")
    };
    let folder = parent.join(format!("{}-{}", std::process::id(), std::env::consts::ARCH));
    std::fs::create_dir_all(&folder).unwrap();
    let runtime = profile.join(if cfg!(windows) {
        "lua51.dll"
    } else {
        "liblua51.so"
    });
    let deployed = folder.join(if cfg!(windows) {
        "lua51.dll"
    } else {
        "liblua5.1.so.0"
    });
    std::fs::copy(&runtime, deployed).unwrap();
    let headers = manifest.join("../../../vendor/lua-src/lua-5.1.5");
    let module = folder.join(if cfg!(windows) {
        "probe.dll"
    } else {
        "probe.so"
    });
    let host = folder.join(if cfg!(windows) { "host.exe" } else { "host" });
    for (is_module, name, output) in [
        (true, "capi-module.c", &module),
        (false, "capi-host.c", &host),
    ] {
        let source = manifest.join("tests").join(name);
        let result = if cfg!(windows) {
            let finder = PathBuf::from(std::env::var_os("ProgramFiles(x86)").unwrap())
                .join("Microsoft Visual Studio/Installer/vswhere.exe");
            let setup_name = if cfg!(target_arch = "x86") {
                "vcvars32.bat"
            } else if cfg!(target_arch = "aarch64") {
                "vcvarsamd64_arm64.bat"
            } else {
                "vcvars64.bat"
            };
            let found = Command::new(finder)
                .args([
                    "-latest",
                    "-products",
                    "*",
                    "-find",
                    &format!("VC/Auxiliary/Build/{setup_name}"),
                ])
                .output()
                .unwrap();
            assert!(found.status.success());
            let setup = String::from_utf8(found.stdout).unwrap().trim().to_string();
            let mut args = vec![
                "/nologo".into(),
                "/O2".into(),
                "/MT".into(),
                "/DLUA_BUILD_AS_DLL".into(),
                "/DCAPI_DIAGNOSTICS".into(),
                format!("/I{}", headers.display()),
                source.to_string_lossy().into_owned(),
                profile.join("lua51.dll.lib").to_string_lossy().into_owned(),
            ];
            if is_module {
                args.push("/LD".into());
            }
            args.extend([
                "/link".into(),
                "/STACK:33554432".into(),
                format!("/OUT:{}", output.display()),
            ]);
            let response = folder.join("build.rsp");
            std::fs::write(
                &response,
                args.iter()
                    .map(|arg| format!("\"{arg}\""))
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
            cc.arg("-O2");
            if cfg!(target_arch = "x86") {
                cc.args(["-m32", "-msse2", "-mfpmath=sse"]);
            }
            if is_module {
                cc.args(["-shared", "-fPIC"]);
            }
            cc.arg("-DCAPI_DIAGNOSTICS")
                .arg("-I")
                .arg(&headers)
                .arg(source);
            if !is_module {
                cc.arg(&runtime)
                    .arg(format!("-Wl,-rpath,{}", folder.display()))
                    .arg("-ldl");
            }
            cc.arg("-o").arg(output).output().unwrap()
        };
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let result = Command::new(&host)
        .arg("--module")
        .arg(&module)
        .current_dir(&folder)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        include_str!("capi-host.expected")
            .lines()
            .collect::<Vec<_>>()
    );
    let diagnostics = String::from_utf8(result.stderr).unwrap();
    assert!(
        diagnostics.starts_with("CAPI operations=")
            && diagnostics
                .trim_end()
                .ends_with("states=6 groups=5 libraries=1"),
        "{diagnostics}"
    );
    let memory = Command::new(&host)
        .arg("--memory")
        .current_dir(&folder)
        .output()
        .unwrap();
    assert!(
        memory.status.success(),
        "{}{}",
        String::from_utf8_lossy(&memory.stdout),
        String::from_utf8_lossy(&memory.stderr)
    );
    assert!(memory.stderr.is_empty());
    assert_eq!(
        String::from_utf8(memory.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        include_str!("memory.expected").lines().collect::<Vec<_>>()
    );
    for mode in [
        "--memory-default-panic",
        "--memory-returning-panic",
        "--memory-child-panic",
    ] {
        let memory = Command::new(&host)
            .arg(mode)
            .current_dir(&folder)
            .output()
            .unwrap();
        assert_eq!(memory.status.code(), Some(1), "{mode}");
        if mode == "--memory-default-panic" {
            assert!(memory.stdout.is_empty());
            assert_eq!(
                String::from_utf8(memory.stderr).unwrap().trim_end(),
                "PANIC: unprotected error in call to Lua API (not enough memory)"
            );
        } else {
            assert!(memory.stderr.is_empty());
            assert_eq!(
                String::from_utf8(memory.stdout)
                    .unwrap()
                    .lines()
                    .collect::<Vec<_>>(),
                [
                    "memory.panic.status\t4",
                    "memory.panic.top\t1",
                    "memory.panic.value\tnot enough memory"
                ]
            );
        }
    }
    let tables = Command::new(&host)
        .arg("--table-limits")
        .current_dir(&folder)
        .output()
        .unwrap();
    assert!(
        tables.status.success(),
        "{}{}",
        String::from_utf8_lossy(&tables.stdout),
        String::from_utf8_lossy(&tables.stderr)
    );
    assert!(tables.stderr.is_empty());
    assert_eq!(
        String::from_utf8(tables.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        include_str!("table-limits.expected")
            .lines()
            .collect::<Vec<_>>()
    );
    let tables = Command::new(&host)
        .arg("--table-child-panic")
        .current_dir(&folder)
        .output()
        .unwrap();
    assert_eq!(tables.status.code(), Some(1));
    assert!(tables.stderr.is_empty());
    assert_eq!(
        String::from_utf8(tables.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        [
            "table.limit.panic.status\t2",
            "table.limit.panic.top\t1",
            "table.limit.panic.value\ttable overflow"
        ]
    );
    for symbol in ["owned_default_panic", "owned_returning_panic"] {
        let result = Command::new(&host)
            .arg(symbol)
            .current_dir(&folder)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        if symbol == "owned_default_panic" {
            assert_eq!(
                String::from_utf8(result.stderr).unwrap().trim_end(),
                "PANIC: unprotected error in call to Lua API (default owned panic)"
            );
        } else {
            assert!(result.stderr.is_empty());
            assert!(
                String::from_utf8(result.stdout)
                    .unwrap()
                    .contains("returning.panic.status\t2")
            );
        }
    }
}
