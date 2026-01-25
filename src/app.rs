use crate::assets::{AssetManager, DensityGraphSlot};
use crate::config::{self, DisplayMode};
use crate::core::display;
use crate::core::gfx::{self as renderer, BackendType, RenderList, create_backend};
use crate::core::input::{self, InputEvent};
use crate::core::space::{self as space, Metrics};
use crate::game::chart::ChartData;
use crate::game::parsing::simfile as song_loading;
use crate::game::{profile, scores, scroll::ScrollSpeedSetting};
use crate::screens::{
    Screen as CurrentScreen, ScreenAction, evaluation, gameplay, init, input as input_screen,
    mappings, menu, options, player_options, sandbox, select_color, select_music, select_profile,
    select_style,
};
use crate::ui::color;
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    monitor::MonitorHandle,
    window::Window,
};

use log::{error, info, warn};
use std::cmp;
use std::{error::Error, path::PathBuf, sync::Arc, time::Instant};

use crate::ui::actors::Actor;
/* -------------------- gamepad -------------------- */
use crate::core::input::{GpSystemEvent, PadEvent};

/* -------------------- user events -------------------- */
#[derive(Debug, Clone)]
pub enum UserEvent {
    Pad(PadEvent),
    GamepadSystem(GpSystemEvent),
}

/// Imperative effects to be executed by the shell.
enum Command {
    ExitNow,
    SetBanner(Option<PathBuf>),
    SetDensityGraph {
        slot: DensityGraphSlot,
        chart_opt: Option<ChartData>,
    },
    FetchOnlineGrade(String),
    PlayMusic {
        path: PathBuf,
        looped: bool,
        volume: f32,
    },
    StopMusic,
    SetEvaluationGraphData(Option<(String, rssp::graph::GraphImageData)>),
    SetDynamicBackground(Option<PathBuf>),
    UpdateScrollSpeed(ScrollSpeedSetting),
    UpdateSessionMusicRate(f32),
    UpdatePreferredDifficulty(usize),
    UpdateLastPlayed {
        music_path: Option<PathBuf>,
        chart_hash: Option<String>,
        difficulty_index: usize,
    },
}

/* -------------------- transition timing constants -------------------- */
const FADE_OUT_DURATION: f32 = 0.4;
const MENU_TO_SELECT_COLOR_OUT_DURATION: f32 = 1.0;
const MENU_ACTORS_FADE_DURATION: f32 = 0.65;

/* -------------------- transition state machine -------------------- */
#[derive(Debug)]
enum TransitionState {
    Idle,
    FadingOut {
        elapsed: f32,
        duration: f32,
        target: CurrentScreen,
    },
    FadingIn {
        elapsed: f32,
        duration: f32,
    },
    ActorsFadeOut {
        elapsed: f32,
        duration: f32,
        target: CurrentScreen,
    },
    ActorsFadeIn {
        elapsed: f32,
    },
}

/// Shell-level state: timing, window, renderer flags.
pub struct ShellState {
    frame_count: u32,
    last_title_update: Instant,
    last_frame_time: Instant,
    start_time: Instant,
    vsync_enabled: bool,
    display_mode: DisplayMode,
    display_monitor: usize,
    metrics: Metrics,
    last_fps: f32,
    last_vpf: u32,
    current_frame_vpf: u32,
    show_overlay: bool,
    transition: TransitionState,
    display_width: u32,
    display_height: u32,
    pending_window_position: Option<PhysicalPosition<i32>>,
    gamepad_overlay_state: Option<(String, Instant)>,
    gamepad_startup_complete: bool,
    pending_exit: bool,
    shift_held: bool,
}

/// Active screen data bundle.
pub struct ScreensState {
    current_screen: CurrentScreen,
    menu_state: menu::State,
    gameplay_state: Option<gameplay::State>,
    options_state: options::State,
    mappings_state: mappings::State,
    input_state: input_screen::State,
    player_options_state: Option<player_options::State>,
    init_state: init::State,
    select_profile_state: select_profile::State,
    select_color_state: select_color::State,
    select_style_state: select_style::State,
    select_music_state: select_music::State,
    sandbox_state: sandbox::State,
    evaluation_state: evaluation::State,
}

/// Session-wide values that survive screen swaps.
pub struct SessionState {
    preferred_difficulty_index: usize,
    session_start_time: Option<Instant>,
}

/// Pure-ish container for the high-level game state.
/// This keeps screen flow, timing and UI state separate from the window/renderer shell.
pub struct AppState {
    shell: ShellState,
    screens: ScreensState,
    session: SessionState,
}

impl ShellState {
    fn new(cfg: &config::Config, show_overlay: bool) -> Self {
        let metrics = space::metrics_for_window(cfg.display_width, cfg.display_height);
        Self {
            frame_count: 0,
            last_title_update: Instant::now(),
            last_frame_time: Instant::now(),
            start_time: Instant::now(),
            vsync_enabled: cfg.vsync,
            display_mode: cfg.display_mode(),
            metrics,
            last_fps: 0.0,
            last_vpf: 0,
            current_frame_vpf: 0,
            show_overlay,
            transition: TransitionState::Idle,
            display_width: cfg.display_width,
            display_height: cfg.display_height,
            display_monitor: cfg.display_monitor,
            pending_window_position: None,
            gamepad_overlay_state: None,
            gamepad_startup_complete: false,
            pending_exit: false,
            shift_held: false,
        }
    }

    fn update_gamepad_overlay(&mut self, now: Instant) {
        if let Some((_, start_time)) = self.gamepad_overlay_state {
            const HOLD_DURATION: f32 = 3.33;
            const FADE_OUT_DURATION: f32 = 0.25;
            const TOTAL_DURATION: f32 = HOLD_DURATION + FADE_OUT_DURATION;
            if now.duration_since(start_time).as_secs_f32() > TOTAL_DURATION {
                self.gamepad_overlay_state = None;
            }
        }
    }
}

impl SessionState {
    const fn new(preferred_difficulty_index: usize) -> Self {
        Self {
            preferred_difficulty_index,
            session_start_time: None,
        }
    }
}

impl ScreensState {
    fn new(color_index: i32, preferred_difficulty_index: usize) -> Self {
        let mut menu_state = menu::init();
        menu_state.active_color_index = color_index;

        let mut select_profile_state = select_profile::init();
        select_profile_state.active_color_index = color_index;

        let mut select_color_state = select_color::init();
        select_color_state.active_color_index = color_index;
        select_color::snap_scroll_to_active(&mut select_color_state);
        select_color_state.bg_from_index = color_index;
        select_color_state.bg_to_index = color_index;

        let mut select_music_state = select_music::init();
        select_music_state.active_color_index = color_index;
        select_music_state.preferred_difficulty_index = preferred_difficulty_index;
        select_music_state.selected_steps_index = preferred_difficulty_index;

        let mut select_style_state = select_style::init();
        select_style_state.active_color_index = color_index;

        let mut options_state = options::init();
        options_state.active_color_index = color_index;

        let mut mappings_state = mappings::init();
        mappings_state.active_color_index = color_index;

        let mut input_state = input_screen::init();
        input_state.active_color_index = color_index;

        let mut init_state = init::init();
        init_state.active_color_index = color_index;

        let mut evaluation_state = evaluation::init(None);
        evaluation_state.active_color_index = color_index;

        Self {
            current_screen: CurrentScreen::Init,
            menu_state,
            gameplay_state: None,
            options_state,
            mappings_state,
            input_state,
            player_options_state: None,
            init_state,
            select_profile_state,
            select_color_state,
            select_style_state,
            select_music_state,
            sandbox_state: sandbox::init(),
            evaluation_state,
        }
    }

    fn step_idle(
        &mut self,
        delta_time: f32,
        now: Instant,
        session: &SessionState,
    ) -> Option<ScreenAction> {
        match self.current_screen {
            CurrentScreen::Gameplay => {
                self.gameplay_state.as_mut().map(|gs| gameplay::update(gs, delta_time))
            }
            CurrentScreen::Init => Some(init::update(&mut self.init_state, delta_time)),
            CurrentScreen::Options => options::update(&mut self.options_state, delta_time),
            CurrentScreen::Mappings => {
                mappings::update(&mut self.mappings_state, delta_time);
                None
            }
            CurrentScreen::Input => {
                input_screen::update(&mut self.input_state, delta_time);
                None
            }
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &mut self.player_options_state {
                    player_options::update(pos, delta_time);
                }
                None
            }
            CurrentScreen::Sandbox => {
                sandbox::update(&mut self.sandbox_state, delta_time);
                None
            }
            CurrentScreen::SelectProfile => {
                select_profile::update(&mut self.select_profile_state, delta_time);
                None
            }
            CurrentScreen::SelectColor => {
                select_color::update(&mut self.select_color_state, delta_time);
                None
            }
            CurrentScreen::SelectStyle => {
                select_style::update(&mut self.select_style_state, delta_time)
            }
            CurrentScreen::Evaluation => {
                if let Some(start) = session.session_start_time {
                    self.evaluation_state.session_elapsed = now.duration_since(start).as_secs_f32();
                }
                evaluation::update(&mut self.evaluation_state, delta_time);
                None
            }
            CurrentScreen::SelectMusic => {
                if let Some(start) = session.session_start_time {
                    self.select_music_state.session_elapsed =
                        now.duration_since(start).as_secs_f32();
                }
                Some(select_music::update(
                    &mut self.select_music_state,
                    delta_time,
                ))
            }
            CurrentScreen::Menu => None,
        }
    }
}

impl AppState {
    fn new(
        cfg: config::Config,
        profile_data: profile::Profile,
        show_overlay: bool,
        color_index: i32,
    ) -> Self {
        let max_diff_index = crate::ui::color::FILE_DIFFICULTY_NAMES
            .len()
            .saturating_sub(1);
        let preferred = if max_diff_index == 0 {
            0
        } else {
            cmp::min(profile_data.last_difficulty_index, max_diff_index)
        };

        let shell = ShellState::new(&cfg, show_overlay);
        let session = SessionState::new(preferred);
        let screens = ScreensState::new(color_index, preferred);

        Self {
            shell,
            screens,
            session,
        }
    }
}

pub struct App {
    window: Option<Arc<Window>>,
    backend: Option<renderer::Backend>,
    backend_type: BackendType,
    asset_manager: AssetManager,
    state: AppState,
    software_renderer_threads: u8,
}

impl App {
    #[inline(always)]
    const fn is_actor_fade_screen(screen: CurrentScreen) -> bool {
        matches!(
            screen,
            CurrentScreen::Menu
                | CurrentScreen::Options
                | CurrentScreen::Mappings
                | CurrentScreen::Input
        )
    }

    fn update_options_monitor_specs(&mut self, event_loop: &ActiveEventLoop) {
        let monitors: Vec<MonitorHandle> = event_loop.available_monitors().collect();
        let specs = display::monitor_specs(&monitors);
        options::update_monitor_specs(&mut self.state.screens.options_state, specs);
    }

    fn new(
        backend_type: BackendType,
        show_overlay: bool,
        color_index: i32,
        config: config::Config,
        profile_data: profile::Profile,
    ) -> Self {
        let software_renderer_threads = config.software_renderer_threads;
        let state = AppState::new(config, profile_data, show_overlay, color_index);
        Self {
            window: None,
            backend: None,
            backend_type,
            asset_manager: AssetManager::new(),
            state,
            software_renderer_threads,
        }
    }

    fn handle_action(
        &mut self,
        action: ScreenAction,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let commands = match action {
            ScreenAction::Navigate(screen) => {
                self.handle_navigation_action(screen);
                Vec::new()
            }
            ScreenAction::Exit => self.handle_exit_action(),
            ScreenAction::SelectProfile(selected) => {
                let profile_data = profile::set_active_profile(selected);
                if let Some(backend) = self.backend.as_mut() {
                    self.asset_manager
                        .set_profile_avatar(backend, profile_data.avatar_path.clone());
                }

                let max_diff_index = crate::ui::color::FILE_DIFFICULTY_NAMES
                    .len()
                    .saturating_sub(1);
                let preferred = if max_diff_index == 0 {
                    0
                } else {
                    cmp::min(profile_data.last_difficulty_index, max_diff_index)
                };
                self.state.session.preferred_difficulty_index = preferred;

                let current_color_index = self.state.screens.select_profile_state.active_color_index;
                self.state.screens.select_music_state = select_music::init();
                self.state.screens.select_music_state.active_color_index = current_color_index;
                self.state.screens.select_music_state.preferred_difficulty_index = preferred;
                self.state.screens.select_music_state.selected_steps_index = preferred;

                self.handle_navigation_action(CurrentScreen::SelectColor);
                Vec::new()
            }
            ScreenAction::RequestBanner(path_opt) => vec![Command::SetBanner(path_opt)],
            ScreenAction::RequestDensityGraph { slot, chart_opt } => {
                vec![Command::SetDensityGraph { slot, chart_opt }]
            }
            ScreenAction::FetchOnlineGrade(hash) => vec![Command::FetchOnlineGrade(hash)],
            ScreenAction::ChangeGraphics {
                renderer,
                display_mode,
                resolution,
                monitor,
            } => {
                // Ensure options menu reflects current hardware state before processing changes
                self.update_options_monitor_specs(event_loop);

                let mut pending_resolution = None;
                if let Some((w, h)) = resolution {
                    self.state.shell.display_width = w;
                    self.state.shell.display_height = h;
                    config::update_display_resolution(w, h);
                    options::sync_display_resolution(&mut self.state.screens.options_state, w, h);
                    pending_resolution = Some((w, h));
                }
                let (_, monitor_count, chosen_monitor) = display::resolve_monitor(
                    event_loop,
                    monitor.unwrap_or(self.state.shell.display_monitor),
                );

                match (renderer, display_mode) {
                    (Some(new_backend), Some(mode)) => {
                        // When both change, avoid touching the old window; update state/config
                        // first so the new renderer is created directly in the target mode.
                        let prev_mode = self.state.shell.display_mode;
                        let fullscreen_type = match mode {
                            DisplayMode::Fullscreen(ft) => ft,
                            DisplayMode::Windowed => {
                                if let DisplayMode::Fullscreen(ft) = prev_mode {
                                    ft
                                } else {
                                    config::get().fullscreen_type
                                }
                            }
                        };
                        self.state.shell.display_mode = mode;
                        self.state.shell.display_monitor = chosen_monitor;
                        config::update_display_mode(mode);
                        config::update_display_monitor(chosen_monitor);
                        options::sync_display_mode(
                            &mut self.state.screens.options_state,
                            mode,
                            fullscreen_type,
                            chosen_monitor,
                            monitor_count,
                        );
                        self.switch_renderer(new_backend, pending_resolution, event_loop)?;
                    }
                    (None, Some(mode)) => {
                        self.apply_display_mode(mode, Some(chosen_monitor), event_loop)?;
                        if let Some((w, h)) = pending_resolution {
                            self.apply_resolution(w, h, event_loop)?;
                        }
                    }
                    (Some(new_backend), None) => {
                        if monitor.is_some() {
                            self.state.shell.display_monitor = chosen_monitor;
                            config::update_display_monitor(chosen_monitor);
                            let fullscreen_type = match self.state.shell.display_mode {
                                DisplayMode::Fullscreen(ft) => ft,
                                DisplayMode::Windowed => config::get().fullscreen_type,
                            };
                            options::sync_display_mode(
                                &mut self.state.screens.options_state,
                                self.state.shell.display_mode,
                                fullscreen_type,
                                chosen_monitor,
                                monitor_count,
                            );
                        }
                        self.switch_renderer(new_backend, pending_resolution, event_loop)?;
                    }
                    (None, None) => {
                        if monitor.is_some() {
                            // Move the existing window/fullscreen session to the chosen monitor.
                            self.apply_display_mode(
                                self.state.shell.display_mode,
                                Some(chosen_monitor),
                                event_loop,
                            )?;
                        }
                        if let Some((w, h)) = pending_resolution {
                            self.apply_resolution(w, h, event_loop)?;
                        }
                    }
                }
                Vec::new()
            }
            ScreenAction::UpdateShowOverlay(show) => {
                self.state.shell.show_overlay = show;
                config::update_show_stats(show);
                options::sync_show_stats(&mut self.state.screens.options_state, show);
                Vec::new()
            }
            ScreenAction::None => Vec::new(),
        };
        self.run_commands(commands, event_loop)
    }

    fn handle_navigation_action(&mut self, target: CurrentScreen) {
        let from = self.state.screens.current_screen;
        self.persist_gameplay_offset_if_changed(from, target);

        if from == CurrentScreen::Init && target == CurrentScreen::Menu {
            info!("Instant navigation Init→Menu (out-transition handled by Init screen)");
            self.state.screens.current_screen = target;
            self.state.shell.transition = TransitionState::ActorsFadeIn { elapsed: 0.0 };
            crate::ui::runtime::clear_all();
            return;
        }

        if !matches!(self.state.shell.transition, TransitionState::Idle) {
            return;
        }

        self.state.shell.pending_exit = false;
        if self.is_actor_only_fade(from, target) {
            self.start_actor_fade(from, target);
        } else {
            self.start_global_fade(target);
        }
    }

    fn persist_gameplay_offset_if_changed(&self, from: CurrentScreen, to: CurrentScreen) {
        if from != CurrentScreen::Gameplay || to == CurrentScreen::Gameplay {
            return;
        }
        if let Some(gs) = &self.state.screens.gameplay_state
            && (gs.global_offset_seconds - gs.initial_global_offset_seconds).abs() > f32::EPSILON
        {
            config::update_global_offset(gs.global_offset_seconds);
        }
    }

    fn is_actor_only_fade(&self, from: CurrentScreen, to: CurrentScreen) -> bool {
        (from == CurrentScreen::Menu
            && (to == CurrentScreen::Options
                || to == CurrentScreen::SelectProfile
                || to == CurrentScreen::SelectColor))
            || ((from == CurrentScreen::Options
                || from == CurrentScreen::SelectProfile
                || from == CurrentScreen::SelectColor)
                && to == CurrentScreen::Menu)
            || (from == CurrentScreen::SelectProfile && to == CurrentScreen::SelectColor)
            || (from == CurrentScreen::SelectProfile && to == CurrentScreen::SelectStyle)
            || (from == CurrentScreen::SelectStyle && to == CurrentScreen::SelectProfile)
            || (from == CurrentScreen::SelectColor && to == CurrentScreen::SelectStyle)
            || (from == CurrentScreen::SelectStyle && to == CurrentScreen::SelectColor)
            || (from == CurrentScreen::Options && to == CurrentScreen::Mappings)
            || (from == CurrentScreen::Mappings && to == CurrentScreen::Options)
    }

    fn start_actor_fade(&mut self, from: CurrentScreen, target: CurrentScreen) {
        info!("Starting actor-only fade out to screen: {target:?}");
        let duration = if from == CurrentScreen::Menu
            && (target == CurrentScreen::SelectProfile
                || target == CurrentScreen::SelectColor
                || target == CurrentScreen::Options)
        {
            MENU_TO_SELECT_COLOR_OUT_DURATION
        } else if from == CurrentScreen::SelectColor {
            select_color::exit_anim_duration()
        } else if from == CurrentScreen::SelectProfile {
            select_profile::exit_anim_duration()
        } else {
            FADE_OUT_DURATION
        };
        self.state.shell.transition = TransitionState::ActorsFadeOut {
            elapsed: 0.0,
            duration,
            target,
        };
    }

    fn start_global_fade(&mut self, target: CurrentScreen) {
        info!("Starting global fade out to screen: {target:?}");
        let (_, out_duration) =
            self.get_out_transition_for_screen(self.state.screens.current_screen);
        self.state.shell.transition = TransitionState::FadingOut {
            elapsed: 0.0,
            duration: out_duration,
            target,
        };
    }

    fn handle_exit_action(&mut self) -> Vec<Command> {
        if self.state.screens.current_screen == CurrentScreen::Menu
            && matches!(self.state.shell.transition, TransitionState::Idle)
        {
            info!("Exit requested from Menu; playing menu out-transition before shutdown.");
            let (_, out_duration) =
                self.get_out_transition_for_screen(self.state.screens.current_screen);
            self.state.shell.transition = TransitionState::FadingOut {
                elapsed: 0.0,
                duration: out_duration,
                target: self.state.screens.current_screen,
            };
            self.state.shell.pending_exit = true;
            Vec::new()
        } else {
            info!("Exit action received. Shutting down.");
            vec![Command::ExitNow]
        }
    }

    fn route_input_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: InputEvent,
    ) -> Result<(), Box<dyn Error>> {
        let action = match self.state.screens.current_screen {
            CurrentScreen::Menu => {
                crate::screens::menu::handle_input(&mut self.state.screens.menu_state, &ev)
            }
            CurrentScreen::SelectProfile => crate::screens::select_profile::handle_input(
                &mut self.state.screens.select_profile_state,
                &ev,
            ),
            CurrentScreen::SelectColor => crate::screens::select_color::handle_input(
                &mut self.state.screens.select_color_state,
                &ev,
            ),
            CurrentScreen::SelectStyle => crate::screens::select_style::handle_input(
                &mut self.state.screens.select_style_state,
                &ev,
            ),
            CurrentScreen::Options => {
                crate::screens::options::handle_input(&mut self.state.screens.options_state, &ev)
            }
            CurrentScreen::Mappings => {
                crate::screens::mappings::handle_input(&mut self.state.screens.mappings_state, &ev)
            }
            CurrentScreen::Input => {
                crate::screens::input::handle_input(&mut self.state.screens.input_state, &ev)
            }
            CurrentScreen::SelectMusic => crate::screens::select_music::handle_input(
                &mut self.state.screens.select_music_state,
                &ev,
            ),
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &mut self.state.screens.player_options_state {
                    crate::screens::player_options::handle_input(pos, &ev)
                } else {
                    ScreenAction::None
                }
            }
            CurrentScreen::Evaluation => crate::screens::evaluation::handle_input(
                &mut self.state.screens.evaluation_state,
                &ev,
            ),
            CurrentScreen::Sandbox => {
                crate::screens::sandbox::handle_input(&mut self.state.screens.sandbox_state, &ev)
            }
            CurrentScreen::Init => {
                crate::screens::init::handle_input(&mut self.state.screens.init_state, &ev)
            }
            CurrentScreen::Gameplay => {
                if let Some(gs) = &mut self.state.screens.gameplay_state {
                    crate::game::gameplay::handle_input(gs, &ev)
                } else {
                    ScreenAction::None
                }
            }
        };
        if matches!(action, ScreenAction::None) {
            return Ok(());
        }
        self.handle_action(action, event_loop)
    }

    fn run_commands(
        &mut self,
        commands: Vec<Command>,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        for command in commands {
            self.execute_command(command, event_loop)?;
        }
        Ok(())
    }

    fn execute_command(
        &mut self,
        command: Command,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        match command {
            Command::ExitNow => {
                event_loop.exit();
            }
            Command::SetBanner(path_opt) => self.apply_banner(path_opt),
            Command::SetDensityGraph { slot, chart_opt } => self.apply_density_graph(slot, chart_opt),
            Command::FetchOnlineGrade(hash) => self.spawn_grade_fetch(hash),
            Command::PlayMusic {
                path,
                looped,
                volume,
            } => self.play_music_command(path, looped, volume),
            Command::StopMusic => self.stop_music_command(),
            Command::SetEvaluationGraphData(graph_request) => {
                self.apply_evaluation_graph(graph_request);
            }
            Command::SetDynamicBackground(path_opt) => self.apply_dynamic_background(path_opt),
            Command::UpdateScrollSpeed(setting) => profile::update_scroll_speed(setting),
            Command::UpdateSessionMusicRate(rate) => {
                crate::game::profile::set_session_music_rate(rate);
            }
            Command::UpdatePreferredDifficulty(idx) => {
                self.state.session.preferred_difficulty_index = idx;
            }
            Command::UpdateLastPlayed {
                music_path,
                chart_hash,
                difficulty_index,
            } => {
                profile::update_last_played(
                    music_path.as_deref(),
                    chart_hash.as_deref(),
                    difficulty_index,
                );
            }
        }
        Ok(())
    }

    fn apply_banner(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            if let Some(path) = path_opt {
                let key = self.asset_manager.set_dynamic_banner(backend, Some(path));
                self.state.screens.select_music_state.current_banner_key = key;
            } else {
                self.asset_manager.destroy_dynamic_banner(backend);
                let color_index = self.state.screens.select_music_state.active_color_index;
                let banner_num = color_index.rem_euclid(12) + 1;
                let key = format!("banner{banner_num}.png");
                self.state.screens.select_music_state.current_banner_key = key;
            }
        }
    }

    fn apply_density_graph(&mut self, slot: DensityGraphSlot, chart_opt: Option<ChartData>) {
        if let Some(backend) = self.backend.as_mut() {
            let graph_request = chart_opt.and_then(|chart| {
                let graph_width = 1024;
                let graph_height = 256;
                let bottom_color = [0, 184, 204];
                let top_color = [130, 0, 161];
                let bg_color = [30, 40, 47];

                rssp::graph::generate_density_graph_rgba_data(
                    &chart.measure_nps_vec,
                    chart.max_nps,
                    graph_width,
                    graph_height,
                    bottom_color,
                    top_color,
                    bg_color,
                )
                .ok()
                .map(|data| (chart.short_hash, data))
            });

            let key = self.asset_manager.set_density_graph(backend, slot, graph_request);
            match slot {
                DensityGraphSlot::SelectMusicP1 => {
                    self.state.screens.select_music_state.current_graph_key = key;
                }
                DensityGraphSlot::SelectMusicP2 => {
                    self.state.screens.select_music_state.current_graph_key_p2 = key;
                }
                DensityGraphSlot::Evaluation => {}
            }
        }
    }

    fn spawn_grade_fetch(&self, hash: String) {
        info!("Fetching online grade for chart hash: {hash}");
        let profile = profile::get();
        std::thread::spawn(move || {
            if let Err(e) = scores::fetch_and_store_grade(profile, hash) {
                warn!("Failed to fetch online grade: {e}");
            }
        });
    }

    fn play_music_command(&self, path: PathBuf, looped: bool, volume: f32) {
        crate::core::audio::play_music(path, crate::core::audio::Cut::default(), looped, volume);
    }

    fn stop_music_command(&self) {
        crate::core::audio::stop_music();
    }

    fn apply_evaluation_graph(
        &mut self,
        graph_request: Option<(String, rssp::graph::GraphImageData)>,
    ) {
        if let Some(backend) = self.backend.as_mut() {
            let key = if let Some((key, data)) = graph_request {
                self.asset_manager
                    .set_density_graph(backend, DensityGraphSlot::Evaluation, Some((key, data)))
            } else {
                self.asset_manager.set_density_graph(backend, DensityGraphSlot::Evaluation, None)
            };
            self.state
                .screens
                .evaluation_state
                .density_graph_texture_key = key;
        }
    }

    fn apply_dynamic_background(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            let key = self.asset_manager.set_dynamic_background(backend, path_opt);
            if let Some(gs) = &mut self.state.screens.gameplay_state {
                gs.background_texture_key = key;
            }
        }
    }

    fn build_screen<'a>(
        &self,
        actors: &'a [Actor],
        clear_color: [f32; 4],
        total_elapsed: f32,
    ) -> RenderList<'a> {
        let fonts = self.asset_manager.fonts();
        crate::ui::compose::build_screen(
            actors,
            clear_color,
            &self.state.shell.metrics,
            fonts,
            total_elapsed,
        )
    }

    fn get_current_actors(&self) -> (Vec<Actor>, [f32; 4]) {
        const CLEAR: [f32; 4] = [0.03, 0.03, 0.03, 1.0];
        let mut screen_alpha_multiplier = 1.0;

        let is_actor_fade_screen = Self::is_actor_fade_screen(self.state.screens.current_screen);

        if is_actor_fade_screen {
            match self.state.shell.transition {
                TransitionState::ActorsFadeIn { elapsed } => {
                    screen_alpha_multiplier = (elapsed / MENU_ACTORS_FADE_DURATION).clamp(0.0, 1.0);
                }
                    TransitionState::ActorsFadeOut {
                        elapsed, duration, ..
                    } => {
                    screen_alpha_multiplier = 1.0 - (elapsed / duration).clamp(0.0, 1.0);
                }
                _ => {}
            }
        }

        let mut actors = match self.state.screens.current_screen {
            CurrentScreen::Menu => {
                menu::get_actors(&self.state.screens.menu_state, screen_alpha_multiplier)
            }
            CurrentScreen::Gameplay => {
                if let Some(gs) = &self.state.screens.gameplay_state {
                    gameplay::get_actors(gs, &self.asset_manager)
                } else {
                    vec![]
                }
            }
            CurrentScreen::Options => options::get_actors(
                &self.state.screens.options_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::Mappings => mappings::get_actors(
                &self.state.screens.mappings_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::Input => input_screen::get_actors(&self.state.screens.input_state),
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &self.state.screens.player_options_state {
                    player_options::get_actors(pos, &self.asset_manager)
                } else {
                    vec![]
                }
            }
            CurrentScreen::SelectProfile => select_profile::get_actors(
                &self.state.screens.select_profile_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::SelectColor => select_color::get_actors(
                &self.state.screens.select_color_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::SelectStyle => {
                select_style::get_actors(&self.state.screens.select_style_state)
            }
            CurrentScreen::SelectMusic => select_music::get_actors(
                &self.state.screens.select_music_state,
                &self.asset_manager,
            ),
            CurrentScreen::Sandbox => sandbox::get_actors(&self.state.screens.sandbox_state),
            CurrentScreen::Init => init::get_actors(&self.state.screens.init_state),
            CurrentScreen::Evaluation => {
                evaluation::get_actors(&self.state.screens.evaluation_state, &self.asset_manager)
            }
        };

        if self.state.shell.show_overlay {
            let overlay = crate::ui::components::stats_overlay::build(
                self.backend_type,
                self.state.shell.last_fps,
                self.state.shell.last_vpf,
            );
            actors.extend(overlay);
        }

        // Gamepad connection overlay (always on top of screen, but below transitions)
        if let Some((msg, _)) = &self.state.shell.gamepad_overlay_state {
            let params = crate::ui::components::gamepad_overlay::Params { message: msg };
            actors.extend(crate::ui::components::gamepad_overlay::build(params));
        }

        match &self.state.shell.transition {
            TransitionState::FadingOut { .. } => {
                let (out_actors, _) =
                    self.get_out_transition_for_screen(self.state.screens.current_screen);
                actors.extend(out_actors);
            }
            TransitionState::ActorsFadeOut { target, .. } => {
                // Special case: Menu → SelectColor / Menu → Options should keep the heart
                // background bright and only fade UI, but still play the hearts splash.
                if self.state.screens.current_screen == CurrentScreen::Menu
                    && (*target == CurrentScreen::SelectProfile
                        || *target == CurrentScreen::SelectColor
                        || *target == CurrentScreen::Options)
                {
                    let splash = crate::ui::components::menu_splash::build(
                        self.state.screens.menu_state.active_color_index,
                    );
                    actors.extend(splash);
                }
            }
            TransitionState::FadingIn { .. } => {
                let (in_actors, _) =
                    self.get_in_transition_for_screen(self.state.screens.current_screen);
                actors.extend(in_actors);
            }
            _ => {}
        }

        (actors, CLEAR)
    }

    fn get_out_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        match screen {
            CurrentScreen::Menu => {
                menu::out_transition(self.state.screens.menu_state.active_color_index)
            }
            CurrentScreen::Gameplay => gameplay::out_transition(),
            CurrentScreen::Options => options::out_transition(),
            CurrentScreen::Mappings => mappings::out_transition(),
            CurrentScreen::PlayerOptions => player_options::out_transition(),
            CurrentScreen::SelectProfile => select_profile::out_transition(),
            CurrentScreen::SelectColor => select_color::out_transition(),
            CurrentScreen::SelectStyle => select_style::out_transition(),
            CurrentScreen::SelectMusic => select_music::out_transition(),
            CurrentScreen::Sandbox => sandbox::out_transition(),
            CurrentScreen::Init => init::out_transition(),
            CurrentScreen::Evaluation => evaluation::out_transition(),
            CurrentScreen::Input => input_screen::out_transition(),
        }
    }

    fn get_in_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        match screen {
            CurrentScreen::Menu => menu::in_transition(),
            CurrentScreen::Gameplay => gameplay::in_transition(),
            CurrentScreen::Options => options::in_transition(),
            CurrentScreen::Mappings => mappings::in_transition(),
            CurrentScreen::PlayerOptions => player_options::in_transition(),
            CurrentScreen::SelectProfile => select_profile::in_transition(),
            CurrentScreen::SelectColor => select_color::in_transition(),
            CurrentScreen::SelectStyle => select_style::in_transition(),
            CurrentScreen::SelectMusic => select_music::in_transition(),
            CurrentScreen::Sandbox => sandbox::in_transition(),
            CurrentScreen::Evaluation => evaluation::in_transition(),
            CurrentScreen::Input => input_screen::in_transition(),
            CurrentScreen::Init => (vec![], 0.0),
        }
    }

    #[inline(always)]
    fn update_fps_title(&mut self, window: &Window, now: Instant) {
        self.state.shell.frame_count += 1;
        let elapsed = now.duration_since(self.state.shell.last_title_update);
        if elapsed.as_secs_f32() >= 1.0 {
            let fps = self.state.shell.frame_count as f32 / elapsed.as_secs_f32();
            self.state.shell.last_fps = fps;
            self.state.shell.last_vpf = self.state.shell.current_frame_vpf;
            let screen_name = format!("{:?}", self.state.screens.current_screen);
            window.set_title(&format!(
                "DeadSync - {:?} | {} | {:.2} FPS",
                self.backend_type, screen_name, fps
            ));
            self.state.shell.frame_count = 0;
            self.state.shell.last_title_update = now;
        }
    }

    fn init_graphics(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        // Collect monitors and update options immediately so the initial menu state is correct.
        self.update_options_monitor_specs(event_loop);

        let mut window_attributes = Window::default_attributes()
            .with_title(format!("DeadSync - {:?}", self.backend_type))
            .with_resizable(true)
            .with_transparent(false);

        let window_width = self.state.shell.display_width;
        let window_height = self.state.shell.display_height;
        let (monitor_handle, monitor_count, monitor_idx) =
            display::resolve_monitor(event_loop, self.state.shell.display_monitor);
        self.state.shell.display_monitor = monitor_idx;
        let fullscreen_type = match self.state.shell.display_mode {
            DisplayMode::Fullscreen(ft) => ft,
            DisplayMode::Windowed => config::get().fullscreen_type,
        };
        options::sync_display_mode(
            &mut self.state.screens.options_state,
            self.state.shell.display_mode,
            fullscreen_type,
            self.state.shell.display_monitor,
            monitor_count,
        );

        match self.state.shell.display_mode {
            DisplayMode::Fullscreen(fullscreen_type) => {
                let fullscreen = display::fullscreen_mode(
                    fullscreen_type,
                    window_width,
                    window_height,
                    monitor_handle,
                    event_loop,
                );
                window_attributes = window_attributes.with_fullscreen(fullscreen);
            }
            DisplayMode::Windowed => {
                window_attributes = window_attributes
                    .with_inner_size(PhysicalSize::new(window_width, window_height));
                if let Some(pos) = self.state.shell.pending_window_position.take() {
                    window_attributes = window_attributes.with_position(pos);
                } else if let Some(pos) = display::default_window_position(
                    window_width,
                    window_height,
                    monitor_handle,
                ) {
                    window_attributes = window_attributes.with_position(pos);
                }
            }
        }

        let window = Arc::new(event_loop.create_window(window_attributes)?);
        // Re-assert the opaque hint so compositors do not apply alpha-based blending.
        window.set_transparent(false);
        let sz = window.inner_size();
        self.state.shell.metrics = crate::core::space::metrics_for_window(sz.width, sz.height);
        crate::core::space::set_current_metrics(self.state.shell.metrics);
        let mut backend = create_backend(
            self.backend_type,
            window.clone(),
            self.state.shell.vsync_enabled,
        )?;

        if self.backend_type == BackendType::Software {
            let threads = match self.software_renderer_threads {
                0 => None,
                n => Some(n as usize),
            };
            backend.configure_software_threads(threads);
        }

        self.asset_manager.load_initial_assets(&mut backend)?;

        self.window = Some(window);
        self.backend = Some(backend);
        info!("Starting event loop...");
        Ok(())
    }

    fn switch_renderer(
        &mut self,
        target: BackendType,
        desired_size: Option<(u32, u32)>,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        if target == self.backend_type {
            return Ok(());
        }

        let previous_backend = self.backend_type;
        let mut old_window_pos: Option<PhysicalPosition<i32>> = None;
        if let Some((w, h)) = desired_size {
            self.state.shell.display_width = w;
            self.state.shell.display_height = h;
        }
        if let Some(window) = &self.window {
            if desired_size.is_none() {
                let sz = window.inner_size();
                self.state.shell.display_width = sz.width;
                self.state.shell.display_height = sz.height;
            }
            if matches!(self.state.shell.display_mode, DisplayMode::Fullscreen(_)) {
                window.set_fullscreen(None);
            }
            if matches!(self.state.shell.display_mode, DisplayMode::Windowed)
                && let Ok(pos) = window.outer_position() {
                    old_window_pos = Some(pos);
                }
            window.set_visible(false);
        }

        if let Some(mut backend) = self.backend.take() {
            self.asset_manager.destroy_dynamic_assets(&mut backend);
            backend.dispose_textures(&mut self.asset_manager.textures);
            backend.cleanup();
        }
        self.backend = None;
        self.window = None;
        self.state.shell.pending_window_position = old_window_pos;

        self.backend_type = target;
        self.state.shell.frame_count = 0;
        self.state.shell.last_title_update = Instant::now();
        self.state.shell.last_frame_time = Instant::now();

        match self.init_graphics(event_loop) {
            Ok(()) => {
                config::update_video_renderer(target);
                options::sync_video_renderer(&mut self.state.screens.options_state, target);
                crate::ui::runtime::clear_all();
                self.reset_dynamic_assets_after_renderer_switch();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                info!("Switched renderer to {target:?}");
                Ok(())
            }
            Err(e) => {
                error!("Failed to switch renderer to {target:?}: {e}");
                self.backend_type = previous_backend;
                if let Err(restoration_err) = self.init_graphics(event_loop) {
                    error!(
                        "Failed to restore previous renderer {previous_backend:?}: {restoration_err}"
                    );
                }
                options::sync_video_renderer(
                    &mut self.state.screens.options_state,
                    previous_backend,
                );
                let (_, monitor_count, monitor_idx) =
                    display::resolve_monitor(event_loop, self.state.shell.display_monitor);
                self.state.shell.display_monitor = monitor_idx;
                let fullscreen_type = match self.state.shell.display_mode {
                    DisplayMode::Fullscreen(ft) => ft,
                    DisplayMode::Windowed => config::get().fullscreen_type,
                };
                options::sync_display_mode(
                    &mut self.state.screens.options_state,
                    self.state.shell.display_mode,
                    fullscreen_type,
                    monitor_idx,
                    monitor_count,
                );
                self.state.shell.pending_window_position = None;
                config::update_video_renderer(previous_backend);
                Err(e)
            }
        }
    }

    fn apply_display_mode(
        &mut self,
        mode: DisplayMode,
        monitor_override: Option<usize>,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let (monitor_handle, monitor_count, resolved_monitor) = display::resolve_monitor(
            event_loop,
            monitor_override.unwrap_or(self.state.shell.display_monitor),
        );
        self.state.shell.display_monitor = resolved_monitor;
        let previous_mode = self.state.shell.display_mode;

        if let Some(window) = &self.window {
            if matches!(previous_mode, DisplayMode::Windowed) {
                let sz = window.inner_size();
                self.state.shell.display_width = sz.width;
                self.state.shell.display_height = sz.height;
                if let Ok(pos) = window.outer_position() {
                    self.state.shell.pending_window_position = Some(pos);
                }
            }

            match mode {
                DisplayMode::Windowed => {
                    window.set_fullscreen(None);
                    let size = PhysicalSize::new(
                        self.state.shell.display_width,
                        self.state.shell.display_height,
                    );
                    let _ = window.request_inner_size(size);
                    if let Some(pos) = self.state.shell.pending_window_position.take() {
                        window.set_outer_position(pos);
                    } else if let Some(pos) = display::default_window_position(
                        self.state.shell.display_width,
                        self.state.shell.display_height,
                        monitor_handle,
                    ) {
                        window.set_outer_position(pos);
                    }
                }
                DisplayMode::Fullscreen(fullscreen_type) => {
                    let fullscreen = display::fullscreen_mode(
                        fullscreen_type,
                        self.state.shell.display_width,
                        self.state.shell.display_height,
                        monitor_handle,
                        event_loop,
                    );
                    window.set_fullscreen(fullscreen);
                }
            }

            let sz = window.inner_size();
            self.state.shell.metrics = space::metrics_for_window(sz.width, sz.height);
            space::set_current_metrics(self.state.shell.metrics);
            if let Some(backend) = &mut self.backend {
                backend.resize(sz.width, sz.height);
            }
        }

        self.state.shell.display_mode = mode;

        let fullscreen_type = match mode {
            DisplayMode::Fullscreen(ft) => ft,
            DisplayMode::Windowed => match previous_mode {
                DisplayMode::Fullscreen(ft) => ft,
                DisplayMode::Windowed => config::get().fullscreen_type,
            },
        };
        config::update_display_mode(mode);
        config::update_display_monitor(self.state.shell.display_monitor);
        options::sync_display_mode(
            &mut self.state.screens.options_state,
            mode,
            fullscreen_type,
            self.state.shell.display_monitor,
            monitor_count,
        );
        Ok(())
    }

    fn apply_resolution(
        &mut self,
        width: u32,
        height: u32,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        self.state.shell.display_width = width;
        self.state.shell.display_height = height;
        let (monitor_handle, _, resolved_monitor) =
            display::resolve_monitor(event_loop, self.state.shell.display_monitor);
        self.state.shell.display_monitor = resolved_monitor;

        if let Some(window) = &self.window {
            match self.state.shell.display_mode {
                DisplayMode::Windowed => {
                    let size = PhysicalSize::new(width, height);
                    let _ = window.request_inner_size(size);
                }
                DisplayMode::Fullscreen(fullscreen_type) => {
                    let fullscreen = display::fullscreen_mode(
                        fullscreen_type,
                        width,
                        height,
                        monitor_handle,
                        event_loop,
                    );
                    window.set_fullscreen(fullscreen);
                }
            }

            let sz = window.inner_size();
            self.state.shell.metrics = space::metrics_for_window(sz.width, sz.height);
            space::set_current_metrics(self.state.shell.metrics);
            if let Some(backend) = &mut self.backend {
                backend.resize(sz.width, sz.height);
            }
        }

        Ok(())
    }

    fn reset_dynamic_assets_after_renderer_switch(&mut self) {
        self.apply_banner(None);
        self.apply_density_graph(DensityGraphSlot::SelectMusicP1, None);
        self.apply_density_graph(DensityGraphSlot::SelectMusicP2, None);
        self.apply_evaluation_graph(None);
        self.apply_dynamic_background(None);

        select_music::trigger_immediate_refresh(&mut self.state.screens.select_music_state);
        self.state.screens.select_music_state.current_graph_key = "__white".to_string();
        self.state
            .screens
            .select_music_state
            .current_graph_key_p2 = "__white".to_string();
    }

    /* -------------------- keyboard: map -> route -------------------- */

    #[inline(always)]
    fn handle_key_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        key_event: winit::event::KeyEvent,
    ) {
        // Track Shift key state for raw combos (e.g., global offset adjust)
        if let winit::keyboard::PhysicalKey::Code(code) = key_event.physical_key {
            use winit::event::ElementState;
            use winit::keyboard::KeyCode;
            match code {
                KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                    self.state.shell.shift_held = key_event.state == ElementState::Pressed;
                }
                _ => {}
            }
        }

        if self.state.screens.current_screen == CurrentScreen::Sandbox {
            let action = crate::screens::sandbox::handle_raw_key_event(
                &mut self.state.screens.sandbox_state,
                &key_event,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Sandbox raw key action: {e}");
                }
                return;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Menu {
            let action = crate::screens::menu::handle_raw_key_event(
                &mut self.state.screens.menu_state,
                &key_event,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Menu raw key action: {e}");
                }
                return;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Mappings {
            let action = crate::screens::mappings::handle_raw_key_event(
                &mut self.state.screens.mappings_state,
                &key_event,
            );
            if !matches!(action, ScreenAction::None)
                && let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Mappings raw key action: {e}");
                }
            // On the Mappings screen, arrows/Enter/Escape are handled entirely
            // via raw keycodes; do not route through the virtual keymap.
            return;
        } else if self.state.screens.current_screen == CurrentScreen::SelectMusic {
            // Route screen-specific raw key handling (e.g., F7 fetch) to the screen
            let action = crate::screens::select_music::handle_raw_key_event(
                &mut self.state.screens.select_music_state,
                &key_event,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle SelectMusic raw key action: {e}");
                }
                return;
            }
        } else if self.state.screens.current_screen == CurrentScreen::Gameplay
            && let Some(gs) = &mut self.state.screens.gameplay_state
        {
            let action = crate::game::gameplay::handle_raw_key_event(
                gs,
                &key_event,
                self.state.shell.shift_held,
            );
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Gameplay raw key action: {e}");
                }
                return;
            }
        }
        let is_transitioning = !matches!(self.state.shell.transition, TransitionState::Idle);
        let _event_timestamp = Instant::now();

        if key_event.state == winit::event::ElementState::Pressed
            && key_event.physical_key == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F3)
        {
            self.state.shell.show_overlay = !self.state.shell.show_overlay;
            let show = self.state.shell.show_overlay;
            log::info!("Overlay {}", if show { "ON" } else { "OFF" });
            config::update_show_stats(show);
            options::sync_show_stats(&mut self.state.screens.options_state, show);
        }
        // Screen-specific Escape handling resides in per-screen raw handlers now

        if is_transitioning {
            return;
        }

        for ev in input::map_key_event(&key_event) {
            if let Err(e) = self.route_input_event(event_loop, ev) {
                log::error!("Failed to handle input: {e}");
                event_loop.exit();
                return;
            }
        }
    }

    /* -------------------- pad event routing -------------------- */

    #[inline(always)]
    fn handle_pad_event(&mut self, event_loop: &ActiveEventLoop, ev: PadEvent) {
        let is_transitioning = !matches!(self.state.shell.transition, TransitionState::Idle);
        if is_transitioning || self.state.screens.current_screen == CurrentScreen::Init {
            return;
        }
        for iev in input::map_pad_event(&ev) {
            if let Err(e) = self.route_input_event(event_loop, iev) {
                error!("Failed to handle pad input: {e}");
                event_loop.exit();
                return;
            }
        }
    }

    // legacy virtual-action dispatcher removed; screens own their input

    #[cfg(any())]
    #[inline(always)]
    fn poll_gamepad_and_dispatch(&mut self, _event_loop: &ActiveEventLoop) {}

    fn on_fade_complete(&mut self, target: CurrentScreen, event_loop: &ActiveEventLoop) {
        if self.state.shell.pending_exit {
            info!("Fade-out complete; exiting application.");
            event_loop.exit();
            return;
        }

        let prev = self.state.screens.current_screen;
        self.state.screens.current_screen = target;
        if target == CurrentScreen::SelectColor {
            select_color::on_enter(&mut self.state.screens.select_color_state);
        }

        let mut commands: Vec<Command> = Vec::new();

        commands.extend(self.handle_audio_and_profile_on_fade(prev, target));
        self.handle_screen_state_on_fade(prev, target);
        commands.extend(self.handle_screen_entry_on_fade(prev, target));

        // Ensure monitor specs are fresh when entering the options screen
        if target == CurrentScreen::Options {
            self.update_options_monitor_specs(event_loop);
        }

        let (_, in_duration) = self.get_in_transition_for_screen(target);
        self.state.shell.transition = TransitionState::FadingIn {
            elapsed: 0.0,
            duration: in_duration,
        };
        crate::ui::runtime::clear_all();
        let _ = self.run_commands(commands, event_loop);
    }

    fn handle_audio_and_profile_on_fade(
        &mut self,
        prev: CurrentScreen,
        target: CurrentScreen,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        let menu_music_enabled = config::get().menu_music;
        let target_menu_music = menu_music_enabled
            && matches!(target, CurrentScreen::SelectColor | CurrentScreen::SelectStyle);
        let prev_menu_music =
            menu_music_enabled && matches!(prev, CurrentScreen::SelectColor | CurrentScreen::SelectStyle);

        if target_menu_music {
            if !prev_menu_music {
                commands.push(Command::PlayMusic {
                    path: PathBuf::from("assets/music/in_two (loop).ogg"),
                    looped: true,
                    volume: 1.0,
                });
            }
        } else if prev_menu_music {
            commands.push(Command::StopMusic);
        } else if target != CurrentScreen::Gameplay
            && !((prev == CurrentScreen::SelectMusic && target == CurrentScreen::PlayerOptions)
                || (prev == CurrentScreen::PlayerOptions && target == CurrentScreen::SelectMusic))
        {
            commands.push(Command::StopMusic);
        }

        if prev == CurrentScreen::Gameplay {
            commands.push(Command::StopMusic);
            if let Some(backend) = self.backend.as_mut() {
                self.asset_manager.set_dynamic_background(backend, None);
            }
        }

        if prev == CurrentScreen::SelectMusic || prev == CurrentScreen::PlayerOptions {
            if prev == CurrentScreen::PlayerOptions
                && let Some(po_state) = &self.state.screens.player_options_state
            {
                let play_style = profile::get_session_play_style();
                let player_side = profile::get_session_player_side();
                let persisted_idx = match play_style {
                    profile::PlayStyle::Versus => 0,
                    profile::PlayStyle::Single | profile::PlayStyle::Double => match player_side {
                        profile::PlayerSide::P1 => 0,
                        profile::PlayerSide::P2 => 1,
                    },
                };
                let speed_mod = &po_state.speed_mod[persisted_idx];
                let setting = match speed_mod.mod_type.as_str() {
                    "C" => Some(ScrollSpeedSetting::CMod(speed_mod.value)),
                    "X" => Some(ScrollSpeedSetting::XMod(speed_mod.value)),
                    "M" => Some(ScrollSpeedSetting::MMod(speed_mod.value)),
                    _ => None,
                };

                if let Some(setting) = setting {
                    commands.push(Command::UpdateScrollSpeed(setting));
                    info!("Saved scroll speed: {setting}");
                } else {
                    warn!(
                        "Unsupported speed mod '{}' not saved to profile.",
                        speed_mod.mod_type
                    );
                }

                commands.push(Command::UpdateSessionMusicRate(po_state.music_rate));
                info!("Session music rate set to {:.2}x", po_state.music_rate);

                self.state.session.preferred_difficulty_index = po_state.chart_difficulty_index;
                commands.push(Command::UpdatePreferredDifficulty(
                    po_state.chart_difficulty_index,
                ));
                info!(
                    "Updated preferred difficulty index to {} from PlayerOptions",
                    self.state.session.preferred_difficulty_index
                );
            }

            if !(target == CurrentScreen::SelectMusic
                || target == CurrentScreen::PlayerOptions
                || target == CurrentScreen::Gameplay)
            {
                commands.push(Command::StopMusic);
            }
        }

        if prev == CurrentScreen::SelectMusic {
            self.state.session.preferred_difficulty_index = self
                .state
                .screens
                .select_music_state
                .preferred_difficulty_index;
        }
        commands
    }

    fn handle_screen_state_on_fade(&mut self, prev: CurrentScreen, target: CurrentScreen) {
        if prev == CurrentScreen::SelectColor {
            let idx = self.state.screens.select_color_state.active_color_index;
            self.state.screens.menu_state.active_color_index = idx;
            self.state.screens.select_profile_state.active_color_index = idx;
            self.state.screens.select_style_state.active_color_index = idx;
            self.state.screens.select_music_state.active_color_index = idx;
            self.state.screens.options_state.active_color_index = idx;
            self.state.screens.input_state.active_color_index = idx;
            if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                gs.active_color_index = idx;
                gs.player_color = color::simply_love_rgba(idx);
            }
        }

        if target == CurrentScreen::Menu {
            let current_color_index = self.state.screens.menu_state.active_color_index;
            self.state.screens.menu_state = menu::init();
            self.state.screens.menu_state.active_color_index = current_color_index;
        } else if target == CurrentScreen::Options {
            let current_color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.options_state = options::init();
            self.state.screens.options_state.active_color_index = current_color_index;
        } else if target == CurrentScreen::Mappings {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.mappings_state = mappings::init();
            self.state.screens.mappings_state.active_color_index = color_index;
        } else if target == CurrentScreen::SelectProfile {
            let current_color_index = self.state.screens.select_profile_state.active_color_index;
            self.state.screens.select_profile_state = select_profile::init();
            self.state.screens.select_profile_state.active_color_index = current_color_index;
            if prev == CurrentScreen::Menu {
                let p2 = self.state.screens.menu_state.started_by_p2;
                select_profile::set_joined(&mut self.state.screens.select_profile_state, !p2, p2);
            }
        } else if target == CurrentScreen::SelectStyle {
            let current_color_index = self.state.screens.select_style_state.active_color_index;
            self.state.screens.select_style_state = select_style::init();
            self.state.screens.select_style_state.active_color_index = current_color_index;
            self.state.screens.select_style_state.selected_index =
                match profile::get_session_play_style() {
                    profile::PlayStyle::Single => 0,
                    profile::PlayStyle::Versus => 1,
                    profile::PlayStyle::Double => 2,
                };
        } else if target == CurrentScreen::PlayerOptions {
            let (song_arc, chart_steps_index, preferred_difficulty_index) = {
                let sm_state = &self.state.screens.select_music_state;
                let entry = sm_state.entries.get(sm_state.selected_index).unwrap();
                let song = match entry {
                    select_music::MusicWheelEntry::Song(s) => s,
                    _ => panic!("Cannot open player options on a pack header"),
                };
                (
                    song.clone(),
                    sm_state.selected_steps_index,
                    sm_state.preferred_difficulty_index,
                )
            };

            let color_index = self.state.screens.select_music_state.active_color_index;
            self.state.screens.player_options_state = Some(player_options::init(
                song_arc,
                chart_steps_index,
                preferred_difficulty_index,
                color_index,
            ));
        }
    }

    fn handle_screen_entry_on_fade(
        &mut self,
        prev: CurrentScreen,
        target: CurrentScreen,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        if target == CurrentScreen::Gameplay {
            if let Some(po_state) = self.state.screens.player_options_state.take() {
                let song_arc = po_state.song;
                let target_chart_type = profile::get_session_play_style().chart_type();
                let chart_ref = select_music::chart_for_steps_index(
                    &song_arc,
                    target_chart_type,
                    po_state.chart_steps_index,
                )
                .expect("No chart found for selected stepchart");
                let chart = Arc::new(chart_ref.clone());

                // Keep SelectMusic's current stepchart in sync with what we're about to play.
                self.state.screens.select_music_state.preferred_difficulty_index =
                    po_state.chart_difficulty_index;
                self.state.screens.select_music_state.selected_steps_index = po_state.chart_steps_index;

                commands.push(Command::UpdateLastPlayed {
                    music_path: song_arc.music_path.clone(),
                    chart_hash: Some(chart_ref.short_hash.clone()),
                    difficulty_index: po_state.chart_difficulty_index,
                });

                let to_scroll_speed = |m: &player_options::SpeedMod| match m.mod_type.as_str() {
                    "X" => crate::game::scroll::ScrollSpeedSetting::XMod(m.value),
                    "C" => crate::game::scroll::ScrollSpeedSetting::CMod(m.value),
                    "M" => crate::game::scroll::ScrollSpeedSetting::MMod(m.value),
                    _ => crate::game::scroll::ScrollSpeedSetting::default(),
                };
                let scroll_speeds = [
                    to_scroll_speed(&po_state.speed_mod[0]),
                    to_scroll_speed(&po_state.speed_mod[1]),
                ];

                let color_index = po_state.active_color_index;
                let gs = gameplay::init(
                    song_arc,
                    chart,
                    color_index,
                    po_state.music_rate,
                    scroll_speeds,
                    po_state.player_profiles,
                );

                commands.push(Command::SetDynamicBackground(
                    gs.song.background_path.clone(),
                ));
                self.state.screens.gameplay_state = Some(gs);
            } else {
                panic!("Navigating to Gameplay without PlayerOptions state!");
            }
        }

        if target == CurrentScreen::Evaluation {
            let gameplay_results = self.state.screens.gameplay_state.take();
            let color_idx = gameplay_results.as_ref().map_or(
                self.state.screens.evaluation_state.active_color_index,
                |gs| gs.active_color_index,
            );
            self.state.screens.evaluation_state = evaluation::init(gameplay_results);
            self.state.screens.evaluation_state.active_color_index = color_idx;

            let graph_request =
                if let Some(score_info) = &self.state.screens.evaluation_state.score_info {
                    let graph_width = 1024;
                    let graph_height = 256;
                    let bg_color = [16, 21, 25];
                    let top_color = [54, 25, 67];
                    let bottom_color = [38, 84, 91];

                    let graph_data = rssp::graph::generate_density_graph_rgba_data(
                        &score_info.chart.measure_nps_vec,
                        score_info.chart.max_nps,
                        graph_width,
                        graph_height,
                        bottom_color,
                        top_color,
                        bg_color,
                    )
                    .ok();

                    let key = format!("{}_eval", score_info.chart.short_hash);
                    graph_data.map(|data| (key, data))
                } else {
                    None
                };

            commands.push(Command::SetEvaluationGraphData(graph_request));
        }

        if target == CurrentScreen::SelectMusic {
            if self.state.session.session_start_time.is_none() {
                self.state.session.session_start_time = Some(Instant::now());
                info!("Session timer started.");
            }

            match prev {
                CurrentScreen::PlayerOptions => {
                    let preferred = self.state.session.preferred_difficulty_index;
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = preferred;

                    let desired_steps_index = self
                        .state
                        .screens
                        .player_options_state
                        .as_ref()
                        .map_or(preferred, |po| po.chart_steps_index);
                    self.state.screens.select_music_state.selected_steps_index = desired_steps_index;

                    if let Some(select_music::MusicWheelEntry::Song(song)) = self
                        .state
                        .screens
                        .select_music_state
                        .entries
                        .get(self.state.screens.select_music_state.selected_index)
                    {
                        let chart_type = profile::get_session_play_style().chart_type();
                        if select_music::chart_for_steps_index(
                            song,
                            chart_type,
                            desired_steps_index,
                        )
                        .is_none()
                        {
                            let mut best_match_index = None;
                            let mut min_diff = i32::MAX;
                            for i in 0..color::FILE_DIFFICULTY_NAMES.len() {
                                if select_music::is_difficulty_playable(song, i) {
                                    let diff = (i as i32 - preferred as i32).abs();
                                    if diff < min_diff {
                                        min_diff = diff;
                                        best_match_index = Some(i);
                                    }
                                }
                            }
                            if let Some(idx) = best_match_index {
                                self.state.screens.select_music_state.selected_steps_index = idx;
                            }
                        }
                    }

                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                }
                CurrentScreen::Gameplay | CurrentScreen::Evaluation => {
                    select_music::reset_preview_after_gameplay(
                        &mut self.state.screens.select_music_state,
                    );
                }
                _ => {
                    let current_color_index =
                        self.state.screens.select_music_state.active_color_index;
                    self.state.screens.select_music_state = select_music::init();
                    self.state.screens.select_music_state.active_color_index = current_color_index;
                    self.state
                        .screens
                        .select_music_state
                        .selected_steps_index = self.state.session.preferred_difficulty_index;
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = self.state.session.preferred_difficulty_index;

                    // Treat the initial selection as already "settled" so preview/graphs can start
                    // immediately after the transition, matching ITG/Simply Love behavior.
                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                }
            }

            // Prime the delayed panes (tech counts, breakdown, etc.) for the selected chart so they
            // render immediately on entry (no initial debounce delay).
            select_music::prime_displayed_chart_data(&mut self.state.screens.select_music_state);

            // Load the selected entry's banner during the fade-in so it appears immediately.
            let banner_path = match self
                .state
                .screens
                .select_music_state
                .entries
                .get(self.state.screens.select_music_state.selected_index)
            {
                Some(select_music::MusicWheelEntry::Song(song)) => song.banner_path.clone(),
                Some(select_music::MusicWheelEntry::PackHeader { banner_path, .. }) => {
                    banner_path.clone()
                }
                None => None,
            };
            commands.push(Command::SetBanner(banner_path));

            // Pre-render the density graph during the fade-in so the panel isn't blank on entry.
            let chart_to_graph = match self
                .state
                .screens
                .select_music_state
                .entries
                .get(self.state.screens.select_music_state.selected_index)
            {
                Some(select_music::MusicWheelEntry::Song(song)) => {
                    let chart_type = profile::get_session_play_style().chart_type();
                    select_music::chart_for_steps_index(
                        song,
                        chart_type,
                        self.state.screens.select_music_state.selected_steps_index,
                    )
                    .cloned()
                }
                _ => None,
            };
            commands.push(Command::SetDensityGraph {
                slot: DensityGraphSlot::SelectMusicP1,
                chart_opt: chart_to_graph,
            });

            if profile::get_session_play_style() == profile::PlayStyle::Versus {
                let chart_to_graph_p2 = match self
                    .state
                    .screens
                    .select_music_state
                    .entries
                    .get(self.state.screens.select_music_state.selected_index)
                {
                    Some(select_music::MusicWheelEntry::Song(song)) => {
                        let chart_type = profile::get_session_play_style().chart_type();
                        select_music::chart_for_steps_index(
                            song,
                            chart_type,
                            self.state.screens.select_music_state.p2_selected_steps_index,
                        )
                        .cloned()
                    }
                    _ => None,
                };
                commands.push(Command::SetDensityGraph {
                    slot: DensityGraphSlot::SelectMusicP2,
                    chart_opt: chart_to_graph_p2,
                });
            }
        }
        commands
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::GamepadSystem(ev) => {
                if self.state.screens.current_screen == CurrentScreen::Sandbox {
                    crate::screens::sandbox::handle_gamepad_system_event(
                        &mut self.state.screens.sandbox_state,
                        &ev,
                    );
                }
                match &ev {
                    GpSystemEvent::StartupComplete => {
                        self.state.shell.gamepad_startup_complete = true;
                    }
                    GpSystemEvent::Connected {
                        name, id, backend, ..
                    } => {
                        info!(
                            "Gamepad connected: {} (ID: {}) via {:?}",
                            name,
                            usize::from(*id),
                            backend
                        );
                        if self.state.shell.gamepad_startup_complete {
                            self.state.shell.gamepad_overlay_state = Some((
                                format!(
                                    "Connected: {} (ID: {}) via {:?}",
                                    name,
                                    usize::from(*id),
                                    backend
                                ),
                                Instant::now(),
                            ));
                        }
                    }
                    GpSystemEvent::Disconnected {
                        name, id, backend, ..
                    } => {
                        info!(
                            "Gamepad disconnected: {} (ID: {}) via {:?}",
                            name,
                            usize::from(*id),
                            backend
                        );
                        if self.state.shell.gamepad_startup_complete {
                            self.state.shell.gamepad_overlay_state = Some((
                                format!(
                                    "Disconnected: {} (ID: {}) via {:?}",
                                    name,
                                    usize::from(*id),
                                    backend
                                ),
                                Instant::now(),
                            ));
                        }
                    }
                }
            }
            UserEvent::Pad(ev) => {
                if self.state.screens.current_screen == CurrentScreen::Sandbox {
                    crate::screens::sandbox::handle_raw_pad_event(
                        &mut self.state.screens.sandbox_state,
                        &ev,
                    );
                } else if self.state.screens.current_screen == CurrentScreen::Mappings {
                    crate::screens::mappings::handle_raw_pad_event(
                        &mut self.state.screens.mappings_state,
                        &ev,
                    );
                } else if self.state.screens.current_screen == CurrentScreen::Input {
                    crate::screens::input::handle_raw_pad_event(
                        &mut self.state.screens.input_state,
                        &ev,
                    );
                }
                self.handle_pad_event(event_loop, ev);
            }
        }
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            if let Err(e) = self.init_graphics(event_loop) {
                error!("Failed to initialize graphics: {e}");
                event_loop.exit();
            }
            // After all initial loading is complete, start network checks.
            crate::core::network::init();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if window_id != window.id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                info!("Close requested. Shutting down.");
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    self.state.shell.metrics =
                        space::metrics_for_window(new_size.width, new_size.height);
                    space::set_current_metrics(self.state.shell.metrics);
                    if let Some(backend) = &mut self.backend {
                        backend.resize(new_size.width, new_size.height);
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                self.handle_key_event(event_loop, key_event);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let delta_time = now
                    .duration_since(self.state.shell.last_frame_time)
                    .as_secs_f32();
                self.state.shell.last_frame_time = now;
                let total_elapsed = now
                    .duration_since(self.state.shell.start_time)
                    .as_secs_f32();
                crate::ui::runtime::tick(delta_time);

                // --- Manage gamepad overlay lifetime ---
                self.state.shell.update_gamepad_overlay(now);

                let mut finished_fading_out_to: Option<CurrentScreen> = None;

                match &mut self.state.shell.transition {
                    TransitionState::FadingOut {
                        elapsed,
                        duration,
                        target,
                    } => {
                        *elapsed += delta_time;
                        if *elapsed >= *duration {
                            finished_fading_out_to = Some(*target);
                        }
                    }
                    TransitionState::ActorsFadeOut {
                        elapsed,
                        duration,
                        target,
                    } => {
                        *elapsed += delta_time;
                        if *elapsed >= *duration {
                            let target_screen = *target;
                            let prev = self.state.screens.current_screen;
                            self.state.screens.current_screen = target_screen;
                            if target_screen == CurrentScreen::SelectColor {
                                select_color::on_enter(&mut self.state.screens.select_color_state);
                            }

                            // SelectProfile/SelectColor/SelectStyle share the looping menu BGM.
                            // Keep SelectMusic preview playing when moving to/from PlayerOptions.
                            let menu_music_enabled = config::get().menu_music;
                            let target_menu_music = menu_music_enabled
                                && matches!(
                                    target_screen,
                                    CurrentScreen::SelectColor | CurrentScreen::SelectStyle
                                );
                            let prev_menu_music = menu_music_enabled
                                && matches!(
                                    prev,
                                    CurrentScreen::SelectColor | CurrentScreen::SelectStyle
                                );
                            let keep_preview = (prev == CurrentScreen::SelectMusic
                                && target_screen == CurrentScreen::PlayerOptions)
                                || (prev == CurrentScreen::PlayerOptions
                                    && target_screen == CurrentScreen::SelectMusic);

                            if target_menu_music {
                                if !prev_menu_music {
                                    crate::core::audio::play_music(
                                        std::path::PathBuf::from("assets/music/in_two (loop).ogg"),
                                        crate::core::audio::Cut::default(),
                                        true,
                                        1.0,
                                    );
                                }
                            } else if prev_menu_music {
                                crate::core::audio::stop_music();
                            } else if !keep_preview {
                                crate::core::audio::stop_music();
                            }

                            if target_screen == CurrentScreen::Menu {
                                let current_color_index =
                                    self.state.screens.menu_state.active_color_index;
                                self.state.screens.menu_state = menu::init();
                                self.state.screens.menu_state.active_color_index =
                                    current_color_index;
                            } else if target_screen == CurrentScreen::Options {
                                let current_color_index =
                                    self.state.screens.options_state.active_color_index;
                                self.state.screens.options_state = options::init();
                                self.state.screens.options_state.active_color_index =
                                    current_color_index;
                            } else if target_screen == CurrentScreen::SelectProfile {
                                let current_color_index =
                                    self.state.screens.select_profile_state.active_color_index;
                                self.state.screens.select_profile_state = select_profile::init();
                                self.state.screens.select_profile_state.active_color_index =
                                    current_color_index;
                                if prev == CurrentScreen::Menu {
                                    let p2 = self.state.screens.menu_state.started_by_p2;
                                    select_profile::set_joined(
                                        &mut self.state.screens.select_profile_state,
                                        !p2,
                                        p2,
                                    );
                                }
                            } else if target_screen == CurrentScreen::SelectStyle {
                                let current_color_index =
                                    self.state.screens.select_style_state.active_color_index;
                                self.state.screens.select_style_state = select_style::init();
                                self.state.screens.select_style_state.active_color_index =
                                    current_color_index;
                                let both_joined = select_profile::both_players_joined(
                                    &self.state.screens.select_profile_state,
                                );
                                self.state.screens.select_style_state.selected_index = if both_joined
                                {
                                    1 // "2 Players"
                                } else {
                                    match profile::get_session_play_style() {
                                        profile::PlayStyle::Single => 0,
                                        profile::PlayStyle::Versus => 1,
                                        profile::PlayStyle::Double => 2,
                                    }
                                };
                            } else if target_screen == CurrentScreen::Mappings {
                                let color_index =
                                    self.state.screens.options_state.active_color_index;
                                self.state.screens.mappings_state = mappings::init();
                                self.state.screens.mappings_state.active_color_index = color_index;
                            }

                            if prev == CurrentScreen::SelectColor {
                                let idx = self.state.screens.select_color_state.active_color_index;
                                self.state.screens.menu_state.active_color_index = idx;
                                self.state.screens.select_profile_state.active_color_index = idx;
                                self.state.screens.select_style_state.active_color_index = idx;
                                self.state.screens.select_music_state.active_color_index = idx;
                                if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                                    gs.active_color_index = idx;
                                    gs.player_color = color::simply_love_rgba(idx);
                                }
                                self.state.screens.options_state.active_color_index = idx;
                            }

                            if target_screen == CurrentScreen::Options {
                                self.update_options_monitor_specs(event_loop);
                            }

                            self.state.shell.transition = if Self::is_actor_fade_screen(target_screen) {
                                TransitionState::ActorsFadeIn { elapsed: 0.0 }
                            } else {
                                TransitionState::Idle
                            };
                            crate::ui::runtime::clear_all();
                        }
                    }
                    TransitionState::FadingIn { elapsed, duration } => {
                        *elapsed += delta_time;
                        if *elapsed >= *duration {
                            self.state.shell.transition = TransitionState::Idle;
                        }
                    }
                    TransitionState::ActorsFadeIn { elapsed } => {
                        *elapsed += delta_time;
                        if *elapsed >= MENU_ACTORS_FADE_DURATION {
                            self.state.shell.transition = TransitionState::Idle;
                        }
                    }
                    TransitionState::Idle => {
                        if let Some(action) =
                            self.state
                                .screens
                                .step_idle(delta_time, now, &self.state.session)
                            && !matches!(action, ScreenAction::None) {
                                let _ = self.handle_action(action, event_loop);
                            }
                    }
                }

                if let Some(target) = finished_fading_out_to {
                    self.on_fade_complete(target, event_loop);
                }

                if self.window.as_ref().map(|w| w.id()) != Some(window_id) {
                    return;
                }

                let (actors, clear_color) = self.get_current_actors();
                let screen = self.build_screen(&actors, clear_color, total_elapsed);
                self.update_fps_title(&window, now);

                if let Some(backend) = &mut self.backend {
                    match backend.draw(&screen, &self.asset_manager.textures) {
                        Ok(vpf) => self.state.shell.current_frame_vpf = vpf,
                        Err(e) => {
                            error!("Failed to draw frame: {e}");
                            event_loop.exit();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(backend) = &mut self.backend {
            self.asset_manager.destroy_dynamic_assets(backend);
            backend.dispose_textures(&mut self.asset_manager.textures);
            backend.cleanup();
        }
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::get();
    let backend_type = config.video_renderer;
    let show_stats = config.show_stats;
    let color_index = config.simply_love_color;
    let profile_data = profile::get();

    song_loading::scan_and_load_songs("songs");
    song_loading::scan_and_load_courses("courses", "songs");
    let event_loop: EventLoop<UserEvent> = EventLoop::<UserEvent>::with_user_event().build()?;

    // Spawn background thread to pump pad input and emit user events; decoupled from frame rate.
    let proxy: EventLoopProxy<UserEvent> = event_loop.create_proxy();
    std::thread::spawn(move || {
        let proxy_pad = proxy.clone();
        input::run_pad_backend(
            move |pe| {
                let _ = proxy_pad.send_event(UserEvent::Pad(pe));
            },
            move |se| {
                let _ = proxy.send_event(UserEvent::GamepadSystem(se));
            },
        );
    });

    let mut app = App::new(backend_type, show_stats, color_index, config, profile_data);
    event_loop.run_app(&mut app)?;
    Ok(())
}
