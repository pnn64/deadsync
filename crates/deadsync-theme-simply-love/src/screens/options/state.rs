use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavWrap {
    Wrap,
    Clamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmenuKind {
    System,
    Graphics,
    Input,
    InputBackend,
    SmxConfig,
    Lights,
    OnlineScoring,
    NullOrDie,
    NullOrDieOptions,
    SyncPacks,
    Machine,
    Advanced,
    Course,
    Gameplay,
    Sound,
    SelectMusic,
    GrooveStats,
    ArrowCloud,
    ScoreImport,
    Folders,
}

impl SubmenuKind {
    pub(super) const ALL: [Self; 20] = [
        Self::System,
        Self::Graphics,
        Self::Input,
        Self::InputBackend,
        Self::SmxConfig,
        Self::Lights,
        Self::OnlineScoring,
        Self::NullOrDie,
        Self::NullOrDieOptions,
        Self::SyncPacks,
        Self::Machine,
        Self::Advanced,
        Self::Course,
        Self::Gameplay,
        Self::Sound,
        Self::SelectMusic,
        Self::GrooveStats,
        Self::ArrowCloud,
        Self::ScoreImport,
        Self::Folders,
    ];
    pub(super) const COUNT: usize = Self::ALL.len();

    #[inline]
    pub(super) const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug)]
pub(super) struct SubmenuState {
    pub(super) choice_indices: Vec<usize>,
    pub(super) cursor_indices: Vec<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct SubmenuStates([SubmenuState; SubmenuKind::COUNT]);

impl SubmenuStates {
    pub(super) fn new(init: impl FnMut(usize) -> SubmenuState) -> Self {
        Self(std::array::from_fn(init))
    }

    pub(super) fn iter_mut(&mut self) -> std::slice::IterMut<'_, SubmenuState> {
        self.0.iter_mut()
    }
}

impl std::ops::Index<SubmenuKind> for SubmenuStates {
    type Output = SubmenuState;
    #[inline]
    fn index(&self, kind: SubmenuKind) -> &SubmenuState {
        &self.0[kind.index()]
    }
}

impl std::ops::IndexMut<SubmenuKind> for SubmenuStates {
    #[inline]
    fn index_mut(&mut self, kind: SubmenuKind) -> &mut SubmenuState {
        &mut self.0[kind.index()]
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct OptionsStartInput {
    pub(super) held: bool,
    pub(super) held_for: Duration,
    pub(super) next_repeat_at: Duration,
}

#[inline(always)]
pub(super) const fn is_launcher_submenu(kind: SubmenuKind) -> bool {
    matches!(
        kind,
        SubmenuKind::Input | SubmenuKind::OnlineScoring | SubmenuKind::NullOrDie
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsView {
    Main,
    Submenu(SubmenuKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DescriptionCacheKey {
    Main(usize),
    Submenu(SubmenuKind, usize),
}

/// A pre-wrapped block of text in the description pane, ready for rendering.
#[derive(Clone, Debug)]
pub(super) enum RenderedHelpBlock {
    Paragraph { text: Arc<str>, line_count: usize },
    Bullet { text: Arc<str>, line_count: usize },
}

#[derive(Clone, Debug)]
pub(super) struct DescriptionLayout {
    pub(super) key: DescriptionCacheKey,
    pub(super) blocks: Vec<RenderedHelpBlock>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SubmenuTransition {
    None,
    FadeOutToSubmenu,
    FadeInSubmenu,
    FadeOutToMain,
    FadeInMain,
}

pub struct State {
    pub(super) updater_capabilities: SimplyLoveUpdaterCapabilities,
    pub(super) app_paths: AppPathsView,
    pub(super) audio_options: AudioOptionsView,
    pub(super) song_packs: Vec<OptionsSongPackView>,
    pub(super) pack_sync: OptionsPackSyncView,
    pub(super) scorebox_cycle_mask: u8,
    pub(super) auto_screenshot_mask: u8,
    pub(super) chart_info_mask: u8,
    pub selected: usize,
    pub(super) prev_selected: usize,
    pub active_color_index: i32, // <-- ADDED
    pub(super) bg: visual_style_bg::State,
    pub(super) nav_key_held_direction: Option<NavDirection>,
    pub(super) nav_key_held_for: Duration,
    pub(super) nav_key_next_repeat_at: Duration,
    pub(super) nav_lr_held_direction: Option<isize>,
    pub(super) nav_lr_held_for: Duration,
    pub(super) nav_lr_next_repeat_at: Duration,
    pub(super) view: OptionsView,
    pub(super) submenu_transition: SubmenuTransition,
    pub(super) pending_submenu_kind: Option<SubmenuKind>,
    pub(super) pending_submenu_parent_kind: Option<SubmenuKind>,
    pub(super) submenu_parent_kind: Option<SubmenuKind>,
    pub(super) submenu_fade_t: f32,
    pub(super) content_alpha: f32,
    pub(super) reload_ui: Option<ReloadUiState>,
    pub(super) download_packs_overlay: DownloadPacksOverlayState,
    pub(super) stepmaniaonline_snapshot: Arc<deadsync_online::stepmaniaonline::Snapshot>,
    pub(super) pending_pack_reload_dirs: Vec<PathBuf>,
    pub(super) score_import_ui: Option<ScoreImportUiState>,
    pub(super) apply_replaygain_ui: Option<ApplyReplayGainUiState>,
    pub(super) pack_sync_overlay: shared_pack_sync::OverlayState,
    pub(super) score_import_confirm: Option<ScoreImportConfirmState>,
    pub(super) sync_pack_confirm: Option<SyncPackConfirmState>,
    pub(super) menu_lr_chord: screen_input::MenuLrChordTracker,
    pub(super) menu_lr_undo: i8,
    pub(super) start_input: [OptionsStartInput; 2],
    pub(super) pending_dedicated_menu_buttons: Option<bool>,
    pub(super) pending_sfx: Vec<&'static str>,
    pub(super) pending_sync: Vec<crate::SimplyLoveSyncRequest>,
    pub(super) pending_online: Vec<crate::SimplyLoveOnlineRequest>,
    // Submenu state
    pub(super) sub_selected: usize,
    pub(super) sub_prev_selected: usize,
    pub(super) sub_inline_x: f32,
    pub(super) sub: SubmenuStates,
    pub(super) system_noteskin_choices: Vec<String>,
    pub(super) smx_bg_pack_choices: Vec<String>,
    pub(super) smx_judge_pack_choices: Vec<String>,
    pub(super) smx_assignment: SmxAssignmentView,
    pub(super) smx_assignment_status: String,
    pub(super) score_import_profiles: Vec<crate::SimplyLoveScoreImportProfile>,
    pub(super) score_import_profile_choices: Vec<String>,
    pub(super) score_import_profile_ids: Vec<Option<String>>,
    pub(super) score_import_pack_options: Vec<ScoreImportPackOption>,
    pub(super) score_import_pack_selected: HashSet<String>,
    pub(super) score_import_pack_picker: Option<ScoreImportPackPicker>,
    pub(super) sync_pack_choices: Vec<String>,
    pub(super) sync_pack_filters: Vec<Option<String>>,
    pub(super) sound_device_options: Vec<SoundDeviceOption>,
    #[cfg(target_os = "linux")]
    pub(super) linux_backend_choices: Vec<String>,
    pub(super) master_volume_pct: i32,
    pub(super) sfx_volume_pct: i32,
    pub(super) assist_tick_volume_pct: i32,
    pub(super) music_volume_pct: i32,
    pub(super) smx_default_light_brightness_pct: i32,
    pub(super) global_offset_ms: i32,
    pub(super) visual_delay_ms: i32,
    pub(super) input_debounce_ms: i32,
    pub(super) null_or_die_fingerprint_tenths: i32,
    pub(super) null_or_die_window_tenths: i32,
    pub(super) null_or_die_step_tenths: i32,
    pub(super) null_or_die_magic_offset_tenths: i32,
    pub(super) video_renderer_at_load: RendererChoice,
    pub(super) display_mode_at_load: DisplayModeChoice,
    pub(super) display_monitor_at_load: usize,
    pub(super) display_width_at_load: u32,
    pub(super) display_height_at_load: u32,
    pub(super) max_fps_at_load: u16,
    pub(super) vsync_at_load: bool,
    pub(super) present_mode_policy_at_load: PresentPolicyChoice,
    pub(super) high_dpi_at_load: bool,
    pub(super) software_threads_at_load: u8,
    pub(super) display_mode_choices: Vec<String>,
    pub(super) software_thread_choices: Vec<u8>,
    pub(super) software_thread_labels: Vec<String>,
    pub(super) max_fps_choices: Vec<u16>,
    pub(super) resolution_choices: Vec<(u32, u32)>,
    pub(super) refresh_rate_choices: Vec<u32>, // New: stored in millihertz
    // Hardware info
    pub monitor_specs: Vec<GraphicsMonitorView>,
    // Cursor ring tween (StopTweening/BeginTweening parity with ITGmania ScreenOptions::TweenCursor).
    pub(super) cursor_initialized: bool,
    pub(super) cursor_from_x: f32,
    pub(super) cursor_from_y: f32,
    pub(super) cursor_from_w: f32,
    pub(super) cursor_from_h: f32,
    pub(super) cursor_to_x: f32,
    pub(super) cursor_to_y: f32,
    pub(super) cursor_to_w: f32,
    pub(super) cursor_to_h: f32,
    pub(super) cursor_t: f32,
    // Shared row tween state for the active view (main list or submenu list).
    pub(super) row_tweens: Vec<RowTween>,
    pub(super) submenu_layout_cache_kind: Cell<Option<SubmenuKind>>,
    pub(super) submenu_row_layout_cache: RefCell<Vec<Option<SubmenuRowLayout>>>,
    pub(super) description_layout_cache: RefCell<Option<DescriptionLayout>>,
    pub(super) graphics_prev_visible_rows: Vec<usize>,
    pub(super) advanced_prev_visible_rows: Vec<usize>,
    pub(super) select_music_prev_visible_rows: Vec<usize>,
    pub(super) i18n_revision: u64,
}

pub fn init(view: OptionsInitView) -> State {
    let OptionsInitView {
        config: cfg,
        updater_capabilities,
        app_paths,
        audio: audio_options,
        graphics: graphics_options,
        song_packs,
        pack_sync,
        noteskins: noteskin_catalog,
        machine_noteskin,
        smx_assignment,
        smx_gifs: smx_gif_catalog,
        score_import_profiles,
    } = view;
    let mut system_noteskin_choices = noteskin_catalog.names;
    if system_noteskin_choices.is_empty() {
        system_noteskin_choices.push(deadsync_profile::NoteSkin::DEFAULT_NAME.to_string());
    }
    let smx_bg_pack_choices = smx_gif_catalog.background_packs;
    let smx_judge_pack_choices = smx_gif_catalog.judgment_packs;
    let software_thread_choices = graphics_options.software_thread_choices.clone();
    let software_thread_labels = software_thread_choice_labels(&software_thread_choices);
    let max_fps_choices = build_max_fps_choices();
    let sound_device_options = build_sound_device_options(&audio_options);
    let master_volume_pct = i32::from(audio_options.master_volume.min(100));
    let sfx_volume_pct = i32::from(audio_options.sfx_volume.min(100));
    let assist_tick_volume_pct = i32::from(audio_options.assist_tick_volume.min(100));
    let music_volume_pct = i32::from(audio_options.music_volume.min(100));
    #[cfg(target_os = "linux")]
    let linux_backend_choices = build_linux_backend_choices(&audio_options);
    let smx_assignment_status = smx_assignment_status(&smx_assignment);
    let machine_noteskin_idx = system_noteskin_choices
        .iter()
        .position(|name| name.eq_ignore_ascii_case(machine_noteskin.as_str()))
        .unwrap_or(0);
    let mut state = State {
        updater_capabilities,
        app_paths,
        audio_options,
        song_packs,
        pack_sync,
        scorebox_cycle_mask: scorebox_cycle_mask_from_config(&cfg),
        auto_screenshot_mask: cfg.auto_screenshot_eval,
        chart_info_mask: select_music_chart_info_mask_from_config(&cfg),
        selected: 0,
        prev_selected: 0,
        active_color_index: cfg.simply_love_color,
        bg: visual_style_bg::State::new(),

        nav_key_held_direction: None,
        nav_key_held_for: Duration::ZERO,
        nav_key_next_repeat_at: NAV_INITIAL_HOLD_DELAY,
        nav_lr_held_direction: None,
        nav_lr_held_for: Duration::ZERO,
        nav_lr_next_repeat_at: NAV_INITIAL_HOLD_DELAY,
        submenu_transition: SubmenuTransition::None,
        pending_submenu_kind: None,
        pending_submenu_parent_kind: None,
        submenu_parent_kind: None,
        submenu_fade_t: 0.0,
        content_alpha: 1.0,
        reload_ui: None,
        download_packs_overlay: DownloadPacksOverlayState::Hidden,
        stepmaniaonline_snapshot: Arc::new(deadsync_online::stepmaniaonline::Snapshot::default()),
        pending_pack_reload_dirs: Vec::new(),
        score_import_ui: None,
        apply_replaygain_ui: None,
        pack_sync_overlay: shared_pack_sync::OverlayState::Hidden,
        score_import_confirm: None,
        sync_pack_confirm: None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: 0,
        start_input: [OptionsStartInput::default(); 2],
        pending_dedicated_menu_buttons: None,
        pending_sfx: Vec::new(),
        pending_sync: Vec::new(),
        pending_online: Vec::new(),
        view: OptionsView::Main,
        sub_selected: 0,
        sub_prev_selected: 0,
        sub_inline_x: f32::NAN,
        sub: SubmenuStates::new(|i| {
            let len = submenu_rows(SubmenuKind::ALL[i]).len();
            SubmenuState {
                choice_indices: vec![0; len],
                cursor_indices: vec![0; len],
            }
        }),
        system_noteskin_choices,
        smx_bg_pack_choices,
        smx_judge_pack_choices,
        smx_assignment: smx_assignment.clone(),
        smx_assignment_status,
        score_import_profiles,
        score_import_profile_choices: vec![
            tr("OptionsScoreImport", "NoEligibleProfiles").to_string(),
        ],
        score_import_profile_ids: vec![None],
        score_import_pack_options: Vec::new(),
        score_import_pack_selected: HashSet::new(),
        score_import_pack_picker: None,
        sync_pack_choices: vec![tr("OptionsSyncPack", "AllPacks").to_string()],
        sync_pack_filters: vec![None],
        sound_device_options,
        #[cfg(target_os = "linux")]
        linux_backend_choices,
        master_volume_pct,
        sfx_volume_pct,
        assist_tick_volume_pct,
        music_volume_pct,
        smx_default_light_brightness_pct: i32::from(cfg.smx_default_light_brightness.min(100)),
        global_offset_ms: {
            let ms = (cfg.global_offset_seconds * 1000.0).round() as i32;
            ms.clamp(GLOBAL_OFFSET_MIN_MS, GLOBAL_OFFSET_MAX_MS)
        },
        visual_delay_ms: {
            let ms = (cfg.visual_delay_seconds * 1000.0).round() as i32;
            ms.clamp(VISUAL_DELAY_MIN_MS, VISUAL_DELAY_MAX_MS)
        },
        input_debounce_ms: {
            let ms = (cfg.input_debounce_seconds * 1000.0).round() as i32;
            ms.clamp(INPUT_DEBOUNCE_MIN_MS, INPUT_DEBOUNCE_MAX_MS)
        },
        null_or_die_fingerprint_tenths: tenths_from_f64(cfg.null_or_die_fingerprint_ms).clamp(
            NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
            NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
        ),
        null_or_die_window_tenths: tenths_from_f64(cfg.null_or_die_window_ms).clamp(
            NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
            NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
        ),
        null_or_die_step_tenths: tenths_from_f64(cfg.null_or_die_step_ms).clamp(
            NULL_OR_DIE_POSITIVE_MS_MIN_TENTHS,
            NULL_OR_DIE_POSITIVE_MS_MAX_TENTHS,
        ),
        null_or_die_magic_offset_tenths: tenths_from_f64(cfg.null_or_die_magic_offset_ms).clamp(
            NULL_OR_DIE_MAGIC_OFFSET_MIN_TENTHS,
            NULL_OR_DIE_MAGIC_OFFSET_MAX_TENTHS,
        ),
        video_renderer_at_load: graphics_options.renderer,
        display_mode_at_load: graphics_options.display_mode,
        display_monitor_at_load: graphics_options.monitor,
        display_width_at_load: graphics_options.width,
        display_height_at_load: graphics_options.height,
        max_fps_at_load: graphics_options.max_fps,
        vsync_at_load: graphics_options.vsync,
        present_mode_policy_at_load: graphics_options.present_policy,
        high_dpi_at_load: graphics_options.high_dpi,
        software_threads_at_load: graphics_options.software_threads,
        display_mode_choices: build_display_mode_choices(&[]),
        software_thread_choices,
        software_thread_labels,
        max_fps_choices,
        resolution_choices: Vec::new(),
        refresh_rate_choices: Vec::new(),
        monitor_specs: Vec::new(),
        cursor_initialized: false,
        cursor_from_x: 0.0,
        cursor_from_y: 0.0,
        cursor_from_w: 0.0,
        cursor_from_h: 0.0,
        cursor_to_x: 0.0,
        cursor_to_y: 0.0,
        cursor_to_w: 0.0,
        cursor_to_h: 0.0,
        cursor_t: 1.0,
        row_tweens: Vec::new(),
        submenu_layout_cache_kind: Cell::new(None),
        submenu_row_layout_cache: RefCell::new(Vec::new()),
        description_layout_cache: RefCell::new(None),
        graphics_prev_visible_rows: Vec::new(),
        advanced_prev_visible_rows: Vec::new(),
        select_music_prev_visible_rows: Vec::new(),
        i18n_revision: crate::assets::i18n::revision(),
    };

    sync_video_renderer(&mut state, graphics_options.renderer);
    sync_display_mode(
        &mut state,
        graphics_options.display_mode,
        graphics_options.fullscreen,
        graphics_options.monitor,
        1,
    );
    sync_display_resolution(&mut state, graphics_options.width, graphics_options.height);

    set_choice_by_id(
        &mut state.sub[SubmenuKind::System].choice_indices,
        SYSTEM_OPTIONS_ROWS,
        SubRowId::Game,
        0,
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::System].choice_indices,
        SYSTEM_OPTIONS_ROWS,
        SubRowId::Theme,
        0,
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::System].choice_indices,
        SYSTEM_OPTIONS_ROWS,
        SubRowId::Language,
        language_choice_index(cfg.language_flag),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::System].choice_indices,
        SYSTEM_OPTIONS_ROWS,
        SubRowId::LogLevel,
        log_level_choice_index(cfg.log_level),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::System].choice_indices,
        SYSTEM_OPTIONS_ROWS,
        SubRowId::LogFile,
        usize::from(cfg.log_to_file),
    );
    if let Some(noteskin_row_idx) = SYSTEM_OPTIONS_ROWS
        .iter()
        .position(|row| row.id == SubRowId::DefaultNoteSkin)
        && let Some(slot) = state.sub[SubmenuKind::System]
            .choice_indices
            .get_mut(noteskin_row_idx)
    {
        *slot = machine_noteskin_idx;
    }

    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::PresentMode,
        present_mode_choice_index(graphics_options.vsync, graphics_options.present_policy),
    );
    sync_max_fps(&mut state, graphics_options.max_fps);
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::ShowStats,
        cfg.show_stats_mode.min(3) as usize,
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::ValidationLayers,
        yes_no_choice_index(cfg.gfx_debug),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::HighDpi,
        yes_no_choice_index(graphics_options.high_dpi),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::HideMouseCursor,
        yes_no_choice_index(cfg.hide_mouse_cursor),
    );
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::SoftwareRendererThreads,
    ) {
        *slot = thread_choice_index(
            &state.software_thread_choices,
            graphics_options.software_threads,
        );
    }
    #[cfg(target_os = "windows")]
    set_choice_by_id(
        &mut state.sub[SubmenuKind::InputBackend].choice_indices,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::GamepadBackend,
        windows_backend_choice_index(cfg.windows_gamepad_backend),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::InputBackend].choice_indices,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::UseFsrs,
        yes_no_choice_index(cfg.use_fsrs),
    );
    // StepManiaX config sub-page choices.
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SmxConfig].choice_indices,
        SMX_CONFIG_OPTIONS_ROWS,
        SubRowId::SmxInput,
        yes_no_choice_index(cfg.smx_input),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SmxConfig].choice_indices,
        SMX_CONFIG_OPTIONS_ROWS,
        SubRowId::SmxPanelLights,
        yes_no_choice_index(cfg.smx_panel_lights),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SmxConfig].choice_indices,
        SMX_CONFIG_OPTIONS_ROWS,
        SubRowId::SmxUnderglowTheme,
        yes_no_choice_index(cfg.smx_underglow_theme),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SmxConfig].choice_indices,
        SMX_CONFIG_OPTIONS_ROWS,
        SubRowId::SmxUnderglowGrb,
        yes_no_choice_index(cfg.smx_underglow_grb),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SmxConfig].choice_indices,
        SMX_CONFIG_OPTIONS_ROWS,
        SubRowId::SmxManagesPadConfig,
        yes_no_choice_index(cfg.smx_manages_pad_config),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SmxConfig].choice_indices,
        SMX_CONFIG_OPTIONS_ROWS,
        SubRowId::SmxDefaultPadConfig,
        cfg.smx_default_pad_config.index(),
    );
    // Bg/judge pack selections: index 0 = default, 1..N = user packs. These rows'
    // real choice lists are provided dynamically by the layout (the static row
    // definition holds a single placeholder), so write the index directly like the
    // noteskin and software-thread rows do; `set_choice_by_id` would clamp it to
    // the placeholder length and always show "Default".
    let bg_pack_idx = if cfg.smx_pad_gifs_pack.is_empty() {
        0
    } else {
        state
            .smx_bg_pack_choices
            .iter()
            .position(|n| n == cfg.smx_pad_gifs_pack.as_str())
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::SmxConfig].choice_indices,
        SMX_CONFIG_OPTIONS_ROWS,
        SubRowId::SmxBgPack,
    ) {
        *slot = bg_pack_idx;
    }
    let judge_pack_idx = if cfg.smx_judge_gifs_pack.is_empty() {
        0
    } else {
        state
            .smx_judge_pack_choices
            .iter()
            .position(|n| n == cfg.smx_judge_gifs_pack.as_str())
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::SmxConfig].choice_indices,
        SMX_CONFIG_OPTIONS_ROWS,
        SubRowId::SmxJudgePack,
    ) {
        *slot = judge_pack_idx;
    }
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SmxConfig].choice_indices,
        SMX_CONFIG_OPTIONS_ROWS,
        SubRowId::SmxIdleLights,
        usize::from(cfg.smx_idle_lights_black),
    );
    // Single-pad P1/P2 picker: reflect the slot the SDK currently has the lone pad
    // in (slot 1 = P2, index 1; slot 0 = P1, index 0). The slot already accounts for
    // both the saved serial assignment and the hardware jumper, so reading it covers
    // a pad placed on P2 by its jumper alone, which a serial-only comparison misses.
    let single_pad_is_p2 = smx_assignment.pads[1].connected;
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SmxConfig].choice_indices,
        SMX_CONFIG_OPTIONS_ROWS,
        SubRowId::SmxSinglePadPlayer,
        usize::from(single_pad_is_p2),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::InputBackend].choice_indices,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::MenuNavigation,
        usize::from(cfg.three_key_navigation),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::InputBackend].choice_indices,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::OptionsNavigation,
        usize::from(cfg.arcade_options_navigation),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::InputBackend].choice_indices,
        INPUT_BACKEND_OPTIONS_ROWS,
        SubRowId::MenuButtons,
        usize::from(cfg.only_dedicated_menu_buttons),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Lights].choice_indices,
        LIGHTS_OPTIONS_ROWS,
        SubRowId::LightsDriver,
        lights_driver_choice_index(cfg.lights_driver),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Lights].choice_indices,
        LIGHTS_OPTIONS_ROWS,
        SubRowId::GameplayPadLights,
        lights_gameplay_pad_choice_index(cfg.lights_gameplay_pad_lights),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Lights].choice_indices,
        LIGHTS_OPTIONS_ROWS,
        SubRowId::LightsSimplifyBass,
        yes_no_choice_index(cfg.lights_simplify_bass),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectProfile,
        usize::from(cfg.machine_show_select_profile),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectColor,
        usize::from(cfg.machine_show_select_color),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::PreferredColor,
        cfg.simply_love_color
            .rem_euclid(color::DECORATIVE_RGBA.len() as i32) as usize,
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectStyle,
        usize::from(cfg.machine_show_select_style),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::PreferredStyle,
        machine_preferred_play_style_choice_index(cfg.machine_preferred_style),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::SelectPlayMode,
        usize::from(cfg.machine_show_select_play_mode),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::PreferredMode,
        machine_preferred_play_mode_choice_index(cfg.machine_preferred_play_mode),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::Font,
        machine_font_choice_index(cfg.machine_font),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::BarColor,
        machine_bar_color_choice_index(cfg.machine_bar_color),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::EvaluationStyle,
        machine_evaluation_style_choice_index(cfg.machine_evaluation_style),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::EvalSummary,
        usize::from(cfg.machine_show_eval_summary),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::NiceSound,
        usize::from(cfg.machine_nice_sound),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::NameEntry,
        usize::from(cfg.machine_show_name_entry),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::GameoverScreen,
        usize::from(cfg.machine_show_gameover),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::MenuMusic,
        usize::from(cfg.menu_music),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::VisualStyle,
        visual_style_choice_index(cfg.visual_style),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::ThemeVariant,
        srpg_variant_choice_index(cfg.srpg_variant),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::Replays,
        usize::from(cfg.machine_enable_replays),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::HeartRateMonitors,
        usize::from(cfg.machine_enable_heart_rate_monitors),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::PerPlayerGlobalOffsets,
        usize::from(cfg.machine_allow_per_player_global_offsets),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::PackIniOffsets,
        usize::from(cfg.machine_pack_ini_offsets),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::DefaultSyncOffset,
        default_sync_offset_choice_index(cfg.machine_default_sync_offset),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::KeyboardFeatures,
        usize::from(cfg.keyboard_features),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::VideoBgs,
        usize::from(cfg.show_video_backgrounds),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::RandomBackgroundMode,
        random_background_mode_choice_index(cfg.random_background_mode),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::VersionOverlay,
        usize::from(cfg.show_version_overlay),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::VersionOverlaySide,
        version_overlay_side_choice_index(cfg.version_overlay_side),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Machine].choice_indices,
        MACHINE_OPTIONS_ROWS,
        SubRowId::WriteCurrentScreen,
        usize::from(cfg.write_current_screen),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Advanced].choice_indices,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::DefaultFailType,
        default_fail_type_choice_index(cfg.default_fail_type),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Advanced].choice_indices,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::BannerCache,
        usize::from(cfg.banner_cache),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Advanced].choice_indices,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::CdTitleCache,
        usize::from(cfg.cdtitle_cache),
    );
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Advanced].choice_indices,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::SongParsingThreads,
    ) {
        *slot = thread_choice_index(&state.software_thread_choices, cfg.song_parsing_threads);
    }
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Advanced].choice_indices,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::CacheSongs,
        usize::from(cfg.cachesongs),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Advanced].choice_indices,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::FastLoad,
        usize::from(cfg.fastload),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Advanced].choice_indices,
        ADVANCED_OPTIONS_ROWS,
        SubRowId::AllowSongDeletion,
        usize::from(cfg.allow_song_deletion),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::SyncGraph,
        sync_graph_mode_choice_index(cfg.null_or_die_sync_graph),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::GraphOrientation,
        null_or_die_graph_orientation_choice_index(cfg.null_or_die_graph_orientation),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::SyncConfidence,
        sync_confidence_choice_index(cfg.null_or_die_confidence_percent),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::PackSyncThreads,
        thread_choice_index(
            &state.software_thread_choices,
            cfg.null_or_die_pack_sync_threads,
        ),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::KernelTarget,
        null_or_die_kernel_target_choice_index(cfg.null_or_die_kernel_target),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::KernelType,
        null_or_die_kernel_type_choice_index(cfg.null_or_die_kernel_type),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::NullOrDieOptions].choice_indices,
        NULL_OR_DIE_OPTIONS_ROWS,
        SubRowId::FullSpectrogram,
        yes_no_choice_index(cfg.null_or_die_full_spectrogram),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Course].choice_indices,
        COURSE_OPTIONS_ROWS,
        SubRowId::ShowRandomCourses,
        yes_no_choice_index(cfg.show_random_courses),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Course].choice_indices,
        COURSE_OPTIONS_ROWS,
        SubRowId::ShowMostPlayed,
        yes_no_choice_index(cfg.show_most_played_courses),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Course].choice_indices,
        COURSE_OPTIONS_ROWS,
        SubRowId::ShowIndividualScores,
        yes_no_choice_index(cfg.show_course_individual_scores),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Course].choice_indices,
        COURSE_OPTIONS_ROWS,
        SubRowId::AutosubmitIndividual,
        yes_no_choice_index(cfg.autosubmit_course_scores_individually),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].choice_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::BgBrightness,
        bg_brightness_choice_index(cfg.bg_brightness),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].choice_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::CenteredP1Notefield,
        usize::from(cfg.center_1player_notefield),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].choice_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::AnimatedBanners,
        match cfg.gameplay_banner_mode {
            config::GameplayBannerMode::Static => 0,
            config::GameplayBannerMode::Once => 1,
            config::GameplayBannerMode::Loop => 2,
        },
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].choice_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::ZmodRatingBox,
        usize::from(cfg.zmod_rating_box_text),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].choice_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::BpmDecimal,
        usize::from(cfg.show_bpm_decimal),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].choice_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::BpmPosition,
        usize::from(cfg.gameplay_bpm_position == config::GameplayBpmPosition::NearField),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].choice_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::DelayedBack,
        usize::from(cfg.delayed_back),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Gameplay].choice_indices,
        GAMEPLAY_OPTIONS_ROWS,
        SubRowId::AutoScreenshot,
        auto_screenshot_cursor_index(cfg.auto_screenshot_eval),
    );

    let sound_device_idx = sound_device_choice_index(
        &state.sound_device_options,
        state.audio_options.output_device,
    );
    set_sound_choice_index(&mut state, SubRowId::SoundDevice, sound_device_idx);
    let output_mode = state.audio_options.output_mode;
    set_sound_choice_index(
        &mut state,
        SubRowId::AudioOutputMode,
        output_mode.choice_index(),
    );
    #[cfg(target_os = "linux")]
    let linux_backend_idx =
        linux_audio_backend_choice_index(&state, &state.audio_options.selected_backend_name);
    #[cfg(target_os = "linux")]
    set_sound_choice_index(&mut state, SubRowId::LinuxAudioBackend, linux_backend_idx);
    #[cfg(target_os = "linux")]
    set_sound_choice_index(
        &mut state,
        SubRowId::AlsaExclusive,
        output_mode.exclusive_choice_index(),
    );
    let sound_rate_idx = sample_rate_choice_index(&state, state.audio_options.sample_rate_hz);
    set_sound_choice_index(&mut state, SubRowId::AudioSampleRate, sound_rate_idx);
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Sound].choice_indices,
        SOUND_OPTIONS_ROWS,
        SubRowId::MineSounds,
        usize::from(cfg.mine_hit_sound),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Sound].choice_indices,
        SOUND_OPTIONS_ROWS,
        SubRowId::RateModPreservesPitch,
        usize::from(state.audio_options.preserve_pitch),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Sound].choice_indices,
        SOUND_OPTIONS_ROWS,
        SubRowId::ReplayGain,
        usize::from(state.audio_options.replay_gain),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowBanners,
        yes_no_choice_index(cfg.show_select_music_banners),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowVideoBanners,
        yes_no_choice_index(cfg.show_select_music_video_banners),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowBreakdown,
        yes_no_choice_index(cfg.show_select_music_breakdown),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::BreakdownStyle,
        breakdown_style_choice_index(cfg.select_music_breakdown_style),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowNativeLanguage,
        translated_titles_choice_index(cfg.translated_titles),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::MusicWheelSpeed,
        music_wheel_scroll_speed_choice_index(cfg.music_wheel_switch_speed),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::MusicWheelStyle,
        select_music_wheel_style_choice_index(cfg.select_music_wheel_style),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::HideInactiveSeries,
        yes_no_choice_index(cfg.hide_inactive_series),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::SeriesSort,
        yes_no_choice_index(cfg.sort_music_wheel_by_series),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::SongSelectBg,
        select_music_song_select_bg_mode_choice_index(cfg.select_music_song_select_bg_mode),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::SwitchProfile,
        yes_no_choice_index(cfg.allow_switch_profile_in_menu),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowCdTitles,
        yes_no_choice_index(cfg.show_select_music_cdtitles),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowWheelGrades,
        yes_no_choice_index(cfg.show_music_wheel_grades),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowWheelLamps,
        yes_no_choice_index(cfg.show_music_wheel_lamps),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ItlRank,
        select_music_itl_rank_mode_choice_index(cfg.select_music_itl_rank_mode),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ItlWheelData,
        select_music_itl_wheel_mode_choice_index(cfg.select_music_itl_wheel_mode),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::NewPackBadge,
        select_music_new_pack_mode_choice_index(cfg.select_music_new_pack_mode),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::FolderStats,
        yes_no_choice_index(cfg.show_select_music_folder_stats),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowPatternInfo,
        select_music_pattern_info_mode_choice_index(cfg.select_music_pattern_info_mode),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::StepArtistBox,
        select_music_step_artist_box_mode_choice_index(cfg.select_music_step_artist_box_mode),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ChartInfo,
        select_music_chart_info_cursor_index(
            cfg.select_music_chart_info_peak_nps,
            cfg.select_music_chart_info_effective_bpm,
            cfg.select_music_chart_info_matrix_rating,
        ),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::MusicPreviews,
        yes_no_choice_index(cfg.show_select_music_previews),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::PreviewMarker,
        yes_no_choice_index(cfg.show_select_music_preview_marker),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::LoopMusic,
        usize::from(cfg.select_music_preview_loop),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::PreviewStartsImmediately,
        yes_no_choice_index(cfg.select_music_preview_starts_immediately),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowGameplayTimer,
        yes_no_choice_index(cfg.show_select_music_gameplay_timer),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowStageDisplay,
        yes_no_choice_index(cfg.show_select_music_stage_display),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::ShowGsBox,
        yes_no_choice_index(cfg.show_select_music_scorebox),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::GsBoxPlacement,
        select_music_scorebox_placement_choice_index(cfg.select_music_scorebox_placement),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::SelectMusic].choice_indices,
        SELECT_MUSIC_OPTIONS_ROWS,
        SubRowId::GsBoxLeaderboards,
        scorebox_cycle_cursor_index(
            cfg.select_music_scorebox_cycle_itg,
            cfg.select_music_scorebox_cycle_ex,
            cfg.select_music_scorebox_cycle_hard_ex,
            cfg.select_music_scorebox_cycle_tournaments,
        ),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::GrooveStats].choice_indices,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::EnableGrooveStats,
        yes_no_choice_index(cfg.enable_groovestats),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::GrooveStats].choice_indices,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::ShowSrpgShop,
        yes_no_choice_index(cfg.show_srpg_shop),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::GrooveStats].choice_indices,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::SrpgShopFolder,
        srpg_shop_folder_choice_index(cfg.srpg_shop_folder),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::GrooveStats].choice_indices,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::EnableBoogieStats,
        yes_no_choice_index(cfg.enable_boogiestats),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::GrooveStats].choice_indices,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::AutoPopulateScores,
        yes_no_choice_index(cfg.auto_populate_gs_scores),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::GrooveStats].choice_indices,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::AutoDownloadUnlocks,
        yes_no_choice_index(cfg.auto_download_unlocks),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::GrooveStats].choice_indices,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::SeparateUnlocksByPlayer,
        yes_no_choice_index(cfg.separate_unlocks_by_player),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::ArrowCloud].choice_indices,
        ARROWCLOUD_OPTIONS_ROWS,
        SubRowId::EnableArrowCloud,
        yes_no_choice_index(cfg.enable_arrowcloud),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::ArrowCloud].choice_indices,
        ARROWCLOUD_OPTIONS_ROWS,
        SubRowId::ArrowCloudSubmitFails,
        yes_no_choice_index(cfg.submit_arrowcloud_fails),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::ArrowCloud].choice_indices,
        ARROWCLOUD_OPTIONS_ROWS,
        SubRowId::ArrowCloudQrLogin,
        arrowcloud_qr_login_when_choice_index(cfg.arrowcloud_qr_login_when),
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::GrooveStats].choice_indices,
        GROOVESTATS_OPTIONS_ROWS,
        SubRowId::GrooveStatsQrLogin,
        groovestats_qr_login_when_choice_index(cfg.groovestats_qr_login_when),
    );
    refresh_score_import_options(&mut state);
    refresh_null_or_die_options(&mut state);
    set_choice_by_id(
        &mut state.sub[SubmenuKind::ScoreImport].choice_indices,
        SCORE_IMPORT_OPTIONS_ROWS,
        SubRowId::ScoreImportOnlyMissing,
        yes_no_choice_index(false),
    );
    sync_submenu_cursor_indices(&mut state);
    state
}
