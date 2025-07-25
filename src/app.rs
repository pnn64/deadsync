use crate::assets::AssetManager;
use crate::audio::AudioManager;
use crate::config;
use crate::graphics::renderer::Renderer;
use crate::graphics::vulkan_base::VulkanBase;
use crate::parsing::simfile::{scan_packs, SongInfo}; // Import for gameplay transition AND helper
use crate::screens::{gameplay, menu, options, score, select_music};
use crate::state::{
    AppState, GameState, MenuState, MusicWheelEntry, OptionsState, ScoreScreenState,
    SelectMusicState,
};
use crate::utils::fps::FPSCounter;

use ash::vk;
use log::{error, info, trace, warn};
use std::collections::HashMap;
use std::error::Error;

use std::time::{Duration, Instant};
use winit::{
    dpi::PhysicalSize,
    event::{Event, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::ModifiersState,
    platform::run_on_demand::EventLoopExtRunOnDemand,
    window::WindowBuilder,
};

const RESIZE_DEBOUNCE_DURATION: Duration = Duration::from_millis(0);
const PREVIEW_RESTART_DELAY: f32 = 0.25; // Seconds (used in App::update for SelectMusicState)

pub struct App {
    vulkan_base: VulkanBase,
    renderer: Renderer,
    audio_manager: AudioManager,
    asset_manager: AssetManager,
    song_library: Vec<SongInfo>,
    current_app_state: AppState,
    menu_state: MenuState,
    select_music_state: SelectMusicState,
    options_state: OptionsState,
    game_state: Option<GameState>,
    score_state: ScoreScreenState,
    fps_counter: FPSCounter,
    last_frame_time: Instant,
    rng: rand::rngs::ThreadRng,
    next_app_state: Option<AppState>,
    pending_resize: Option<(PhysicalSize<u32>, Instant)>,
    swapchain_is_known_bad: bool,
    pack_colors: HashMap<String, [f32; 4]>,
    current_modifiers: ModifiersState, // Added
}

impl App {
    pub fn new(event_loop: &EventLoop<()>) -> Result<Self, Box<dyn Error>> {
        info!("Creating Application...");

        let window = WindowBuilder::new()
            .with_title(config::WINDOW_TITLE)
            .with_inner_size(winit::dpi::LogicalSize::new(
                f64::from(config::WINDOW_WIDTH),
                f64::from(config::WINDOW_HEIGHT),
            ))
            .build(event_loop)?;

        let vulkan_base = VulkanBase::new(window)?;
        info!("Vulkan Initialized. GPU: {}", vulkan_base.get_gpu_name());

        let initial_surface_resolution = vulkan_base.surface_resolution;
        let renderer = Renderer::new(
            &vulkan_base,
            (
                initial_surface_resolution.width as f32,
                initial_surface_resolution.height as f32,
            ),
        )?;
        info!("Renderer Initialized.");

        let mut audio_manager = AudioManager::new()?;
        info!("Audio Manager Initialized.");

        let mut asset_manager = AssetManager::new();
        asset_manager.load_all(&vulkan_base, &renderer, &mut audio_manager)?;
        info!("Asset Manager Initialized and Assets Loaded.");

        info!("Scanning for songs...");
        let song_library = scan_packs(std::path::Path::new("songs")); // Explicit Path
        info!("Found {} songs.", song_library.len());

        let mut unique_pack_names: Vec<String> = song_library
            .iter()
            .map(|s| {
                s.folder_path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown Pack")
                    .to_string()
            })
            .collect();
        unique_pack_names.sort_unstable_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        unique_pack_names.dedup();

        let mut pack_colors = HashMap::new();
        for (i, pack_name) in unique_pack_names.iter().enumerate() {
            let color_index = i % config::PACK_NAME_COLOR_PALETTE.len();
            pack_colors.insert(
                pack_name.clone(),
                config::PACK_NAME_COLOR_PALETTE[color_index],
            );
        }
        info!("Assigned colors to {} unique packs.", pack_colors.len());

        vulkan_base
            .wait_idle()
            .map_err(|e| format!("Error waiting for GPU idle after setup: {}", e))?;
        info!("GPU idle after setup.");

        Ok(App {
            vulkan_base,
            renderer,
            audio_manager,
            asset_manager,
            song_library,
            current_app_state: AppState::Menu,
            menu_state: MenuState::default(),
            select_music_state: SelectMusicState::default(),
            options_state: OptionsState::default(),
            game_state: None,
            score_state: ScoreScreenState::default(),
            fps_counter: FPSCounter::new(),
            last_frame_time: Instant::now(),
            rng: rand::rng(),
            next_app_state: None,
            pending_resize: None,
            swapchain_is_known_bad: false,
            pack_colors,
            current_modifiers: ModifiersState::empty(), // Initialize
        })
    }

    pub fn run(mut self, mut event_loop: EventLoop<()>) -> Result<(), Box<dyn Error>> {
        info!("Starting Event Loop...");
        self.last_frame_time = Instant::now();

        event_loop.run_on_demand(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);

            match event {
                Event::WindowEvent { event: window_event, window_id } if window_id == self.vulkan_base.window.id() => {
                    if let WindowEvent::ModifiersChanged(modifiers) = window_event {
                        self.current_modifiers = modifiers.state();
                    }
                    match window_event {
                        WindowEvent::RedrawRequested => {
                            if self.swapchain_is_known_bad {
                                trace!("Skipping render because swapchain is known to be bad. Waiting for resize.");
                                self.vulkan_base.window.request_redraw();
                            } else {
                                match self.render() {
                                    Ok(needs_resize_from_render) => {
                                        if needs_resize_from_render {
                                            info!("Render reported suboptimal swapchain, scheduling resize check.");
                                            let current_size = self.vulkan_base.window.inner_size();
                                            self.pending_resize = Some((current_size, Instant::now()));
                                            self.swapchain_is_known_bad = true;
                                        } else {
                                            self.swapchain_is_known_bad = false;
                                        }
                                    }
                                    Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::SUBOPTIMAL_KHR) => {
                                        info!("Render failed with out-of-date/suboptimal, scheduling resize check.");
                                        let current_size = self.vulkan_base.window.inner_size();
                                        self.pending_resize = Some((current_size, Instant::now()));
                                        self.swapchain_is_known_bad = true;
                                    }
                                    Err(e) => {
                                        error!("Failed to render frame: {:?}. Exiting.", e);
                                        elwt.exit();
                                    }
                                }
                            }
                        },
                        _ => self.handle_window_event(window_event, elwt),
                    }
                }

                Event::AboutToWait => {
                    if let Some(new_state) = self.next_app_state.take() {
                        self.transition_state(new_state);
                    }

                    if self.try_process_pending_resize().is_err() {
                        error!("Failed to process pending resize in AboutToWait. Exiting.");
                        elwt.exit();
                        return;
                    }

                    if self.pending_resize.is_none() && !self.swapchain_is_known_bad {
                        let now = Instant::now();
                        let dt = (now - self.last_frame_time).as_secs_f32().max(0.0).min(config::MAX_DELTA_TIME);
                        self.last_frame_time = now;
                        self.update(dt);
                        self.vulkan_base.window.request_redraw();
                    } else if self.pending_resize.is_some() || self.swapchain_is_known_bad {
                        self.vulkan_base.window.request_redraw();
                    }
                },

                Event::LoopExiting => {
                    info!("Event loop exiting.");
                }

                _ => {}
            }

            if self.current_app_state == AppState::Exiting {
                 elwt.exit();
             }
        })?;

        Ok(())
    }

    fn try_process_pending_resize(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some((target_size, last_event_time)) = self.pending_resize {
            if Instant::now().duration_since(last_event_time) >= RESIZE_DEBOUNCE_DURATION {
                info!(
                    "Debounce time elapsed, processing resize to {:?}.",
                    target_size
                );
                let actual_target_size = self.pending_resize.take().unwrap().0;

                match self.handle_actual_resize(actual_target_size) {
                    Ok(_) => {
                        self.swapchain_is_known_bad = false;
                        info!("Resize processed successfully.");
                    }
                    Err(e) => {
                        error!("handle_actual_resize failed: {}. Re-queueing resize.", e);
                        self.pending_resize = Some((actual_target_size, Instant::now()));
                        self.swapchain_is_known_bad = true;
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_window_event(
        &mut self,
        event: WindowEvent,
        _elwt: &winit::event_loop::EventLoopWindowTarget<()>,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Close requested, setting state to Exiting.");
                self.next_app_state = Some(AppState::Exiting);
            }
            WindowEvent::Resized(new_size) => {
                trace!(
                    "Window resized event (raw): {:?}, updating pending resize.",
                    new_size
                );
                self.pending_resize = Some((new_size, Instant::now()));
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                self.handle_keyboard_input(key_event);
            }
            // WindowEvent::ModifiersChanged is handled in the main event loop now
            _ => {}
        }
    }

    // find_pack_banner moved to select_music.rs as a private helper

    fn rebuild_music_wheel_entries(&mut self) {
        select_music::rebuild_music_wheel_entries_logic(
            &mut self.select_music_state,
            &self.song_library,
            &self.pack_colors,
        );
        // Logic related to resetting preview state is now inside rebuild_music_wheel_entries_logic
        info!(
            "Rebuilt music wheel. New #entries: {}, selected_index: {}",
            self.select_music_state.entries.len(),
            self.select_music_state.selected_index
        );
    }

    // start_actual_preview_playback moved to select_music.rs

    fn handle_music_selection_change(&mut self) {
        select_music::handle_selection_change_logic(
            &mut self.select_music_state,
            &mut self.asset_manager,
            &mut self.audio_manager,
            &self.renderer,
            &self.vulkan_base,
        );
    }

    fn handle_keyboard_input(&mut self, key_event: KeyEvent) {
        trace!("Keyboard Input: {:?}", key_event);
        let mut requested_state: Option<AppState> = None;
        let mut selection_changed_in_music_by_input = false;

        match self.current_app_state {
            AppState::Menu => {
                requested_state =
                    menu::handle_input(&key_event, &mut self.menu_state, &self.audio_manager);
            }
            AppState::SelectMusic => {
                // let _original_selected_index_before_input = self.select_music_state.selected_index; // Can be removed if not used
                let original_expanded_pack_name =
                    self.select_music_state.expanded_pack_name.clone(); // Capture before

                let (next_state, sel_changed_by_nav_or_toggle) = select_music::handle_input(
                    &key_event,
                    &mut self.select_music_state,
                    &self.audio_manager,
                );
                requested_state = next_state;
                selection_changed_in_music_by_input = sel_changed_by_nav_or_toggle;

                if self.select_music_state.expanded_pack_name != original_expanded_pack_name {
                    self.rebuild_music_wheel_entries();
                    selection_changed_in_music_by_input = true;
                }
            }
            AppState::Options => {
                requested_state = options::handle_input(&key_event, &mut self.options_state);
            }
            AppState::Gameplay => {
                if let Some(ref mut gs) = self.game_state {
                    gameplay::handle_input(&key_event, gs, self.current_modifiers);
                // Pass modifiers
                } else {
                    warn!("Received input in Gameplay state, but game_state is None.");
                    requested_state = None;
                }
            }
            AppState::Exiting => {
                requested_state = None;
            }
            AppState::ScoreScreen => {
                requested_state = score::handle_input(&key_event, &mut self.score_state);
            }
        }

        if requested_state.is_some() {
            self.next_app_state = requested_state;
        }

        if self.current_app_state == AppState::SelectMusic && selection_changed_in_music_by_input {
            self.handle_music_selection_change();
        }
    }

    fn transition_state(&mut self, new_state: AppState) {
        if new_state == self.current_app_state {
            return;
        }
        info!(
            "Transitioning state from {:?} -> {:?}",
            self.current_app_state, new_state
        );

        match self.current_app_state {
            AppState::SelectMusic => {
                self.audio_manager.stop_preview();
                self.select_music_state.preview_audio_path = None;
                self.select_music_state.preview_playback_started_at = None;
                self.select_music_state.is_awaiting_preview_restart = false;
                self.select_music_state.selection_landed_at = None;
                self.select_music_state.is_preview_actions_scheduled = false;
                self.select_music_state.last_difficulty_nav_key = None;
                self.select_music_state.last_difficulty_nav_time = None;

                if let Some(mut graph_tex) = self.select_music_state.current_graph_texture.take() {
                    info!("Destroying NPS graph texture on state transition from SelectMusic.");
                    graph_tex.destroy(&self.vulkan_base.device);
                }
                self.select_music_state.current_graph_song_chart_key = None;

                if new_state != AppState::Gameplay {
                    info!("Transitioning from SelectMusic to non-Gameplay state ({:?}). Resetting DynamicBanner.", new_state);
                    if let Some(fallback_res) = self
                        .asset_manager
                        .get_texture(crate::assets::TextureId::FallbackBanner)
                    {
                        self.renderer.update_texture_descriptor(
                            &self.vulkan_base.device,
                            crate::graphics::renderer::DescriptorSetId::DynamicBanner,
                            fallback_res,
                        );
                    }
                    self.asset_manager
                        .clear_current_banner(&self.vulkan_base.device);
                } else {
                    info!("Transitioning from SelectMusic to Gameplay. DynamicBanner for song should persist.");
                }
            }
            AppState::Gameplay => {
                self.audio_manager.stop_music();
                self.game_state = None;
                info!("Gameplay state cleared.");

                if new_state != AppState::SelectMusic && new_state != AppState::ScoreScreen {
                    info!("Transitioning from Gameplay to non-SelectMusic state ({:?}). Resetting DynamicBanner.", new_state);
                    if let Some(fallback_res) = self
                        .asset_manager
                        .get_texture(crate::assets::TextureId::FallbackBanner)
                    {
                        self.renderer.update_texture_descriptor(
                            &self.vulkan_base.device,
                            crate::graphics::renderer::DescriptorSetId::DynamicBanner,
                            fallback_res,
                        );
                    }
                    self.asset_manager
                        .clear_current_banner(&self.vulkan_base.device);
                } else {
                    info!("Transitioning from Gameplay to SelectMusic. SelectMusic will handle banner refresh.");
                }
            }
            AppState::ScoreScreen => {
                // Cleanup for ScoreScreen if any specific resources were used.
                // Currently ScoreScreenState is simple.
                // If transitioning away from ScoreScreen, reset banner if not going to Gameplay
                if new_state != AppState::Gameplay {
                    info!("Transitioning from ScoreScreen to non-Gameplay state ({:?}). Resetting DynamicBanner.", new_state);
                    if let Some(fallback_res) = self
                        .asset_manager
                        .get_texture(crate::assets::TextureId::FallbackBanner)
                    {
                        self.renderer.update_texture_descriptor(
                            &self.vulkan_base.device,
                            crate::graphics::renderer::DescriptorSetId::DynamicBanner,
                            fallback_res,
                        );
                    }
                    self.asset_manager
                        .clear_current_banner(&self.vulkan_base.device);
                }
            }
            _ => {}
        }

        match new_state {
            AppState::Menu => {
                self.menu_state = MenuState::default();
            }
            AppState::SelectMusic => {
                self.select_music_state = SelectMusicState::default();
                self.rebuild_music_wheel_entries(); // Calls the local wrapper
                self.handle_music_selection_change(); // Calls the local wrapper
                info!(
                    "Populated SelectMusic state with {} entries and handled initial selection.",
                    self.select_music_state.entries.len()
                );
            }
            AppState::Options => {
                self.options_state = OptionsState::default();
            }
            AppState::Gameplay => {
                info!("Initializing Gameplay State...");
                let selected_entry_opt = self
                    .select_music_state
                    .entries
                    .get(self.select_music_state.selected_index);

                if let Some(MusicWheelEntry::Song(selected_song_arc)) = selected_entry_opt {
                    let target_difficulty_name = select_music::DIFFICULTY_NAMES
                        [self.select_music_state.selected_difficulty_index];
                    info!(
                        "Attempting to start gameplay for song '{}' with difficulty: {}",
                        selected_song_arc.title, target_difficulty_name
                    );

                    let chart_to_play_idx_option = selected_song_arc.charts.iter().position(|c| {
                        c.difficulty.eq_ignore_ascii_case(target_difficulty_name)
                            && c.stepstype == "dance-single"
                            && c.processed_data.is_some()
                            && c.processed_data
                                .as_ref()
                                .map_or(false, |pd| !pd.measures.is_empty())
                    });

                    let final_selected_chart_idx = match chart_to_play_idx_option {
                        Some(idx) => idx,
                        None => {
                            warn!(
                                "No chart found for '{}' with difficulty '{}' and stepstype 'dance-single'. Trying first available processable 'dance-single' chart.",
                                selected_song_arc.title, target_difficulty_name
                            );
                            selected_song_arc.charts.iter().position(|c|
                                c.stepstype == "dance-single" &&
                                c.processed_data.is_some() &&
                                c.processed_data.as_ref().map_or(false, |pd| !pd.measures.is_empty())
                            ).unwrap_or_else(|| {
                                warn!("No processable 'dance-single' charts found for song '{}', defaulting to chart index 0. Gameplay might be empty or wrong type.", selected_song_arc.title);
                                0
                            })
                        }
                    };
                    info!(
                        "Final selected chart index for gameplay: {}",
                        final_selected_chart_idx
                    );

                    if self.asset_manager.get_current_banner_path().as_ref()
                        != selected_song_arc.banner_path.as_ref()
                    {
                        warn!("DynamicBanner might not be correctly set for Gameplay. Expected {:?}, AssetManager has {:?}. Re-loading.",
                            selected_song_arc.banner_path, self.asset_manager.get_current_banner_path());
                        self.asset_manager.load_song_banner(
                            &self.vulkan_base,
                            &self.renderer,
                            selected_song_arc,
                        );
                    }

                    if selected_song_arc.audio_path.is_none() {
                        error!("Cannot start gameplay: Audio path missing for selected song '{}'. Returning to SelectMusic.", selected_song_arc.title);
                        self.next_app_state = Some(AppState::SelectMusic);
                        return;
                    }
                    let song_info_for_gameplay = selected_song_arc.clone();

                    info!(
                        "Preparing gameplay for song: {}",
                        song_info_for_gameplay.title
                    );

                    let window_size_f32 = (
                        self.vulkan_base.surface_resolution.width as f32,
                        self.vulkan_base.surface_resolution.height as f32,
                    );

                    let visual_and_logic_start_instant = Instant::now();
                    let audio_target_start_instant = visual_and_logic_start_instant
                        + Duration::from_secs_f32(config::GAME_LEAD_IN_DURATION_SECONDS);

                    self.game_state = Some(gameplay::initialize_game_state(
                        window_size_f32.0,
                        window_size_f32.1,
                        audio_target_start_instant,
                        song_info_for_gameplay,
                        final_selected_chart_idx,
                    ));
                    info!("Gameplay state initialized. Music will start after lead-in.");
                } else {
                    error!("Cannot start gameplay: Selected item is not a song or selection is invalid. Returning to SelectMusic.");
                    self.next_app_state = Some(AppState::SelectMusic);
                    return;
                }
            }
            AppState::ScoreScreen => {
                self.score_state = ScoreScreenState::default();
                info!("Initialized ScoreScreen state.");
            }
            AppState::Exiting => {}
        }

        self.current_app_state = new_state;
        self.vulkan_base.window.set_title(&format!(
            "{} | {:?}",
            config::WINDOW_TITLE,
            self.current_app_state
        ));
    }

    fn update(&mut self, dt: f32) {
        trace!("Update Start (dt: {:.4} s)", dt);
        let mut selection_changed_by_held_key_scroll = false;

        match self.current_app_state {
            AppState::Menu => menu::update(&mut self.menu_state, dt),
            AppState::SelectMusic => {
                if select_music::update(&mut self.select_music_state, dt, &self.audio_manager) {
                    selection_changed_by_held_key_scroll = true;
                }

                if self.select_music_state.is_preview_actions_scheduled {
                    if let Some(landed_at) = self.select_music_state.selection_landed_at {
                        let elapsed_since_landed = Instant::now().duration_since(landed_at);

                        if elapsed_since_landed >= select_music::SELECTION_START_PLAY_DELAY {
                            info!(
                                "{}ms play delay elapsed. Attempting to start preview playback.",
                                select_music::SELECTION_START_PLAY_DELAY.as_millis()
                            );
                            select_music::start_preview_playback_logic(
                                &mut self.select_music_state,
                                &mut self.audio_manager,
                            );
                            self.select_music_state.is_preview_actions_scheduled = false;
                            self.select_music_state.selection_landed_at = None;
                        }
                    } else {
                        warn!("Preview actions scheduled but selection_landed_at is None. Cancelling.");
                        self.select_music_state.is_preview_actions_scheduled = false;
                    }
                }

                if self.select_music_state.is_awaiting_preview_restart {
                    self.select_music_state.preview_restart_delay_timer -= dt;
                    if self.select_music_state.preview_restart_delay_timer <= 0.0 {
                        select_music::start_preview_playback_logic(
                            &mut self.select_music_state,
                            &mut self.audio_manager,
                        );
                        self.select_music_state.is_awaiting_preview_restart = false;
                    }
                } else if self.select_music_state.preview_audio_path.is_some()
                    && self
                        .select_music_state
                        .preview_playback_started_at
                        .is_some()
                    && !self.audio_manager.is_preview_playing()
                    && !self.select_music_state.is_preview_actions_scheduled
                {
                    info!("Preview finished, scheduling restart.");
                    self.select_music_state.is_awaiting_preview_restart = true;
                    self.select_music_state.preview_restart_delay_timer = PREVIEW_RESTART_DELAY;
                    self.select_music_state.preview_playback_started_at = None;
                }
            }
            AppState::Options => options::update(&mut self.options_state, dt),
            AppState::Gameplay => {
                if let Some(ref mut gs) = self.game_state {
                    if let Some(next_app_state_from_gameplay) =
                        gameplay::update(gs, dt, &mut self.rng)
                    {
                        self.next_app_state = Some(next_app_state_from_gameplay);
                    }

                    if !gs.music_started && gs.lead_in_timer <= 0.0 {
                        if let Some(audio_path) = &gs.song_info.audio_path {
                            info!(
                                "Lead-in complete. Starting music playback for: {:?}",
                                audio_path.file_name().unwrap_or_default()
                            );
                            match self.audio_manager.play_music(audio_path, 1.0) {
                                Ok(_) => {
                                    gs.music_started = true;
                                }
                                Err(e) => {
                                    error!("Failed to start gameplay music after lead-in: {}. Returning to SelectMusic.", e);
                                    self.next_app_state = Some(AppState::SelectMusic);
                                }
                            }
                        } else {
                            error!("Cannot start music: Audio path missing for song '{}' even after lead-in. Returning to SelectMusic.", gs.song_info.title);
                            self.next_app_state = Some(AppState::SelectMusic);
                        }
                    }
                }
            }
            AppState::ScoreScreen => {
                score::update(&mut self.score_state, dt);
            }
            AppState::Exiting => {}
        }

        if self.current_app_state == AppState::SelectMusic && selection_changed_by_held_key_scroll {
            self.handle_music_selection_change(); // Calls the local wrapper
        }

        if let Some(fps) = self.fps_counter.update() {
            let title_suffix = match self.current_app_state {
                AppState::Gameplay => {
                    let beat_str = self
                        .game_state
                        .as_ref()
                        .map_or_else(|| "N/A".to_string(), |gs| format!("{:.2}", gs.current_beat));
                    let offset_str = self.game_state.as_ref().map_or_else(
                        || "".to_string(),
                        |gs| format!(" (Offset: {:.3}s)", gs.current_global_offset_sec),
                    );
                    format!(
                        "Gameplay | FPS: {} | Beat: {} {}",
                        fps, beat_str, offset_str
                    )
                }
                AppState::Menu => format!("Menu | FPS: {}", fps),
                AppState::SelectMusic => format!("Select Music | FPS: {}", fps),
                AppState::Options => format!("Options | FPS: {}", fps),
                AppState::ScoreScreen => format!("Score Screen | FPS: {}", fps),
                AppState::Exiting => "Exiting...".to_string(),
            };
            self.vulkan_base.window.set_title(&format!(
                "{} | {}",
                config::WINDOW_TITLE,
                title_suffix
            ));
        }
        trace!("Update End");
    }

    fn handle_actual_resize(
        &mut self,
        target_size: PhysicalSize<u32>,
    ) -> Result<(), Box<dyn Error>> {
        info!("Handling actual resize to physical size: {:?}", target_size);

        if target_size.width == 0 || target_size.height == 0 {
            info!(
                "Window is minimized or zero size ({:?}). Deferring Vulkan resize further.",
                target_size
            );
            self.pending_resize = Some((target_size, Instant::now()));
            self.swapchain_is_known_bad = true;
            return Ok(());
        }

        self.vulkan_base
            .rebuild_swapchain_resources(target_size.width, target_size.height)?;

        let new_vulkan_surface_extent = self.vulkan_base.surface_resolution;
        let new_vulkan_size_f32 = (
            new_vulkan_surface_extent.width as f32,
            new_vulkan_surface_extent.height as f32,
        );

        self.renderer
            .update_projection_matrix(&self.vulkan_base, new_vulkan_size_f32)?;

        if let Some(ref mut gs) = self.game_state {
            gs.window_size = new_vulkan_size_f32;
        }

        info!(
            "Actual resize handling complete. New Vulkan surface resolution: {:?}",
            new_vulkan_surface_extent
        );
        Ok(())
    }

    fn render(&mut self) -> Result<bool, vk::Result> {
        let current_surface_resolution = self.vulkan_base.surface_resolution;
        if current_surface_resolution.width == 0 || current_surface_resolution.height == 0 {
            trace!("Skipping render due to zero-sized surface resolution.");
            return Ok(true);
        }

        let draw_result = self.vulkan_base.draw_frame(|device, cmd_buf| {
            trace!("Render: Beginning frame drawing...");
            self.renderer
                .begin_frame(device, cmd_buf, current_surface_resolution);

            match self.current_app_state {
                AppState::Menu => {
                    menu::draw(
                        &self.renderer,
                        &self.menu_state,
                        &self.asset_manager,
                        device,
                        cmd_buf,
                    );
                }
                AppState::SelectMusic => {
                    select_music::draw(
                        &self.renderer,
                        &self.select_music_state,
                        &self.asset_manager,
                        device,
                        cmd_buf,
                    );
                }
                AppState::Options => {
                    options::draw(
                        &self.renderer,
                        &self.options_state,
                        &self.asset_manager,
                        device,
                        cmd_buf,
                    );
                }
                AppState::Gameplay => {
                    if let Some(ref gs) = self.game_state {
                        gameplay::draw(&self.renderer, gs, &self.asset_manager, device, cmd_buf);
                    } else {
                        warn!("Attempted to draw Gameplay state, but game_state is None.");
                    }
                }
                AppState::ScoreScreen => {
                    score::draw(
                        &self.renderer,
                        &self.score_state,
                        &self.asset_manager,
                        device,
                        cmd_buf,
                    );
                }
                AppState::Exiting => {
                    trace!("Render: In Exiting state, drawing nothing specific.");
                }
            }
            trace!("Render: Frame drawing commands recorded.");
        });

        match draw_result {
            Ok(needs_resize) => Ok(needs_resize),
            Err(e @ vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(e @ vk::Result::SUBOPTIMAL_KHR) => {
                Err(e)
            }
            Err(e) => {
                error!("Error during Vulkan draw_frame: {:?}", e);
                Err(e)
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        info!("Dropping App - Cleaning up resources...");
        if let Err(e) = self.vulkan_base.wait_idle() {
            error!("Error waiting for GPU idle during App drop: {}", e);
        }

        self.audio_manager.stop_music();
        self.audio_manager.stop_preview();
        info!("Audio stopped.");

        if let Some(mut graph_tex) = self.select_music_state.current_graph_texture.take() {
            info!("Destroying NPS graph texture during App drop.");
            graph_tex.destroy(&self.vulkan_base.device);
        }

        self.asset_manager.destroy(&self.vulkan_base.device);
        info!("Assets destroyed.");

        self.renderer.destroy(&self.vulkan_base.device);
        info!("Renderer destroyed.");

        info!("App cleanup finished. VulkanBase will now be dropped.");
    }
}
