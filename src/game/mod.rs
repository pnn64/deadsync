pub mod course;
pub mod import;
pub mod online;
pub mod pad_profiles;
pub mod parsing;
pub mod profile;
pub mod random_movies;
pub mod scores;
pub mod song;
pub mod stage_stats;

pub type GameplayCoreState = deadsync_gameplay::GameplayRuntimeState<
    deadsync_profile::Profile,
    deadsync_input::InputEdge,
    parsing::song_lua::SongLuaOverlayActor,
    deadsync_song_lua::SongLuaCapturedActor,
    deadsync_song_lua::SongLuaOverlayStateDelta,
>;
