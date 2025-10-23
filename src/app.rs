use crate::core::gfx::{self as renderer, create_backend, BackendType, RenderList};
use crate::core::input;
use crate::core::input::InputState;
use crate::core::space::{self as space, Metrics};
use crate::gameplay::{profile, scores};
use crate::assets::AssetManager;
use crate::ui::color;
use crate::screens::{gameplay, menu, options, init, select_color, select_music, sandbox, evaluation, Screen as CurrentScreen, ScreenAction, Screen};
use crate::gameplay::parsing::simfile as song_loading;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::Window,
};

use log::{error, warn, info};
use std::{error::Error, sync::Arc, time::Instant};

use crate::ui::actors::Actor;
/* -------------------- gamepad -------------------- */
use crate::core::gamepad;
use crate::core::gamepad::{GpSystemEvent, PadEvent, PadDir, PadButton, FaceBtn};
use gilrs::{Gilrs, GamepadId};

/* -------------------- transition timing constants -------------------- */
const FADE_OUT_DURATION: f32 = 0.4;
const MENU_ACTORS_FADE_DURATION: f32 = 0.65;

/* -------------------- transition state machine -------------------- */
#[derive(Debug)]
enum TransitionState {
    Idle,
    FadingOut { elapsed: f32, duration: f32, target: CurrentScreen, actors: Vec<Actor> },
    FadingIn  { elapsed: f32, duration: f32, actors: Vec<Actor> },
    ActorsFadeOut { elapsed: f32, target: CurrentScreen },
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
    input_state: InputState,
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

    /* gamepad */
    gilrs: Option<Gilrs>,
    active_gamepad_id: Option<GamepadId>,
    gamepad_state: gamepad::GamepadState,
    gamepad_overlay_state: Option<(String, Instant)>,
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
            window: None, backend: None, backend_type, asset_manager: AssetManager::new(),
            current_screen: CurrentScreen::Init, init_state, menu_state, gameplay_state: None, options_state,
            select_color_state, select_music_state, sandbox_state: sandbox::init(), evaluation_state,
            input_state: input::init_state(), frame_count: 0, last_title_update: Instant::now(), last_frame_time: Instant::now(),
            start_time: Instant::now(), metrics: space::metrics_for_window(display_width, display_height), preferred_difficulty_index: 2, // Default to Medium
            vsync_enabled, fullscreen_enabled, show_overlay, last_fps: 0.0, last_vpf: 0, 
            current_frame_vpf: 0, transition: TransitionState::Idle,
            session_start_time: None,
            display_width,
            display_height,

            gilrs: gamepad::try_init(),
            active_gamepad_id: None,
            gamepad_state: gamepad::GamepadState::default(),
            gamepad_overlay_state: None,
        }
    }

    fn handle_action(&mut self, action: ScreenAction, event_loop: &ActiveEventLoop) -> Result<(), Box<dyn Error>> {
        match action {
            ScreenAction::Navigate(screen) => {
                let from = self.current_screen;
                let to = screen;

                if from == CurrentScreen::Init && to == CurrentScreen::Menu {
                    info!("Instant navigation Init→Menu (out-transition handled by Init screen)");
                    self.current_screen = screen;
                    self.transition = TransitionState::ActorsFadeIn { elapsed: 0.0 };
                    crate::ui::runtime::clear_all();
                    return Ok(());
                }

                if matches!(self.transition, TransitionState::Idle) {
                    let is_actor_only_fade =
                        (from == CurrentScreen::Menu && (to == CurrentScreen::Options || to == CurrentScreen::SelectColor)) ||
                        ((from == CurrentScreen::Options || from == CurrentScreen::SelectColor) && to == CurrentScreen::Menu);

                    if is_actor_only_fade {
                        info!("Starting actor-only fade out to screen: {:?}", screen);
                        self.transition = TransitionState::ActorsFadeOut { elapsed: 0.0, target: screen };
                    } else {
                        info!("Starting global fade out to screen: {:?}", screen);                        
                        let (out_actors, out_duration) = self.get_out_transition_for_screen(self.current_screen);
                        self.transition = TransitionState::FadingOut {
                            elapsed: 0.0,
                            duration: out_duration,
                            target: screen,
                            actors: out_actors,
                        };
                    }
                }
            }
            ScreenAction::Exit => {
                info!("Exit action received. Shutting down.");
                event_loop.exit();
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
                TransitionState::ActorsFadeOut { elapsed, .. } => {
                    screen_alpha_multiplier = 1.0 - (elapsed / FADE_OUT_DURATION).clamp(0.0, 1.0);
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
            TransitionState::FadingOut { actors: out_actors, .. } => {
                actors.extend(out_actors.clone());
            }
            TransitionState::FadingIn { actors: in_actors, .. } => {
                actors.extend(in_actors.clone());
            }
            _ => {}
        }

        (actors, CLEAR)
    }
    
    fn get_out_transition_for_screen(&self, screen: CurrentScreen) -> (Vec<Actor>, f32) {
        match screen {
            CurrentScreen::Menu => menu::out_transition(),
            CurrentScreen::Gameplay => gameplay::out_transition(),
            CurrentScreen::Options => options::out_transition(),
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
            .with_resizable(true);

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

    /* -------------------- keyboard path stays as-is -------------------- */

    #[inline(always)]
    fn handle_virtual_key_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        key_event: winit::event::KeyEvent,
    ) {
        let is_transitioning = !matches!(self.transition, TransitionState::Idle);

        // IMPORTANT: do NOT mirror keyboard arrows into InputState while in Gameplay.
        // Gameplay judges directly from KeyEvent; InputState is reserved for gamepad.
        if self.current_screen != CurrentScreen::Gameplay {
            input::handle_keyboard_input(&key_event, &mut self.input_state);
        }

        if key_event.state == winit::event::ElementState::Pressed {
            if let winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F3) = key_event.physical_key {
                self.show_overlay = !self.show_overlay;
                log::info!("Overlay {}", if self.show_overlay { "ON" } else { "OFF" });
            }
            if let winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F4) = key_event.physical_key {
                if self.current_screen == CurrentScreen::Menu {
                    let _ = self.handle_action(ScreenAction::Navigate(CurrentScreen::Sandbox), event_loop);
                    return;
                }
            }
            if let winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) = key_event.physical_key {
                if self.current_screen == CurrentScreen::Menu {
                    if let Err(e) = self.handle_action(ScreenAction::Exit, event_loop) {
                        log::error!("Failed to handle exit action: {}", e);
                        event_loop.exit();
                    }
                    return;
                }
            }
            if let winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F7) = key_event.physical_key {
                if self.current_screen == CurrentScreen::SelectMusic {
                    if let Some(select_music::MusicWheelEntry::Song(song)) =
                        self.select_music_state.entries.get(self.select_music_state.selected_index)
                    {
                        let difficulty_name =
                            color::FILE_DIFFICULTY_NAMES[self.select_music_state.selected_difficulty_index];
                        if let Some(chart) = song
                            .charts
                            .iter()
                            .find(|c| c.difficulty.eq_ignore_ascii_case(difficulty_name))
                        {
                            let action = ScreenAction::FetchOnlineGrade(chart.short_hash.clone());
                            if let Err(e) = self.handle_action(action, event_loop) {
                                log::error!("Failed to handle FetchOnlineGrade action: {}", e);
                            }
                            return;
                        }
                    }
                }
            }
        }

        if is_transitioning {
            return;
        }

        let action = match self.current_screen {
            CurrentScreen::Menu => menu::handle_key_press(&mut self.menu_state, &key_event),
            CurrentScreen::Gameplay => {
                if let Some(gs) = &mut self.gameplay_state {
                    gameplay::handle_key_press(gs, &key_event)
                } else {
                    ScreenAction::None
                }
            }
            CurrentScreen::Options => options::handle_key_press(&mut self.options_state, &key_event),
            CurrentScreen::SelectColor => select_color::handle_key_press(&mut self.select_color_state, &key_event),
            CurrentScreen::Sandbox => sandbox::handle_key_press(&mut self.sandbox_state, &key_event),
            CurrentScreen::SelectMusic => select_music::handle_key_press(&mut self.select_music_state, &key_event),
            CurrentScreen::Init => init::handle_key_press(&mut self.init_state, &key_event),
            CurrentScreen::Evaluation => evaluation::handle_key_press(&mut self.evaluation_state, &key_event),
        };
        if let Err(e) = self.handle_action(action.clone(), event_loop) {
            log::error!("Failed to handle action: {}", e);
            event_loop.exit();
        }
    }

    /* -------------------- new: gamepad helpers -------------------- */

    #[inline(always)]
    fn apply_dir_from_pad(&mut self, event_loop: &ActiveEventLoop, dir: PadDir, pressed: bool) {
        // 1) always update InputState so gameplay can read it for arrow hits
        match dir {
            PadDir::Up    => self.input_state.up = pressed,
            PadDir::Down  => self.input_state.down = pressed,
            PadDir::Left  => self.input_state.left = pressed,
            PadDir::Right => self.input_state.right = pressed,
        }

        // 2) SelectMusic has complex wheel logic that is handled in its own module.
        if matches!(self.current_screen, CurrentScreen::SelectMusic) {
            let action = select_music::handle_pad_dir(&mut self.select_music_state, dir, pressed);
            if let Err(e) = self.handle_action(action, event_loop) {
                error!("Failed to handle pad-dir action: {}", e);
                event_loop.exit();
            }
        }
    }

    #[inline(always)]
    fn handle_pad_event(&mut self, event_loop: &ActiveEventLoop, ev: PadEvent) {
        let is_transitioning = !matches!(self.transition, TransitionState::Idle);
        if is_transitioning || self.current_screen == CurrentScreen::Init {
            return;
        }

        match ev {
            PadEvent::Dir { dir, pressed } => {
                self.apply_dir_from_pad(event_loop, dir, pressed);

                // Handle simple navigation for other menu screens directly here.
                if pressed {
                    match self.current_screen {
                        CurrentScreen::Menu => {
                            let delta: isize = match dir { PadDir::Up => -1, PadDir::Down => 1, _ => 0 };
                            if delta != 0 {
                                crate::core::audio::play_sfx("assets/sounds/change.ogg");
                                let n = crate::screens::menu::OPTION_COUNT as isize;
                                let cur = self.menu_state.selected_index as isize;
                                self.menu_state.selected_index = ((cur + delta + n) % n) as usize;
                            }
                        }
                        CurrentScreen::Options => {
                            let delta: isize = match dir { PadDir::Up => -1, PadDir::Down => 1, _ => 0 };
                            if delta != 0 {
                                let total = crate::screens::options::ITEMS.len();
                                if total > 0 {
                                    self.options_state.selected = ((self.options_state.selected as isize + delta + total as isize) % total as isize) as usize;
                                }
                            }
                        }
                        CurrentScreen::SelectColor => {
                            let delta: i32 = match dir { PadDir::Left => -1, PadDir::Right => 1, _ => 0 };
                            if delta != 0 {
                                let state = &mut self.select_color_state;
                                let num_colors = crate::ui::color::DECORATIVE_HEX.len() as i32;
                                state.active_color_index += delta;
                                crate::config::update_simply_love_color(state.active_color_index.rem_euclid(num_colors));
                                
                                let showing_now = if state.bg_fade_t < crate::screens::select_color::BG_FADE_DURATION {
                                    let a = (state.bg_fade_t / crate::screens::select_color::BG_FADE_DURATION).clamp(0.0, 1.0);
                                    if (1.0 - a) >= a { state.bg_from_index } else { state.bg_to_index }
                                } else {
                                    state.bg_to_index
                                };
                                state.bg_from_index = showing_now;
                                state.bg_to_index = state.active_color_index;
                                state.bg_fade_t = 0.0;
                            }
                        }
                        _ => {},
                    }
                }
            }

            PadEvent::Face { btn, pressed } => {
                if let CurrentScreen::Gameplay = self.current_screen {
                    let dir = match btn {
                        FaceBtn::NorthY => PadDir::Up,
                        FaceBtn::WestX  => PadDir::Left,
                        FaceBtn::EastB  => PadDir::Right,
                        FaceBtn::SouthA => PadDir::Down,
                    };
                    self.apply_dir_from_pad(event_loop, dir, pressed);
                } else if pressed && btn == FaceBtn::SouthA {
                    // A button acts as Confirm/Enter in menus
                    let mut play_sound = true;
                    let action = match self.current_screen {
                        CurrentScreen::Menu => match self.menu_state.selected_index {
                            0 => ScreenAction::Navigate(Screen::SelectColor), // "GAMEPLAY" -> SelectColor
                            1 => ScreenAction::Navigate(Screen::Options),
                            2 => ScreenAction::Exit,
                            _ => ScreenAction::None,
                        },
                        CurrentScreen::Options => {
                            if self.options_state.selected == crate::screens::options::ITEMS.len() - 1 {
                                ScreenAction::Navigate(Screen::Menu)
                            } else {
                                play_sound = false; // Don't play sound on non-functional options
                                ScreenAction::None
                            }
                        },
                        CurrentScreen::SelectColor => ScreenAction::Navigate(Screen::SelectMusic),
                        CurrentScreen::SelectMusic => {
                            play_sound = false; // select_music handles its own sounds
                            select_music::handle_pad_button(&mut self.select_music_state, PadButton::Confirm, true)
                        },
                        CurrentScreen::Evaluation => {
                            play_sound = false; // No sound when leaving eval
                            ScreenAction::Navigate(Screen::SelectMusic)
                        },
                        _ => { play_sound = false; ScreenAction::None }
                    };
                    if play_sound { crate::core::audio::play_sfx("assets/sounds/start.ogg"); }
                    if let Err(e) = self.handle_action(action, event_loop) { error!("Failed to handle A-button action: {}", e); }
                } 
            }

            PadEvent::Button { btn, pressed } => {
                match btn {
                    PadButton::Confirm if pressed => {
                        let mut play_sound = true;
                        let action = match self.current_screen {
                            CurrentScreen::Menu => match self.menu_state.selected_index {
                                0 => ScreenAction::Navigate(Screen::SelectColor),
                                1 => ScreenAction::Navigate(Screen::Options),
                                2 => ScreenAction::Exit,
                                _ => ScreenAction::None,
                            },
                            CurrentScreen::Options => {
                                if self.options_state.selected == crate::screens::options::ITEMS.len() - 1 {
                                    ScreenAction::Navigate(Screen::Menu)
                                } else {
                                    play_sound = false;
                                    ScreenAction::None
                                }
                            },
                            CurrentScreen::SelectColor => ScreenAction::Navigate(Screen::SelectMusic),
                            CurrentScreen::SelectMusic => {
                                play_sound = false; // select_music handles its own sounds
                                select_music::handle_pad_button(&mut self.select_music_state, PadButton::Confirm, true)
                            },
                            CurrentScreen::Gameplay => {
                                if let Some(gs) = &mut self.gameplay_state {
                                    gs.hold_to_exit_key = Some(winit::keyboard::KeyCode::Enter);
                                    gs.hold_to_exit_start = Some(Instant::now());
                                }
                                play_sound = false;
                                ScreenAction::None
                            },
                            CurrentScreen::Evaluation => {
                                play_sound = false;
                                ScreenAction::Navigate(Screen::SelectMusic)
                            },
                            _ => {
                                play_sound = false;
                                ScreenAction::None
                            }
                        };

                        if play_sound { crate::core::audio::play_sfx("assets/sounds/start.ogg"); }
                        if let Err(e) = self.handle_action(action, event_loop) { error!("Failed to handle Start-button action: {}", e); }
                    }
                    PadButton::Confirm if !pressed => {
                        if self.current_screen == CurrentScreen::Gameplay {
                             if let Some(gs) = &mut self.gameplay_state {
                                if gs.hold_to_exit_key == Some(winit::keyboard::KeyCode::Enter) {
                                    gs.hold_to_exit_key = None;
                                    gs.hold_to_exit_start = None;
                                }
                            }
                        }
                    }
                    PadButton::Back if pressed => {
                        crate::core::audio::play_sfx("assets/sounds/back.ogg");
                        let action = match self.current_screen {
                            CurrentScreen::Menu => ScreenAction::Exit,
                            CurrentScreen::Evaluation => ScreenAction::Navigate(Screen::SelectMusic),
                            CurrentScreen::Gameplay => ScreenAction::Navigate(Screen::SelectMusic),
                            // Default for Options, SelectColor, SelectMusic, Sandbox is to go back to Menu
                            _ => ScreenAction::Navigate(CurrentScreen::Menu),
                        };
                        if let Err(e) = self.handle_action(action, event_loop) {
                                 error!("Failed to handle back button action: {}", e);
                                 event_loop.exit();
                        }
                    }
                    PadButton::F7 if pressed => {
                        if let CurrentScreen::SelectMusic = self.current_screen {
                            if let Some(select_music::MusicWheelEntry::Song(song)) =
                                self.select_music_state.entries.get(self.select_music_state.selected_index)
                            {
                                let difficulty_name = color::FILE_DIFFICULTY_NAMES[self.select_music_state.selected_difficulty_index];
                                if let Some(chart) = song.charts.iter().find(|c| c.difficulty.eq_ignore_ascii_case(difficulty_name)) {
                                    let action = ScreenAction::FetchOnlineGrade(chart.short_hash.clone());
                                    if let Err(e) = self.handle_action(action, event_loop) {
                                        error!("Failed to fetch online grade: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    #[inline(always)]
    fn poll_gamepad_and_dispatch(&mut self, event_loop: &ActiveEventLoop) {
        let Some(g) = &mut self.gilrs else { return; };

        let want_f7 = matches!(self.current_screen, CurrentScreen::SelectMusic);
        let (pad_events, sys_events) =
            gamepad::poll_and_collect(g, &mut self.active_gamepad_id, &mut self.gamepad_state, want_f7);

        for ev in sys_events {
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

        for ev in pad_events {
            self.handle_pad_event(event_loop, ev);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            if let Err(e) = self.init_graphics(event_loop) {
                error!("Failed to initialize graphics: {}", e);
                event_loop.exit();
            }
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
                self.handle_virtual_key_event(event_loop, key_event);
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
                    TransitionState::FadingOut { elapsed, duration, target, .. } => {
                        *elapsed += delta_time;
                        if *elapsed >= *duration {
                            finished_fading_out_to = Some(*target);
                        }
                    }
                    TransitionState::ActorsFadeOut { elapsed, target } => {
                        *elapsed += delta_time;
                        if *elapsed >= FADE_OUT_DURATION {
                            let prev = self.current_screen;
                            self.current_screen = *target;

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
                    TransitionState::FadingIn { elapsed, duration, .. } => {
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
                                let action = gameplay::update(gs, &self.input_state, delta_time);
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
                    let prev = self.current_screen;
                    self.current_screen = target;
                    
                    // When leaving gameplay, stop music and unload the dynamic background
                    if prev == CurrentScreen::Gameplay { 
                        crate::core::audio::stop_music();
                        if let Some(backend) = self.backend.as_mut() {
                            self.asset_manager.set_dynamic_background(backend, None);
                        }
                    }

                    if prev == CurrentScreen::SelectMusic {
                        crate::core::audio::stop_music();
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
                    }

                    if target == CurrentScreen::Gameplay {
                        let (song_arc, chart) = {
                            let sm_state = &self.select_music_state;
                            let entry = sm_state.entries.get(sm_state.selected_index).unwrap();
                            let song = match entry {
                                select_music::MusicWheelEntry::Song(s) => s,
                                _ => panic!("Cannot start gameplay on a pack header"),
                            };
                            let difficulty_name = color::FILE_DIFFICULTY_NAMES[sm_state.selected_difficulty_index];
                            let chart_ref = song.charts.iter().find(|c| c.difficulty.eq_ignore_ascii_case(difficulty_name)).unwrap();
                            (song.clone(), Arc::new(chart_ref.clone()))
                        };
                        
                        let color_index = self.menu_state.active_color_index;
                        let mut gs = gameplay::init(song_arc, chart, color_index);
                        // Load dynamic background
                        if let Some(backend) = self.backend.as_mut() {
                            gs.background_texture_key = self.asset_manager.set_dynamic_background(backend, gs.song.background_path.clone());
                        }
                        self.gameplay_state = Some(gs);
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

                        if prev != CurrentScreen::Gameplay && prev != CurrentScreen::Evaluation {
                            let current_color_index = self.select_music_state.active_color_index;
                            self.select_music_state = select_music::init();
                            self.select_music_state.active_color_index = current_color_index;
                            self.select_music_state.selected_difficulty_index = self.preferred_difficulty_index;
                            self.select_music_state.preferred_difficulty_index = self.preferred_difficulty_index;
                        }
                    }

                    let (in_actors, in_duration) = self.get_in_transition_for_screen(target);
                    self.transition = TransitionState::FadingIn { 
                        elapsed: 0.0,
                        duration: in_duration,
                        actors: in_actors
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Pump gamepad → pad events → handlers
        self.poll_gamepad_and_dispatch(event_loop);

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
    let event_loop = EventLoop::new()?;
    let mut app = App::new(backend_type, vsync_enabled, fullscreen_enabled, show_stats, color_index);
    event_loop.run_app(&mut app)?;
    Ok(())
}
