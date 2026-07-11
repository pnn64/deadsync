pub mod arrowcloud_login;
pub mod components;
pub mod credits;
pub mod evaluation;
pub mod evaluation_summary;
pub(crate) mod favorite_code;
pub mod gameover;
pub mod gameplay;
#[cfg(test)]
mod gameplay_regression_tests;
pub mod groovestats_login;
pub mod init;
pub mod initials;
pub mod input;
pub mod manage_local_profiles;
pub mod mappings;
pub mod menu;
pub mod options;
pub mod overscan_adjustment;
pub(crate) mod pack_sync;
pub mod pad_config;
pub mod player_options;
pub mod practice;
pub mod profile_load;
pub mod sandbox;
pub mod select_color;
pub mod select_course;
pub mod select_mode;
pub mod select_music;
pub mod select_profile;
pub mod select_style;
pub mod smx_assign;
pub mod test_lights;

pub use deadsync_screens::{DensityGraphSlot, DensityGraphSource, Screen, ScreenAction};

#[inline(always)]
pub(crate) fn progress_percent_tenths(done: usize, total: usize) -> u32 {
    if total == 0 {
        return 0;
    }
    (((done.min(total) as u128) * 1000) / total as u128) as u32
}

#[inline(always)]
pub(crate) fn progress_count_text(done: usize, total: usize) -> String {
    let pct = progress_percent_tenths(done, total);
    format!("{done}/{total} ({}.{:01}%)", pct / 10, pct % 10)
}
