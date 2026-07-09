//! SMX pad-config active marker and cached profile-list policy.
//!
//! The app owns hardware I/O and profile file loading. This module owns the
//! pure bookkeeping that decides when cached state is stale, when active
//! markers change, and when managed pad-config resolution must rerun.

use crate::pad_config::PadConfigProfile;
use deadsync_config::prelude::SmxPadPreset;

/// What DeadSync last applied to an SMX pad, so the UI can flag the active one.
/// `preset` = a built-in preset (name is its label); otherwise a saved config.
#[derive(Clone, PartialEq, Eq)]
pub struct AppliedPadConfig {
    pub preset: bool,
    pub name: String,
}

/// A request from the UI to the app-owned pad-config controller. `pad` is the
/// pad slot (0/1) in every variant: the same key the resolver uses.
pub enum PadConfigIntent {
    /// A preset/config was manually applied to a pad, so mark it active.
    Override {
        pad: usize,
        applied: AppliedPadConfig,
    },
    /// Something the resolver signature cannot see changed for this pad, so
    /// re-resolve and re-apply it.
    Invalidate { pad: usize },
    /// The saved-config list changed, but the applied config did not.
    RefreshList { pad: usize },
}

/// Inputs that determine which config resolves for an SMX pad.
#[derive(Clone, PartialEq, Eq)]
pub struct PadConfigSignature {
    pub preset: SmxPadPreset,
    pub serial: String,
    pub profile_id: Option<String>,
    pub pad_type: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
struct ProfilesSig {
    profile_id: Option<String>,
    pad_type: Option<String>,
}

#[derive(Default)]
pub struct PadConfigSync {
    /// What DeadSync last applied to each pad (index = pad slot 0/1).
    pub applied: [Option<AppliedPadConfig>; 2],
    /// Last-resolved inputs per pad slot; `None` forces a re-resolve.
    pub signature: [Option<PadConfigSignature>; 2],
    /// Per-pad saved-config list, already filtered to the pad's backend and
    /// sensor type.
    profiles: [Vec<PadConfigProfile>; 2],
    /// Inputs the cached `profiles[pad]` was built for.
    profiles_sig: [Option<ProfilesSig>; 2],
}

impl PadConfigSync {
    /// Apply a queued request from the UI.
    pub fn apply_intent(&mut self, intent: PadConfigIntent) {
        match intent {
            PadConfigIntent::Override { pad, applied } => {
                if pad < 2 {
                    self.applied[pad] = Some(applied);
                }
            }
            PadConfigIntent::Invalidate { pad } => {
                if pad < 2 {
                    self.signature[pad] = None;
                    self.profiles_sig[pad] = None;
                }
            }
            PadConfigIntent::RefreshList { pad } => {
                if pad < 2 {
                    self.profiles_sig[pad] = None;
                }
            }
        }
    }

    /// A manual threshold edit diverged the pad from any saved config/preset.
    pub fn mark_diverged(&mut self, pad: usize) {
        if pad < 2 {
            self.applied[pad] = None;
        }
    }

    /// Drop every pad's resolve signature so the managed resolver re-runs.
    pub fn reset_signatures(&mut self) {
        self.signature = [None, None];
    }

    /// Whether the cached resolve signature for `pad` already matches these
    /// inputs. Compared by borrow so the steady-state hot path allocates nothing.
    pub fn signature_matches(
        &self,
        pad: usize,
        preset: SmxPadPreset,
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

    /// Whether `profiles[pad]` needs rebuilding for these inputs.
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

    /// Store a freshly loaded and filtered config list.
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

    /// The cached saved-config list for a pad.
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
        assert!(s.profiles_stale(0, Some("p1"), Some("fsr")));
        s.store_profiles(
            0,
            Some("p1".to_owned()),
            Some("fsr".to_owned()),
            vec![cfg("A")],
        );
        assert!(!s.profiles_stale(0, Some("p1"), Some("fsr")));
        assert_eq!(s.profiles_for(0).len(), 1);
        assert!(s.profiles_stale(0, Some("p2"), Some("fsr")));
        assert!(s.profiles_stale(0, Some("p1"), Some("loadcell")));
        assert!(s.profiles_stale(1, Some("p1"), Some("fsr")));
    }

    #[test]
    fn invalidate_drops_cached_profiles() {
        let mut s = PadConfigSync::default();
        s.store_profiles(1, Some("p1".to_owned()), None, vec![cfg("A")]);
        assert!(!s.profiles_stale(1, Some("p1"), None));
        s.apply_intent(PadConfigIntent::Invalidate { pad: 1 });
        assert!(s.profiles_stale(1, Some("p1"), None));
    }

    #[test]
    fn refresh_list_rebuilds_list_without_touching_resolve_signature() {
        let mut s = PadConfigSync::default();
        s.signature[0] = Some(PadConfigSignature {
            preset: SmxPadPreset::Medium,
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
        s.apply_intent(PadConfigIntent::RefreshList { pad: 0 });
        assert!(s.profiles_stale(0, Some("p1"), Some("fsr")));
        assert!(s.signature[0].is_some());
    }

    #[test]
    fn reset_signatures_clears_all_so_managed_resolve_reruns() {
        let mut s = PadConfigSync::default();
        for pad in 0..2 {
            s.signature[pad] = Some(PadConfigSignature {
                preset: SmxPadPreset::Medium,
                serial: "S".to_owned(),
                profile_id: Some("p1".to_owned()),
                pad_type: Some("fsr".to_owned()),
            });
        }
        s.reset_signatures();
        assert!(s.signature[0].is_none());
        assert!(s.signature[1].is_none());
    }

    #[test]
    fn override_keeps_signature_so_only_a_session_reset_reresolves() {
        let mut s = PadConfigSync::default();
        s.signature[0] = Some(PadConfigSignature {
            preset: SmxPadPreset::Low,
            serial: "S".to_owned(),
            profile_id: Some("p".to_owned()),
            pad_type: Some("fsr".to_owned()),
        });
        s.apply_intent(PadConfigIntent::Override {
            pad: 0,
            applied: AppliedPadConfig {
                preset: false,
                name: "FS".to_owned(),
            },
        });
        assert!(s.signature[0].is_some());
        assert_eq!(s.snapshot()[0].as_ref().unwrap().name, "FS");
        s.reset_signatures();
        assert!(s.signature[0].is_none());
    }

    #[test]
    fn signature_matches_compares_every_field_by_borrow() {
        let mut s = PadConfigSync::default();
        assert!(!s.signature_matches(0, SmxPadPreset::Low, "S1", Some("p1"), Some("fsr")));
        s.signature[0] = Some(PadConfigSignature {
            preset: SmxPadPreset::Medium,
            serial: "S1".to_owned(),
            profile_id: Some("p1".to_owned()),
            pad_type: Some("fsr".to_owned()),
        });
        assert!(s.signature_matches(0, SmxPadPreset::Medium, "S1", Some("p1"), Some("fsr")));
        assert!(!s.signature_matches(0, SmxPadPreset::High, "S1", Some("p1"), Some("fsr")));
        assert!(!s.signature_matches(0, SmxPadPreset::Medium, "S2", Some("p1"), Some("fsr")));
        assert!(!s.signature_matches(0, SmxPadPreset::Medium, "S1", None, Some("fsr")));
        assert!(!s.signature_matches(0, SmxPadPreset::Medium, "S1", Some("p1"), None));
        assert!(!s.signature_matches(1, SmxPadPreset::Medium, "S1", Some("p1"), Some("fsr")));
        assert!(!s.signature_matches(9, SmxPadPreset::Medium, "S1", Some("p1"), Some("fsr")));
    }
}
