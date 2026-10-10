//! Loading feedback.
//!
//! This screen is waiting on four independent things -- the catalog, the pack
//! details, the doubles list and every banner -- and a marketplace that shows
//! nothing while it waits reads as broken rather than busy. So anything that
//! can be pending says so, in the same place it will eventually put the answer.
//!
//! The sprite is the engine's own `LoadingSpinner_10x3`, the one the updater
//! overlay uses, so a player sees one spinner in this game rather than two.

use crate::act;
use deadlib_present::actors::Actor;
use std::sync::LazyLock;
use std::time::Instant;

const SPINNER_TEXTURE: &str = "submit/LoadingSpinner_10x3.png";
const SPINNER_FRAMES: u32 = 30;
const SPINNER_FPS: f32 = 30.0;

/// Inline size, for a spinner standing in for a row or a card.
pub(super) const SMALL_PX: f32 = 20.0;

/// Seconds on the process-wide clock every piece of waiting feedback here is
/// animated by, so it all moves together.
pub(super) fn seconds() -> f32 {
    static SPIN_START: LazyLock<Instant> = LazyLock::new(Instant::now);
    SPIN_START.elapsed().as_secs_f32()
}

/// A spinning sprite centred on the given point.
///
/// The phase comes from a process-wide clock rather than from screen state, so
/// every spinner on screen turns together -- several out of step reads as
/// several unrelated problems.
pub(super) fn frame() -> u32 {
    ((seconds() * SPINNER_FPS) as u32) % SPINNER_FRAMES
}

/// The wheel in Simply Love's active colour, which is what the original tints
/// every one of its spinners with.
pub(super) fn accent_actor(cx: f32, cy: f32, px: f32, z: i16, accent: [f32; 4]) -> Actor {
    let frame = frame();
    act!(sprite(SPINNER_TEXTURE):
        align(0.5, 0.5):
        xy(cx, cy):
        setsize(px, px):
        setstate(frame):
        z(z):
        diffuse(accent[0], accent[1], accent[2], 1.0)
    )
}

/// A spinner with a word beside it, for a region that is waiting on something
/// nameable. The label sits to the right so a column of these lines up.
pub(super) fn with_label(
    actors: &mut Vec<Actor>,
    cx: f32,
    cy: f32,
    px: f32,
    label: &str,
    z: i16,
    accent: [f32; 4],
) {
    actors.push(accent_actor(cx, cy, px, z, accent));
    actors.push(act!(text:
        font("miso"):
        settext(label.to_owned()):
        align(0.0, 0.5):
        xy(cx + px * 0.5 + 8.0, cy):
        zoom(0.5):
        horizalign(left):
        diffuse(1.0, 1.0, 1.0, 0.6):
        z(z)
    ));
}

/// Three dots that cycle, for a label that is already words and only needs to
/// look alive. Reserves all three slots so the text never shifts.
pub(super) fn ellipsis() -> &'static str {
    static START: LazyLock<Instant> = LazyLock::new(Instant::now);
    // A hair under a second end to end: fast enough to read as alive, slow
    // enough not to strobe.
    match ((START.elapsed().as_secs_f32() / 0.25) as u32) % 4 {
        0 => "   ",
        1 => ".  ",
        2 => ".. ",
        _ => "...",
    }
}
