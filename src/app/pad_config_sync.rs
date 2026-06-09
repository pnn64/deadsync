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
use deadsync_profile::pad_config::PadConfigProfile;

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

/// Inputs that decide the *contents* of a pad's saved-config list: which profile
/// file to read, filtered to which sensor type. Cheaper than [`Sig`] — preset and
/// serial don't change the list — so the cached list only reloads when this moves.
#[derive(Clone, PartialEq, Eq)]
struct ProfilesSig {
    profile_id: Option<String>,
    pad_type: Option<String>,
}

#[derive(Default)]
pub struct PadConfigSync {
    /// What deadsync last applied to each pad (index = pad slot 0/1) — the
    /// active-marker source of truth.
    pub applied: [Option<AppliedPadConfig>; 2],
    /// Last-resolved inputs per pad slot; `None` forces a re-resolve.
    pub signature: [Option<Sig>; 2],
    /// Per-pad saved-config list, already filtered to the pad's backend + sensor
    /// type. Rebuilt only when [`ProfilesSig`] changes (or on `Invalidate`), so the
    /// Configure Pads overlay no longer re-reads `padconfig.ini` every frame.
    profiles: [Vec<PadConfigProfile>; 2],
    /// Inputs the cached `profiles[pad]` was built for; `None` forces a reload.
    profiles_sig: [Option<ProfilesSig>; 2],
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
                    // Management edits (delete / overwrite / set-default) change the
                    // saved configs without moving `ProfilesSig`, so drop the cached
                    // list too — it rebuilds on the next refresh.
                    self.profiles_sig[pad] = None;
                }
            }
            // List changed but the applied config didn't → rebuild the list only.
            // (Re-resolving here would clobber freshly-captured live values.)
            PadConfigIntent::RefreshList { pad } => {
                if pad < 2 {
                    self.profiles_sig[pad] = None;
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

    /// Drop every pad's resolve signature so the managed resolver re-runs and
    /// re-applies each pad's default on the next frame. Called when a new play
    /// session begins: a manual Apply from the previous session set `applied` and
    /// wrote the pad but left the signature intact, so without this the override
    /// would carry into the next session instead of the managed default being
    /// reasserted. (A full app restart already resolves fresh; this makes a
    /// session restart behave the same.)
    pub fn reset_signatures(&mut self) {
        self.signature = [None, None];
    }

    /// Whether the cached resolve signature for `pad` already matches these
    /// inputs. Compared by borrow so the steady-state hot path allocates nothing
    /// (no throwaway `Sig`) just to discover that nothing changed; the owned `Sig`
    /// is only built when we actually re-resolve.
    pub fn signature_matches(
        &self,
        pad: usize,
        preset: crate::config::SmxPadPreset,
        serial: &str,
        profile_id: Option<&str>,
        pad_type: Option<&str>,
    ) -> bool {
        match self.signature.get(pad).and_then(Option::as_ref) {
            Some(sig) => {
                sig.preset == preset
                    && sig.serial == serial
                    && sig.profile_id.as_deref() == profile_id
                    && sig.pad_type.as_deref() == pad_type
            }
            None => false,
        }
    }

    /// The active markers, for the screen to mirror for display.
    pub fn snapshot(&self) -> [Option<AppliedPadConfig>; 2] {
        self.applied.clone()
    }

    /// Whether `profiles[pad]` needs rebuilding for these inputs. Cheap (no I/O):
    /// the caller does the `pad_profiles::load` + filter only when this is `true`.
    pub fn profiles_stale(
        &self,
        pad: usize,
        profile_id: Option<&str>,
        pad_type: Option<&str>,
    ) -> bool {
        if pad >= 2 {
            return false;
        }
        match &self.profiles_sig[pad] {
            Some(sig) => {
                sig.profile_id.as_deref() != profile_id || sig.pad_type.as_deref() != pad_type
            }
            None => true,
        }
    }

    /// Store a freshly loaded + filtered config list and remember the inputs it was
    /// built for, so [`profiles_stale`](Self::profiles_stale) stays `false` until
    /// they change.
    pub fn store_profiles(
        &mut self,
        pad: usize,
        profile_id: Option<String>,
        pad_type: Option<String>,
        list: Vec<PadConfigProfile>,
    ) {
        if pad >= 2 {
            return;
        }
        self.profiles[pad] = list;
        self.profiles_sig[pad] = Some(ProfilesSig {
            profile_id,
            pad_type,
        });
    }

    /// The cached saved-config list for a pad (already filtered to its backend +
    /// sensor type). Empty until the first refresh.
    pub fn profiles_for(&self, pad: usize) -> &[PadConfigProfile] {
        if pad >= 2 {
            return &[];
        }
        &self.profiles[pad]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(name: &str) -> PadConfigProfile {
        PadConfigProfile {
            name: name.to_owned(),
            backend: "smx".to_owned(),
            pad_type: Some("fsr".to_owned()),
            serial: None,
            default_for_serials: Vec::new(),
            global_default: false,
            settings: Vec::new(),
        }
    }

    #[test]
    fn profiles_reload_only_when_inputs_change() {
        let mut s = PadConfigSync::default();
        // Never loaded → stale.
        assert!(s.profiles_stale(0, Some("p1"), Some("fsr")));
        s.store_profiles(
            0,
            Some("p1".to_owned()),
            Some("fsr".to_owned()),
            vec![cfg("A")],
        );
        // Same inputs → cache is served, no reload.
        assert!(!s.profiles_stale(0, Some("p1"), Some("fsr")));
        assert_eq!(s.profiles_for(0).len(), 1);
        // Profile switch → stale.
        assert!(s.profiles_stale(0, Some("p2"), Some("fsr")));
        // Sensor type becoming known → stale.
        assert!(s.profiles_stale(0, Some("p1"), Some("loadcell")));
        // The other slot is independent.
        assert!(s.profiles_stale(1, Some("p1"), Some("fsr")));
    }

    #[test]
    fn invalidate_drops_cached_profiles() {
        let mut s = PadConfigSync::default();
        s.store_profiles(1, Some("p1".to_owned()), None, vec![cfg("A")]);
        assert!(!s.profiles_stale(1, Some("p1"), None));
        // A management edit can't move the inputs, so it must clear the cache.
        s.apply_intent(PadConfigIntent::Invalidate { pad: 1 });
        assert!(s.profiles_stale(1, Some("p1"), None));
    }

    #[test]
    fn refresh_list_rebuilds_list_without_touching_resolve_signature() {
        let mut s = PadConfigSync::default();
        s.signature[0] = Some(Sig {
            preset: crate::config::SmxPadPreset::Medium,
            serial: "S".to_owned(),
            profile_id: Some("p1".to_owned()),
            pad_type: Some("fsr".to_owned()),
        });
        s.store_profiles(
            0,
            Some("p1".to_owned()),
            Some("fsr".to_owned()),
            vec![cfg("A")],
        );
        // A new save / rename changed the list but not what's applied to the pad.
        s.apply_intent(PadConfigIntent::RefreshList { pad: 0 });
        // List rebuilds...
        assert!(s.profiles_stale(0, Some("p1"), Some("fsr")));
        // ...but the resolve signature is untouched, so the pad isn't rewritten.
        assert!(s.signature[0].is_some());
    }

    #[test]
    fn reset_signatures_clears_all_so_managed_resolve_reruns() {
        use crate::config::SmxPadPreset;
        let mut s = PadConfigSync::default();
        for pad in 0..2 {
            s.signature[pad] = Some(Sig {
                preset: SmxPadPreset::Medium,
                serial: "S".to_owned(),
                profile_id: Some("p1".to_owned()),
                pad_type: Some("fsr".to_owned()),
            });
        }
        // A new play session drops every signature so the resolver re-runs and
        // re-applies each pad's default (discarding a prior session's manual Apply).
        s.reset_signatures();
        assert!(s.signature[0].is_none());
        assert!(s.signature[1].is_none());
    }

    #[test]
    fn override_keeps_signature_so_only_a_session_reset_reresolves() {
        use crate::config::SmxPadPreset;
        let mut s = PadConfigSync::default();
        // The managed resolver has already run for pad 0 (signature cached).
        s.signature[0] = Some(Sig {
            preset: SmxPadPreset::Low,
            serial: "S".to_owned(),
            profile_id: Some("p".to_owned()),
            pad_type: Some("fsr".to_owned()),
        });
        // A manual Apply marks a config active but must NOT touch the signature —
        // otherwise the resolver keeps short-circuiting and never reverts to the default.
        s.apply_intent(PadConfigIntent::Override {
            pad: 0,
            applied: AppliedPadConfig {
                preset: false,
                name: "FS".to_owned(),
            },
        });
        assert!(s.signature[0].is_some());
        assert_eq!(s.snapshot()[0].as_ref().unwrap().name, "FS");
        // Starting a new session drops the signature, so the next resolve reapplies
        // the managed default instead of leaving the override in place.
        s.reset_signatures();
        assert!(s.signature[0].is_none());
    }

    #[test]
    fn signature_matches_compares_every_field_by_borrow() {
        use crate::config::SmxPadPreset;
        let mut s = PadConfigSync::default();
        // No cached signature → never matches.
        assert!(!s.signature_matches(0, SmxPadPreset::Low, "S1", Some("p1"), Some("fsr")));
        s.signature[0] = Some(Sig {
            preset: SmxPadPreset::Medium,
            serial: "S1".to_owned(),
            profile_id: Some("p1".to_owned()),
            pad_type: Some("fsr".to_owned()),
        });
        // Exact match.
        assert!(s.signature_matches(0, SmxPadPreset::Medium, "S1", Some("p1"), Some("fsr")));
        // Any single field differing → no match.
        assert!(!s.signature_matches(0, SmxPadPreset::High, "S1", Some("p1"), Some("fsr")));
        assert!(!s.signature_matches(0, SmxPadPreset::Medium, "S2", Some("p1"), Some("fsr")));
        assert!(!s.signature_matches(0, SmxPadPreset::Medium, "S1", None, Some("fsr")));
        assert!(!s.signature_matches(0, SmxPadPreset::Medium, "S1", Some("p1"), None));
        // Other slot is independent; out-of-range is safe (no panic).
        assert!(!s.signature_matches(1, SmxPadPreset::Medium, "S1", Some("p1"), Some("fsr")));
        assert!(!s.signature_matches(9, SmxPadPreset::Medium, "S1", Some("p1"), Some("fsr")));
    }
}
