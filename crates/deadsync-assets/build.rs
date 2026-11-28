//! Build identity of the host side of a song-Lua bake: the noteskin, model
//! and multitap resolvers this crate supplies to the baker, and everything
//! they link.
include!("../deadsync-song-lua/bake_build_identity.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../deadsync-song-lua/bake_build_identity.rs");
    bake_build_identity("DEADSYNC_ASSETS_BAKE_BUILD");
}
