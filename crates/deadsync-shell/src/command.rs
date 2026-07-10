use deadlib_present::density;
use deadlib_render::MeshVertex;
use deadlib_renderer::Backend;
use deadsync_assets::{AssetManager, media_path_key};
use deadsync_online::score_compat as scores;
use deadsync_profile::compat as profile;
use deadsync_profile::{PlayStyle, PlayerSide};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_screens::{DensityGraphSlot, DensityGraphSource, Screen};
use log::{debug, warn};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::SessionState;
use crate::dynamic_media::DynamicMedia;

pub const WHITE_GRAPH_KEY: &str = "__white";

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
pub struct DeferredCommandResourceContext {
    pub current_screen: Screen,
    pub select_music_color_index: i32,
    pub select_course_color_index: i32,
    pub video_started_at_sec: f32,
    pub show_video_backgrounds: bool,
    pub wide_screen: bool,
}

pub enum DeferredCommandEffect {
    None,
    ExitNow,
    Shutdown,
    Banner {
        slot: BannerSlot,
        key: String,
    },
    CdTitle(Option<String>),
    DensityGraph {
        slot: DensityGraphSlot,
        mesh: Option<Arc<[MeshVertex]>>,
    },
    DynamicBackground(DynamicBackgroundMediaResult),
}

pub enum DeferredCommandRootEffect {
    None,
    Banner {
        slot: BannerSlot,
        key: String,
    },
    CdTitle(Option<String>),
    DensityGraph {
        slot: DensityGraphSlot,
        mesh: Option<Arc<[MeshVertex]>>,
        graph_key: &'static str,
    },
    DynamicBackground {
        media: DynamicBackgroundMediaResult,
        update_gameplay: bool,
        update_practice: bool,
        preserve_dirty: bool,
    },
}

pub struct DeferredCommandApplyPlan {
    pub process: DeferredCommandProcessPlan,
    pub root_effect: DeferredCommandRootEffect,
}

pub struct CommandTimingResult {
    pub kind: CommandKind,
    pub label: &'static str,
    pub elapsed_ms: f64,
    pub log: CommandTimingLog,
}

pub struct CommandExecutionResult {
    pub effect: DeferredCommandEffect,
    pub timing: CommandTimingResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeferredCommandProcessPlan {
    Continue,
    ExitNow,
    Shutdown,
}

pub fn deferred_command_process_plan(effect: &DeferredCommandEffect) -> DeferredCommandProcessPlan {
    match effect {
        DeferredCommandEffect::ExitNow => DeferredCommandProcessPlan::ExitNow,
        DeferredCommandEffect::Shutdown => DeferredCommandProcessPlan::Shutdown,
        _ => DeferredCommandProcessPlan::Continue,
    }
}

pub fn apply_deferred_command_process_plan(plan: DeferredCommandProcessPlan) -> bool {
    match plan {
        DeferredCommandProcessPlan::Continue => false,
        DeferredCommandProcessPlan::ExitNow => true,
        DeferredCommandProcessPlan::Shutdown => {
            if let Err(e) = deadlib_platform::power::shutdown_host() {
                warn!("host shutdown failed; exiting application only: {e}");
            }
            true
        }
    }
}

pub fn deferred_command_apply_plan(effect: DeferredCommandEffect) -> DeferredCommandApplyPlan {
    let process = deferred_command_process_plan(&effect);
    if process != DeferredCommandProcessPlan::Continue {
        return DeferredCommandApplyPlan {
            process,
            root_effect: DeferredCommandRootEffect::None,
        };
    }

    let root_effect = match effect {
        DeferredCommandEffect::None => DeferredCommandRootEffect::None,
        DeferredCommandEffect::ExitNow | DeferredCommandEffect::Shutdown => {
            DeferredCommandRootEffect::None
        }
        DeferredCommandEffect::Banner { slot, key } => {
            DeferredCommandRootEffect::Banner { slot, key }
        }
        DeferredCommandEffect::CdTitle(key) => DeferredCommandRootEffect::CdTitle(key),
        DeferredCommandEffect::DensityGraph { slot, mesh } => {
            DeferredCommandRootEffect::DensityGraph {
                slot,
                mesh,
                graph_key: WHITE_GRAPH_KEY,
            }
        }
        DeferredCommandEffect::DynamicBackground(media) => {
            DeferredCommandRootEffect::DynamicBackground {
                media,
                update_gameplay: true,
                update_practice: true,
                preserve_dirty: true,
            }
        }
    };

    DeferredCommandApplyPlan {
        process,
        root_effect,
    }
}

pub fn apply_banner_media(
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: &mut Backend,
    path_opt: Option<PathBuf>,
    fallback_color_index: i32,
) -> String {
    if let Some(path) = path_opt {
        dynamic_media.set_banner(assets, backend, Some(path))
    } else {
        dynamic_media.destroy_banner(assets, backend);
        fallback_banner_key(fallback_color_index)
    }
}

pub fn apply_cdtitle_media(
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: &mut Backend,
    path_opt: Option<PathBuf>,
) -> Option<String> {
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

pub fn apply_deferred_command_resources(
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: Option<&mut Backend>,
    command: DeferredCommand,
    context: DeferredCommandResourceContext,
) -> DeferredCommandEffect {
    match command {
        DeferredCommand::ExitNow => DeferredCommandEffect::ExitNow,
        DeferredCommand::Shutdown => DeferredCommandEffect::Shutdown,
        DeferredCommand::SetBanner(path_opt) => {
            let Some(backend) = backend else {
                return DeferredCommandEffect::None;
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
            DeferredCommandEffect::Banner { slot, key }
        }
        DeferredCommand::SetCdTitle(path_opt) => {
            let Some(backend) = backend else {
                return DeferredCommandEffect::None;
            };
            DeferredCommandEffect::CdTitle(apply_cdtitle_media(
                dynamic_media,
                assets,
                backend,
                path_opt,
            ))
        }
        DeferredCommand::SetPackBanner(path_opt) => {
            if let Some(backend) = backend {
                apply_pack_banner_media(dynamic_media, assets, backend, path_opt);
            }
            DeferredCommandEffect::None
        }
        DeferredCommand::SetWheelItemBackgrounds(paths) => {
            if let Some(backend) = backend {
                apply_wheel_item_backgrounds_media(dynamic_media, assets, backend, paths);
            }
            DeferredCommandEffect::None
        }
        DeferredCommand::SetDensityGraph { slot, chart_opt } => {
            DeferredCommandEffect::DensityGraph {
                slot,
                mesh: build_density_graph_mesh(chart_opt, context.wide_screen),
            }
        }
        DeferredCommand::SetDynamicBackground(path_opt) => {
            let Some(backend) = backend else {
                return DeferredCommandEffect::None;
            };
            DeferredCommandEffect::DynamicBackground(apply_dynamic_background_media(
                dynamic_media,
                assets,
                backend,
                path_opt,
                context.video_started_at_sec,
                context.show_video_backgrounds,
            ))
        }
    }
}

pub fn execute_command_resources<EvaluationPage>(
    command: Command,
    session: &mut SessionState<EvaluationPage>,
    dynamic_media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: Option<&mut Backend>,
    context: DeferredCommandResourceContext,
) -> CommandExecutionResult {
    let kind = command.kind();
    let started = Instant::now();
    let effect = execute_shell_command(command, session)
        .map(|command| {
            apply_deferred_command_resources(dynamic_media, assets, backend, command, context)
        })
        .unwrap_or(DeferredCommandEffect::None);
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

/// Commands whose concrete process, renderer, or theme-state effects remain in the root app.
pub enum DeferredCommand {
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
    SetDynamicBackground(Option<PathBuf>),
}

/// Execute commands fully owned by the shell and return effects that still need root resources.
pub fn execute_shell_command<EvaluationPage>(
    command: Command,
    session: &mut SessionState<EvaluationPage>,
) -> Option<DeferredCommand> {
    match command {
        Command::ExitNow => Some(DeferredCommand::ExitNow),
        Command::Shutdown => Some(DeferredCommand::Shutdown),
        Command::SetBanner(path) => Some(DeferredCommand::SetBanner(path)),
        Command::SetCdTitle(path) => Some(DeferredCommand::SetCdTitle(path)),
        Command::SetPackBanner(path) => Some(DeferredCommand::SetPackBanner(path)),
        Command::SetWheelItemBackgrounds(paths) => {
            Some(DeferredCommand::SetWheelItemBackgrounds(paths))
        }
        Command::SetDensityGraph { slot, chart_opt } => {
            Some(DeferredCommand::SetDensityGraph { slot, chart_opt })
        }
        Command::SetDynamicBackground(path) => Some(DeferredCommand::SetDynamicBackground(path)),
        Command::FetchOnlineGrade(hash) => {
            spawn_online_grade_fetch(hash);
            None
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
            None
        }
        Command::StopMusic => {
            deadsync_audio_stream::stop_music();
            None
        }
        Command::UpdateScrollSpeed { side, setting } => {
            profile::update_scroll_speed_for_side(side, setting);
            None
        }
        Command::UpdateSessionMusicRate(rate) => {
            profile::set_session_music_rate(rate);
            None
        }
        Command::UpdatePreferredDifficulty(index) => {
            session.preferred_difficulty_index = index;
            None
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
            None
        }
    }
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

    #[test]
    fn deferred_density_graph_builds_screen_update_without_backend() {
        let effect = apply_deferred_command_resources(
            &mut DynamicMedia::new(),
            &mut AssetManager::new(),
            None,
            DeferredCommand::SetDensityGraph {
                slot: DensityGraphSlot::SelectMusicP1,
                chart_opt: None,
            },
            DeferredCommandResourceContext {
                current_screen: Screen::SelectMusic,
                select_music_color_index: 0,
                select_course_color_index: 0,
                video_started_at_sec: 0.0,
                show_video_backgrounds: true,
                wide_screen: true,
            },
        );

        assert!(matches!(
            effect,
            DeferredCommandEffect::DensityGraph {
                slot: DensityGraphSlot::SelectMusicP1,
                mesh: None,
            }
        ));
    }

    #[test]
    fn deferred_process_commands_become_root_effects() {
        let context = DeferredCommandResourceContext {
            current_screen: Screen::SelectMusic,
            select_music_color_index: 0,
            select_course_color_index: 0,
            video_started_at_sec: 0.0,
            show_video_backgrounds: true,
            wide_screen: true,
        };

        assert!(matches!(
            apply_deferred_command_resources(
                &mut DynamicMedia::new(),
                &mut AssetManager::new(),
                None,
                DeferredCommand::ExitNow,
                context,
            ),
            DeferredCommandEffect::ExitNow
        ));
        assert!(matches!(
            apply_deferred_command_resources(
                &mut DynamicMedia::new(),
                &mut AssetManager::new(),
                None,
                DeferredCommand::Shutdown,
                context,
            ),
            DeferredCommandEffect::Shutdown
        ));
    }

    #[test]
    fn deferred_process_plan_is_pure_before_side_effects() {
        assert_eq!(
            deferred_command_process_plan(&DeferredCommandEffect::None),
            DeferredCommandProcessPlan::Continue,
        );
        assert_eq!(
            deferred_command_process_plan(&DeferredCommandEffect::ExitNow),
            DeferredCommandProcessPlan::ExitNow,
        );
        assert_eq!(
            deferred_command_process_plan(&DeferredCommandEffect::Shutdown),
            DeferredCommandProcessPlan::Shutdown,
        );
        assert!(!apply_deferred_command_process_plan(
            DeferredCommandProcessPlan::Continue
        ));
    }

    #[test]
    fn deferred_apply_plan_routes_process_effects_without_root_mutation() {
        let plan = deferred_command_apply_plan(DeferredCommandEffect::ExitNow);
        assert_eq!(plan.process, DeferredCommandProcessPlan::ExitNow);
        assert!(matches!(plan.root_effect, DeferredCommandRootEffect::None));

        let plan = deferred_command_apply_plan(DeferredCommandEffect::Shutdown);
        assert_eq!(plan.process, DeferredCommandProcessPlan::Shutdown);
        assert!(matches!(plan.root_effect, DeferredCommandRootEffect::None));
    }

    #[test]
    fn deferred_apply_plan_captures_graph_and_background_policy() {
        let graph = deferred_command_apply_plan(DeferredCommandEffect::DensityGraph {
            slot: DensityGraphSlot::SelectMusicP2,
            mesh: None,
        });
        assert_eq!(graph.process, DeferredCommandProcessPlan::Continue);
        assert!(matches!(
            graph.root_effect,
            DeferredCommandRootEffect::DensityGraph {
                slot: DensityGraphSlot::SelectMusicP2,
                mesh: None,
                graph_key: WHITE_GRAPH_KEY,
            }
        ));

        let background = deferred_command_apply_plan(DeferredCommandEffect::DynamicBackground(
            DynamicBackgroundMediaResult {
                path: Some("background.png".into()),
                path_key: Some(Arc::<str>::from("background-key")),
                texture_key: Arc::<str>::from("texture-key"),
                allow_video: true,
            },
        ));
        assert_eq!(background.process, DeferredCommandProcessPlan::Continue);
        assert!(matches!(
            background.root_effect,
            DeferredCommandRootEffect::DynamicBackground {
                update_gameplay: true,
                update_practice: true,
                preserve_dirty: true,
                ..
            }
        ));
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
            DeferredCommandResourceContext {
                current_screen: Screen::SelectMusic,
                select_music_color_index: 0,
                select_course_color_index: 0,
                video_started_at_sec: 0.0,
                show_video_backgrounds: true,
                wide_screen: true,
            },
        );

        assert_eq!(result.timing.kind, CommandKind::SetDensityGraph);
        assert_eq!(result.timing.label, "SetDensityGraph");
        assert!(matches!(
            result.effect,
            DeferredCommandEffect::DensityGraph {
                slot: DensityGraphSlot::SelectMusicP2,
                mesh: None,
            }
        ));
    }

    #[test]
    fn root_resource_commands_are_deferred_without_losing_payloads() {
        let mut session = SessionState::<()>::new(0, [0; 2]);
        let command = execute_shell_command(
            Command::SetDensityGraph {
                slot: DensityGraphSlot::SelectMusicP2,
                chart_opt: None,
            },
            &mut session,
        );
        assert!(matches!(
            command,
            Some(DeferredCommand::SetDensityGraph {
                slot: DensityGraphSlot::SelectMusicP2,
                chart_opt: None,
            })
        ));

        let command = execute_shell_command(
            Command::SetDynamicBackground(Some("background.png".into())),
            &mut session,
        );
        assert!(matches!(
            command,
            Some(DeferredCommand::SetDynamicBackground(Some(path)))
                if path == PathBuf::from("background.png")
        ));
    }

    #[test]
    fn preferred_difficulty_command_updates_shell_session_directly() {
        let mut session = SessionState::<()>::new(1, [0; 2]);
        let command = execute_shell_command(Command::UpdatePreferredDifficulty(4), &mut session);
        assert!(command.is_none());
        assert_eq!(session.preferred_difficulty_index, 4);
    }
}
