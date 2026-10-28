//! Bind persistent programs to this compiler/runtime source and target build.
use std::{env, fs, path::Path};

fn mix(state: &mut [u64; 2], bytes: &[u8]) {
    for &byte in bytes {
        state[0] = (state[0] ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        state[1] = (state[1] ^ u64::from(byte).wrapping_add(1))
            .rotate_left(7)
            .wrapping_mul(0x9e3779b185ebca87);
    }
    state[0] ^= bytes.len() as u64;
    state[1] ^= (bytes.len() as u64).rotate_left(32);
}

fn sources(path: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(path).expect("read Lua source directory") {
        let path = entry.expect("read Lua source entry").path();
        if path.is_dir() {
            sources(&path, files);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "rs" || ext == "c" || ext == "h")
        {
            files.push(path);
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=src");
    let mut files = vec!["build.rs".into(), "Cargo.toml".into()];
    sources(Path::new("src"), &mut files);
    files.sort_unstable();
    let mut state = [0xcbf29ce484222325, 0x6a09e667f3bcc909];
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        mix(
            &mut state,
            path.to_string_lossy().replace('\\', "/").as_bytes(),
        );
        // Checkout line endings do not change the compiler's build identity.
        let bytes = fs::read(&path).expect("read Lua build input");
        let normalized: Vec<_> = bytes
            .iter()
            .enumerate()
            .filter(|&(i, byte)| *byte != b'\r' || bytes.get(i + 1) != Some(&b'\n'))
            .map(|(_, &byte)| byte)
            .collect();
        mix(&mut state, &normalized);
    }
    for name in [
        "CARGO_CFG_TARGET_ARCH",
        "CARGO_CFG_TARGET_OS",
        "CARGO_CFG_TARGET_ENV",
        "CARGO_CFG_TARGET_ENDIAN",
        "CARGO_CFG_TARGET_POINTER_WIDTH",
        "CARGO_CFG_TARGET_FEATURE",
        "CARGO_ENCODED_RUSTFLAGS",
        "DEBUG",
        "OPT_LEVEL",
    ] {
        mix(&mut state, name.as_bytes());
        mix(&mut state, env::var(name).unwrap_or_default().as_bytes());
    }
    let compiler = std::process::Command::new(env::var_os("RUSTC").expect("Rust compiler path"))
        .arg("-vV")
        .output()
        .expect("Rust compiler version");
    assert!(compiler.status.success(), "Rust compiler version failed");
    mix(&mut state, &compiler.stdout);
    println!(
        "cargo:rustc-env=DEADLIB_LUA_CACHE_BUILD={:016x}{:016x}",
        state[0], state[1]
    );
    println!(
        "cargo:rustc-env=DEADLIB_LUA_NATIVE_ABI_VERSION={:016x}",
        state[0]
    );
    if env::var_os("CARGO_FEATURE_NATIVE_C_API").is_some() {
        native_image(state[0]);
    }
}

fn native_image(version: u64) {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    if os != "windows" && os != "linux" {
        return;
    }
    let out = std::path::PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let root = std::path::PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = root.join("src/native_abi/boundary.c");
    let name = if os == "windows" {
        "lua51.dll"
    } else {
        "liblua5.1.so.0"
    };
    let image = out.join(name);
    let define = format!("DEADLUA_ABI_VERSION=0x{version:016x}ULL");
    let log = if os == "windows" && env::var("CARGO_CFG_TARGET_ENV").unwrap() == "msvc" {
        let program_files =
            env::var_os("ProgramFiles(x86)").expect("Visual Studio installer directory");
        let finder = std::path::PathBuf::from(program_files)
            .join("Microsoft Visual Studio/Installer/vswhere.exe");
        let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
        let setup = match arch.as_str() {
            "x86" => "vcvars32.bat",
            "aarch64" => "vcvarsamd64_arm64.bat",
            _ => "vcvars64.bat",
        };
        let found = std::process::Command::new(finder)
            .args([
                "-latest",
                "-products",
                "*",
                "-find",
                &format!("VC/Auxiliary/Build/{setup}"),
            ])
            .output()
            .expect("find Visual Studio C compiler");
        assert!(found.status.success(), "Visual Studio discovery failed");
        let setup = String::from_utf8(found.stdout)
            .unwrap()
            .lines()
            .next()
            .expect("Visual Studio C compiler setup")
            .to_string();
        let response = out.join("native-abi.rsp");
        let args = [
            "/nologo".to_string(),
            "/O2".into(),
            "/MT".into(),
            "/LD".into(),
            "/D_CRT_SECURE_NO_WARNINGS".into(),
            format!("/D{define}"),
            source.to_string_lossy().into_owned(),
            "/link".into(),
            format!("/OUT:{}", image.display()),
            format!("/IMPLIB:{}", out.join("lua51.lib").display()),
        ];
        std::fs::write(
            &response,
            args.iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(" "),
        )
        .unwrap();
        let script = out.join("native-abi.cmd");
        std::fs::write(
            &script,
            format!(
                "@call \"{setup}\" >nul\n@cl @\"{}\"\n@exit /b %errorlevel%\n",
                response.display()
            ),
        )
        .unwrap();
        std::process::Command::new("cmd")
            .args(["/d", "/c"])
            .arg(script)
            .current_dir(&out)
            .output()
            .expect("compile Lua C ABI image")
    } else {
        let target = env::var("TARGET").unwrap().replace('-', "_");
        let cc_key = format!("CC_{target}");
        let cc = env::var_os(&cc_key)
            .or_else(|| env::var_os("CC"))
            .unwrap_or_else(|| "cc".into());
        println!("cargo:rerun-if-env-changed={cc_key}");
        println!("cargo:rerun-if-env-changed=CC");
        let mut compiler = std::process::Command::new(cc);
        compiler
            .args(["-shared", "-O2", "-fPIC", "-std=c11"])
            .arg(format!("-D{define}"));
        if os == "linux" {
            compiler.arg("-Wl,-soname,liblua5.1.so.0");
        }
        if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "x86" {
            compiler.args(["-m32", "-msse2", "-mfpmath=sse"]);
        }
        compiler
            .arg(source)
            .arg("-o")
            .arg(&image)
            .current_dir(&out)
            .output()
            .expect("compile Lua C ABI image")
    };
    std::fs::write(
        out.join("native-abi-build.log"),
        [log.stdout.as_slice(), log.stderr.as_slice()].concat(),
    )
    .unwrap();
    assert!(
        log.status.success(),
        "Lua C ABI image build failed: {}{}",
        String::from_utf8_lossy(&log.stdout),
        String::from_utf8_lossy(&log.stderr)
    );
    println!("cargo:rustc-env=DEADLIB_LUA_NATIVE_IMAGE={name}");
    let mut identity = [0xcbf29ce484222325, 0x6a09e667f3bcc909];
    mix(
        &mut identity,
        &fs::read(&image).expect("read native ABI image"),
    );
    println!(
        "cargo:rustc-env=DEADLIB_LUA_NATIVE_IMAGE_ID={:016x}{:016x}",
        identity[0], identity[1]
    );
}
