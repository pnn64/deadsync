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
use crate::config::{LogLevel, VersionOverlaySide};
use deadlib_present::actors::Actor;
use deadlib_present::space::{screen_height, screen_width};
use std::sync::{Arc, OnceLock};

/// Z just under the FPS/stats overlay (`32020`) so the two don't fight
/// for visual prominence when both are enabled, and comfortably above
/// transitions (so the watermark stays visible through fades, matching
/// the stats overlay behavior).
const VERSION_OVERLAY_Z: i16 = 32010;

const MARGIN_X: f32 = 8.0;
const MARGIN_Y: f32 = -4.0;
/// Vertical offset of the log-level warning above the version line.
/// Picked to clear the version glyph height at `zoom(0.55)` with a
/// small visual gap.
const WARNING_OFFSET_Y: f32 = -12.0;

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

/// Returns the warning label to display for verbose log levels, or
/// `None` when the level is quiet enough that no warning is warranted.
/// Debug/Trace levels generate a lot of disk I/O and can mask real
/// problems behind a wall of noise, so we surface them on-screen the
/// same way we surface the build version.
#[inline]
fn log_warning_text(level: LogLevel) -> Option<&'static str> {
    match level {
        LogLevel::Debug => Some("log: debug"),
        LogLevel::Trace => Some("log: trace"),
        LogLevel::Error | LogLevel::Warn | LogLevel::Info => None,
    }
}

/// Builds the version watermark actor list. Returns a text actor
/// anchored to the bottom-left or bottom-right of the window based on
/// `side`, plus an optional amber log-level warning stacked just above
/// it when `log_level` is verbose (Debug/Trace). The vector wrapper
/// matches the convention used by other shared overlay components
/// (`stats_overlay`, `gamepad_overlay`).
pub fn build(side: VersionOverlaySide, log_level: LogLevel) -> Vec<Actor> {
    let text = version_text();
    let w = screen_width();
    let h = screen_height();
    let warning = log_warning_text(log_level);

    let mut actors = Vec::with_capacity(1 + usize::from(warning.is_some()));
    let version_x = match side {
        VersionOverlaySide::Left => MARGIN_X,
        VersionOverlaySide::Right => w - MARGIN_X,
    };
    let version_actor = match side {
        VersionOverlaySide::Left => act!(text:
            align(0.0, 1.0):
            xy(version_x, h + MARGIN_Y):
            font("miso"):
            zoom(0.55):
            settext(text):
            diffuse(1.0, 1.0, 1.0, 0.55):
            horizalign(left):
            z(VERSION_OVERLAY_Z)
        ),
        VersionOverlaySide::Right => act!(text:
            align(1.0, 1.0):
            xy(version_x, h + MARGIN_Y):
            font("miso"):
            zoom(0.55):
            settext(text):
            diffuse(1.0, 1.0, 1.0, 0.55):
            horizalign(right):
            z(VERSION_OVERLAY_Z)
        ),
    };
    actors.push(version_actor);

    if let Some(label) = warning {
        // Amber so the warning reads as "heads up" without screaming;
        // alpha lifted slightly above the version watermark (0.55 → 0.7)
        // because the warning is the more actionable of the two.
        let warning_y = h + MARGIN_Y + WARNING_OFFSET_Y;
        let warning_actor = match side {
            VersionOverlaySide::Left => act!(text:
                align(0.0, 1.0):
                xy(version_x, warning_y):
                font("miso"):
                zoom(0.55):
                settext(label):
                diffuse(1.0, 0.78, 0.25, 0.7):
                horizalign(left):
                z(VERSION_OVERLAY_Z)
            ),
            VersionOverlaySide::Right => act!(text:
                align(1.0, 1.0):
                xy(version_x, warning_y):
                font("miso"):
                zoom(0.55):
                settext(label):
                diffuse(1.0, 0.78, 0.25, 0.7):
                horizalign(right):
                z(VERSION_OVERLAY_Z)
            ),
        };
        actors.push(warning_actor);
    }

    actors
}
