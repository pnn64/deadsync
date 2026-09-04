fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc")
    {
        return;
    }

    // The crate name contains "updater", which triggers Windows installer
    // detection when a test executable has no explicit UAC manifest. Mark the
    // test harness as an ordinary unelevated process so `cargo test` can run it.
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTUAC:level='asInvoker'");
}
