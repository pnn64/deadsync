use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use deadlib_platform::display::{self, FullscreenType, MonitorSpec};
use deadlib_present::space::{self, Metrics};
use deadlib_render::{BackendType, PresentModePolicy};
use deadlib_renderer::{Backend, create_backend, render_size_for_physical, render_size_for_window};
use deadsync_assets::AssetManager;
use deadsync_config::app_config::DisplayMode;
use deadsync_theme_simply_love::views::SimplyLoveDensityGraphSlot as DensityGraphSlot;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::dynamic_media::DynamicMedia;
use crate::window::{AppWindowConfig, AppWindowSetup, DisplayModeChange, ResolutionChange};
use crate::{Command, ShellState};

#[derive(Clone, Copy)]
pub struct RendererInitConfig {
    pub backend_type: BackendType,
    pub high_dpi: bool,
    pub vsync_enabled: bool,
    pub present_mode_policy: PresentModePolicy,
    pub gfx_debug_enabled: bool,
    pub software_renderer_threads: u8,
}

pub struct RendererInitResult {
    pub window: Arc<Window>,
    pub backend: Backend,
    pub metrics: Metrics,
}

#[derive(Clone, Copy)]
pub struct RendererStartupSettings {
    pub backend_type: BackendType,
    pub high_dpi: bool,
    pub fallback_fullscreen_type: FullscreenType,
    pub hide_cursor: bool,
    pub gfx_debug_enabled: bool,
    pub software_renderer_threads: u8,
}

#[derive(Clone, Copy)]
pub struct RendererStartupConfig {
    pub window: AppWindowConfig,
    pub renderer: RendererInitConfig,
}

pub struct RendererStartupResult {
    pub window: Arc<Window>,
    pub backend: Backend,
    pub monitor_count: usize,
    pub fullscreen_type: FullscreenType,
}

#[derive(Clone, Copy)]
pub struct RendererSwitchWindowConfig {
    pub backend_type: BackendType,
    pub high_dpi: bool,
    pub display_mode: DisplayMode,
    pub current_width: u32,
    pub current_height: u32,
    pub desired_size: Option<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererSwitchWindowResult {
    pub width: u32,
    pub height: u32,
    pub pending_position: Option<PhysicalPosition<i32>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererSwitchRequest {
    pub current: BackendType,
    pub target: BackendType,
    pub force_recreate: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererSwitchRestoreState {
    pub display_mode: DisplayMode,
    pub fullscreen_type: FullscreenType,
    pub monitor: usize,
    pub monitor_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsDisplaySync {
    pub mode: DisplayMode,
    pub fullscreen_type: FullscreenType,
    pub monitor: usize,
    pub monitor_count: usize,
    pub persist_mode: bool,
    pub persist_monitor: bool,
}

#[derive(Clone, Copy)]
pub struct RendererSwitchPlan {
    pub previous: BackendType,
    pub target: BackendType,
    pub window_config: RendererSwitchWindowConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererSwitchBeginPlan {
    pub target: BackendType,
    pub clear_window: bool,
    pub clear_focus: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererSwitchSuccessPlan {
    pub target: BackendType,
    pub persist_renderer: bool,
    pub sync_options_renderer: bool,
    pub clear_present_runtime: bool,
    pub reset_dynamic_assets: bool,
    pub request_redraw: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererSwitchFailurePlan {
    pub previous: BackendType,
    pub persist_renderer: bool,
    pub sync_options_renderer: bool,
    pub restore_display: bool,
}

pub struct RendererSwitchResourceResetPlan {
    pub commands: Vec<Command>,
    pub refresh_select_music: bool,
    pub graph_key: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecreateDisplayState {
    pub mode: DisplayMode,
    pub persist_mode: bool,
    pub fullscreen_type: FullscreenType,
    pub monitor: usize,
    pub monitor_count: usize,
}

pub fn initialize_renderer(
    setup: AppWindowSetup,
    config: RendererInitConfig,
    assets: &mut AssetManager,
    dynamic_media: &mut DynamicMedia,
) -> Result<RendererInitResult, Box<dyn Error>> {
    let window = setup.window;
    let size = render_size_for_window(&window, config.backend_type, config.high_dpi);
    let metrics = space::metrics_for_window(size.width, size.height);
    space::set_current_metrics(metrics);

    let mut backend = create_backend(
        config.backend_type,
        window.clone(),
        config.vsync_enabled,
        config.present_mode_policy,
        config.gfx_debug_enabled,
        config.high_dpi,
    )?;
    if config.backend_type == BackendType::Software {
        backend.configure_software_threads(software_thread_count(config.software_renderer_threads));
    }
    assets.load_initial_assets(&mut backend, deadsync_theme_simply_love::asset_manifest())?;
    dynamic_media.preload_profile_avatars(assets, &mut backend);

    Ok(RendererInitResult {
        window,
        backend,
        metrics,
    })
}

pub fn renderer_startup_config(
    shell: &mut ShellState,
    settings: RendererStartupSettings,
) -> RendererStartupConfig {
    RendererStartupConfig {
        window: AppWindowConfig {
            backend_type: settings.backend_type,
            high_dpi: settings.high_dpi,
            width: shell.display_width,
            height: shell.display_height,
            monitor: shell.display_monitor,
            display_mode: shell.display_mode,
            fallback_fullscreen_type: settings.fallback_fullscreen_type,
            hide_cursor: settings.hide_cursor,
            pending_position: shell.pending_window_position.take(),
        },
        renderer: RendererInitConfig {
            backend_type: settings.backend_type,
            high_dpi: settings.high_dpi,
            vsync_enabled: shell.vsync_enabled,
            present_mode_policy: shell.present_mode_policy,
            gfx_debug_enabled: settings.gfx_debug_enabled,
            software_renderer_threads: settings.software_renderer_threads,
        },
    }
}

pub fn start_renderer_runtime(
    shell: &mut ShellState,
    event_loop: &ActiveEventLoop,
    config: RendererStartupConfig,
    assets: &mut AssetManager,
    dynamic_media: &mut DynamicMedia,
    now: Instant,
) -> Result<RendererStartupResult, Box<dyn Error>> {
    let setup = crate::window::create_app_window(event_loop, config.window)?;
    apply_app_window_setup_state(shell, &setup);
    let monitor_count = setup.monitor_count;
    let fullscreen_type = setup.fullscreen_type;
    let renderer = initialize_renderer(setup, config.renderer, assets, dynamic_media)?;
    apply_renderer_started(shell, renderer.metrics, now);
    Ok(RendererStartupResult {
        window: renderer.window,
        backend: renderer.backend,
        monitor_count,
        fullscreen_type,
    })
}

pub fn apply_app_window_setup_state(shell: &mut ShellState, setup: &AppWindowSetup) {
    shell.display_monitor = setup.monitor;
}

pub fn apply_renderer_started(shell: &mut ShellState, metrics: Metrics, now: Instant) {
    shell.metrics = metrics;
    shell.start_time = now;
    shell.last_title_update = now;
    shell.reset_frame_clock(now);
    shell.frame_count = 0;
    shell.current_frame_vpf = 0;
}

pub fn prepare_renderer_switch_window(
    window: Option<&Window>,
    config: RendererSwitchWindowConfig,
) -> RendererSwitchWindowResult {
    let (mut width, mut height) = config
        .desired_size
        .unwrap_or((config.current_width, config.current_height));
    let mut pending_position = None;
    if let Some(window) = window {
        if config.desired_size.is_none() {
            let size = render_size_for_window(window, config.backend_type, config.high_dpi);
            width = size.width;
            height = size.height;
        }
        if matches!(config.display_mode, DisplayMode::Fullscreen(_)) {
            window.set_fullscreen(None);
        }
        if matches!(config.display_mode, DisplayMode::Windowed)
            && let Ok(position) = window.outer_position()
        {
            pending_position = Some(position);
        }
        window.set_visible(false);
    }
    RendererSwitchWindowResult {
        width,
        height,
        pending_position,
    }
}

pub fn apply_renderer_switch_window_state(
    shell: &mut ShellState,
    result: RendererSwitchWindowResult,
) {
    shell.display_width = result.width;
    shell.display_height = result.height;
    shell.pending_window_position = result.pending_position;
}

pub fn renderer_switch_window_config(
    shell: &ShellState,
    backend_type: BackendType,
    high_dpi: bool,
    desired_size: Option<(u32, u32)>,
) -> RendererSwitchWindowConfig {
    RendererSwitchWindowConfig {
        backend_type,
        high_dpi,
        display_mode: shell.display_mode,
        current_width: shell.display_width,
        current_height: shell.display_height,
        desired_size,
    }
}

pub fn begin_renderer_switch(
    shell: &mut ShellState,
    window: Option<&Window>,
    backend: &mut Option<Backend>,
    assets: &mut AssetManager,
    dynamic_media: &mut DynamicMedia,
    config: RendererSwitchWindowConfig,
    now: Instant,
) {
    let window_state = prepare_renderer_switch_window(window, config);
    dispose_renderer(backend, assets, dynamic_media);
    apply_renderer_switch_window_state(shell, window_state);
    reset_renderer_switch_clock(shell, now);
}

pub fn reset_renderer_switch_clock(shell: &mut ShellState, now: Instant) {
    shell.frame_count = 0;
    shell.last_title_update = now;
    shell.reset_frame_clock(now);
}

pub fn dispose_renderer(
    backend: &mut Option<Backend>,
    assets: &mut AssetManager,
    dynamic_media: &mut DynamicMedia,
) {
    let Some(mut backend) = backend.take() else {
        return;
    };
    dynamic_media.destroy_assets(assets, &mut backend);
    let mut textures = assets.take_textures();
    backend.dispose_textures(&mut textures);
    backend.cleanup();
}

pub fn renderer_switch_needed(request: RendererSwitchRequest) -> bool {
    request.current != request.target || request.force_recreate
}

pub fn renderer_switch_plan(
    shell: &ShellState,
    request: RendererSwitchRequest,
    high_dpi: bool,
    desired_size: Option<(u32, u32)>,
) -> Option<RendererSwitchPlan> {
    renderer_switch_needed(request).then(|| RendererSwitchPlan {
        previous: request.current,
        target: request.target,
        window_config: renderer_switch_window_config(
            shell,
            request.current,
            high_dpi,
            desired_size,
        ),
    })
}

pub fn renderer_switch_resource_reset_commands() -> Vec<Command> {
    vec![
        Command::SetBanner(None),
        Command::SetCdTitle(None),
        Command::SetDensityGraph {
            slot: DensityGraphSlot::SelectMusicP1,
            chart_opt: None,
        },
        Command::SetDensityGraph {
            slot: DensityGraphSlot::SelectMusicP2,
            chart_opt: None,
        },
        Command::SetDynamicBackground(None),
    ]
}

pub fn renderer_switch_resource_reset_plan() -> RendererSwitchResourceResetPlan {
    RendererSwitchResourceResetPlan {
        commands: renderer_switch_resource_reset_commands(),
        refresh_select_music: true,
        graph_key: "__white",
    }
}

pub const fn renderer_switch_begin_plan(plan: &RendererSwitchPlan) -> RendererSwitchBeginPlan {
    RendererSwitchBeginPlan {
        target: plan.target,
        clear_window: true,
        clear_focus: true,
    }
}

pub const fn renderer_switch_success_plan(target: BackendType) -> RendererSwitchSuccessPlan {
    RendererSwitchSuccessPlan {
        target,
        persist_renderer: true,
        sync_options_renderer: true,
        clear_present_runtime: true,
        reset_dynamic_assets: true,
        request_redraw: true,
    }
}

pub const fn renderer_switch_failure_plan(previous: BackendType) -> RendererSwitchFailurePlan {
    RendererSwitchFailurePlan {
        previous,
        persist_renderer: true,
        sync_options_renderer: true,
        restore_display: true,
    }
}

pub fn available_monitor_specs(event_loop: &ActiveEventLoop) -> Vec<MonitorSpec> {
    let monitors: Vec<_> = event_loop.available_monitors().collect();
    display::monitor_specs(&monitors)
}

pub fn graphics_change_context_from_monitor(
    shell: &ShellState,
    current_renderer: BackendType,
    fallback_fullscreen_type: FullscreenType,
    chosen_monitor: usize,
    monitor_count: usize,
) -> GraphicsChangeContext {
    GraphicsChangeContext {
        current_renderer,
        current_display_mode: shell.display_mode,
        current_resolution: (shell.display_width, shell.display_height),
        fallback_fullscreen_type,
        chosen_monitor,
        monitor_count,
    }
}

pub fn graphics_change_context(
    shell: &ShellState,
    current_renderer: BackendType,
    fallback_fullscreen_type: FullscreenType,
    event_loop: &ActiveEventLoop,
    monitor_override: Option<usize>,
) -> GraphicsChangeContext {
    let (_, monitor_count, chosen_monitor) = display::resolve_monitor(
        event_loop,
        monitor_override.unwrap_or(shell.display_monitor),
    );
    graphics_change_context_from_monitor(
        shell,
        current_renderer,
        fallback_fullscreen_type,
        chosen_monitor,
        monitor_count,
    )
}

pub fn apply_renderer_switch_restore_display(
    shell: &mut ShellState,
    event_loop: &ActiveEventLoop,
    fallback_fullscreen_type: FullscreenType,
) -> RendererSwitchRestoreState {
    let (_, monitor_count, monitor) = display::resolve_monitor(event_loop, shell.display_monitor);
    apply_renderer_switch_restore_state(shell, monitor, monitor_count, fallback_fullscreen_type)
}

pub fn apply_renderer_switch_restore_state(
    shell: &mut ShellState,
    monitor: usize,
    monitor_count: usize,
    fallback_fullscreen_type: FullscreenType,
) -> RendererSwitchRestoreState {
    shell.display_monitor = monitor;
    shell.pending_window_position = None;
    let fullscreen_type = match shell.display_mode {
        DisplayMode::Fullscreen(fullscreen_type) => fullscreen_type,
        DisplayMode::Windowed => fallback_fullscreen_type,
    };
    RendererSwitchRestoreState {
        display_mode: shell.display_mode,
        fullscreen_type,
        monitor,
        monitor_count,
    }
}

pub fn startup_display_sync(
    shell: &ShellState,
    fullscreen_type: FullscreenType,
    monitor_count: usize,
) -> GraphicsDisplaySync {
    GraphicsDisplaySync {
        mode: shell.display_mode,
        fullscreen_type,
        monitor: shell.display_monitor,
        monitor_count,
        persist_mode: false,
        persist_monitor: false,
    }
}

pub const fn recreate_display_sync(display: RecreateDisplayState) -> GraphicsDisplaySync {
    GraphicsDisplaySync {
        mode: display.mode,
        fullscreen_type: display.fullscreen_type,
        monitor: display.monitor,
        monitor_count: display.monitor_count,
        persist_mode: display.persist_mode,
        persist_monitor: true,
    }
}

pub const fn restore_display_sync(display: RendererSwitchRestoreState) -> GraphicsDisplaySync {
    GraphicsDisplaySync {
        mode: display.display_mode,
        fullscreen_type: display.fullscreen_type,
        monitor: display.monitor,
        monitor_count: display.monitor_count,
        persist_mode: false,
        persist_monitor: false,
    }
}

pub const fn runtime_display_mode_sync(
    mode: DisplayMode,
    monitor: usize,
    result: &crate::window::DisplayModeResult,
) -> GraphicsDisplaySync {
    GraphicsDisplaySync {
        mode,
        fullscreen_type: result.fullscreen_type,
        monitor,
        monitor_count: result.monitor_count,
        persist_mode: true,
        persist_monitor: true,
    }
}

pub fn apply_recreate_display_change(
    shell: &mut ShellState,
    display: RecreateDisplayChange,
) -> RecreateDisplayState {
    if display.persist_mode {
        shell.display_mode = display.mode;
    }
    shell.display_monitor = display.monitor;
    RecreateDisplayState {
        mode: display.mode,
        persist_mode: display.persist_mode,
        fullscreen_type: display.fullscreen_type,
        monitor: display.monitor,
        monitor_count: display.monitor_count,
    }
}

pub fn refresh_present_config(
    backend: &mut Option<Backend>,
    shell: &ShellState,
    refresh: bool,
) -> bool {
    let Some(backend) = backend else {
        return false;
    };
    if !refresh {
        return false;
    }
    backend.set_present_config(shell.vsync_enabled, shell.present_mode_policy);
    true
}

pub fn sync_renderer_window_size(
    shell: &mut ShellState,
    window: Option<&Window>,
    backend: &mut Option<Backend>,
    backend_type: BackendType,
    high_dpi: bool,
    physical_size: PhysicalSize<u32>,
) -> PhysicalSize<u32> {
    let render_size = window.map_or(physical_size, |window| {
        render_size_for_physical(window, backend_type, high_dpi, physical_size)
    });
    if render_size.width > 0 && render_size.height > 0 {
        shell.metrics = space::metrics_for_window(render_size.width, render_size.height);
        space::set_current_metrics(shell.metrics);
    }
    if let Some(backend) = backend {
        backend.resize(render_size.width, render_size.height);
    }
    render_size
}

pub fn apply_display_mode_result(
    shell: &mut ShellState,
    mode: DisplayMode,
    result: &crate::window::DisplayModeResult,
) {
    shell.display_width = result.width;
    shell.display_height = result.height;
    shell.display_monitor = result.monitor;
    shell.pending_window_position = result.pending_position;
    shell.display_mode = mode;
}

pub fn apply_resolution_result(
    shell: &mut ShellState,
    width: u32,
    height: u32,
    result: &crate::window::ResolutionResult,
) {
    shell.display_width = width;
    shell.display_height = height;
    shell.display_monitor = result.monitor;
}

#[allow(clippy::too_many_arguments)]
pub fn runtime_display_mode_change(
    shell: &ShellState,
    backend_type: BackendType,
    high_dpi: bool,
    mode: DisplayMode,
    monitor_override: Option<usize>,
    fallback_fullscreen_type: FullscreenType,
) -> DisplayModeChange {
    DisplayModeChange {
        backend_type,
        high_dpi,
        width: shell.display_width,
        height: shell.display_height,
        monitor: shell.display_monitor,
        monitor_override,
        previous_mode: shell.display_mode,
        mode,
        fallback_fullscreen_type,
        pending_position: shell.pending_window_position,
    }
}

pub fn runtime_resolution_change(
    shell: &ShellState,
    backend_type: BackendType,
    high_dpi: bool,
    width: u32,
    height: u32,
) -> ResolutionChange {
    ResolutionChange {
        backend_type,
        high_dpi,
        width,
        height,
        monitor: shell.display_monitor,
        display_mode: shell.display_mode,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply_runtime_display_mode(
    shell: &mut ShellState,
    window: Option<&Window>,
    backend: &mut Option<Backend>,
    event_loop: &ActiveEventLoop,
    backend_type: BackendType,
    high_dpi: bool,
    mode: DisplayMode,
    monitor_override: Option<usize>,
    fallback_fullscreen_type: FullscreenType,
) -> crate::window::DisplayModeResult {
    let result = crate::window::apply_window_display_mode(
        window,
        event_loop,
        runtime_display_mode_change(
            shell,
            backend_type,
            high_dpi,
            mode,
            monitor_override,
            fallback_fullscreen_type,
        ),
    );
    if let Some(size) = result.immediate_size {
        sync_renderer_window_size(shell, window, backend, backend_type, high_dpi, size);
    }
    apply_display_mode_result(shell, mode, &result);
    result
}

#[allow(clippy::too_many_arguments)]
pub fn apply_runtime_resolution(
    shell: &mut ShellState,
    window: Option<&Window>,
    backend: &mut Option<Backend>,
    event_loop: &ActiveEventLoop,
    backend_type: BackendType,
    high_dpi: bool,
    width: u32,
    height: u32,
) -> crate::window::ResolutionResult {
    let result = crate::window::apply_window_resolution(
        window,
        event_loop,
        runtime_resolution_change(shell, backend_type, high_dpi, width, height),
    );
    if let Some(size) = result.immediate_size {
        sync_renderer_window_size(shell, window, backend, backend_type, high_dpi, size);
    }
    apply_resolution_result(shell, width, height, &result);
    result
}

const fn software_thread_count(configured: u8) -> Option<usize> {
    match configured {
        0 => None,
        count => Some(count as usize),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsChangeRequest {
    pub renderer: Option<BackendType>,
    pub display_mode: Option<DisplayMode>,
    pub resolution: Option<(u32, u32)>,
    pub monitor_requested: bool,
    pub high_dpi_changed: bool,
    pub present_config_changed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsRuntimeSettings {
    pub renderer: Option<BackendType>,
    pub display_mode: Option<DisplayMode>,
    pub resolution: Option<(u32, u32)>,
    pub monitor_requested: bool,
    pub vsync: Option<bool>,
    pub present_mode_policy: Option<PresentModePolicy>,
    pub max_fps: Option<u16>,
    pub high_dpi: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsRuntimeSettingsResult {
    pub request: GraphicsChangeRequest,
    pub vsync: Option<bool>,
    pub present_mode_policy: Option<PresentModePolicy>,
    pub max_fps: Option<u16>,
    pub high_dpi: Option<bool>,
    pub resolution: Option<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsRuntimeUpdate {
    Vsync(bool),
    MaxFps(u16),
    PresentModePolicy(PresentModePolicy),
    HighDpi(bool),
    Resolution(u32, u32),
}

pub fn graphics_runtime_updates(
    result: &GraphicsRuntimeSettingsResult,
) -> Vec<GraphicsRuntimeUpdate> {
    let mut updates = Vec::with_capacity(5);
    if let Some(vsync) = result.vsync {
        updates.push(GraphicsRuntimeUpdate::Vsync(vsync));
    }
    if let Some(max_fps) = result.max_fps {
        updates.push(GraphicsRuntimeUpdate::MaxFps(max_fps));
    }
    if let Some(policy) = result.present_mode_policy {
        updates.push(GraphicsRuntimeUpdate::PresentModePolicy(policy));
    }
    if let Some(enabled) = result.high_dpi {
        updates.push(GraphicsRuntimeUpdate::HighDpi(enabled));
    }
    if let Some((width, height)) = result.resolution {
        updates.push(GraphicsRuntimeUpdate::Resolution(width, height));
    }
    updates
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsChangeContext {
    pub current_renderer: BackendType,
    pub current_display_mode: DisplayMode,
    pub current_resolution: (u32, u32),
    pub fallback_fullscreen_type: FullscreenType,
    pub chosen_monitor: usize,
    pub monitor_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExistingDisplayChange {
    pub mode: DisplayMode,
    pub monitor: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecreateDisplayChange {
    pub mode: DisplayMode,
    pub persist_mode: bool,
    pub fullscreen_type: FullscreenType,
    pub monitor: usize,
    pub monitor_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsWindowPlan {
    Recreate {
        renderer: BackendType,
        resolution: Option<(u32, u32)>,
        force_recreate: bool,
        display: Option<RecreateDisplayChange>,
    },
    Reconfigure {
        display: Option<ExistingDisplayChange>,
        resolution: Option<(u32, u32)>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsChangePlan {
    pub window: GraphicsWindowPlan,
    pub refresh_present_config: bool,
}

#[inline(always)]
const fn effective_fullscreen_type(
    mode: DisplayMode,
    previous_mode: DisplayMode,
    fallback: FullscreenType,
) -> FullscreenType {
    match mode {
        DisplayMode::Fullscreen(fullscreen_type) => fullscreen_type,
        DisplayMode::Windowed => match previous_mode {
            DisplayMode::Fullscreen(fullscreen_type) => fullscreen_type,
            DisplayMode::Windowed => fallback,
        },
    }
}

pub fn apply_graphics_runtime_settings(
    shell: &mut ShellState,
    settings: GraphicsRuntimeSettings,
) -> GraphicsRuntimeSettingsResult {
    let mut present_config_changed = false;
    if let Some(vsync) = settings.vsync {
        shell.vsync_enabled = vsync;
        present_config_changed = true;
    }
    if let Some(max_fps) = settings.max_fps {
        shell.set_max_fps(max_fps);
    }
    if let Some(policy) = settings.present_mode_policy {
        shell.set_present_mode_policy(policy);
        present_config_changed = true;
    }
    if let Some((width, height)) = settings.resolution {
        shell.display_width = width;
        shell.display_height = height;
    }

    GraphicsRuntimeSettingsResult {
        request: GraphicsChangeRequest {
            renderer: settings.renderer,
            display_mode: settings.display_mode,
            resolution: settings.resolution,
            monitor_requested: settings.monitor_requested,
            high_dpi_changed: settings.high_dpi.is_some(),
            present_config_changed,
        },
        vsync: settings.vsync,
        present_mode_policy: settings.present_mode_policy,
        max_fps: settings.max_fps,
        high_dpi: settings.high_dpi,
        resolution: settings.resolution,
    }
}

pub fn graphics_change_plan(
    request: GraphicsChangeRequest,
    context: GraphicsChangeContext,
) -> GraphicsChangePlan {
    let renderer = request.renderer.unwrap_or(context.current_renderer);
    let high_dpi_affects_renderer = request.high_dpi_changed && renderer == BackendType::OpenGL;
    let resolution = request
        .resolution
        .or_else(|| high_dpi_affects_renderer.then_some(context.current_resolution));
    let recreate_renderer = request.renderer.is_some() || high_dpi_affects_renderer;

    let window = if recreate_renderer {
        let display = request
            .display_mode
            .map(|mode| RecreateDisplayChange {
                mode,
                persist_mode: true,
                fullscreen_type: effective_fullscreen_type(
                    mode,
                    context.current_display_mode,
                    context.fallback_fullscreen_type,
                ),
                monitor: context.chosen_monitor,
                monitor_count: context.monitor_count,
            })
            .or_else(|| {
                request.monitor_requested.then_some(RecreateDisplayChange {
                    mode: context.current_display_mode,
                    persist_mode: false,
                    fullscreen_type: effective_fullscreen_type(
                        context.current_display_mode,
                        context.current_display_mode,
                        context.fallback_fullscreen_type,
                    ),
                    monitor: context.chosen_monitor,
                    monitor_count: context.monitor_count,
                })
            });
        GraphicsWindowPlan::Recreate {
            renderer,
            resolution,
            force_recreate: high_dpi_affects_renderer,
            display,
        }
    } else {
        let display = request
            .display_mode
            .or(request
                .monitor_requested
                .then_some(context.current_display_mode))
            .map(|mode| ExistingDisplayChange {
                mode,
                monitor: context.chosen_monitor,
            });
        GraphicsWindowPlan::Reconfigure {
            display,
            resolution,
        }
    };

    GraphicsChangePlan {
        window,
        refresh_present_config: request.present_config_changed && !recreate_renderer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_config::app_config::Config;

    fn context() -> GraphicsChangeContext {
        GraphicsChangeContext {
            current_renderer: BackendType::Software,
            current_display_mode: DisplayMode::Windowed,
            current_resolution: (1280, 720),
            fallback_fullscreen_type: FullscreenType::Borderless,
            chosen_monitor: 1,
            monitor_count: 2,
        }
    }

    fn request() -> GraphicsChangeRequest {
        GraphicsChangeRequest {
            renderer: None,
            display_mode: None,
            resolution: None,
            monitor_requested: false,
            high_dpi_changed: false,
            present_config_changed: false,
        }
    }

    fn runtime_settings() -> GraphicsRuntimeSettings {
        GraphicsRuntimeSettings {
            renderer: None,
            display_mode: None,
            resolution: None,
            monitor_requested: false,
            vsync: None,
            present_mode_policy: None,
            max_fps: None,
            high_dpi: None,
        }
    }

    #[test]
    fn runtime_settings_update_shell_and_request_window_plan_inputs() {
        let mut shell = ShellState::new(&Config::default(), 0);
        let result = apply_graphics_runtime_settings(
            &mut shell,
            GraphicsRuntimeSettings {
                renderer: Some(BackendType::OpenGL),
                display_mode: Some(DisplayMode::Windowed),
                resolution: Some((1024, 768)),
                monitor_requested: true,
                vsync: Some(false),
                present_mode_policy: Some(PresentModePolicy::Immediate),
                max_fps: Some(144),
                high_dpi: Some(true),
            },
        );

        assert_eq!(shell.display_width, 1024);
        assert_eq!(shell.display_height, 768);
        assert!(!shell.vsync_enabled);
        assert_eq!(shell.present_mode_policy, PresentModePolicy::Immediate);
        assert_eq!(
            result.request,
            GraphicsChangeRequest {
                renderer: Some(BackendType::OpenGL),
                display_mode: Some(DisplayMode::Windowed),
                resolution: Some((1024, 768)),
                monitor_requested: true,
                high_dpi_changed: true,
                present_config_changed: true,
            }
        );
        assert_eq!(result.max_fps, Some(144));
    }

    #[test]
    fn runtime_settings_result_reports_ordered_root_updates() {
        let mut shell = ShellState::new(&Config::default(), 0);
        let result = apply_graphics_runtime_settings(
            &mut shell,
            GraphicsRuntimeSettings {
                resolution: Some((1024, 768)),
                vsync: Some(true),
                present_mode_policy: Some(PresentModePolicy::Immediate),
                max_fps: Some(144),
                high_dpi: Some(false),
                ..runtime_settings()
            },
        );

        assert_eq!(
            graphics_runtime_updates(&result),
            vec![
                GraphicsRuntimeUpdate::Vsync(true),
                GraphicsRuntimeUpdate::MaxFps(144),
                GraphicsRuntimeUpdate::PresentModePolicy(PresentModePolicy::Immediate),
                GraphicsRuntimeUpdate::HighDpi(false),
                GraphicsRuntimeUpdate::Resolution(1024, 768),
            ],
        );
    }

    #[test]
    fn runtime_settings_only_refresh_present_config_for_present_changes() {
        let mut shell = ShellState::new(&Config::default(), 0);

        let resolution = apply_graphics_runtime_settings(
            &mut shell,
            GraphicsRuntimeSettings {
                resolution: Some((1600, 900)),
                ..runtime_settings()
            },
        );
        assert!(!resolution.request.present_config_changed);

        let vsync = apply_graphics_runtime_settings(
            &mut shell,
            GraphicsRuntimeSettings {
                vsync: Some(true),
                ..runtime_settings()
            },
        );
        assert!(vsync.request.present_config_changed);
    }

    #[test]
    fn renderer_started_resets_frame_clock_and_metrics() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.frame_count = 12;
        shell.current_frame_vpf = 4;
        let now = shell.start_time + std::time::Duration::from_secs(2);
        let metrics = space::metrics_for_window(640, 480);

        apply_renderer_started(&mut shell, metrics, now);

        assert_eq!(shell.metrics.left, metrics.left);
        assert_eq!(shell.start_time, now);
        assert_eq!(shell.last_title_update, now);
        assert_eq!(shell.last_frame_time, now);
        assert_eq!(shell.frame_count, 0);
        assert_eq!(shell.current_frame_vpf, 0);
    }

    #[test]
    fn renderer_startup_config_snapshots_shell_and_consumes_pending_position() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.display_width = 1600;
        shell.display_height = 900;
        shell.display_monitor = 2;
        shell.display_mode = DisplayMode::Fullscreen(FullscreenType::Exclusive);
        shell.pending_window_position = Some(PhysicalPosition::new(10, 20));
        shell.vsync_enabled = false;
        shell.present_mode_policy = PresentModePolicy::Immediate;

        let config = renderer_startup_config(
            &mut shell,
            RendererStartupSettings {
                backend_type: BackendType::OpenGL,
                high_dpi: true,
                fallback_fullscreen_type: FullscreenType::Borderless,
                hide_cursor: true,
                gfx_debug_enabled: true,
                software_renderer_threads: 4,
            },
        );

        assert_eq!(shell.pending_window_position, None);
        assert_eq!(config.window.backend_type, BackendType::OpenGL);
        assert!(config.window.high_dpi);
        assert_eq!(config.window.width, 1600);
        assert_eq!(config.window.height, 900);
        assert_eq!(config.window.monitor, 2);
        assert_eq!(
            config.window.display_mode,
            DisplayMode::Fullscreen(FullscreenType::Exclusive)
        );
        assert_eq!(
            config.window.fallback_fullscreen_type,
            FullscreenType::Borderless
        );
        assert!(config.window.hide_cursor);
        assert_eq!(
            config.window.pending_position,
            Some(PhysicalPosition::new(10, 20))
        );
        assert_eq!(config.renderer.backend_type, BackendType::OpenGL);
        assert!(config.renderer.high_dpi);
        assert!(!config.renderer.vsync_enabled);
        assert_eq!(
            config.renderer.present_mode_policy,
            PresentModePolicy::Immediate
        );
        assert!(config.renderer.gfx_debug_enabled);
        assert_eq!(config.renderer.software_renderer_threads, 4);
    }

    #[test]
    fn renderer_switch_state_tracks_size_position_and_clock() {
        let mut shell = ShellState::new(&Config::default(), 0);
        let now = shell.start_time + std::time::Duration::from_secs(1);
        let position = PhysicalPosition::new(12, 34);

        apply_renderer_switch_window_state(
            &mut shell,
            RendererSwitchWindowResult {
                width: 1024,
                height: 768,
                pending_position: Some(position),
            },
        );
        reset_renderer_switch_clock(&mut shell, now);

        assert_eq!(shell.display_width, 1024);
        assert_eq!(shell.display_height, 768);
        assert_eq!(shell.pending_window_position, Some(position));
        assert_eq!(shell.last_title_update, now);
        assert_eq!(shell.last_frame_time, now);
        assert_eq!(shell.frame_count, 0);
    }

    #[test]
    fn renderer_switch_window_config_snapshots_shell_display_state() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.display_width = 1600;
        shell.display_height = 900;
        shell.display_mode = DisplayMode::Fullscreen(FullscreenType::Borderless);

        let config =
            renderer_switch_window_config(&shell, BackendType::OpenGL, true, Some((1280, 720)));

        assert_eq!(config.backend_type, BackendType::OpenGL);
        assert!(config.high_dpi);
        assert_eq!(
            config.display_mode,
            DisplayMode::Fullscreen(FullscreenType::Borderless)
        );
        assert_eq!(config.current_width, 1600);
        assert_eq!(config.current_height, 900);
        assert_eq!(config.desired_size, Some((1280, 720)));
    }

    #[test]
    fn renderer_switch_begin_disposes_backend_and_updates_shell_state() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.frame_count = 7;
        let now = shell.start_time + std::time::Duration::from_secs(1);

        begin_renderer_switch(
            &mut shell,
            None,
            &mut None,
            &mut AssetManager::new(),
            &mut DynamicMedia::new(),
            RendererSwitchWindowConfig {
                backend_type: BackendType::Software,
                high_dpi: false,
                display_mode: DisplayMode::Windowed,
                current_width: 1280,
                current_height: 720,
                desired_size: Some((1024, 768)),
            },
            now,
        );

        assert_eq!(shell.display_width, 1024);
        assert_eq!(shell.display_height, 768);
        assert_eq!(shell.pending_window_position, None);
        assert_eq!(shell.last_title_update, now);
        assert_eq!(shell.last_frame_time, now);
        assert_eq!(shell.frame_count, 0);
    }

    #[test]
    fn recreate_display_change_updates_monitor_and_optionally_mode() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.display_mode = DisplayMode::Fullscreen(FullscreenType::Exclusive);

        let monitor_only = apply_recreate_display_change(
            &mut shell,
            RecreateDisplayChange {
                mode: DisplayMode::Windowed,
                persist_mode: false,
                fullscreen_type: FullscreenType::Exclusive,
                monitor: 2,
                monitor_count: 4,
            },
        );

        assert_eq!(
            shell.display_mode,
            DisplayMode::Fullscreen(FullscreenType::Exclusive)
        );
        assert_eq!(shell.display_monitor, 2);
        assert_eq!(monitor_only.mode, DisplayMode::Windowed);
        assert!(!monitor_only.persist_mode);

        let persisted = apply_recreate_display_change(
            &mut shell,
            RecreateDisplayChange {
                mode: DisplayMode::Windowed,
                persist_mode: true,
                fullscreen_type: FullscreenType::Exclusive,
                monitor: 1,
                monitor_count: 4,
            },
        );

        assert_eq!(shell.display_mode, DisplayMode::Windowed);
        assert_eq!(shell.display_monitor, 1);
        assert!(persisted.persist_mode);
        assert_eq!(persisted.monitor_count, 4);
    }

    #[test]
    fn display_sync_plans_capture_persistence_scope() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.display_mode = DisplayMode::Fullscreen(FullscreenType::Borderless);
        shell.display_monitor = 2;

        assert_eq!(
            startup_display_sync(&shell, FullscreenType::Exclusive, 3),
            GraphicsDisplaySync {
                mode: DisplayMode::Fullscreen(FullscreenType::Borderless),
                fullscreen_type: FullscreenType::Exclusive,
                monitor: 2,
                monitor_count: 3,
                persist_mode: false,
                persist_monitor: false,
            }
        );

        assert_eq!(
            recreate_display_sync(RecreateDisplayState {
                mode: DisplayMode::Windowed,
                persist_mode: true,
                fullscreen_type: FullscreenType::Borderless,
                monitor: 1,
                monitor_count: 2,
            }),
            GraphicsDisplaySync {
                mode: DisplayMode::Windowed,
                fullscreen_type: FullscreenType::Borderless,
                monitor: 1,
                monitor_count: 2,
                persist_mode: true,
                persist_monitor: true,
            }
        );

        assert_eq!(
            restore_display_sync(RendererSwitchRestoreState {
                display_mode: DisplayMode::Windowed,
                fullscreen_type: FullscreenType::Exclusive,
                monitor: 0,
                monitor_count: 1,
            }),
            GraphicsDisplaySync {
                mode: DisplayMode::Windowed,
                fullscreen_type: FullscreenType::Exclusive,
                monitor: 0,
                monitor_count: 1,
                persist_mode: false,
                persist_monitor: false,
            }
        );

        let runtime = runtime_display_mode_sync(
            DisplayMode::Windowed,
            3,
            &crate::window::DisplayModeResult {
                width: 1280,
                height: 720,
                monitor: 3,
                monitor_count: 4,
                pending_position: None,
                fullscreen_type: FullscreenType::Borderless,
                immediate_size: None,
            },
        );
        assert_eq!(
            runtime,
            GraphicsDisplaySync {
                mode: DisplayMode::Windowed,
                fullscreen_type: FullscreenType::Borderless,
                monitor: 3,
                monitor_count: 4,
                persist_mode: true,
                persist_monitor: true,
            }
        );
    }

    #[test]
    fn present_config_refresh_skips_when_no_backend_or_request() {
        let shell = ShellState::new(&Config::default(), 0);

        assert!(!refresh_present_config(&mut None, &shell, true));
        assert!(!refresh_present_config(&mut None, &shell, false));
    }

    #[test]
    fn renderer_switch_restore_clears_pending_position_and_reports_display_state() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.display_monitor = 4;
        shell.pending_window_position = Some(PhysicalPosition::new(50, 60));

        let state =
            apply_renderer_switch_restore_state(&mut shell, 1, 3, FullscreenType::Exclusive);

        assert_eq!(shell.display_monitor, 1);
        assert_eq!(shell.pending_window_position, None);
        assert_eq!(
            state,
            RendererSwitchRestoreState {
                display_mode: DisplayMode::Windowed,
                fullscreen_type: FullscreenType::Exclusive,
                monitor: 1,
                monitor_count: 3,
            }
        );
    }

    #[test]
    fn renderer_switch_needed_allows_forced_same_backend_recreation() {
        assert!(!renderer_switch_needed(RendererSwitchRequest {
            current: BackendType::Software,
            target: BackendType::Software,
            force_recreate: false,
        }));
        assert!(renderer_switch_needed(RendererSwitchRequest {
            current: BackendType::Software,
            target: BackendType::Software,
            force_recreate: true,
        }));
        assert!(renderer_switch_needed(RendererSwitchRequest {
            current: BackendType::Software,
            target: BackendType::OpenGL,
            force_recreate: false,
        }));
    }

    #[test]
    fn renderer_switch_plan_snapshots_previous_target_and_window_config() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.display_width = 1600;
        shell.display_height = 900;
        shell.display_mode = DisplayMode::Fullscreen(FullscreenType::Borderless);

        let plan = renderer_switch_plan(
            &shell,
            RendererSwitchRequest {
                current: BackendType::Software,
                target: BackendType::OpenGL,
                force_recreate: false,
            },
            true,
            Some((1280, 720)),
        )
        .expect("different backend should produce switch plan");

        assert_eq!(plan.previous, BackendType::Software);
        assert_eq!(plan.target, BackendType::OpenGL);
        assert_eq!(plan.window_config.backend_type, BackendType::Software);
        assert!(plan.window_config.high_dpi);
        assert_eq!(
            plan.window_config.display_mode,
            DisplayMode::Fullscreen(FullscreenType::Borderless)
        );
        assert_eq!(plan.window_config.current_width, 1600);
        assert_eq!(plan.window_config.current_height, 900);
        assert_eq!(plan.window_config.desired_size, Some((1280, 720)));

        assert!(
            renderer_switch_plan(
                &shell,
                RendererSwitchRequest {
                    current: BackendType::Software,
                    target: BackendType::Software,
                    force_recreate: false,
                },
                false,
                None,
            )
            .is_none()
        );
    }

    #[test]
    fn renderer_switch_resource_reset_commands_cover_dynamic_media() {
        let reset = renderer_switch_resource_reset_plan();
        let commands = &reset.commands;

        assert!(matches!(commands.first(), Some(Command::SetBanner(None))));
        assert!(matches!(commands.get(1), Some(Command::SetCdTitle(None))));
        assert!(matches!(
            commands.get(2),
            Some(Command::SetDensityGraph {
                slot: DensityGraphSlot::SelectMusicP1,
                chart_opt: None,
            })
        ));
        assert!(matches!(
            commands.get(3),
            Some(Command::SetDensityGraph {
                slot: DensityGraphSlot::SelectMusicP2,
                chart_opt: None,
            })
        ));
        assert!(matches!(
            commands.get(4),
            Some(Command::SetDynamicBackground(None))
        ));
        assert_eq!(commands.len(), 5);
        assert!(reset.refresh_select_music);
        assert_eq!(reset.graph_key, "__white");
    }

    #[test]
    fn renderer_switch_outcome_plans_capture_root_side_effects() {
        let mut shell = ShellState::new(&Config::default(), 0);
        let switch = renderer_switch_plan(
            &shell,
            RendererSwitchRequest {
                current: BackendType::Software,
                target: BackendType::OpenGL,
                force_recreate: false,
            },
            false,
            None,
        )
        .expect("different backend should produce switch plan");

        assert_eq!(
            renderer_switch_begin_plan(&switch),
            RendererSwitchBeginPlan {
                target: BackendType::OpenGL,
                clear_window: true,
                clear_focus: true,
            }
        );
        assert_eq!(
            renderer_switch_success_plan(BackendType::OpenGL),
            RendererSwitchSuccessPlan {
                target: BackendType::OpenGL,
                persist_renderer: true,
                sync_options_renderer: true,
                clear_present_runtime: true,
                reset_dynamic_assets: true,
                request_redraw: true,
            }
        );
        assert_eq!(
            renderer_switch_failure_plan(BackendType::Software),
            RendererSwitchFailurePlan {
                previous: BackendType::Software,
                persist_renderer: true,
                sync_options_renderer: true,
                restore_display: true,
            }
        );

        shell.display_width = 1024;
        assert_eq!(switch.previous, BackendType::Software);
    }

    #[test]
    fn display_and_resolution_results_update_shell_display_state() {
        let mut shell = ShellState::new(&Config::default(), 0);
        let position = PhysicalPosition::new(56, 78);
        apply_display_mode_result(
            &mut shell,
            DisplayMode::Windowed,
            &crate::window::DisplayModeResult {
                width: 1600,
                height: 900,
                monitor: 2,
                monitor_count: 3,
                pending_position: Some(position),
                fullscreen_type: FullscreenType::Borderless,
                immediate_size: None,
            },
        );

        assert_eq!(shell.display_width, 1600);
        assert_eq!(shell.display_height, 900);
        assert_eq!(shell.display_monitor, 2);
        assert_eq!(shell.pending_window_position, Some(position));
        assert_eq!(shell.display_mode, DisplayMode::Windowed);

        apply_resolution_result(
            &mut shell,
            1280,
            720,
            &crate::window::ResolutionResult {
                monitor: 1,
                immediate_size: None,
            },
        );

        assert_eq!(shell.display_width, 1280);
        assert_eq!(shell.display_height, 720);
        assert_eq!(shell.display_monitor, 1);
    }

    #[test]
    fn runtime_display_mode_change_snapshots_shell_state() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.display_width = 1600;
        shell.display_height = 900;
        shell.display_monitor = 3;
        shell.display_mode = DisplayMode::Fullscreen(FullscreenType::Borderless);
        shell.pending_window_position = Some(PhysicalPosition::new(12, 34));

        let change = runtime_display_mode_change(
            &shell,
            BackendType::OpenGL,
            true,
            DisplayMode::Windowed,
            Some(1),
            FullscreenType::Exclusive,
        );

        assert_eq!(change.backend_type, BackendType::OpenGL);
        assert!(change.high_dpi);
        assert_eq!(change.width, 1600);
        assert_eq!(change.height, 900);
        assert_eq!(change.monitor, 3);
        assert_eq!(change.monitor_override, Some(1));
        assert_eq!(
            change.previous_mode,
            DisplayMode::Fullscreen(FullscreenType::Borderless)
        );
        assert_eq!(change.mode, DisplayMode::Windowed);
        assert_eq!(change.fallback_fullscreen_type, FullscreenType::Exclusive);
        assert_eq!(change.pending_position, Some(PhysicalPosition::new(12, 34)));
    }

    #[test]
    fn runtime_resolution_change_snapshots_shell_state() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.display_monitor = 2;
        shell.display_mode = DisplayMode::Fullscreen(FullscreenType::Exclusive);

        let change = runtime_resolution_change(&shell, BackendType::Software, false, 1920, 1080);

        assert_eq!(change.backend_type, BackendType::Software);
        assert!(!change.high_dpi);
        assert_eq!(change.width, 1920);
        assert_eq!(change.height, 1080);
        assert_eq!(change.monitor, 2);
        assert_eq!(
            change.display_mode,
            DisplayMode::Fullscreen(FullscreenType::Exclusive)
        );
    }

    #[test]
    fn renderer_window_size_updates_shell_metrics_without_backend() {
        let mut shell = ShellState::new(&Config::default(), 0);
        let size = sync_renderer_window_size(
            &mut shell,
            None,
            &mut None,
            BackendType::Software,
            false,
            PhysicalSize::new(640, 480),
        );

        assert_eq!(size, PhysicalSize::new(640, 480));
        assert!((shell.metrics.left + 320.0).abs() < f32::EPSILON);
        assert!((shell.metrics.right - 320.0).abs() < f32::EPSILON);
        assert!((shell.metrics.top - 240.0).abs() < f32::EPSILON);
        assert!((shell.metrics.bottom + 240.0).abs() < f32::EPSILON);
    }

    #[test]
    fn zero_renderer_window_size_keeps_previous_metrics() {
        let mut shell = ShellState::new(&Config::default(), 0);
        let before = shell.metrics;
        let size = sync_renderer_window_size(
            &mut shell,
            None,
            &mut None,
            BackendType::Software,
            false,
            PhysicalSize::new(0, 0),
        );

        assert_eq!(size, PhysicalSize::new(0, 0));
        assert_eq!(shell.metrics.left, before.left);
        assert_eq!(shell.metrics.right, before.right);
        assert_eq!(shell.metrics.top, before.top);
        assert_eq!(shell.metrics.bottom, before.bottom);
    }

    #[test]
    fn renderer_and_mode_change_recreates_directly_in_target_mode() {
        let plan = graphics_change_plan(
            GraphicsChangeRequest {
                renderer: Some(BackendType::OpenGLWgpu),
                display_mode: Some(DisplayMode::Fullscreen(FullscreenType::Exclusive)),
                resolution: Some((1920, 1080)),
                ..request()
            },
            context(),
        );
        assert_eq!(
            plan.window,
            GraphicsWindowPlan::Recreate {
                renderer: BackendType::OpenGLWgpu,
                resolution: Some((1920, 1080)),
                force_recreate: false,
                display: Some(RecreateDisplayChange {
                    mode: DisplayMode::Fullscreen(FullscreenType::Exclusive),
                    persist_mode: true,
                    fullscreen_type: FullscreenType::Exclusive,
                    monitor: 1,
                    monitor_count: 2,
                }),
            }
        );
    }

    #[test]
    fn windowed_recreation_preserves_previous_fullscreen_type() {
        let plan = graphics_change_plan(
            GraphicsChangeRequest {
                renderer: Some(BackendType::Software),
                display_mode: Some(DisplayMode::Windowed),
                ..request()
            },
            GraphicsChangeContext {
                current_display_mode: DisplayMode::Fullscreen(FullscreenType::Exclusive),
                ..context()
            },
        );
        let GraphicsWindowPlan::Recreate {
            display: Some(display),
            ..
        } = plan.window
        else {
            panic!("expected renderer recreation with display sync");
        };
        assert_eq!(display.fullscreen_type, FullscreenType::Exclusive);
    }

    #[test]
    fn opengl_high_dpi_change_forces_same_renderer_recreation() {
        let plan = graphics_change_plan(
            GraphicsChangeRequest {
                high_dpi_changed: true,
                ..request()
            },
            GraphicsChangeContext {
                current_renderer: BackendType::OpenGL,
                ..context()
            },
        );
        assert!(matches!(
            plan.window,
            GraphicsWindowPlan::Recreate {
                renderer: BackendType::OpenGL,
                resolution: Some((1280, 720)),
                force_recreate: true,
                display: None,
            }
        ));
    }

    #[test]
    fn non_opengl_high_dpi_change_leaves_window_in_place() {
        let plan = graphics_change_plan(
            GraphicsChangeRequest {
                high_dpi_changed: true,
                ..request()
            },
            context(),
        );
        assert_eq!(
            plan.window,
            GraphicsWindowPlan::Reconfigure {
                display: None,
                resolution: None,
            }
        );
    }

    #[test]
    fn monitor_only_change_reconfigures_existing_display() {
        let plan = graphics_change_plan(
            GraphicsChangeRequest {
                resolution: Some((1024, 768)),
                monitor_requested: true,
                ..request()
            },
            context(),
        );
        assert_eq!(
            plan.window,
            GraphicsWindowPlan::Reconfigure {
                display: Some(ExistingDisplayChange {
                    mode: DisplayMode::Windowed,
                    monitor: 1,
                }),
                resolution: Some((1024, 768)),
            }
        );
    }

    #[test]
    fn monitor_change_is_synced_before_renderer_recreation() {
        let plan = graphics_change_plan(
            GraphicsChangeRequest {
                renderer: Some(BackendType::OpenGLWgpu),
                monitor_requested: true,
                ..request()
            },
            context(),
        );
        let GraphicsWindowPlan::Recreate {
            display: Some(display),
            ..
        } = plan.window
        else {
            panic!("expected monitor sync before recreation");
        };
        assert_eq!(display.mode, DisplayMode::Windowed);
        assert!(!display.persist_mode);
        assert_eq!(display.fullscreen_type, FullscreenType::Borderless);
    }

    #[test]
    fn graphics_change_context_snapshots_shell_and_monitor_choice() {
        let mut shell = ShellState::new(&Config::default(), 0);
        shell.display_width = 1920;
        shell.display_height = 1080;
        shell.display_mode = DisplayMode::Fullscreen(FullscreenType::Exclusive);

        let context = graphics_change_context_from_monitor(
            &shell,
            BackendType::OpenGL,
            FullscreenType::Borderless,
            2,
            4,
        );

        assert_eq!(context.current_renderer, BackendType::OpenGL);
        assert_eq!(
            context.current_display_mode,
            DisplayMode::Fullscreen(FullscreenType::Exclusive)
        );
        assert_eq!(context.current_resolution, (1920, 1080));
        assert_eq!(context.fallback_fullscreen_type, FullscreenType::Borderless);
        assert_eq!(context.chosen_monitor, 2);
        assert_eq!(context.monitor_count, 4);
    }

    #[test]
    fn present_config_refresh_only_targets_existing_renderer() {
        let existing = graphics_change_plan(
            GraphicsChangeRequest {
                present_config_changed: true,
                ..request()
            },
            context(),
        );
        assert!(existing.refresh_present_config);

        let recreated = graphics_change_plan(
            GraphicsChangeRequest {
                renderer: Some(BackendType::OpenGLWgpu),
                present_config_changed: true,
                ..request()
            },
            context(),
        );
        assert!(!recreated.refresh_present_config);
    }

    #[test]
    fn zero_software_threads_uses_renderer_default() {
        assert_eq!(software_thread_count(0), None);
        assert_eq!(software_thread_count(1), Some(1));
        assert_eq!(software_thread_count(8), Some(8));
    }
}
