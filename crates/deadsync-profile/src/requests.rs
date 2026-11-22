use std::path::PathBuf;

use deadsync_rules::scroll::ScrollSpeedSetting;

use crate::{
    ActiveProfile, BackgroundFilter, NoteSkin, PLAYER_SLOTS, PlayMode, PlayStyle,
    PlayerOptionsData, PlayerSide, ScrollOption, SessionSnapshot, active_profile_is_guest,
    app_runtime, play_style_for_joined, preferred_difficulty_indices, profile_combo_carry,
    runtime_active_profiles, runtime_session_snapshot, runtime_set_session_joined,
    runtime_set_session_play_style, runtime_set_session_player_side,
};

/// Profile and session operations that the application can accept from a theme.
#[derive(Clone, Debug)]
pub enum ProfileRequest {
    Select {
        p1: ActiveProfile,
        p2: ActiveProfile,
        p1_joined: bool,
        p2_joined: bool,
        /// Keep the current song selection when switching an active profile.
        fast_switch: bool,
    },
    SetPlayStyle(PlayStyle),
    SetSession {
        play_style: PlayStyle,
        player_side: PlayerSide,
        joined: [bool; PLAYER_SLOTS],
    },
    SetMusicRate(f32),
    SetPlayMode(PlayMode),
    SetMachineDefaultScrollSpeed(ScrollSpeedSetting),
    SetMachineDefaultScroll(ScrollOption),
    SetMachineDefaultBackgroundFilter(BackgroundFilter),
    SetMachineDefaultNoteskin(NoteSkin),
    UpdatePlayerOptions {
        side: PlayerSide,
        options: Box<PlayerOptionsData>,
        judgment_palette_id: Option<String>,
        heart_rate_device_id: Option<String>,
        max_heart_rate: u16,
    },
    UpdateInitials([Option<String>; PLAYER_SLOTS]),
    ToggleFavorite {
        side: PlayerSide,
        chart_hash: String,
    },
    ToggleEvaluationFavorite { side: PlayerSide },
    TogglePackFavorite {
        side: PlayerSide,
        pack_name: String,
    },
    ToggleSeriesFavorite {
        side: PlayerSide,
        series_name: String,
    },
    MarkPacksKnown {
        profile_ids: Vec<String>,
        pack_names: Vec<String>,
    },
    CreateLocalProfile {
        display_name: String,
    },
    RenameLocalProfile {
        profile_id: String,
        display_name: String,
    },
    SetDefaultLocalProfile {
        side: PlayerSide,
        profile_id: String,
    },
    DeleteLocalProfile {
        profile_id: String,
    },
    DiscoverItgProfiles,
    BrowseItgProfiles {
        title: String,
    },
    StartItgProfileImport {
        dir: PathBuf,
    },
    CancelItgProfileImport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileSelectionSessionPlan {
    pub active_side: PlayerSide,
    pub p1_joined: bool,
    pub p2_joined: bool,
    pub play_style: PlayStyle,
}

/// Resolve the game session after the players confirm their joined sides.
#[must_use]
pub const fn profile_selection_session_plan(
    current_style: PlayStyle,
    p1_joined: bool,
    p2_joined: bool,
) -> ProfileSelectionSessionPlan {
    let requires_both_sides = current_style.is_couple() || current_style.is_routine();
    ProfileSelectionSessionPlan {
        active_side: if p1_joined {
            PlayerSide::P1
        } else {
            PlayerSide::P2
        },
        p1_joined: p1_joined || requires_both_sides,
        p2_joined: p2_joined || requires_both_sides,
        play_style: play_style_for_joined(current_style, p1_joined, p2_joined),
    }
}

pub struct ProfileSelection {
    pub previous_session: SessionSnapshot,
    pub previous_guests: [bool; PLAYER_SLOTS],
    pub selected_guests: [bool; PLAYER_SLOTS],
    pub changed_profiles: [bool; PLAYER_SLOTS],
    pub active_side: PlayerSide,
    pub combo_carry: [u32; PLAYER_SLOTS],
    pub preferred_difficulties: [usize; PLAYER_SLOTS],
    pub avatar_paths: [Option<PathBuf>; PLAYER_SLOTS],
}

/// Apply confirmed profile selections and return the state needed to refresh views.
#[must_use]
pub fn select_profiles(
    p1: ActiveProfile,
    p2: ActiveProfile,
    p1_joined: bool,
    p2_joined: bool,
) -> ProfileSelection {
    let previous_session = runtime_session_snapshot();
    let previous_profiles = runtime_active_profiles();
    let changed_profiles = [previous_profiles[0] != p1, previous_profiles[1] != p2];
    let selected_guests = [active_profile_is_guest(&p1), active_profile_is_guest(&p2)];
    let session = profile_selection_session_plan(previous_session.play_style, p1_joined, p2_joined);
    runtime_set_session_player_side(session.active_side);
    runtime_set_session_joined(session.p1_joined, session.p2_joined);
    runtime_set_session_play_style(session.play_style);
    let profiles = app_runtime::set_active_profiles_from_config(p1, p2);
    ProfileSelection {
        previous_session,
        previous_guests: previous_profiles.each_ref().map(active_profile_is_guest),
        selected_guests,
        changed_profiles,
        active_side: session.active_side,
        combo_carry: profile_combo_carry(&profiles),
        preferred_difficulties: preferred_difficulty_indices(&profiles, session.play_style),
        avatar_paths: profiles.map(|profile| profile.avatar_path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_selection_session_follows_confirmed_joined_sides() {
        for (style, p1, p2, expected_style) in [
            (PlayStyle::Single, true, true, PlayStyle::Versus),
            (PlayStyle::Versus, false, true, PlayStyle::Single),
            (PlayStyle::Double, true, false, PlayStyle::Double),
        ] {
            assert_eq!(
                profile_selection_session_plan(style, p1, p2),
                ProfileSelectionSessionPlan {
                    active_side: if p1 { PlayerSide::P1 } else { PlayerSide::P2 },
                    p1_joined: p1,
                    p2_joined: p2,
                    play_style: expected_style,
                }
            );
        }
    }
}
