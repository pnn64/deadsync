use deadlib_present::density;
use deadlib_render::MeshVertex;
use deadsync_online::score_compat as scores;
use deadsync_profile::compat as profile;
use deadsync_profile::{PlayStyle, PlayerSide};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_screens::{DensityGraphSlot, DensityGraphSource, Screen};
use log::{debug, warn};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerSlot {
    SelectMusic,
    SelectCourse,
}

#[inline(always)]
pub const fn banner_slot(screen: Screen) -> BannerSlot {
    if matches!(screen, Screen::SelectCourse) {
        BannerSlot::SelectCourse
    } else {
        BannerSlot::SelectMusic
    }
}

#[inline(always)]
pub fn fallback_banner_key(color_index: i32) -> String {
    let banner_num = color_index.rem_euclid(12) + 1;
    format!("banner{banner_num}.png")
}

pub fn build_density_graph_mesh(
    chart_opt: Option<DensityGraphSource>,
    wide_screen: bool,
) -> Option<Arc<[MeshVertex]>> {
    let graph_w = if wide_screen { 286.0 } else { 276.0 };
    let graph_h = 64.0;
    chart_opt.and_then(|chart| {
        let verts = density::build_density_histogram_mesh(
            &chart.measure_nps_vec,
            chart.max_nps,
            &chart.measure_seconds_vec,
            chart.first_second,
            chart.last_second,
            graph_w,
            graph_h,
            0.0,
            graph_w,
            None,
            1.0,
        );
        (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
    })
}

pub fn spawn_online_grade_fetch(hash: String) {
    debug!("Fetching online grade for chart hash: {hash}");
    let mut spawned = 0;
    for side in [PlayerSide::P1, PlayerSide::P2] {
        if !profile::is_session_side_joined(side) {
            continue;
        }
        let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
            continue;
        };
        let profile = profile::get_for_side(side);
        if profile.groovestats_api_key.is_empty() || profile.groovestats_username.is_empty() {
            continue;
        }

        spawned += 1;
        let hash = hash.clone();
        std::thread::spawn(move || {
            if let Err(e) = scores::fetch_and_store_grade(profile_id, profile, hash) {
                warn!("Failed to fetch online grade: {e}");
            }
        });
    }
    if spawned == 0 {
        warn!(
            "Skipping GrooveStats grade fetch: no joined local profile with GrooveStats configured"
        );
    }
}

/// Imperative effects executed by the shell after a screen update.
pub enum Command {
    ExitNow,
    Shutdown,
    SetBanner(Option<PathBuf>),
    SetCdTitle(Option<PathBuf>),
    SetPackBanner(Option<PathBuf>),
    SetWheelItemBackgrounds(Vec<PathBuf>),
    SetDensityGraph {
        slot: DensityGraphSlot,
        chart_opt: Option<DensityGraphSource>,
    },
    FetchOnlineGrade(String),
    PlayMusic {
        path: PathBuf,
        looped: bool,
        volume: f32,
    },
    StopMusic,
    SetDynamicBackground(Option<PathBuf>),
    UpdateScrollSpeed {
        side: PlayerSide,
        setting: ScrollSpeedSetting,
    },
    UpdateSessionMusicRate(f32),
    UpdatePreferredDifficulty(usize),
    UpdateLastPlayed {
        side: PlayerSide,
        play_style: PlayStyle,
        music_path: Option<PathBuf>,
        chart_hash: Option<String>,
        difficulty_index: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    ExitNow,
    Shutdown,
    SetBanner,
    SetCdTitle,
    SetPackBanner,
    SetWheelItemBackgrounds,
    SetDensityGraph,
    FetchOnlineGrade,
    PlayMusic,
    StopMusic,
    SetDynamicBackground,
    UpdateScrollSpeed,
    UpdateSessionMusicRate,
    UpdatePreferredDifficulty,
    UpdateLastPlayed,
}

impl Command {
    #[inline(always)]
    pub const fn kind(&self) -> CommandKind {
        match self {
            Self::ExitNow => CommandKind::ExitNow,
            Self::Shutdown => CommandKind::Shutdown,
            Self::SetBanner(_) => CommandKind::SetBanner,
            Self::SetCdTitle(_) => CommandKind::SetCdTitle,
            Self::SetPackBanner(_) => CommandKind::SetPackBanner,
            Self::SetWheelItemBackgrounds(_) => CommandKind::SetWheelItemBackgrounds,
            Self::SetDensityGraph { .. } => CommandKind::SetDensityGraph,
            Self::FetchOnlineGrade(_) => CommandKind::FetchOnlineGrade,
            Self::PlayMusic { .. } => CommandKind::PlayMusic,
            Self::StopMusic => CommandKind::StopMusic,
            Self::SetDynamicBackground(_) => CommandKind::SetDynamicBackground,
            Self::UpdateScrollSpeed { .. } => CommandKind::UpdateScrollSpeed,
            Self::UpdateSessionMusicRate(_) => CommandKind::UpdateSessionMusicRate,
            Self::UpdatePreferredDifficulty(_) => CommandKind::UpdatePreferredDifficulty,
            Self::UpdateLastPlayed { .. } => CommandKind::UpdateLastPlayed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandTimingLog {
    None,
    CommandTiming,
    FrameCost,
    Slow,
}

pub const fn command_label(kind: CommandKind) -> &'static str {
    match kind {
        CommandKind::ExitNow => "ExitNow",
        CommandKind::Shutdown => "Shutdown",
        CommandKind::SetBanner => "SetBanner",
        CommandKind::SetCdTitle => "SetCdTitle",
        CommandKind::SetPackBanner => "SetPackBanner",
        CommandKind::SetWheelItemBackgrounds => "SetWheelItemBackgrounds",
        CommandKind::SetDensityGraph => "SetDensityGraph",
        CommandKind::FetchOnlineGrade => "FetchOnlineGrade",
        CommandKind::PlayMusic => "PlayMusic",
        CommandKind::StopMusic => "StopMusic",
        CommandKind::SetDynamicBackground => "SetDynamicBackground",
        CommandKind::UpdateScrollSpeed => "UpdateScrollSpeed",
        CommandKind::UpdateSessionMusicRate => "UpdateSessionMusicRate",
        CommandKind::UpdatePreferredDifficulty => "UpdatePreferredDifficulty",
        CommandKind::UpdateLastPlayed => "UpdateLastPlayed",
    }
}

pub const fn command_logs_frame_cost(kind: CommandKind) -> bool {
    matches!(
        kind,
        CommandKind::SetBanner
            | CommandKind::SetCdTitle
            | CommandKind::SetPackBanner
            | CommandKind::SetWheelItemBackgrounds
            | CommandKind::SetDensityGraph
            | CommandKind::SetDynamicBackground
            | CommandKind::PlayMusic
    )
}

pub fn command_timing_log(kind: CommandKind, elapsed_ms: f64) -> CommandTimingLog {
    if elapsed_ms >= 100.0 {
        CommandTimingLog::Slow
    } else if elapsed_ms >= 16.7 {
        CommandTimingLog::FrameCost
    } else if command_logs_frame_cost(kind) {
        CommandTimingLog::CommandTiming
    } else {
        CommandTimingLog::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_timing_logs_match_command_cost() {
        assert_eq!(command_label(CommandKind::SetBanner), "SetBanner");
        assert!(command_logs_frame_cost(CommandKind::SetBanner));
        assert!(command_logs_frame_cost(CommandKind::PlayMusic));
        assert!(!command_logs_frame_cost(CommandKind::UpdateLastPlayed));
        assert_eq!(
            command_timing_log(CommandKind::UpdateLastPlayed, 1.0),
            CommandTimingLog::None,
        );
        assert_eq!(
            command_timing_log(CommandKind::SetBanner, 1.0),
            CommandTimingLog::CommandTiming,
        );
        assert_eq!(
            command_timing_log(CommandKind::UpdateLastPlayed, 16.7),
            CommandTimingLog::FrameCost,
        );
        assert_eq!(
            command_timing_log(CommandKind::UpdateLastPlayed, 100.0),
            CommandTimingLog::Slow,
        );
    }

    #[test]
    fn banner_policy_preserves_course_slot_and_color_cycle() {
        assert_eq!(banner_slot(Screen::SelectCourse), BannerSlot::SelectCourse);
        assert_eq!(banner_slot(Screen::Gameplay), BannerSlot::SelectMusic);
        assert_eq!(fallback_banner_key(0), "banner1.png");
        assert_eq!(fallback_banner_key(12), "banner1.png");
        assert_eq!(fallback_banner_key(-1), "banner12.png");
    }

    #[test]
    fn density_graph_wrapper_handles_missing_and_populated_charts() {
        assert!(build_density_graph_mesh(None, false).is_none());
        let source = DensityGraphSource {
            max_nps: 8.0,
            measure_nps_vec: vec![2.0, 8.0, 4.0],
            measure_seconds_vec: vec![0.0, 1.0, 2.0],
            first_second: 0.0,
            last_second: 2.0,
        };

        assert!(build_density_graph_mesh(Some(source), true).is_some());
    }
}
