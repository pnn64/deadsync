#![allow(clippy::style)]

use std::path;

#[cfg(feature = "build-bindgen")]
extern crate bindgen;

#[cfg(feature = "build-bindgen")]
fn generate_lib() {
    #[derive(Debug)]
    struct ParseCallbacks;

    impl bindgen::callbacks::ParseCallbacks for ParseCallbacks {
        fn int_macro(&self, name: &str, _value: i64) -> Option<bindgen::callbacks::IntKind> {
            if name.starts_with("OPUS") {
                Some(bindgen::callbacks::IntKind::Int)
            } else {
                None
            }
        }
    }

    const PREPEND_LIB: &str = "
#![no_std]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
";

    let out = path::PathBuf::new().join("src").join("lib.rs");

    let bindings = bindgen::Builder::default().header("src/wrapper.h")
                                              .raw_line(PREPEND_LIB.trim_ascii())
                                              .parse_callbacks(Box::new(ParseCallbacks))
                                              .generate_comments(false)
                                              .layout_tests(false)
                                              .ctypes_prefix("core::ffi")
                                              .allowlist_type("[oO]pus.+")
                                              .allowlist_function("[oO]pus.+")
                                              .allowlist_var("[oO].+")
                                              .use_core()
                                              .rustfmt_configuration_file(Some("bindgen.rustfmt.toml".into()))
                                              .generate()
                                              .expect("Unable to generate bindings");

    bindings.write_to_file(out).expect("Couldn't write bindings!");
}

#[cfg(not(feature = "build-bindgen"))]
fn generate_lib() {
}

#[cfg(feature = "bundled")]
fn get_android_vars() -> Option<(path::PathBuf, &'static str)> {
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");

    if let Ok(android_ndk) = std::env::var("ANDROID_NDK_HOME") {
        let mut toolchain_file = path::PathBuf::new();
        toolchain_file.push(android_ndk);
        toolchain_file.push("build");
        toolchain_file.push("cmake");
        toolchain_file.push("android.toolchain.cmake");

        let target = std::env::var("TARGET").unwrap();
        let abi = match target.as_str() {
            "armv7-linux-androideabi" => "armeabi-v7a",
            "aarch64-linux-android" => "arm64-v8a",
            "i686-linux-android" => "x86",
            "x86_64-linux-android" => "x86_64",
            _ => return None,
        };

        Some((toolchain_file, abi))
    } else {
        None
    }
}

#[cfg(feature = "bundled")]
fn set_cmake_define_if_present(config: &mut cmake::Config, name: &str) {
    if let Ok(value) = std::env::var(name) {
        config.define(name, value);
    } else if let Ok(value) = std::env::var(format!("CARGO_NDK_{}", name)) {
        config.define(name, value);
    } else {
        println!("cargo:warning=Unable to find Android env variable '{}'. Hope for good default...", name);
    }
}

#[cfg(feature = "bundled")]
fn build() {
    const CURRENT_DIR: &str = "opus";

    //Disable LTO if someone tries to force it (e.g. Arch makepkg)
    //This is necessary because cmake crate doesn't pass env variables at configure step, so we will
    //adjust both configure variables and general build env (just in case)
    fn remove_lto_options(cmake: &mut cmake::Config) {
        for (var, cmake_var) in [("CFLAGS", "CMAKE_C_FLAGS"), ("CXXFLAGS", "CMAKE_CXX_FLAGS")] {
            if let Ok(value) = std::env::var(var) {
                if value.contains("-flto") {
                    println!("cargo:warning=env::{var} contains LTO option. Overriding it...");
                    let filtered: String = value
                        .split_whitespace()
                        .filter(|f| !f.starts_with("-flto"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    cmake
                        .configure_arg(format!("-D{cmake_var}={filtered}"))
                        .env(var, filtered);
                }
            }
        }
    }

    //Converts bool to cmake's "ON/OFF"
    fn to_opt(val: bool) -> &'static str {
        if val {
            "ON"
        } else {
            "OFF"
        }
    }

    fn configure_cpu_features(cmake: &mut cmake::Config) {
        const DO_RUNTIME_DETECTION: bool = cfg!(not(feature = "no-runtime-feature-detection"));

        let target_features = std::env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_else(|err| {
            println!("cargo:warning=failed to get CARGO_CFG_TARGET_FEATURE: {err:?}. Assuming no features are guaranteed");
            String::new()
        });

        match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("aarch64") | Ok("arm") => {
                // TODO: feature detection on other arm flags?
                // OPUS_ARM_PRESUME_DOTPROD, OPUS_ARM_PRESUME_EDSP, OPUS_ARM_PRESUME_MEDIA
                let has_neon = target_features.split(',').any(|s| s.trim() == "neon");

                cmake
                    .define("OPUS_ARM_PRESUME_NEON", to_opt(has_neon))
                    .define("OPUS_ARM_MAY_HAVE_NEON", to_opt(DO_RUNTIME_DETECTION));
            }
            Ok("x86_64") | Ok("x86") => {
                let mut has_sse = false;
                let mut has_sse2 = false;
                let mut has_sse41 = false;
                let mut has_avx2 = false;
                let mut has_fma = false;

                for feat in target_features.split(',').map(|s| s.trim()) {
                    match feat {
                        "sse" => has_sse = true,
                        "sse2" => has_sse2 = true,
                        "sse4.1" => has_sse41 = true,
                        "avx2" => has_avx2 = true,
                        "fma" => has_fma = true,
                        _ => {}
                    }
                }

                cmake
                    .define("OPUS_X86_PRESUME_SSE", to_opt(has_sse))
                    .define("OPUS_X86_PRESUME_SSE2", to_opt(has_sse2))
                    .define("OPUS_X86_PRESUME_SSE4_1", to_opt(has_sse41))
                    .define("OPUS_X86_PRESUME_AVX2", to_opt(has_avx2 && has_fma))
                    .define("OPUS_X86_MAY_HAVE_SSE", to_opt(DO_RUNTIME_DETECTION))
                    .define("OPUS_X86_MAY_HAVE_SSE2", to_opt(DO_RUNTIME_DETECTION))
                    .define("OPUS_X86_MAY_HAVE_SSE4_1", to_opt(DO_RUNTIME_DETECTION))
                    .define("OPUS_X86_MAY_HAVE_AVX2", to_opt(DO_RUNTIME_DETECTION));
            }
            Err(err) => {
                println!("cargo:warning=failed to get CARGO_CFG_TARGET_ARCH: {err:?}. CPU feature configuration is left up to CMake");
            }
            _ => {}
        }
    }

    let mut cmake = cmake::Config::new(CURRENT_DIR);
    // Opus overrides CMake's runtime selection, including cmake-rs's /MT flag.
    // Match Rust's CRT to avoid mixing static and dynamic runtimes on MSVC.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let static_crt = std::env::var("CARGO_CFG_TARGET_FEATURE")
            .map(|features| features.split(',').any(|feature| feature == "crt-static"))
            .unwrap_or(false);
        cmake.define("OPUS_STATIC_RUNTIME", to_opt(static_crt));
    }
    let rust_lto = std::env::var("CARGO_ENCODED_RUSTFLAGS").is_ok_and(|opt| opt.contains("linker-plugin-lto"));
    if !rust_lto {
        remove_lto_options(&mut cmake);
    }

    let opt_level = std::env::var("OPT_LEVEL");
    let opt_level = opt_level.as_deref().unwrap_or_else(|err| {
        println!("cargo:warning=OPT_LEVEL error: {err:?}, assuming release");
        "3"
    });

    cmake.define("OPUS_INSTALL_PKG_CONFIG_MODULE", "OFF")
         .define("OPUS_INSTALL_CMAKE_CONFIG_MODULE", "OFF")
         //Defining these variables disable GNUInstallDirs so in addition to /lib
         //define some commonly build stuff too.
         .define("CMAKE_INSTALL_BINDIR", "bin")
         .define("CMAKE_INSTALL_MANDIR", "man")
         .define("CMAKE_INSTALL_INCLUDEDIR", "include")
         .define("CMAKE_INSTALL_OLDINCLUDEDIR", "include")
         .define("CMAKE_INSTALL_LIBDIR", "lib")
         .define("CMAKE_TRY_COMPILE_TARGET_TYPE", "STATIC_LIBRARY")
         .define("CMAKE_INTERPROCEDURAL_OPTIMIZATION", to_opt(rust_lto))
         .define("CMAKE_BUILD_TYPE", match opt_level {
             "s" | "z" => "MinSizeRel",
             // Using release build even in debug to decrease the number of rebuilds
             // Standard CMAKE_BUILD_TYPEs: "Release", "MinSizeRel", "RelWithDebInfo", "Debug"
             "0" | "1" | "2" | "3" => "Release",
             other => {
                 println!("cargo:warning=unexpected OPT_LEVEL='{other}', assuming release");
                 "Release"
             }
         });

    //Keep this up to date with Cargo.toml
    cmake.define("OPUS_DRED", to_opt(cfg!(feature = "dred")))
         .define("OPUS_OSCE", to_opt(cfg!(feature = "osce")))
         .define("OPUS_HARDENING", to_opt(cfg!(not(feature = "no-hardening"))))
         .define("OPUS_STACK_PROTECTOR", to_opt(cfg!(not(feature = "no-stack-protector"))))
         .define("OPUS_FORTIFY_SOURCE", to_opt(cfg!(not(feature = "no-fortify-source"))))
         .define("OPUS_DISABLE_INTRINSICS", to_opt(cfg!(feature = "no-simd")))
         .define("OPUS_FIXED_POINT", to_opt(cfg!(feature = "fixed-point")));

    configure_cpu_features(&mut cmake);

    if let Some((toolchain_file, abi)) = get_android_vars() {
        cmake.define("CMAKE_TOOLCHAIN_FILE", toolchain_file);
        cmake.define("ANDROID_ABI", abi);

        set_cmake_define_if_present(&mut cmake, "ANDROID_PLATFORM");
        set_cmake_define_if_present(&mut cmake, "ANDROID_STL");
        set_cmake_define_if_present(&mut cmake, "ANDROID_ARM_MODE");
        set_cmake_define_if_present(&mut cmake, "ANDROID_ARM_NEON");
    }

    // Use ninja if present on system
    if std::process::Command::new("ninja").arg("--version").status().map(|status| status.success()).unwrap_or(false) {
        cmake.generator("Ninja");
    }

    let mut out_dir = cmake.build();

    println!("cargo:rustc-link-lib=static=opus");

    out_dir.push("lib");
    println!("cargo:rustc-link-search=native={}", out_dir.display());

    // Add lib64 in addition on Linux as some systems may default to lib64
    #[cfg(target_os = "linux")]
    {
        out_dir.pop();
        out_dir.push("lib64");
        println!("cargo:rustc-link-search=native={}", out_dir.display());
    }
}

fn run() {
    generate_lib();

    //dont use any dynamic linking if bundling is requested
    #[cfg(feature = "bundled")]
    build();

    #[cfg(not(feature = "bundled"))]
    {
        enum LinkMode {
            Static,
            Dynamic,
        }

        impl core::fmt::Display for LinkMode {
            fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                match self {
                    Self::Static => fmt.write_str("static"),
                    Self::Dynamic => fmt.write_str("dylib"),
                }
            }
        }

        println!("cargo:rerun-if-env-changed=OPUS_LIB_DIR");
        println!("cargo:rerun-if-env-changed=OPUS_LIB_STATIC");

        let mode = match std::env::var("OPUS_LIB_STATIC") {
            Ok(is_static) if is_static.eq_ignore_ascii_case("true") => LinkMode::Static,
            _ => LinkMode::Dynamic
        };

        if let Ok(dir) = std::env::var("OPUS_LIB_DIR") {
            if !path::Path::new(&dir).exists() {
                println!("cargo:warning=env::OPUS_LIB_DIR='{}' does not exist", dir);
            }
            println!("cargo:rustc-link-search={}", dir);
        }
        //let the linker figure out the library path
        println!("cargo:rustc-link-lib={mode}=opus");
    }
}

fn main() {
    run()
}
