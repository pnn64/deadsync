//! Compile the existing C boundary with private names and export naked jumps.
use std::{collections::BTreeSet, env, fs, path::PathBuf, process::Command};

fn main() {
    println!(
        "cargo:rustc-env=DEADLUA_CAPI_TARGET={}",
        env::var("TARGET").unwrap()
    );
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if !matches!(os.as_str(), "windows" | "linux")
        || !matches!(arch.as_str(), "x86" | "x86_64" | "aarch64")
    {
        println!("cargo:warning=standalone Lua C exports are unavailable for this target");
        return;
    }
    let source =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../src/native_abi");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let embedded = out.join("embedded");
    fs::create_dir_all(&embedded).unwrap();
    let mut exports = BTreeSet::new();
    for entry in fs::read_dir(&source).unwrap() {
        let path = entry.unwrap().path();
        if !path.extension().is_some_and(|e| e == "c" || e == "h") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let mut copy = String::new();
        for line in fs::read_to_string(&path).unwrap().lines() {
            if let Some(line) = line.strip_prefix("API ") {
                let head = line.split_once('(').expect("C API prototype").0;
                let name = head
                    .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .find(|name| !name.is_empty())
                    .unwrap();
                exports.insert(name.to_owned());
                // Rename definitions only: internal calls and stored function
                // pointers must use the same public entries as a C caller.
                if line.contains('{') {
                    let start = head.rfind(name).unwrap();
                    copy.push_str("API ");
                    copy.push_str(&line[..start]);
                    copy.push_str("deadlua_c_");
                    copy.push_str(&line[start..]);
                    copy.push('\n');
                    continue;
                }
            }
            copy.push_str(line);
            copy.push('\n');
        }
        fs::write(embedded.join(path.file_name().unwrap()), copy).unwrap();
    }
    let mut rust = String::from("// Generated from the C image's actual API declarations.\n");
    let jump = if arch == "aarch64" { "b" } else { "jmp" };
    for name in &exports {
        rust.push_str(&format!(
            "unsafe extern \"C\" {{ fn deadlua_c_{name}(); }}\n\
             #[unsafe(naked)]\n#[unsafe(no_mangle)]\n\
             unsafe extern \"C\" fn {name}() {{\n\
             core::arch::naked_asm!(\"{jump} {{target}}\", target = sym deadlua_c_{name});\n}}\n"
        ));
    }
    fs::write(out.join("exports.rs"), rust).unwrap();
    fs::write(
        out.join("exports.txt"),
        exports.into_iter().collect::<Vec<_>>().join("\n") + "\n",
    )
    .unwrap();
    let object = out.join(if os == "windows" {
        "boundary.obj"
    } else {
        "boundary.o"
    });
    let result = if os == "windows" {
        let finder = PathBuf::from(env::var_os("ProgramFiles(x86)").unwrap())
            .join("Microsoft Visual Studio/Installer/vswhere.exe");
        let setup_name = match arch.as_str() {
            "x86" => "vcvars32.bat",
            "aarch64" => "vcvarsamd64_arm64.bat",
            _ => "vcvars64.bat",
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
        let setup = String::from_utf8(found.stdout)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_owned();
        let args = [
            "/nologo".into(),
            "/c".into(),
            "/O2".into(),
            "/MT".into(),
            "/D_CRT_SECURE_NO_WARNINGS".into(),
            "/DDEADLUA_EMBED_RUNTIME".into(),
            embedded.join("boundary.c").to_string_lossy().into_owned(),
            format!("/Fo{}", object.display()),
        ];
        let response = out.join("boundary.rsp");
        fs::write(
            &response,
            args.iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(" "),
        )
        .unwrap();
        let script = out.join("boundary.cmd");
        fs::write(
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
            .current_dir(&out)
            .output()
            .unwrap()
    } else {
        let target = env::var("TARGET").unwrap().replace('-', "_");
        let cc_key = format!("CC_{target}");
        println!("cargo:rerun-if-env-changed={cc_key}");
        println!("cargo:rerun-if-env-changed=CC");
        let mut cc = Command::new(
            env::var_os(&cc_key)
                .or_else(|| env::var_os("CC"))
                .unwrap_or_else(|| "cc".into()),
        );
        cc.args([
            "-c",
            "-O2",
            "-fPIC",
            "-std=c11",
            "-DDEADLUA_EMBED_RUNTIME",
            "-fvisibility=hidden",
        ]);
        if arch == "x86" {
            cc.args(["-m32", "-msse2", "-mfpmath=sse"]);
        }
        cc.arg(embedded.join("boundary.c"))
            .arg("-o")
            .arg(&object)
            .current_dir(&out)
            .output()
            .unwrap()
    };
    fs::write(
        out.join("boundary-build.log"),
        [result.stdout.as_slice(), result.stderr.as_slice()].concat(),
    )
    .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("cargo:rustc-link-arg-cdylib={}", object.display());
    if os == "windows" {
        println!(
            "cargo:rustc-link-arg-cdylib=/MAP:{}",
            out.join("lua51.map").display()
        );
    }
    if os == "linux" {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-Bsymbolic-functions");
        println!("cargo:rustc-link-arg-cdylib=-Wl,-soname,liblua5.1.so.0");
    }
}
