#![cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
use deadlib_lua::{NativeCAbiLoader, NativeLibraryLoader, Vm, compile_named};
use std::{path::PathBuf, process::Command};

#[test]
fn native_file_payloads_preserve_stdio_buffers_ownership_and_close_state() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let folder = manifest.join("../../target/lua-native-files").join(format!(
        "{}-{}",
        std::process::id(),
        std::env::consts::ARCH
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let image = folder.join("native-files.dll");
    let headers = manifest.join("../../vendor/lua-src/lua-5.1.5");
    let source = manifest.join("tests/native-default/native-files.c");
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
            format!("/IMPLIB:{}", folder.join("native-files.lib").display()),
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
    // SAFETY: the image is built above from this exact private protocol.
    let loader = std::sync::Arc::new(
        unsafe { NativeCAbiLoader::from_library(image.to_string_lossy().as_bytes()) }.unwrap(),
    );
    let mut vm = Vm::new_bare();
    let open = vm
        .new_native_function(loader.open_libraries().unwrap())
        .unwrap();
    vm.call(open, &[]).unwrap();
    let library = loader.open(image.to_string_lossy().as_bytes()).unwrap();
    let entry = vm
        .new_native_function(library.symbol(b"luaopen_native_files").unwrap())
        .unwrap();
    let module = vm.call(entry, &[]).unwrap()[0];
    vm.set_global(b"native_files", module).unwrap();
    let program = compile_named(
        include_bytes!("native-boundary/native-files.lua"),
        b"@native-files.lua",
    )
    .unwrap();
    let path = vm.intern(
        manifest
            .join("tests/native-boundary/native-files.lua")
            .to_string_lossy()
            .as_bytes(),
    );
    vm.set_global(b"native_files_source", path).unwrap();
    vm.execute(&program).unwrap();
    drop(vm);
    let mut vm = Vm::new();
    vm.set_native_library_loader(Some(loader.clone()));
    let before = NativeCAbiLoader::statistics().states_created;
    vm.execute(&compile_named(b"early=assert(io.tmpfile());assert(early:write('early\\n')and early:seek('set')==0 and early:read(2)=='ea')",b"=before C activation").unwrap()).unwrap();
    assert_eq!(NativeCAbiLoader::statistics().states_created, before);
    let library = loader.open(image.to_string_lossy().as_bytes()).unwrap();
    let entry = vm
        .new_native_function(library.symbol(b"luaopen_native_files").unwrap())
        .unwrap();
    let module = vm.call(entry, &[]).unwrap()[0];
    vm.set_global(b"native_files", module).unwrap();
    let program = compile_named(
        include_bytes!("native-boundary/file-payloads.lua"),
        b"@file-payloads.lua",
    )
    .unwrap();
    vm.execute(&program).unwrap();
    drop(vm);
    // Existing file pointers stay in their original C image after a late
    // adapter change. Reject the foreign callback before it can use them.
    let mut vm = Vm::new();
    vm.execute(
        &compile_named(
            b"keep=assert(io.tmpfile());assert(keep:write('same image')and keep:seek('set')==0)",
            b"=original image",
        )
        .unwrap(),
    )
    .unwrap();
    vm.set_native_library_loader(Some(loader.clone()));
    let library = loader.open(image.to_string_lossy().as_bytes()).unwrap();
    let entry = vm
        .new_native_function(library.symbol(b"luaopen_native_files").unwrap())
        .unwrap();
    assert!(
        vm.call(entry, &[])
            .unwrap_err()
            .message()
            .contains("one Lua state cannot mix C API images")
    );
    vm.execute(
        &compile_named(
            b"assert(keep:read('*a')=='same image'and keep:close())",
            b"=retained file",
        )
        .unwrap(),
    )
    .unwrap();
}
