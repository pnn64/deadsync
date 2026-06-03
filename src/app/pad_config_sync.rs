//! App-owned source of truth for SMX managed-config resolution + the active
//! marker.
//!
//! This state used to be split: the resolve guard lived on `App` while the
//! active marker (`smx_applied`) lived on the Song Select screen. Because the
//! screen state is rebuilt on transitions, the marker kept getting wiped while
//! the guard stayed put, so it wouldn't repopulate (we patched that in several
//! spots). Now the marker is authoritative here and the screen just mirrors it
//! each frame, so rebuilds can't lose it.
//!
//! The UI can't reach this controller directly (screens don't depend on app), so
//! it queues `PadConfigIntent`s on the Song Select state that the app drains via
//! [`PadConfigSync::apply_intent`].

use crate::screens::select_music::{AppliedPadConfig, PadConfigIntent};

/// Inputs that determine which config resolves for an SMX pad. The resolver only
/// reloads config files / rewrites the pad when this changes — now including the
/// pad's sensor type, so resolution re-runs once FSR vs load-cell becomes known.
#[derive(Clone, PartialEq, Eq)]
pub struct Sig {
    pub preset: crate::config::SmxPadPreset,
    pub serial: String,
    pub profile_id: Option<String>,
    pub pad_type: Option<String>,
}

#[derive(Default)]
pub struct PadConfigSync {
    /// What deadsync last applied to each pad (index 0 = P1, 1 = P2) — the
    /// active-marker source of truth.
    pub applied: [Option<AppliedPadConfig>; 2],
    /// Last-resolved inputs per pad slot; `None` forces a re-resolve.
    pub signature: [Option<Sig>; 2],
}

impl PadConfigSync {
    /// Apply a queued request from the UI.
    pub fn apply_intent(&mut self, intent: PadConfigIntent) {
        match intent {
            // A preset/config was manually applied → mark it active.
            PadConfigIntent::Override { pad, applied } => {
                if pad < 2 {
                    self.applied[pad] = Some(applied);
                }
            }
            // Something the signature can't see changed → force a re-resolve.
            PadConfigIntent::Invalidate { pad } => {
                if pad < 2 {
                    self.signature[pad] = None;
                }
            }
        }
    }

    /// A manual threshold edit diverged the pad from any saved config/preset, so
    /// it no longer matches a known config — drop the active marker.
    pub fn mark_diverged(&mut self, pad: usize) {
        if pad < 2 {
            self.applied[pad] = None;
        }
    }

    /// The active markers, for the screen to mirror for display.
    pub fn snapshot(&self) -> [Option<AppliedPadConfig>; 2] {
        self.applied.clone()
    }
}
