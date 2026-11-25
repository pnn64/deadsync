//! Link settings for test executables, and the build identity that binds
//! persisted song-Lua bakes to the code that produced them.
include!("bake_build_identity.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // Windows otherwise treats test executable names containing "update"
        // as installers and refuses Cargo's normal, unelevated launch.
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-tests=/MANIFESTUAC:level='asInvoker'");
    }
    bake_build_identity("DEADSYNC_SONG_LUA_BAKE_BUILD");
}
