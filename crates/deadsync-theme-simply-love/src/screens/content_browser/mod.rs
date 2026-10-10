//! Content Browser — browse and install song packs without leaving the game.
//!
//! The data layer already exists. `deadsync_online::stepmaniaonline` owns the
//! catalog, the download queue and the install, and the shell already routes
//! `EnsureStepManiaOnlineCatalog`, `RefreshStepManiaOnlineCatalog` and
//! `DownloadStepManiaOnlinePack`. `screens/options/download_packs.rs` drives
//! all of that today as an overlay, three levels into Options.
//!
//! This screen is the destination that work deserves: a first-class entry on
//! the title menu, with the room a full screen has and an overlay does not.
//! Nothing here re-implements the runtime — it binds to it, so the two views
//! can never disagree about what is downloading.

mod chart_window;
mod detail;
mod input;
mod layout;
mod preview;
mod render;
mod spinner;
mod state;
mod sync_dialog;

pub use chart_window::{PreviewSkinModels, preview_skin_models, preview_skin_textures};
pub use input::{handle_input, handle_raw_key_event};
pub use preview::SongRequest;
pub use render::{get_actors, in_transition, out_transition, push_actors};
pub use state::{
    InstalledPack, Services, State, beginner_candidates, beginner_showing, finish_pack_deletion,
    init, on_enter, preview_active, set_music_time, set_preview_skin, song_request_refused,
    sync_reload_events, sync_stepmaniaonline, take_audio_requests, take_pending_reload_dirs,
    take_song_requests, update, wanted_banners, wanted_descriptions, wanted_pack_page,
    wanted_search, wanted_view, wants_beginner_walk, wants_more_pages,
};
pub use sync_dialog::{
    PackShift, apply_sync_analysis_events, begin_pack_measure, finish_pack_sync,
    measured_settled_simfiles, pack_sync_group, set_pack_check, take_pack_shift, wanted_pack_check,
};

#[cfg(test)]
mod owned_results_support;
