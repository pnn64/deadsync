use crate::ActiveTheme;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use deadsync_profile_gameplay::stage_options::{
    JudgmentPaletteChoiceView, PlayerOptionsPlayerView, PlayerOptionsPolicyView, PlayerOptionsView,
};
use deadsync_theme::color::ThemeJudgmentPalette;
pub(crate) use crate::mounted_player_options::PlayerOptionsState;

pub(crate) fn init_view() -> PlayerOptionsView {
    let config = deadsync_config::runtime::get();
    let session = profile::get_session_snapshot();
    let palette_catalog =
        deadsync_config::judgment_palettes::runtime_catalog(ActiveTheme::JUDGMENT_PRESET);
    PlayerOptionsView {
        policy: PlayerOptionsPolicyView::from_config(
            &config,
            deadsync_online::score_compat::is_gs_get_scores_service_allowed(),
        ),
        play_style: session.play_style,
        player_side: session.player_side,
        joined: std::array::from_fn(|idx| {
            session.side_joined(profile_data::player_side_for_index(idx))
        }),
        music_rate: session.music_rate,
        players: std::array::from_fn(|idx| {
            let profile = profile::get_for_side(profile_data::player_side_for_index(idx));
            PlayerOptionsPlayerView {
                options: profile.current_player_options(),
                judgment_palette_id: profile.judgment_palette_id,
                heart_rate_device_id: profile.heart_rate_device_id,
                max_heart_rate: profile.max_heart_rate,
            }
        }),
        judgment_palettes: palette_catalog
            .palettes
            .iter()
            .map(|entry| JudgmentPaletteChoiceView {
                id: entry.id.clone(),
                name: entry.name.clone(),
                palette: entry.palette,
            })
            .collect(),
    }
}

pub(crate) fn active_profiles() -> [profile_data::Profile; MAX_PLAYERS] {
    std::array::from_fn(|idx| profile::get_for_side(profile_data::player_side_for_index(idx)))
}
