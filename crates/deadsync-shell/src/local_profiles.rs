use deadsync_profile::compat as profile;
use deadsync_profile::{ActiveProfile, PlayerSide};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_theme_simply_love::SimplyLoveLocalProfileEvent;
use deadsync_theme_simply_love::views::{
    LocalProfileView, ManageLocalProfilesView, ProfilePickerEntryView, ProfilePickerView,
};
use std::str::FromStr;

pub fn view() -> ManageLocalProfilesView {
    let config = deadsync_config::prelude::get();
    ManageLocalProfilesView {
        profiles: profile::scan_local_profiles()
            .into_iter()
            .map(|profile| LocalProfileView {
                id: profile.id,
                display_name: profile.display_name,
            })
            .collect(),
        default_profile_ids: [
            profile::default_local_profile_id_for_side(PlayerSide::P1),
            profile::default_local_profile_id_for_side(PlayerSide::P2),
        ],
        dedicated_three_key_nav: config.three_key_navigation && config.only_dedicated_menu_buttons,
    }
}

pub fn picker_view() -> ProfilePickerView {
    let config = deadsync_config::prelude::get();
    let default_options = profile::new_profile_player_options();
    let guest_options = profile::guest_player_options();
    let default_speed_mod = default_options.scroll_speed.to_string();
    let default_scroll_option = default_options.scroll_option;
    let player_options_section =
        deadsync_profile::player_options_section(profile::get_session_play_style());
    let profiles = profile::scan_local_profiles()
        .into_iter()
        .map(|summary| {
            let mut speed_mod = default_speed_mod.clone();
            let mut scroll_option = default_scroll_option;
            let mut mini_indicator = default_options.mini_indicator;
            let mut noteskin = default_options.noteskin.clone();
            let mut judgment = default_options.judgment_graphic.clone();
            let ini_path = profile::local_profile_dir_for_id(&summary.id).join("profile.ini");
            let mut ini = deadsync_config::prelude::SimpleIni::new();
            if ini.load(&ini_path).is_ok() {
                let get_player_option = |key: &str| ini.get(player_options_section, key);
                if let Some(raw) = get_player_option("ScrollSpeed") {
                    let trimmed = raw.trim();
                    speed_mod = ScrollSpeedSetting::from_str(trimmed)
                        .map(|setting| format!("{setting}"))
                        .unwrap_or_else(|_| trimmed.to_owned());
                }
                scroll_option = get_player_option("Scroll")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| {
                        let reverse = get_player_option("ReverseScroll")
                            .and_then(|value| value.parse::<u8>().ok())
                            .is_some_and(|value| value != 0);
                        if reverse {
                            deadsync_profile::ScrollOption::Reverse
                        } else {
                            default_scroll_option
                        }
                    });
                mini_indicator = get_player_option("MiniIndicator")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| {
                        let subtractive = get_player_option("SubtractiveScoring")
                            .and_then(parse_ini_bool)
                            .unwrap_or(false);
                        let pacemaker = get_player_option("Pacemaker")
                            .and_then(parse_ini_bool)
                            .unwrap_or(false);
                        if subtractive {
                            deadsync_profile::MiniIndicator::SubtractiveScoring
                        } else if pacemaker {
                            deadsync_profile::MiniIndicator::Pacemaker
                        } else {
                            default_options.mini_indicator
                        }
                    });
                noteskin = get_player_option("NoteSkin")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| default_options.noteskin.clone());
                judgment = get_player_option("JudgmentGraphic")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or_else(|| default_options.judgment_graphic.clone());
            }
            ProfilePickerEntryView {
                id: summary.id.clone(),
                display_name: summary.display_name,
                speed_mod,
                avatar_key: summary
                    .avatar_path
                    .map(|path| path.to_string_lossy().into_owned()),
                total_songs_played: deadsync_online::score_compat::total_songs_played_for_profile(
                    &summary.id,
                ),
                scroll_option,
                mini_indicator,
                noteskin,
                judgment,
            }
        })
        .collect();
    ProfilePickerView {
        game: config.game_flag,
        guest: ProfilePickerEntryView {
            id: String::new(),
            display_name: String::new(),
            speed_mod: guest_options.scroll_speed.to_string(),
            avatar_key: None,
            total_songs_played: 0,
            scroll_option: guest_options.scroll_option,
            mini_indicator: guest_options.mini_indicator,
            noteskin: guest_options.noteskin,
            judgment: guest_options.judgment_graphic,
        },
        profiles,
        default_profiles: [
            profile::get_default_profile_for_side(PlayerSide::P1),
            profile::get_default_profile_for_side(PlayerSide::P2),
        ],
        three_key_navigation: config.three_key_navigation,
    }
}

fn parse_ini_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn create(display_name: &str) -> SimplyLoveLocalProfileEvent {
    let result = profile::create_local_profile(display_name).map_err(|error| {
        log::warn!("Failed to create local profile: {error}");
    });
    SimplyLoveLocalProfileEvent::Created {
        result,
        view: view(),
    }
}

pub fn rename(profile_id: &str, display_name: &str) -> SimplyLoveLocalProfileEvent {
    let result = profile::rename_local_profile(profile_id, display_name).map_err(|error| {
        log::warn!("Failed to rename local profile {profile_id}: {error}");
    });
    SimplyLoveLocalProfileEvent::Renamed {
        profile_id: profile_id.to_owned(),
        result,
        view: view(),
    }
}

pub fn set_default(side: PlayerSide, profile_id: String) -> SimplyLoveLocalProfileEvent {
    profile::set_default_profile_for_side(side, ActiveProfile::Local { id: profile_id });
    SimplyLoveLocalProfileEvent::DefaultSet { view: view() }
}

pub fn delete(profile_id: &str) -> SimplyLoveLocalProfileEvent {
    let result = profile::delete_local_profile(profile_id).map_err(|error| {
        log::warn!("Failed to delete local profile {profile_id}: {error}");
    });
    SimplyLoveLocalProfileEvent::Deleted {
        result,
        view: view(),
    }
}
