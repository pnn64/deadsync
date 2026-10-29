#![cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
use deadlib_lua::{NativeCAbiLoader, Vm};
use std::{path::PathBuf, process::Command};

#[test]
fn public_header_driver_retains_the_root_stack_and_its_library() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}
fn run() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let folder = manifest.join("../../target/lua-native-root").join(format!(
        "{}-{}",
        std::process::id(),
        std::env::consts::ARCH
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let image = folder.join("native-root.dll");
    let headers = manifest.join("../../vendor/lua-src/lua-5.1.5");
    let source = manifest.join("tests/native-default/native-root.c");
    let boundary = manifest.join("src/native_abi/boundary.c");
    let version = format!(
        "DEADLUA_ABI_VERSION=0x{}ULL",
        env!("DEADLIB_LUA_NATIVE_ABI_VERSION")
    );
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
        let args = [
            "/nologo".into(),
            "/LD".into(),
            "/O2".into(),
            "/MT".into(),
            "/DLUA_BUILD_AS_DLL".into(),
            "/DLUA_CORE".into(),
            format!("/D{version}"),
            format!("/I{}", headers.display()),
            source.to_string_lossy().into_owned(),
            boundary.to_string_lossy().into_owned(),
            "/link".into(),
            format!("/OUT:{}", image.display()),
        ];
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
        cc.arg(format!("-D{version}"))
            .arg("-I")
            .arg(headers)
            .arg(source)
            .arg(boundary)
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
    let path = image.to_string_lossy();
    // SAFETY: this image and all four entry signatures are built above.
    let loader = unsafe { NativeCAbiLoader::from_library(path.as_bytes()) }.unwrap();
    let first = unsafe { loader.driver(path.as_bytes(), b"root_first") }.unwrap();
    let second = unsafe { loader.driver(path.as_bytes(), b"root_second") }.unwrap();
    let snapshot = unsafe { loader.driver(path.as_bytes(), b"root_snapshot") }.unwrap();
    let mut vm = Vm::new_bare();
    assert_eq!(first.run(&mut vm).unwrap(), 517);
    drop(first);
    vm.collect_garbage().unwrap();
    assert_eq!(second.run(&mut vm).unwrap(), -37);
    assert_eq!(snapshot.run(&mut vm).unwrap(), 0);
    vm.with_native_stack(|stack| {
        assert_eq!(
            std::str::from_utf8(stack.to_bytes(-1)?.unwrap())
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            include_str!("native-boundary/native-root.expected")
                .lines()
                .collect::<Vec<_>>()
        );
        stack.set_top(2)
    })
    .unwrap();
    drop(second);
    drop(snapshot);
    drop(loader);
    vm.with_native_stack(|stack| {
        stack.push(stack.value(2).unwrap())?;
        stack.call_unprotected(0, Some(1))?;
        assert_eq!(stack.to_bytes(-1)?, Some(b"rooted value".as_slice()));
        Ok(())
    })
    .unwrap();
    drop(vm);
    // SAFETY: same fixture protocol and entry signature as above.
    let loader = unsafe { NativeCAbiLoader::from_library(path.as_bytes()) }.unwrap();
    let fail = unsafe { loader.driver(path.as_bytes(), b"root_error") }.unwrap();
    let mut vm = Vm::new_bare();
    let error = fail.run(&mut vm).unwrap_err();
    assert_eq!(error.message_bytes(), b"unprotected root error");
    vm.with_native_stack(|stack| {
        assert_eq!(stack.top(), 1);
        assert_eq!(
            stack.to_bytes(1)?,
            Some(b"unprotected root error".as_slice())
        );
        stack.set_top(0)
    })
    .unwrap();
}
