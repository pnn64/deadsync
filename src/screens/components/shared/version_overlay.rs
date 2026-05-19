//! Persistent build-version watermark, rendered in the bottom-right corner
//! of every screen. The goal is debuggability: when a user sends in a
//! gameplay video the engineer can read off the running build from the
//! footage without needing the user to dig through logs.
//!
//! Style is deliberately small and low-contrast so the overlay reads as
//! a watermark rather than a UI element. The toggle lives in
//! `Config.show_version_overlay`; the render-loop hook in
//! `app/mod.rs` checks it each frame.

use crate::act;
use crate::engine::present::actors::Actor;
use crate::engine::space::{screen_height, screen_width};
use std::sync::{Arc, OnceLock};

/// Z just under the FPS/stats overlay (`32020`) so the two don't fight
/// for visual prominence when both are enabled, and comfortably above
/// transitions (so the watermark stays visible through fades, matching
/// the stats overlay behavior).
const VERSION_OVERLAY_Z: i16 = 32010;

const MARGIN_X: f32 = -8.0;
const MARGIN_Y: f32 = -4.0;

static VERSION_TEXT: OnceLock<Arc<str>> = OnceLock::new();

#[inline]
fn version_text() -> Arc<str> {
    VERSION_TEXT
        .get_or_init(|| {
            let version = env!("CARGO_PKG_VERSION");
            let formatted = match option_env!("DEADSYNC_BUILD_HASH") {
                Some(hash) if !hash.is_empty() && hash != "unknown" => {
                    let short = &hash[..hash.len().min(7)];
                    format!("v{version}+{short}")
                }
                _ => format!("v{version}"),
            };
            Arc::<str>::from(formatted)
        })
        .clone()
}

/// Builds the version watermark actor list. Returns a single text actor;
/// the vector wrapper matches the convention used by other shared
/// overlay components (`stats_overlay`, `gamepad_overlay`).
pub fn build() -> Vec<Actor> {
    let text = version_text();
    let w = screen_width();
    let h = screen_height();
    vec![act!(text:
        align(1.0, 1.0):
        xy(w + MARGIN_X, h + MARGIN_Y):
        font("miso"):
        zoom(0.55):
        settext(text):
        diffuse(1.0, 1.0, 1.0, 0.55):
        horizalign(right):
        z(VERSION_OVERLAY_Z)
    )]
}
