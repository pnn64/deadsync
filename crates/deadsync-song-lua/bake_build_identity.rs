// Shared by the build scripts of the crates that key the song-Lua bake cache.
// Included with `include!`, so it carries no inner attributes or doc header.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{env, fs};

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

/// Path dependencies named outside dev-dependency tables. Cargo already
/// rebuilds this crate when one of them changes, so hashing them costs no
/// additional rebuilds.
fn path_dependencies(manifest: &Path) -> Vec<PathBuf> {
    let Ok(text) = fs::read_to_string(manifest) else {
        return Vec::new();
    };
    let directory = manifest.parent().expect("manifest directory");
    let mut development = false;
    let mut found = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            development = line.contains("dev-dependencies");
            continue;
        }
        if development {
            continue;
        }
        let Some((_, rest)) = line.split_once("path") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('"') else {
            continue;
        };
        if let Some((path, _)) = rest.split_once('"') {
            found.push(directory.join(path));
        }
    }
    found
}

/// Collect a crate's files. `watch` is set for the crate's own directory:
/// Cargo is told about each entry that counts, never about the directory
/// itself, so churn under an ignored subdirectory does not rerun this script.
fn crate_files(directory: &Path, watch: bool, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_dir = path.is_dir();
        // Sources and embedded assets decide a bake; tests, benches, fuzz
        // targets, documentation and build output never do.
        if is_dir
            && matches!(
                &*name,
                "target" | "tests" | "benches" | "bench" | "fuzz" | "examples" | "docs" | ".git"
            )
        {
            continue;
        }
        if watch {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        if is_dir {
            crate_files(&path, false, files);
        } else {
            files.push(path);
        }
    }
}

/// Hash the crate being built and every workspace crate it links, and expose
/// the result to that crate as the environment variable `variable`. A
/// persisted bake is then reused only by a build whose code could not have
/// produced anything else. A hand-bumped version would miss fixes that change
/// output without changing any format.
fn bake_build_identity(variable: &str) {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let mut crates = BTreeSet::new();
    let mut pending = vec![manifest_dir.clone()];
    while let Some(directory) = pending.pop() {
        let Ok(directory) = directory.canonicalize() else {
            continue;
        };
        if crates.insert(directory.clone()) {
            pending.extend(path_dependencies(&directory.join("Cargo.toml")));
        }
    }
    let root = crates.iter().filter_map(|path| path.parent()).fold(
        manifest_dir.clone(),
        |shared, parent| {
            let mut shared = shared;
            while !parent.starts_with(&shared) {
                if !shared.pop() {
                    break;
                }
            }
            shared
        },
    );
    let mut files = Vec::new();
    for directory in &crates {
        crate_files(directory, true, &mut files);
    }
    // Third-party versions change numeric results as surely as local edits.
    let mut lock = manifest_dir.clone();
    while lock.pop() {
        let candidate = lock.join("Cargo.lock");
        if candidate.is_file() {
            println!("cargo:rerun-if-changed={}", candidate.display());
            files.push(candidate);
            break;
        }
    }
    files.sort_unstable();
    let mut state = [0xcbf29ce484222325, 0x6a09e667f3bcc909];
    for path in files {
        let label = path.strip_prefix(&root).unwrap_or(&path);
        mix(
            &mut state,
            label.to_string_lossy().replace('\\', "/").as_bytes(),
        );
        // Checkout line endings do not change what the baker computes.
        let bytes = fs::read(&path).unwrap_or_default();
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
        "CARGO_FEATURE_TEST_SUPPORT",
        "DEBUG",
        "OPT_LEVEL",
        "RUSTC",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        mix(&mut state, name.as_bytes());
        mix(&mut state, env::var(name).unwrap_or_default().as_bytes());
    }
    if let Some(version) = env::var_os("RUSTC")
        .and_then(|rustc| std::process::Command::new(rustc).arg("-vV").output().ok())
    {
        mix(&mut state, &version.stdout);
    }
    println!(
        "cargo:rustc-env={variable}={:016x}{:016x}",
        state[0], state[1]
    );
}
