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
        } else if path.extension().is_some_and(|ext| ext == "rs") {
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
}
