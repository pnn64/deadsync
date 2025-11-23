use crate::core::gfx::{self as renderer, create_backend, BackendType, RenderList};
use crate::core::input::{self, InputEvent};
use crate::core::space::{self as space, Metrics};
use crate::game::{profile, scores, scroll::ScrollSpeedSetting};
use crate::assets::AssetManager;
use crate::ui::color;
use crate::screens::{gameplay, menu, options, init, select_color, select_music, sandbox, evaluation, player_options, Screen as CurrentScreen, ScreenAction};
use crate::game::parsing::simfile as song_loading;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::Window,
};

use log::{error, warn, info};
use std::{error::Error, sync::Arc, time::Instant};

use crate::ui::actors::Actor;
/* -------------------- gamepad -------------------- */
use crate::core::input::{self as gamepad};
use crate::core::input::{GpSystemEvent, PadEvent};

/* -------------------- user events -------------------- */
#[derive(Debug, Clone)]
pub enum UserEvent {
    Pad(PadEvent),
    GamepadSystem(GpSystemEvent),
}

/* -------------------- transition timing constants -------------------- */
const FADE_OUT_DURATION: f32 = 0.4;
const MENU_TO_SELECT_COLOR_OUT_DURATION: f32 = 1.0;
const MENU_ACTORS_FADE_DURATION: f32 = 0.65;

/* -------------------- transition state machine -------------------- */
#[derive(Debug)]
enum TransitionState {
    Idle,
    FadingOut { elapsed: f32, duration: f32, target: CurrentScreen },
    FadingIn  { elapsed: f32, duration: f32 },
    ActorsFadeOut { elapsed: f32, duration: f32, target: CurrentScreen },
    ActorsFadeIn { elapsed: f32 },
}

pub struct App {
    window: Option<Arc<Window>>,
    backend: Option<renderer::Backend>,
    backend_type: BackendType,
    asset_manager: AssetManager,
    current_screen: CurrentScreen,
    menu_state: menu::State,
    gameplay_state: Option<gameplay::State>,
    options_state: options::State,
    player_options_state: Option<player_options::State>,
    frame_count: u32,
    last_title_update: Instant,
    last_frame_time: Instant,
    start_time: Instant,
    vsync_enabled: bool,
    fullscreen_enabled: bool,
    metrics: Metrics,
    last_fps: f32,
    last_vpf: u32,
    current_frame_vpf: u32,
    show_overlay: bool,
    transition: TransitionState,
    init_state: init::State,
    select_color_state: select_color::State,
    select_music_state: select_music::State,
    preferred_difficulty_index: usize,
    sandbox_state: sandbox::State,
    evaluation_state: evaluation::State,
    session_start_time: Option<Instant>,
    display_width: u32,
    display_height: u32,
    gamepad_overlay_state: Option<(String, Instant)>,
    pending_exit: bool,
    shift_held: bool,
}

impl App {
    fn new(
        backend_type: BackendType,
        vsync_enabled: bool,
        fullscreen_enabled: bool,
        show_overlay: bool,
        color_index: i32,
    ) -> Self {
        let config = crate::config::get();
        let display_width = config.display_width;
        let display_height = config.display_height;

        let mut menu_state = menu::init();
        menu_state.active_color_index = color_index;

        let mut select_color_state = select_color::init();
        select_color_state.active_color_index = color_index;
        select_color_state.scroll = color_index as f32;
        select_color_state.bg_from_index = color_index;
        select_color_state.bg_to_index = color_index;

        let mut select_music_state = select_music::init();
        select_music_state.active_color_index = color_index;

        let mut options_state = options::init();
        options_state.active_color_index = color_index;
        
        let mut init_state = init::init();
        init_state.active_color_index = color_index;

        let mut evaluation_state = evaluation::init(None);
        evaluation_state.active_color_index = color_index;

        Self {
            window: None,
            backend: None,
            backend_type,
            asset_manager: AssetManager::new(),
            current_screen: CurrentScreen::Init,
            init_state,
            menu_state,
            gameplay_state: None,
            options_state,
            player_options_state: None,
            select_color_state,
            select_music_state,
            sandbox_state: sandbox::init(),
            evaluation_state,
            frame_count: 0,
            last_title_update: Instant::now(),
            last_frame_time: Instant::now(),
            start_time: Instant::now(),
            metrics: space::metrics_for_window(display_width, display_height),
            preferred_difficulty_index: 2, // Default to Medium
            vsync_enabled,
            fullscreen_enabled,
            show_overlay,
            last_fps: 0.0,
            last_vpf: 0,
            current_frame_vpf: 0,
            transition: TransitionState::Idle,
            session_start_time: None,
            display_width,
            display_height,
            gamepad_overlay_state: None,
            pending_exit: false,
            shift_held: false,
        }
    }

    fn route_input_event(&mut self, event_loop: &ActiveEventLoop, ev: InputEvent) -> Result<(), Box<dyn Error>> {
        let action = match self.current_screen {
            CurrentScreen::Menu => crate::screens::menu::handle_input(&mut self.menu_state, &ev),
            CurrentScreen::SelectColor => crate::screens::select_color::handle_input(&mut self.select_color_state, &ev),
            CurrentScreen::Options => crate::screens::options::handle_input(&mut self.options_state, &ev),
            CurrentScreen::SelectMusic => crate::screens::select_music::handle_input(&mut self.select_music_state, &ev),
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &mut self.player_options_state { crate::screens::player_options::handle_input(pos, &ev) } else { ScreenAction::None }
            }
            CurrentScreen::Evaluation => crate::screens::evaluation::handle_input(&mut self.evaluation_state, &ev),
            CurrentScreen::Sandbox => crate::screens::sandbox::handle_input(&mut self.sandbox_state, &ev),
            CurrentScreen::Init => crate::screens::init::handle_input(&mut self.init_state, &ev),
            CurrentScreen::Gameplay => {
                if let Some(gs) = &mut self.gameplay_state {
                    crate::game::gameplay::handle_input(gs, &ev)
                } else { ScreenAction::None }
            }
        };
        if let ScreenAction::None = action { return Ok(()); }
        self.handle_action(action, event_loop)
    }

    fn handle_action(&mut self, action: ScreenAction, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        match action {
            ScreenAction::Navigate(screen) => {
                let from = self.current_screen;
                let to = screen;

                // Persist any pending global offset changes when leaving Gameplay.
                if from == CurrentScreen::Gameplay && to != CurrentScreen::Gameplay {
                    if let Some(gs) = &self.gameplay_state {
                        if (gs.global_offset_seconds - gs.initial_global_offset_seconds).abs() > f32::EPSILON {
                            crate::config::update_global_offset(gs.global_offset_seconds);
                        }
                    }
                }

                if from == CurrentScreen::Init && to == CurrentScreen::Menu {
                    info!("Instant navigation Init→Menu (out-transition handled by Init screen)");
                    self.current_screen = screen;
                    self.transition = TransitionState::ActorsFadeIn { elapsed: 0.0 };
                    crate::ui::runtime::clear_all();
                    return Ok(());
                }

                if matches!(self.transition, TransitionState::Idle) {
                    // Any new navigation cancels a pending exit.
                    self.pending_exit = false;
                    let is_actor_only_fade =
                        (from == CurrentScreen::Menu &&
                            (to == CurrentScreen::Options || to == CurrentScreen::SelectColor)) ||
                        ((from == CurrentScreen::Options || from == CurrentScreen::SelectColor) && to == CurrentScreen::Menu);

                    if is_actor_only_fade {
                        info!("Starting actor-only fade out to screen: {:?}", screen);
                        let duration = if from == CurrentScreen::Menu && (to == CurrentScreen::SelectColor || to == CurrentScreen::Options) {
                            MENU_TO_SELECT_COLOR_OUT_DURATION
                        } else {
                            FADE_OUT_DURATION
                        };
                        self.transition = TransitionState::ActorsFadeOut { elapsed: 0.0, duration, target: screen };
                    } else {
                        info!("Starting global fade out to screen: {:?}", screen);                        
                        let (_, out_duration) = self.get_out_transition_for_screen(self.current_screen);
                        self.transition = TransitionState::FadingOut {
                            elapsed: 0.0,
                            duration: out_duration,
                            target: screen,
                        };
                    }
                }
            }
            ScreenAction::Exit => {
                if self.current_screen == CurrentScreen::Menu && matches!(self.transition, TransitionState::Idle) {
                    info!("Exit requested from Menu; playing menu out-transition before shutdown.");
                    let (_, out_duration) = self.get_out_transition_for_screen(self.current_screen);
                    self.transition = TransitionState::FadingOut {
                        elapsed: 0.0,
                        duration: out_duration,
                        target: self.current_screen,
                    };
                    self.pending_exit = true;
                } else {
                    info!("Exit action received. Shutting down.");
                    event_loop.exit();
                }
            }
            ScreenAction::RequestBanner(_) => {}
            ScreenAction::RequestDensityGraph(_) => {}
            ScreenAction::FetchOnlineGrade(hash) => {
                info!("Fetching online grade for chart hash: {}", hash);
                let profile = profile::get();
                std::thread::spawn(move || {
                    if let Err(e) = scores::fetch_and_store_grade(profile, hash) {
                        warn!("Failed to fetch online grade: {}", e);
                    }
                });
            }
            ScreenAction::None => {}
        }
        Ok(())
    }

    fn build_screen(&self, actors: &[Actor], clear_color: [f32; 4], total_elapsed: f32) -> RenderList {
        self.asset_manager.with_fonts(|fonts| {
            crate::ui::compose::build_screen(actors, clear_color, &self.metrics, fonts, total_elapsed)
        })
    }

    fn get_current_actors(&self) -> (Vec<Actor>, [f32; 4]) {
        const CLEAR: [f32; 4] = [0.03, 0.03, 0.03, 1.0];
        let mut screen_alpha_multiplier = 1.0;

        let is_actor_fade_screen = matches!(self.current_screen, CurrentScreen::Menu | CurrentScreen::Options | CurrentScreen::SelectColor);

        if is_actor_fade_screen {
            match self.transition {
                TransitionState::ActorsFadeIn { elapsed } => {
                    screen_alpha_multiplier = (elapsed / MENU_ACTORS_FADE_DURATION).clamp(0.0, 1.0);
                },
                TransitionState::ActorsFadeOut { elapsed, duration, .. } => {
                    screen_alpha_multiplier = 1.0 - (elapsed / duration).clamp(0.0, 1.0);
                },
                _ => {},
            }
        }

        let mut actors = match self.current_screen {
            CurrentScreen::Menu     => menu::get_actors(&self.menu_state, screen_alpha_multiplier),
            CurrentScreen::Gameplay => {
                if let Some(gs) = &self.gameplay_state {
                    gameplay::get_actors(gs, &self.asset_manager)
                } else { vec![] }
            },
            CurrentScreen::Options  => options::get_actors(&self.options_state, screen_alpha_multiplier),
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &self.player_options_state {
                    player_options::get_actors(pos, &self.asset_manager)
                } else { vec![] }
            },
            CurrentScreen::SelectColor => select_color::get_actors(&self.select_color_state, screen_alpha_multiplier),
            CurrentScreen::SelectMusic => select_music::get_actors(&self.select_music_state, &self.asset_manager),
            CurrentScreen::Sandbox  => sandbox::get_actors(&self.sandbox_state),
            CurrentScreen::Init     => init::get_actors(&self.init_state),
            CurrentScreen::Evaluation => evaluation::get_actors(&self.evaluation_state, &self.asset_manager),
        };

        if self.show_overlay {
            let overlay = crate::ui::components::stats_overlay::build(self.backend_type, self.last_fps, self.last_vpf);
            actors.extend(overlay);
        }

        // Gamepad connection overlay (always on top of screen, but below transitions)
        if let Some((msg, _)) = &self.gamepad_overlay_state {
            let params = crate::ui::components::gamepad_overlay::Params { message: msg };
            actors.extend(crate::ui::components::gamepad_overlay::build(params));
        }

        match &self.transition {
            TransitionState::FadingOut { .. } => {
                let (out_actors, _) = self.get_out_transition_for_screen(self.current_screen);
                actors.extend(out_actors);
            }
            TransitionState::ActorsFadeOut { target, .. } => {
                // Special case: Menu → SelectColor / Menu → Options should keep the heart
                // background bright and only fade UI, but still play the hearts splash.
                if self.current_screen == CurrentScreen::Menu
                    && (*target == CurrentScreen::SelectColor || *target == CurrentScreen::Options)
                {
                    let splash = crate::ui::components::menu_splash::build(self.menu_state.active_color_index);
                    actors.extend(splash);
                }
            }
            TransitionState::FadingIn { .. } => {
                let (in_actors, _) = self.get_in_transition_for_screen(self.current_screen);
                actors.extend(in_actors);
            }
            _ => {}
        }

        (actors, CLEAR)
    }
    
    fn get_out_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        match screen {
            CurrentScreen::Menu => menu::out_transition(self.menu_state.active_color_index),
            CurrentScreen::Gameplay => gameplay::out_transition(),
            CurrentScreen::Options => options::out_transition(),
            CurrentScreen::PlayerOptions => player_options::out_transition(),
            CurrentScreen::SelectColor => select_color::out_transition(),
            CurrentScreen::SelectMusic => select_music::out_transition(),
            CurrentScreen::Sandbox => sandbox::out_transition(),
            CurrentScreen::Init => init::out_transition(),
            CurrentScreen::Evaluation => evaluation::out_transition(),
        }
    }

    fn get_in_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        match screen {
            CurrentScreen::Menu => menu::in_transition(),
            CurrentScreen::Gameplay => gameplay::in_transition(),
            CurrentScreen::Options => options::in_transition(),
            CurrentScreen::PlayerOptions => player_options::in_transition(),
            CurrentScreen::SelectColor => select_color::in_transition(),
            CurrentScreen::SelectMusic => select_music::in_transition(),
            CurrentScreen::Sandbox => sandbox::in_transition(),
            CurrentScreen::Evaluation => evaluation::in_transition(),
            CurrentScreen::Init => (vec![], 0.0),
        }
    }

    #[inline(always)]
    fn update_fps_title(&mut self, window: &Window, now: Instant) {
        self.frame_count += 1;
        let elapsed = now.duration_since(self.last_title_update);
        if elapsed.as_secs_f32() >= 1.0 {
            let fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.last_fps = fps;
            self.last_vpf = self.current_frame_vpf;
            let screen_name = format!("{:?}", self.current_screen);
            window.set_title(&format!("DeadSync - {:?} | {} | {:.2} FPS", self.backend_type, screen_name, fps));
            self.frame_count = 0;
            self.last_title_update = now;
        }
    }

    fn init_graphics(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        let mut window_attributes = Window::default_attributes()
            .with_title(format!("DeadSync - {:?}", self.backend_type))
            .with_resizable(true)
            .with_transparent(false);

        let window_width = self.display_width;
        let window_height = self.display_height;

        if self.fullscreen_enabled {
            let fullscreen = if let Some(mon) = event_loop.primary_monitor() {
                let best_mode = mon.video_modes()
                    .filter(|m| { let sz = m.size(); sz.width == window_width && sz.height == window_height })
                    .max_by_key(|m| m.refresh_rate_millihertz());
                if let Some(mode) = best_mode {
                    log::info!("Fullscreen: using EXCLUSIVE {}x{} @ {} mHz", window_width, window_height, mode.refresh_rate_millihertz());
                    Some(winit::window::Fullscreen::Exclusive(mode))
                } else {
                    log::warn!("No exact EXCLUSIVE mode {}x{}; using BORDERLESS.", window_width, window_height);
                    Some(winit::window::Fullscreen::Borderless(Some(mon)))
                }
            } else {
                log::warn!("No primary monitor reported; using BORDERLESS fullscreen.");
                Some(winit::window::Fullscreen::Borderless(None))
            };
            window_attributes = window_attributes.with_fullscreen(fullscreen);
        } else {
            window_attributes = window_attributes.with_inner_size(PhysicalSize::new(window_width, window_height));
        }

        let window = Arc::new(event_loop.create_window(window_attributes)?);
        // Re-assert the opaque hint so compositors do not apply alpha-based blending.
        window.set_transparent(false);
        let sz = window.inner_size();
        self.metrics = crate::core::space::metrics_for_window(sz.width, sz.height);
        crate::core::space::set_current_metrics(self.metrics);
        let mut backend = create_backend(self.backend_type, window.clone(), self.vsync_enabled)?;
        
        self.asset_manager.load_initial_assets(&mut backend)?;

        self.window = Some(window);
        self.backend = Some(backend);
        info!("Starting event loop...");
        Ok(())
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
                    self.shift_held = key_event.state == ElementState::Pressed;
                }
                _ => {}
            }
        }

        if self.current_screen == CurrentScreen::Sandbox {
            let action = crate::screens::sandbox::handle_raw_key_event(&mut self.sandbox_state, &key_event);
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Sandbox raw key action: {}", e);
                }
                return;
            }
        } else if self.current_screen == CurrentScreen::Menu {
            let action = crate::screens::menu::handle_raw_key_event(&mut self.menu_state, &key_event);
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle Menu raw key action: {}", e);
                }
                return;
            }
        } else if self.current_screen == CurrentScreen::SelectMusic {
            // Route screen-specific raw key handling (e.g., F7 fetch) to the screen
            let action = crate::screens::select_music::handle_raw_key_event(&mut self.select_music_state, &key_event);
            if !matches!(action, ScreenAction::None) {
                if let Err(e) = self.handle_action(action, event_loop) {
                    log::error!("Failed to handle SelectMusic raw key action: {}", e);
                }
                return;
            }
        } else if self.current_screen == CurrentScreen::Gameplay {
            if let Some(gs) = &mut self.gameplay_state {
                let action = crate::game::gameplay::handle_raw_key_event(gs, &key_event, self.shift_held);
                if !matches!(action, ScreenAction::None) {
                    if let Err(e) = self.handle_action(action, event_loop) {
                        log::error!("Failed to handle Gameplay raw key action: {}", e);
                    }
                    return;
                }
            }
        }
        let is_transitioning = !matches!(self.transition, TransitionState::Idle);
        let _event_timestamp = Instant::now();

        if key_event.state == winit::event::ElementState::Pressed {
            if let winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F3) = key_event.physical_key {
                self.show_overlay = !self.show_overlay;
                log::info!("Overlay {}", if self.show_overlay { "ON" } else { "OFF" });
            }
            // Screen-specific Escape handling resides in per-screen raw handlers now
        }

        if is_transitioning { return; }

        for ev in input::map_key_event(&key_event) {
            if let Err(e) = self.route_input_event(event_loop, ev) {
                log::error!("Failed to handle input: {}", e);
                event_loop.exit();
                return;
            }
        }
    }

    /* -------------------- pad event routing -------------------- */

    #[inline(always)]
    fn handle_pad_event(&mut self, event_loop: &ActiveEventLoop, ev: PadEvent) {
        let is_transitioning = !matches!(self.transition, TransitionState::Idle);
        if is_transitioning || self.current_screen == CurrentScreen::Init {
            return;
        }
        for iev in input::map_pad_event(&ev) {
            if let Err(e) = self.route_input_event(event_loop, iev) {
                error!("Failed to handle pad input: {}", e);
                event_loop.exit();
                return;
            }
        }
    }

    // legacy virtual-action dispatcher removed; screens own their input

    #[cfg(any())]
    #[inline(always)]
    fn poll_gamepad_and_dispatch(&mut self, _event_loop: &ActiveEventLoop) {}
}

impl ApplicationHandler<UserEvent> for App {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::GamepadSystem(ev) => {
                let msg = match ev {
                    GpSystemEvent::Connected { name, id } => {
                        info!("Gamepad connected: {} (ID: {})", name, usize::from(id));
                        format!("Connected: {} (ID: {})", name, usize::from(id))
                    }
                    GpSystemEvent::Disconnected { name, id } => {
                        info!("Gamepad disconnected: {} (ID: {})", name, usize::from(id));
                        format!("Disconnected: {} (ID: {})", name, usize::from(id))
                    }
                };
                self.gamepad_overlay_state = Some((msg, Instant::now()));
            }
            UserEvent::Pad(ev) => {
                if self.current_screen == CurrentScreen::Sandbox {
                    crate::screens::sandbox::handle_raw_pad_event(&mut self.sandbox_state, &ev);
                }
                self.handle_pad_event(event_loop, ev);
            }
        }
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            if let Err(e) = self.init_graphics(event_loop) {
                error!("Failed to initialize graphics: {}", e);
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
        let Some(window) = self.window.as_ref().cloned() else { return; };
        if window_id != window.id() { return; }

        match event {
            WindowEvent::CloseRequested => {
                info!("Close requested. Shutting down.");
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    self.metrics = space::metrics_for_window(new_size.width, new_size.height);
                    space::set_current_metrics(self.metrics);
                    if let Some(backend) = &mut self.backend {
                        backend.resize(new_size.width, new_size.height);
                    }
                }
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                self.handle_key_event(event_loop, key_event);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let delta_time = now.duration_since(self.last_frame_time).as_secs_f32();
                self.last_frame_time = now;
                let total_elapsed = now.duration_since(self.start_time).as_secs_f32();
                crate::ui::runtime::tick(delta_time);

                // --- Manage gamepad overlay lifetime ---
                if let Some((_, start_time)) = self.gamepad_overlay_state {
                    // Corresponds to the animation durations in gamepad_overlay.rs
                    const HOLD_DURATION: f32 = 3.33;
                    const FADE_OUT_DURATION: f32 = 0.25;
                    const TOTAL_DURATION: f32 = HOLD_DURATION + FADE_OUT_DURATION;
                    if now.duration_since(start_time).as_secs_f32() > TOTAL_DURATION {
                        self.gamepad_overlay_state = None;
                    }
                }

                let mut finished_fading_out_to: Option<CurrentScreen> = None;

                match &mut self.transition {
                    TransitionState::FadingOut { elapsed, duration, target } => {
                        *elapsed += delta_time;
                        if *elapsed >= *duration {
                            finished_fading_out_to = Some(*target);
                        }
                    }
                    TransitionState::ActorsFadeOut { elapsed, duration, target } => {
                        *elapsed += delta_time;
                        if *elapsed >= *duration {
                            let prev = self.current_screen;
                            self.current_screen = *target;
                            // Only SelectColor has its own looping BGM; keep SelectMusic preview
                            // playing when moving to/from PlayerOptions.
                            if *target == CurrentScreen::SelectColor {
                                crate::core::audio::play_music(
                                    std::path::PathBuf::from("assets/music/in_two (loop).ogg"),
                                    crate::core::audio::Cut::default(),
                                    true,
                                    1.0,
                                );
                            } else if !((prev == CurrentScreen::SelectMusic && *target == CurrentScreen::PlayerOptions)
                                || (prev == CurrentScreen::PlayerOptions && *target == CurrentScreen::SelectMusic)) {
                                crate::core::audio::stop_music();
                            }

                            if *target == CurrentScreen::Menu {
                                let current_color_index = self.menu_state.active_color_index;
                                self.menu_state = menu::init();
                                self.menu_state.active_color_index = current_color_index;
                            } else if *target == CurrentScreen::Options {
                                let current_color_index = self.options_state.active_color_index;
                                self.options_state = options::init();
                                self.options_state.active_color_index = current_color_index;
                            }

                            if prev == CurrentScreen::SelectColor {
                                let idx = self.select_color_state.active_color_index;
                                self.menu_state.active_color_index = idx;
                                self.select_music_state.active_color_index = idx;
                                if let Some(gs) = self.gameplay_state.as_mut() {
                                    gs.active_color_index = idx;
                                    gs.player_color = color::simply_love_rgba(idx);
                                }
                                self.options_state.active_color_index = idx;
                            }

                            self.transition = TransitionState::ActorsFadeIn { elapsed: 0.0 };
                            crate::ui::runtime::clear_all();
                        }
                    }
                    TransitionState::FadingIn { elapsed, duration } => {
                        *elapsed += delta_time;
                        if *elapsed >= *duration {
                            self.transition = TransitionState::Idle;
                        }
                    }
                    TransitionState::ActorsFadeIn { elapsed } => {
                        *elapsed += delta_time;
                        if *elapsed >= MENU_ACTORS_FADE_DURATION {
                            self.transition = TransitionState::Idle;
                        }
                    }
                    TransitionState::Idle => {
                        match self.current_screen {
                            CurrentScreen::Gameplay => if let Some(gs) = &mut self.gameplay_state {
                                let action = gameplay::update(gs, delta_time);
                                if let ScreenAction::Navigate(_) | ScreenAction::Exit = action.clone() {
                                    if self.handle_action(action, event_loop).is_err() {}
                                }
                            },
                            CurrentScreen::Init => {
                                let action = init::update(&mut self.init_state, delta_time);
                                if let ScreenAction::Navigate(_) | ScreenAction::Exit = action.clone() {
                                    if self.handle_action(action, event_loop).is_err() {}
                                }
                            }
                            CurrentScreen::Options => {
                                options::update(&mut self.options_state, delta_time);
                            }
                            CurrentScreen::PlayerOptions => {
                                if let Some(pos) = &mut self.player_options_state {
                                    player_options::update(pos, delta_time);
                                }
                            }
                            CurrentScreen::Sandbox => sandbox::update(&mut self.sandbox_state, delta_time),
                            CurrentScreen::SelectColor => select_color::update(&mut self.select_color_state, delta_time),
                            CurrentScreen::Evaluation => {
                                if let Some(start) = self.session_start_time {
                                    self.evaluation_state.session_elapsed = now.duration_since(start).as_secs_f32();
                                }
                                evaluation::update(&mut self.evaluation_state, delta_time);
                            },
                            CurrentScreen::SelectMusic => {
                                if let Some(start) = self.session_start_time {
                                    self.select_music_state.session_elapsed = now.duration_since(start).as_secs_f32();
                                }
                                let action = select_music::update(&mut self.select_music_state, delta_time);
                                if let Some(backend) = self.backend.as_mut() {
                                    match action {
                                        ScreenAction::RequestBanner(path_opt) => {
                                            if let Some(path) = path_opt {
                                                let key = self.asset_manager.set_dynamic_banner(backend, Some(path));
                                                self.select_music_state.current_banner_key = key;
                                            } else {
                                                self.asset_manager.destroy_dynamic_assets(backend);
                                                let color_index = self.select_music_state.active_color_index;
                                                let banner_num = color_index.rem_euclid(12) + 1;
                                                let key = format!("banner{}.png", banner_num);
                                                self.select_music_state.current_banner_key = key;
                                            }
                                        }
                                        ScreenAction::RequestDensityGraph(chart_opt) => {
                                            let graph_request = if let Some(chart) = chart_opt {
                                                let graph_width = 1024;
                                                let graph_height = 256;
                                                let bottom_color = [0, 184, 204];
                                                let top_color    = [130, 0, 161];
                                                let bg_color     = [30, 40, 47];

                                                let graph_data = rssp::graph::generate_density_graph_rgba_data(
                                                    &chart.measure_nps_vec,
                                                    chart.max_nps,
                                                    graph_width, graph_height,
                                                    bottom_color,
                                                    top_color,
                                                    bg_color,
                                                ).ok();
                                                
                                                graph_data.map(|data| (chart.short_hash, data))
                                            } else {
                                                None
                                            };

                                            let key = self.asset_manager.set_density_graph(backend, graph_request);
                                            self.select_music_state.current_graph_key = key;
                                        }
                                        _ => { let _ = self.handle_action(action, event_loop); },
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }

                if let Some(target) = finished_fading_out_to {
                    if self.pending_exit {
                        info!("Fade-out complete; exiting application.");
                        event_loop.exit();
                        return;
                    }
                    let prev = self.current_screen;
                    self.current_screen = target;
                    // Only SelectColor has looping BGM; keep SelectMusic preview when moving
                    // between SelectMusic and PlayerOptions.
                    if target == CurrentScreen::SelectColor {
                        crate::core::audio::play_music(
                            std::path::PathBuf::from("assets/music/in_two (loop).ogg"),
                            crate::core::audio::Cut::default(),
                            true,
                            1.0,
                        );
                    } else if !((prev == CurrentScreen::SelectMusic && target == CurrentScreen::PlayerOptions)
                        || (prev == CurrentScreen::PlayerOptions && target == CurrentScreen::SelectMusic)) {
                        crate::core::audio::stop_music();
                    }
                    
                    // When leaving gameplay, stop music and unload the dynamic background
                    if prev == CurrentScreen::Gameplay { 
                        crate::core::audio::stop_music();
                        if let Some(backend) = self.backend.as_mut() {
                            self.asset_manager.set_dynamic_background(backend, None);
                        }
                    }

                    if prev == CurrentScreen::SelectMusic || prev == CurrentScreen::PlayerOptions {
                        // When leaving PlayerOptions, persist any user-chosen settings
                        if prev == CurrentScreen::PlayerOptions {
                            if let Some(po_state) = &self.player_options_state {
                                // Save speed mod to profile
                                let setting = match po_state.speed_mod.mod_type.as_str() {
                                    "C" => Some(ScrollSpeedSetting::CMod(po_state.speed_mod.value)),
                                    "X" => Some(ScrollSpeedSetting::XMod(po_state.speed_mod.value)),
                                    "M" => Some(ScrollSpeedSetting::MMod(po_state.speed_mod.value)),
                                    _ => None,
                                };

                                if let Some(setting) = setting {
                                    profile::update_scroll_speed(setting);
                                    info!("Saved scroll speed: {}", setting);
                                } else {
                                    warn!(
                                        "Unsupported speed mod '{}' not saved to profile.",
                                        po_state.speed_mod.mod_type
                                    );
                                }

                                // Persist session music rate
                                crate::game::profile::set_session_music_rate(po_state.music_rate);
                                info!("Session music rate set to {:.2}x", po_state.music_rate);

                                // Reflect difficulty changes back to SelectMusic
                                self.preferred_difficulty_index = po_state.chart_difficulty_index;
                                info!("Updated preferred difficulty index to {} from PlayerOptions", self.preferred_difficulty_index);
                            }
                        }
                        // Keep preview alive when returning to SelectMusic/PlayerOptions.
                        if !(target == CurrentScreen::SelectMusic || target == CurrentScreen::PlayerOptions) {
                            crate::core::audio::stop_music();
                        }
                    }

                    if prev == CurrentScreen::SelectMusic {
                        self.preferred_difficulty_index = self.select_music_state.preferred_difficulty_index;
                    }

                    if prev == CurrentScreen::SelectColor {
                        let idx = self.select_color_state.active_color_index;
                        self.menu_state.active_color_index = idx;
                        self.select_music_state.active_color_index = idx;
                        self.options_state.active_color_index = idx;
                        if let Some(gs) = self.gameplay_state.as_mut() {
                            gs.active_color_index = idx;
                            gs.player_color = color::simply_love_rgba(idx);
                        }
                    }

                    if target == CurrentScreen::Menu {
                        let current_color_index = self.menu_state.active_color_index;
                        self.menu_state = menu::init();
                        self.menu_state.active_color_index = current_color_index;
                    } else if target == CurrentScreen::Options {
                        let current_color_index = self.options_state.active_color_index;
                        self.options_state = options::init();
                        self.options_state.active_color_index = current_color_index;
                    } else if target == CurrentScreen::PlayerOptions {
                        let (song_arc, chart_difficulty_index) = {
                            let sm_state = &self.select_music_state;
                            let entry = sm_state.entries.get(sm_state.selected_index).unwrap();
                            let song = match entry {
                                select_music::MusicWheelEntry::Song(s) => s,
                                _ => panic!("Cannot open player options on a pack header"),
                            };
                            (song.clone(), sm_state.selected_difficulty_index)
                        };
                        
                        let color_index = self.select_music_state.active_color_index;
                        self.player_options_state = Some(player_options::init(song_arc, chart_difficulty_index, color_index));
                    }

                    if target == CurrentScreen::Gameplay {
                        if let Some(po_state) = self.player_options_state.take() {
                            let song_arc = po_state.song;
                            let chart_difficulty_index = po_state.chart_difficulty_index;
                            let difficulty_name = color::FILE_DIFFICULTY_NAMES[chart_difficulty_index];
                            // Prefer a dance-single chart for the selected difficulty; fall back to any matching difficulty.
                            let chart_ref = song_arc
                                .charts
                                .iter()
                                .find(|c| c.chart_type.eq_ignore_ascii_case("dance-single") && c.difficulty.eq_ignore_ascii_case(difficulty_name))
                                .or_else(|| song_arc.charts.iter().find(|c| c.difficulty.eq_ignore_ascii_case(difficulty_name)))
                                .expect("No chart found for selected difficulty");
                            let chart = Arc::new(chart_ref.clone());

                            let color_index = po_state.active_color_index;
                            let mut gs = gameplay::init(song_arc, chart, color_index, po_state.music_rate);
                            
                            if let Some(backend) = self.backend.as_mut() {
                                gs.background_texture_key = self.asset_manager.set_dynamic_background(backend, gs.song.background_path.clone());
                            }
                            self.gameplay_state = Some(gs);
                        } else {
                            panic!("Navigating to Gameplay without PlayerOptions state!");
                        }
                    }

                    if target == CurrentScreen::Evaluation {
                        let gameplay_results = self.gameplay_state.take();
                        let color_idx = gameplay_results.as_ref().map_or(
                            self.evaluation_state.active_color_index,
                            |gs| gs.active_color_index
                        );
                        self.evaluation_state = evaluation::init(gameplay_results);
                        self.evaluation_state.active_color_index = color_idx;

                        if let Some(backend) = self.backend.as_mut() {
                            let graph_request = if let Some(score_info) = &self.evaluation_state.score_info {
                                 let graph_width = 1024;
                                 let graph_height = 256;
                                 let bg_color     = [16, 21, 25];
                                 let top_color    = [54, 25, 67];
                                 let bottom_color = [38, 84, 91];
 
                                 let graph_data = rssp::graph::generate_density_graph_rgba_data(
                                     &score_info.chart.measure_nps_vec,
                                     score_info.chart.max_nps,
                                     graph_width, graph_height,
                                     bottom_color,
                                     top_color,
                                     bg_color,
                                 ).ok();
                                 
                                let key = format!("{}_eval", score_info.chart.short_hash);
                                graph_data.map(|data| (key, data))
                            } else {
                                None
                            };
                            
                            let key = if let Some((key, data)) = graph_request {
                                self.asset_manager.set_density_graph(backend, Some((key, data)))
                            } else {
                                self.asset_manager.set_density_graph(backend, None)
                            };
                            self.evaluation_state.density_graph_texture_key = key;
                        }
                    }

                    if target == CurrentScreen::SelectMusic {
                        if self.session_start_time.is_none() {
                            self.session_start_time = Some(Instant::now());
                            info!("Session timer started.");
                        }

                        match prev {
                            CurrentScreen::PlayerOptions => {
                                // Preserve wheel state; only sync difficulty choice back from PlayerOptions
                                let preferred = self.preferred_difficulty_index;
                                self.select_music_state.preferred_difficulty_index = preferred;
                                self.select_music_state.selected_difficulty_index = preferred;

                                // Clamp to the nearest playable difficulty for the currently selected song
                                if let Some(select_music::MusicWheelEntry::Song(song)) =
                                    self.select_music_state.entries.get(self.select_music_state.selected_index)
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
                                        self.select_music_state.selected_difficulty_index = idx;
                                    }
                                }

                                // Nudge delayed updates to refresh graph immediately on return
                                select_music::trigger_immediate_refresh(&mut self.select_music_state);
                            }
                            CurrentScreen::Gameplay | CurrentScreen::Evaluation => {
                                // Gameplay/Evaluation stop the actual preview music; ask SelectMusic
                                // to invalidate preview state and regenerate delayed assets.
                                select_music::reset_preview_after_gameplay(&mut self.select_music_state);
                            }
                            _ => {
                                let current_color_index = self.select_music_state.active_color_index;
                                self.select_music_state = select_music::init();
                                self.select_music_state.active_color_index = current_color_index;
                                self.select_music_state.selected_difficulty_index = self.preferred_difficulty_index;
                                self.select_music_state.preferred_difficulty_index = self.preferred_difficulty_index;
                            }
                        }
                    }

                    let (_, in_duration) = self.get_in_transition_for_screen(target);
                    self.transition = TransitionState::FadingIn { 
                        elapsed: 0.0,
                        duration: in_duration,
                    };
                    crate::ui::runtime::clear_all();
                }

                let (actors, clear_color) = self.get_current_actors();
                let screen = self.build_screen(&actors, clear_color, total_elapsed);
                self.update_fps_title(&window, now);

                if let Some(backend) = &mut self.backend {
                    match backend.draw(&screen, &self.asset_manager.textures) {
                        Ok(vpf) => self.current_frame_vpf = vpf,
                        Err(e) => {
                            error!("Failed to draw frame: {}", e);
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
    let _ = env_logger::builder().filter_level(log::LevelFilter::Info).try_init();
    let config = crate::config::get();
    let backend_type = config.video_renderer;
    let vsync_enabled = config.vsync;
    let fullscreen_enabled = !config.windowed;
    let show_stats = config.show_stats;
    let color_index = config.simply_love_color;

    song_loading::scan_and_load_songs("songs");
    let event_loop: EventLoop<UserEvent> = EventLoop::<UserEvent>::with_user_event().build()?;

    // Spawn background thread to pump gilrs and emit user events; decoupled from frame rate.
    let proxy: EventLoopProxy<UserEvent> = event_loop.create_proxy();
    std::thread::spawn(move || {
        let mut maybe_gilrs = gamepad::try_init();
        if let Some(mut g) = maybe_gilrs.take() {
            let mut active_id = None;
            let mut gp_state = gamepad::GamepadState::default();
            loop {
                let (pad_events, sys_events) = gamepad::poll_and_collect(&mut g, &mut active_id, &mut gp_state);
                let pad_empty = pad_events.is_empty();
                let sys_empty = sys_events.is_empty();
                for se in sys_events { let _ = proxy.send_event(UserEvent::GamepadSystem(se)); }
                for pe in pad_events { let _ = proxy.send_event(UserEvent::Pad(pe)); }
                if pad_empty && sys_empty {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        }
    });

    let mut app = App::new(backend_type, vsync_enabled, fullscreen_enabled, show_stats, color_index);
    event_loop.run_app(&mut app)?;
    Ok(())
}
