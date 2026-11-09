// Compile the production playback source with asset fixtures in an integration
// crate. This avoids the library-unit-test identity cycle through deadsync-assets.
pub use deadsync_song_lua::*;
#[expect(
    dead_code,
    reason = "playback uses only the shared quaternion composition helpers"
)]
#[path = "../src/actor_quaternion.rs"]
mod actor_quaternion;
#[path = "../src/runtime_fallback.rs"]
mod runtime_fallback;
pub mod playback {
    include!("../src/playback.rs");
    mod tests {
        include!("playback/cases.rs");
    }
}
#[path = "support/paths.rs"]
mod tests;
