// Preserve Symphonia's release wrapping arithmetic during sanitizer fuzzing.
// Native code retains cargo-fuzz's checked arithmetic and all instrumentation.
use std::{ffi::OsString, process::Command};
fn main() {
    let mut args = std::env::args_os().skip(1);
    let compiler = args.next().expect("compiler path");
    let mut args: Vec<OsString> = args.collect();
    let reference = args.windows(2).any(|a| a[0] == "--crate-name" && a[1] == "symphonia_bundle_flac");
    if reference {
        args.push("-C".into());
        args.push("overflow-checks=off".into());
        eprintln!("FLAC fuzz reference policy: symphonia_bundle_flac uses release wrapping arithmetic; sanitizer/debug assertions retained");
    }
    if let Some(root) = std::env::var_os("DEADLIB_FLAC_RUSTC_AUDIT") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).expect("audit directory");
        std::fs::write(root.join(format!("{}.txt", std::process::id())),
            format!("compiler: {compiler:?}\narguments: {args:#?}\n")).expect("compiler audit");
    }
    let status = Command::new(compiler).args(args).status().expect("rustc execution");
    std::process::exit(status.code().unwrap_or(1));
}
