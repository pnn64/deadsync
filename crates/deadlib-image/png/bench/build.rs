fn main() {
    println!("cargo:rerun-if-env-changed=PNG_COMPARISON_LIB");
    if std::env::var_os("CARGO_FEATURE_COMPARISON").is_some() {
        let path = std::env::var("PNG_COMPARISON_LIB")
            .expect("set PNG_COMPARISON_LIB to the CMake Release output directory");
        println!("cargo:rustc-link-search=native={path}");
        println!("cargo:rustc-link-lib=dylib=png_contenders");
    }
}
