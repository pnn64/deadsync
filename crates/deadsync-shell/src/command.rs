use deadlib_present::density;
use deadlib_render::Backend;
use deadlib_render_core::MeshVertex;
use deadsync_assets::{AssetManager, media_path_key};
use deadsync_online::score_compat as scores;
use deadsync_profile::compat as profile;
use deadsync_profile::{PlayStyle, PlayerSide};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_theme::views::DensityGraphView as DensityGraphSource;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
use deadsync_theme_simply_love::views::SimplyLoveDensityGraphSlot as DensityGraphSlot;
use log::{debug, warn};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::SessionState;
use crate::dynamic_media::DynamicMedia;

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

pub struct DynamicBackgroundMediaResult {
    pub path: Option<PathBuf>,
    pub path_key: Option<Arc<str>>,
    pub texture_key: Arc<str>,
    pub allow_video: bool,
}

#[derive(Clone, Copy)]
pub struct CommandContext {
    pub current_screen: Screen,
    pub select_music_color_index: i32,
    pub select_course_color_index: i32,
    pub video_started_at_sec: f32,
    pub show_video_backgrounds: bool,
    pub wide_screen: bool,
}

pub enum CommandEffect {
    None,
    ExitNow,
    Shutdown,
    Banner {
        slot: BannerSlot,
        key: Arc<str>,
    },
    CdTitle(Option<Arc<str>>),
    DensityGraph {
        slot: DensityGraphSlot,
        mesh: Option<Arc<[MeshVertex]>>,
    },
    DynamicBackground(DynamicBackgroundMediaResult),
}

pub struct CommandTimingResult {
    #[cfg(test)]
    pub kind: CommandKind,
    pub label: &'static str,
    pub elapsed_ms: f64,
    pub log: CommandTimingLog,
}

pub struct CommandExecutionResult {
    pub effect: CommandEffect,
    pub timing: CommandTimingResult,
}

pub fn apply_banner_media(
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: &mut Backend,
    path_opt: Option<PathBuf>,
    fallback_color_index: i32,
) -> Arc<str> {
    if let Some(path) = path_opt {
        dynamic_media.set_banner(assets, backend, Some(path))
    } else {
        dynamic_media.destroy_banner(assets, backend);
        Arc::<str>::from(fallback_banner_key(fallback_color_index))
    }
}

pub fn apply_cdtitle_media(
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: &mut Backend,
    path_opt: Option<PathBuf>,
) -> Option<Arc<str>> {
    dynamic_media.set_cdtitle(assets, backend, path_opt)
}

pub fn apply_pack_banner_media(
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: &mut Backend,
    path_opt: Option<PathBuf>,
) {
    dynamic_media.set_pack_banner(assets, backend, path_opt);
}

pub fn apply_wheel_item_backgrounds_media(
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: &mut Backend,
    paths: Vec<PathBuf>,
) {
    dynamic_media.set_wheel_item_backgrounds(assets, backend, paths);
}

pub fn apply_dynamic_background_media(
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: &mut Backend,
    path_opt: Option<PathBuf>,
    video_started_at_sec: f32,
    allow_video: bool,
) -> DynamicBackgroundMediaResult {
    let texture_key = dynamic_media.set_background(
        assets,
        backend,
        path_opt.clone(),
        video_started_at_sec,
        allow_video,
    );
    DynamicBackgroundMediaResult {
        path_key: path_opt.as_deref().map(media_path_key),
        path: path_opt,
        texture_key: Arc::<str>::from(texture_key),
        allow_video,
    }
}

fn apply_command<EvaluationPage>(
    command: Command,
    session: &mut SessionState<EvaluationPage>,
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: Option<&mut Backend>,
    context: CommandContext,
) -> CommandEffect {
    match command {
        Command::ExitNow => CommandEffect::ExitNow,
        Command::Shutdown => CommandEffect::Shutdown,
        Command::SetBanner(path_opt) => {
            let Some(backend) = backend else {
                return CommandEffect::None;
            };
            let slot = banner_slot(context.current_screen);
            let fallback_color_index = match slot {
                BannerSlot::SelectMusic => context.select_music_color_index,
                BannerSlot::SelectCourse => context.select_course_color_index,
            };
            let key = apply_banner_media(
                dynamic_media,
                assets,
                backend,
                path_opt,
                fallback_color_index,
            );
            CommandEffect::Banner { slot, key }
        }
        Command::SetCdTitle(path_opt) => {
            let Some(backend) = backend else {
                return CommandEffect::None;
            };
            CommandEffect::CdTitle(apply_cdtitle_media(
                dynamic_media,
                assets,
                backend,
                path_opt,
            ))
        }
        Command::SetPackBanner(path_opt) => {
            if let Some(backend) = backend {
                apply_pack_banner_media(dynamic_media, assets, backend, path_opt);
            }
            CommandEffect::None
        }
        Command::SetWheelItemBackgrounds(paths) => {
            if let Some(backend) = backend {
                apply_wheel_item_backgrounds_media(dynamic_media, assets, backend, paths);
            }
            CommandEffect::None
        }
        Command::SetDensityGraph { slot, chart_opt } => CommandEffect::DensityGraph {
            slot,
            mesh: build_density_graph_mesh(chart_opt, context.wide_screen),
        },
        Command::SetDynamicBackground(path_opt) => {
            let Some(backend) = backend else {
                return CommandEffect::None;
            };
            CommandEffect::DynamicBackground(apply_dynamic_background_media(
                dynamic_media,
                assets,
                backend,
                path_opt,
                context.video_started_at_sec,
                context.show_video_backgrounds,
            ))
        }
        Command::FetchOnlineGrade(hash) => {
            spawn_online_grade_fetch(hash);
            CommandEffect::None
        }
        Command::PlayMusic {
            path,
            looped,
            volume,
        } => {
            deadsync_audio_stream::play_music(
                path,
                deadsync_audio_stream::Cut::default(),
                looped,
                volume,
            );
            CommandEffect::None
        }
        Command::StopMusic => {
            deadsync_audio_stream::stop_music();
            CommandEffect::None
        }
        Command::UpdateScrollSpeed { side, setting } => {
            profile::update_scroll_speed_for_side(side, setting);
            CommandEffect::None
        }
        Command::UpdateSessionMusicRate(rate) => {
            profile::set_session_music_rate(rate);
            CommandEffect::None
        }
        Command::UpdatePreferredDifficulty(index) => {
            session.preferred_difficulty_index = index;
            CommandEffect::None
        }
        Command::UpdateLastPlayed {
            side,
            play_style,
            music_path,
            chart_hash,
            difficulty_index,
        } => {
            profile::update_last_played_for_side(
                side,
                play_style,
                music_path.as_deref(),
                chart_hash.as_deref(),
                difficulty_index,
            );
            CommandEffect::None
        }
    }
}

pub fn execute_command_resources<EvaluationPage>(
    command: Command,
    session: &mut SessionState<EvaluationPage>,
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: Option<&mut Backend>,
    context: CommandContext,
) -> CommandExecutionResult {
    let kind = command.kind();
    let started = Instant::now();
    let effect = apply_command(command, session, dynamic_media, assets, backend, context);
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    CommandExecutionResult {
        effect,
        timing: command_timing_result(kind, elapsed_ms),
    }
}

fn spawn_online_grade_fetch(hash: String) {
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

pub fn command_timing_result(kind: CommandKind, elapsed_ms: f64) -> CommandTimingResult {
    CommandTimingResult {
        #[cfg(test)]
        kind,
        label: command_label(kind),
        elapsed_ms,
        log: command_timing_log(kind, elapsed_ms),
    }
}

pub fn log_command_timing_for_screen(timing: CommandTimingResult, screen: Screen) {
    match timing.log {
        CommandTimingLog::Slow => {
            warn!(
                "Slow command: {} took {:.2}ms on screen {:?}",
                timing.label, timing.elapsed_ms, screen
            );
        }
        CommandTimingLog::FrameCost => {
            debug!(
                "Frame-cost command: {} took {:.2}ms on screen {:?}",
                timing.label, timing.elapsed_ms, screen
            );
        }
        CommandTimingLog::CommandTiming => {
            debug!(
                "Command timing: {} took {:.2}ms on screen {:?}",
                timing.label, timing.elapsed_ms, screen
            );
        }
        CommandTimingLog::None => {}
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
        let timing = command_timing_result(CommandKind::SetBanner, 1.0);
        assert_eq!(timing.kind, CommandKind::SetBanner);
        assert_eq!(timing.label, "SetBanner");
        assert_eq!(timing.log, CommandTimingLog::CommandTiming);
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

    fn command_context() -> CommandContext {
        CommandContext {
            current_screen: Screen::SelectMusic,
            select_music_color_index: 0,
            select_course_color_index: 0,
            video_started_at_sec: 0.0,
            show_video_backgrounds: true,
            wide_screen: true,
        }
    }

    #[test]
    fn density_graph_command_builds_screen_update_without_backend() {
        let mut session = SessionState::<()>::new(0, [0; 2]);
        let result = execute_command_resources(
            Command::SetDensityGraph {
                slot: DensityGraphSlot::SelectMusicP1,
                chart_opt: None,
            },
            &mut session,
            &mut DynamicMedia::new(),
            &mut AssetManager::new(),
            None,
            command_context(),
        );

        assert!(matches!(
            result.effect,
            CommandEffect::DensityGraph {
                slot: DensityGraphSlot::SelectMusicP1,
                mesh: None,
            }
        ));
    }

    #[test]
    fn process_commands_become_root_effects() {
        for (command, expected) in [
            (Command::ExitNow, CommandKind::ExitNow),
            (Command::Shutdown, CommandKind::Shutdown),
        ] {
            let mut session = SessionState::<()>::new(0, [0; 2]);
            let result = execute_command_resources(
                command,
                &mut session,
                &mut DynamicMedia::new(),
                &mut AssetManager::new(),
                None,
                command_context(),
            );
            assert_eq!(result.timing.kind, expected);
            assert!(matches!(
                (expected, result.effect),
                (CommandKind::ExitNow, CommandEffect::ExitNow)
                    | (CommandKind::Shutdown, CommandEffect::Shutdown)
            ));
        }
    }

    #[test]
    fn timed_command_execution_combines_shell_and_resource_effects() {
        let mut session = SessionState::<()>::new(0, [0; 2]);
        let result = execute_command_resources(
            Command::SetDensityGraph {
                slot: DensityGraphSlot::SelectMusicP2,
                chart_opt: None,
            },
            &mut session,
            &mut DynamicMedia::new(),
            &mut AssetManager::new(),
            None,
            command_context(),
        );

        assert_eq!(result.timing.kind, CommandKind::SetDensityGraph);
        assert_eq!(result.timing.label, "SetDensityGraph");
        assert!(matches!(
            result.effect,
            CommandEffect::DensityGraph {
                slot: DensityGraphSlot::SelectMusicP2,
                mesh: None,
            }
        ));
    }

    #[test]
    fn preferred_difficulty_command_updates_shell_session_directly() {
        let mut session = SessionState::<()>::new(1, [0; 2]);
        let result = execute_command_resources(
            Command::UpdatePreferredDifficulty(4),
            &mut session,
            &mut DynamicMedia::new(),
            &mut AssetManager::new(),
            None,
            command_context(),
        );
        assert!(matches!(result.effect, CommandEffect::None));
        assert_eq!(session.preferred_difficulty_index, 4);
    }
}
