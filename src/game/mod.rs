pub mod course;
pub mod import;
pub mod online;
pub mod pad_profiles;
pub mod parsing;
pub mod profile;
pub mod random_movies;
pub mod scores;
pub mod song;

pub use deadsync_profile_gameplay::{
    GameplayProfile, chart_effects_from_profile, gameplay_attack_mode,
    gameplay_play_style_from_profile, gameplay_player_side_from_profile,
    gameplay_tick_mode_from_profile, profile_side_from_gameplay, profile_tick_mode_from_gameplay,
    score_display_mode_from_profile, scroll_effects_from_option,
    tap_explosion_options_from_profile,
};

pub type GameplayCoreState = deadsync_gameplay::GameplayRuntimeState<
    GameplayProfile,
    parsing::song_lua::SongLuaOverlayActor,
    deadsync_song_lua::SongLuaCapturedActor,
    deadsync_gameplay::SongLuaRuntimeOverlayStateDelta<deadsync_song_lua::SongLuaOverlayStateDelta>,
>;
