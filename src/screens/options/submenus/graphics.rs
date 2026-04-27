use super::super::*;

pub(in crate::screens::options) const GRAPHICS_OPTIONS_ROWS: &[SubRow] = &[
    SubRow {
        id: SubRowId::VideoRenderer,
        label: lookup_key("OptionsGraphics", "VideoRenderer"),
        choices: VIDEO_RENDERER_LABELS,
        inline: false,
    },
    SubRow {
        id: SubRowId::SoftwareRendererThreads,
        label: lookup_key("OptionsGraphics", "SoftwareRendererThreads"),
        choices: &[localized_choice("Common", "Auto")],
        inline: false,
    },
    SubRow {
        id: SubRowId::DisplayMode,
        label: lookup_key("OptionsGraphics", "DisplayMode"),
        choices: &[
            localized_choice("OptionsGraphics", "Windowed"),
            localized_choice("OptionsGraphics", "Fullscreen"),
            localized_choice("OptionsGraphics", "Borderless"),
        ], // Replaced dynamically
        inline: true,
    },
    SubRow {
        id: SubRowId::DisplayAspectRatio,
        label: lookup_key("OptionsGraphics", "DisplayAspectRatio"),
        choices: DISPLAY_ASPECT_RATIO_CHOICES,
        inline: true,
    },
    SubRow {
        id: SubRowId::DisplayResolution,
        label: lookup_key("OptionsGraphics", "DisplayResolution"),
        choices: &[
            literal_choice("1920x1080"),
            literal_choice("1600x900"),
            literal_choice("1280x720"),
            literal_choice("1024x768"),
            literal_choice("800x600"),
        ], // Replaced dynamically
        inline: false,
    },
    SubRow {
        id: SubRowId::RefreshRate,
        label: lookup_key("OptionsGraphics", "RefreshRate"),
        choices: &[
            localized_choice("Common", "Default"),
            literal_choice("60 Hz"),
            literal_choice("75 Hz"),
            literal_choice("120 Hz"),
            literal_choice("144 Hz"),
            literal_choice("165 Hz"),
            literal_choice("240 Hz"),
            literal_choice("360 Hz"),
        ], // Replaced dynamically
        inline: false,
    },
    SubRow {
        id: SubRowId::FullscreenType,
        label: lookup_key("OptionsGraphics", "FullscreenType"),
        choices: &[
            localized_choice("OptionsGraphics", "FullscreenTypeExclusive"),
            localized_choice("OptionsGraphics", "FullscreenTypeBorderless"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::VSync,
        label: lookup_key("OptionsGraphics", "VSync"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::PresentMode,
        label: lookup_key("OptionsGraphics", "PresentMode"),
        choices: &[literal_choice("Mailbox"), literal_choice("Immediate")],
        inline: true,
    },
    SubRow {
        id: SubRowId::MaxFps,
        label: lookup_key("OptionsGraphics", "MaxFps"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::MaxFpsValue,
        label: lookup_key("OptionsGraphics", "MaxFpsValue"),
        choices: &[localized_choice("Common", "Off")], // Replaced dynamically
        inline: false,
    },
    SubRow {
        id: SubRowId::ShowStats,
        label: lookup_key("OptionsGraphics", "ShowStats"),
        choices: &[
            localized_choice("Common", "Off"),
            localized_choice("OptionsGraphics", "ShowStatsFPS"),
            localized_choice("OptionsGraphics", "ShowStatsFPSStutter"),
            localized_choice("OptionsGraphics", "ShowStatsFPSStutterTiming"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::ValidationLayers,
        label: lookup_key("OptionsGraphics", "ValidationLayers"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::HighDpi,
        label: lookup_key("OptionsGraphics", "HighDPI"),
        choices: &[
            localized_choice("Common", "No"),
            localized_choice("Common", "Yes"),
        ],
        inline: true,
    },
    SubRow {
        id: SubRowId::VisualDelay,
        label: lookup_key("OptionsGraphics", "VisualDelay"),
        choices: &[literal_choice("0 ms")],
        inline: false,
    },
];

pub(in crate::screens::options) const GRAPHICS_OPTIONS_ITEMS: &[Item] = &[
    Item {
        id: ItemId::GfxVideoRenderer,
        name: lookup_key("OptionsGraphics", "VideoRenderer"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "VideoRendererHelp",
        ))],
    },
    Item {
        id: ItemId::GfxSoftwareThreads,
        name: lookup_key("OptionsGraphics", "SoftwareRendererThreads"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "SoftwareRendererThreadsHelp",
        ))],
    },
    Item {
        id: ItemId::GfxDisplayMode,
        name: lookup_key("OptionsGraphics", "DisplayMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "DisplayModeHelp",
        ))],
    },
    Item {
        id: ItemId::GfxDisplayAspectRatio,
        name: lookup_key("OptionsGraphics", "DisplayAspectRatio"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "DisplayAspectRatioHelp",
        ))],
    },
    Item {
        id: ItemId::GfxDisplayResolution,
        name: lookup_key("OptionsGraphics", "DisplayResolution"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "DisplayResolutionHelp",
        ))],
    },
    Item {
        id: ItemId::GfxRefreshRate,
        name: lookup_key("OptionsGraphics", "RefreshRate"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "RefreshRateHelp",
        ))],
    },
    Item {
        id: ItemId::GfxFullscreenType,
        name: lookup_key("OptionsGraphics", "FullscreenType"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "FullscreenTypeHelp",
        ))],
    },
    Item {
        id: ItemId::GfxVSync,
        name: lookup_key("OptionsGraphics", "VSync"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "VSyncHelp",
        ))],
    },
    Item {
        id: ItemId::GfxPresentMode,
        name: lookup_key("OptionsGraphics", "PresentMode"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "PresentModeHelp",
        ))],
    },
    Item {
        id: ItemId::GfxMaxFps,
        name: lookup_key("OptionsGraphics", "MaxFps"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "MaxFpsHelp",
        ))],
    },
    Item {
        id: ItemId::GfxMaxFpsValue,
        name: lookup_key("OptionsGraphics", "MaxFpsValue"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "MaxFpsValueHelp",
        ))],
    },
    Item {
        id: ItemId::GfxShowStats,
        name: lookup_key("OptionsGraphics", "ShowStats"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "ShowStatsHelp",
        ))],
    },
    Item {
        id: ItemId::GfxValidationLayers,
        name: lookup_key("OptionsGraphics", "ValidationLayers"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "ValidationLayersHelp",
        ))],
    },
    Item {
        id: ItemId::GfxHighDpi,
        name: lookup_key("OptionsGraphics", "HighDPI"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "HighDPIHelp",
        ))],
    },
    Item {
        id: ItemId::GfxVisualDelay,
        name: lookup_key("OptionsGraphics", "VisualDelay"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsGraphicsHelp",
            "VisualDelayHelp",
        ))],
    },
    Item {
        id: ItemId::Exit,
        name: lookup_key("Options", "Exit"),
        help: &[HelpEntry::Paragraph(lookup_key(
            "OptionsHelp",
            "ExitSubHelp",
        ))],
    },
];

#[cfg(all(target_os = "windows", not(target_pointer_width = "32")))]
pub(in crate::screens::options) const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::DirectX, "DirectX"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
pub(in crate::screens::options) const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::DirectX, "DirectX"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(target_os = "macos", not(target_pointer_width = "32")))]
pub(in crate::screens::options) const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::Metal, "Metal (wgpu)"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(
    not(any(target_os = "windows", target_os = "macos")),
    not(target_pointer_width = "32")
))]
pub(in crate::screens::options) const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::Vulkan, "Vulkan"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::VulkanWgpu, "Vulkan (wgpu)"),
    (BackendType::Software, "Software"),
];
#[cfg(all(not(target_os = "windows"), target_pointer_width = "32"))]
pub(in crate::screens::options) const VIDEO_RENDERER_OPTIONS: &[(BackendType, &str)] = &[
    (BackendType::OpenGL, "OpenGL"),
    (BackendType::OpenGLWgpu, "OpenGL (wgpu)"),
    (BackendType::Software, "Software"),
];

#[cfg(all(target_os = "windows", not(target_pointer_width = "32")))]
pub(in crate::screens::options) const VIDEO_RENDERER_LABELS: &[Choice] = &[
    localized_choice("OptionsGraphics", "RendererOpenGL"),
    localized_choice("OptionsGraphics", "RendererVulkan"),
    localized_choice("OptionsGraphics", "RendererDirectX"),
    localized_choice("OptionsGraphics", "RendererOpenGLWgpu"),
    localized_choice("OptionsGraphics", "RendererVulkanWgpu"),
    localized_choice("OptionsGraphics", "RendererSoftware"),
];
#[cfg(all(target_os = "windows", target_pointer_width = "32"))]
pub(in crate::screens::options) const VIDEO_RENDERER_LABELS: &[Choice] = &[
    localized_choice("OptionsGraphics", "RendererOpenGL"),
    localized_choice("OptionsGraphics", "RendererDirectX"),
    localized_choice("OptionsGraphics", "RendererOpenGLWgpu"),
    localized_choice("OptionsGraphics", "RendererSoftware"),
];
#[cfg(all(target_os = "macos", not(target_pointer_width = "32")))]
pub(in crate::screens::options) const VIDEO_RENDERER_LABELS: &[Choice] = &[
    localized_choice("OptionsGraphics", "RendererOpenGL"),
    localized_choice("OptionsGraphics", "RendererVulkan"),
    localized_choice("OptionsGraphics", "RendererMetal"),
    localized_choice("OptionsGraphics", "RendererOpenGLWgpu"),
    localized_choice("OptionsGraphics", "RendererVulkanWgpu"),
    localized_choice("OptionsGraphics", "RendererSoftware"),
];
#[cfg(all(
    not(any(target_os = "windows", target_os = "macos")),
    not(target_pointer_width = "32")
))]
pub(in crate::screens::options) const VIDEO_RENDERER_LABELS: &[Choice] = &[
    localized_choice("OptionsGraphics", "RendererOpenGL"),
    localized_choice("OptionsGraphics", "RendererVulkan"),
    localized_choice("OptionsGraphics", "RendererOpenGLWgpu"),
    localized_choice("OptionsGraphics", "RendererVulkanWgpu"),
    localized_choice("OptionsGraphics", "RendererSoftware"),
];
#[cfg(all(not(target_os = "windows"), target_pointer_width = "32"))]
pub(in crate::screens::options) const VIDEO_RENDERER_LABELS: &[Choice] = &[
    localized_choice("OptionsGraphics", "RendererOpenGL"),
    localized_choice("OptionsGraphics", "RendererOpenGLWgpu"),
    localized_choice("OptionsGraphics", "RendererSoftware"),
];

pub(in crate::screens::options) const DISPLAY_ASPECT_RATIO_CHOICES: &[Choice] = &[
    literal_choice("16:9"),
    literal_choice("16:10"),
    literal_choice("4:3"),
    literal_choice("1:1"),
];

pub(in crate::screens::options) const MAX_FPS_MIN: u16 = 5;
pub(in crate::screens::options) const MAX_FPS_MAX: u16 = 1000;
pub(in crate::screens::options) const MAX_FPS_STEP: u16 = 1;
pub(in crate::screens::options) const MAX_FPS_DEFAULT: u16 = 60;

pub(in crate::screens::options) const DEFAULT_RESOLUTION_CHOICES: &[(u32, u32)] = &[
    (1920, 1080),
    (1600, 900),
    (1280, 720),
    (1024, 768),
    (800, 600),
];

pub(in crate::screens::options) fn build_display_mode_choices(
    monitor_specs: &[MonitorSpec],
) -> Vec<String> {
    if monitor_specs.is_empty() {
        return vec![
            tr("OptionsGraphics", "Screen1Fallback").to_string(),
            tr("OptionsGraphics", "Windowed").to_string(),
        ];
    }
    let mut out = Vec::with_capacity(monitor_specs.len() + 1);
    for spec in monitor_specs {
        out.push(spec.name.clone());
    }
    out.push(tr("OptionsGraphics", "Windowed").to_string());
    out
}

pub(in crate::screens::options) fn backend_to_renderer_choice_index(backend: BackendType) -> usize {
    VIDEO_RENDERER_OPTIONS
        .iter()
        .position(|(b, _)| *b == backend)
        .unwrap_or(0)
}

pub(in crate::screens::options) fn renderer_choice_index_to_backend(idx: usize) -> BackendType {
    VIDEO_RENDERER_OPTIONS
        .get(idx)
        .map_or_else(|| VIDEO_RENDERER_OPTIONS[0].0, |(backend, _)| *backend)
}

pub(in crate::screens::options) fn selected_video_renderer(state: &State) -> BackendType {
    let choice_idx = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::VideoRenderer,
    ).unwrap_or(0);
    renderer_choice_index_to_backend(choice_idx)
}

pub(in crate::screens::options) fn build_software_thread_choices() -> Vec<u8> {
    let max_threads = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(8)
        .clamp(2, 32);
    let mut out = Vec::with_capacity(max_threads + 1);
    out.push(0); // Auto
    for n in 1..=max_threads {
        out.push(n as u8);
    }
    out
}

pub(in crate::screens::options) fn software_thread_choice_labels(values: &[u8]) -> Vec<String> {
    values
        .iter()
        .map(|v| {
            if *v == 0 {
                tr("Common", "Auto").to_string()
            } else {
                v.to_string()
            }
        })
        .collect()
}

pub(in crate::screens::options) fn software_thread_choice_index(
    values: &[u8],
    thread_count: u8,
) -> usize {
    values
        .iter()
        .position(|&v| v == thread_count)
        .unwrap_or_else(|| {
            values
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| v.abs_diff(thread_count))
                .map_or(0, |(idx, _)| idx)
        })
}

pub(in crate::screens::options) fn software_thread_from_choice(values: &[u8], idx: usize) -> u8 {
    values.get(idx).copied().unwrap_or(0)
}

pub(in crate::screens::options) fn build_max_fps_choices() -> Vec<u16> {
    let mut out = Vec::with_capacity(
        1 + usize::from(MAX_FPS_MAX.saturating_sub(MAX_FPS_MIN)) / usize::from(MAX_FPS_STEP),
    );
    let mut fps = MAX_FPS_MIN;
    while fps <= MAX_FPS_MAX {
        out.push(fps);
        fps = fps.saturating_add(MAX_FPS_STEP);
    }
    out
}

pub(in crate::screens::options) fn selected_max_fps_label(state: &State) -> String {
    let idx = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::MaxFpsValue,
    )
    .unwrap_or(0);
    max_fps_from_choice(&state.max_fps_choices, idx).to_string()
}

pub(in crate::screens::options) fn adjust_max_fps_value_choice(
    state: &mut State,
    delta: isize,
    wrap: NavWrap,
) -> bool {
    let n = state.max_fps_choices.len() as isize;
    if n == 0 {
        return false;
    }
    let current = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].cursor_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::MaxFpsValue,
    )
    .unwrap_or(0)
    .min(state.max_fps_choices.len().saturating_sub(1)) as isize;
    let raw = current + delta;
    let new_index = match wrap {
        NavWrap::Wrap => raw.rem_euclid(n) as usize,
        NavWrap::Clamp => raw.clamp(0, n - 1) as usize,
    };
    if new_index == current as usize {
        return false;
    }
    set_max_fps_value_choice_index(state, new_index);
    true
}

pub(in crate::screens::options) fn current_submenu_row_id(
    state: &State,
) -> Option<(SubmenuKind, SubRowId)> {
    let kind = match state.view {
        OptionsView::Submenu(kind) => kind,
        OptionsView::Main => return None,
    };
    let row_idx = submenu_visible_row_to_actual(state, kind, state.sub_selected)?;
    submenu_rows(kind).get(row_idx).map(|row| (kind, row.id))
}

#[inline(always)]
pub(in crate::screens::options) fn on_max_fps_value_row(state: &State) -> bool {
    matches!(
        current_submenu_row_id(state),
        Some((SubmenuKind::Graphics, SubRowId::MaxFpsValue))
    )
}

pub(in crate::screens::options) fn max_fps_hold_delta(delta: isize, held_for: Duration) -> isize {
    let multiplier = if held_for >= MAX_FPS_HOLD_FASTEST_AFTER {
        50
    } else if held_for >= MAX_FPS_HOLD_FASTER_AFTER {
        25
    } else if held_for >= MAX_FPS_HOLD_FAST_AFTER {
        10
    } else {
        5
    };
    delta * multiplier
}

#[inline(always)]
pub(in crate::screens::options) const fn clamped_max_fps(max_fps: u16) -> u16 {
    if max_fps < MAX_FPS_MIN {
        MAX_FPS_MIN
    } else if max_fps > MAX_FPS_MAX {
        MAX_FPS_MAX
    } else {
        max_fps
    }
}

pub(in crate::screens::options) fn max_fps_choice_index(values: &[u16], max_fps: u16) -> usize {
    let target = clamped_max_fps(max_fps);
    values.iter().position(|&v| v == target).unwrap_or_else(|| {
        values
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| v.abs_diff(target))
            .map_or(0, |(idx, _)| idx)
    })
}

pub(in crate::screens::options) fn max_fps_from_choice(values: &[u16], idx: usize) -> u16 {
    values.get(idx).copied().unwrap_or(MAX_FPS_DEFAULT)
}

impl ChoiceEnum for PresentModePolicy {
    const ALL: &'static [Self] = &[Self::Mailbox, Self::Immediate];
    const DEFAULT: Self = Self::Mailbox;
}

pub(in crate::screens::options) fn selected_present_mode_policy(state: &State) -> PresentModePolicy {
    get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::PresentMode,
    ).map_or(state.present_mode_policy_at_load, PresentModePolicy::from_choice)
}

pub(in crate::screens::options) fn selected_high_dpi(state: &State) -> bool {
    GRAPHICS_OPTIONS_ROWS
        .iter()
        .position(|row| row.id == SubRowId::HighDpi)
        .and_then(|idx| state.sub[SubmenuKind::Graphics].choice_indices.get(idx).copied())
        .is_some_and(yes_no_from_choice)
}

#[inline(always)]
pub(in crate::screens::options) fn set_max_fps_enabled_choice(state: &mut State, enabled: bool) {
    let idx = yes_no_choice_index(enabled);
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::MaxFps,
    ) {
        *slot = idx;
    }
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].cursor_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::MaxFps,
    ) {
        *slot = idx;
    }
}

#[inline(always)]
pub(in crate::screens::options) fn set_max_fps_value_choice_index(state: &mut State, idx: usize) {
    let max_idx = state.max_fps_choices.len().saturating_sub(1);
    let clamped = idx.min(max_idx);
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::MaxFpsValue,
    ) {
        *slot = clamped;
    }
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].cursor_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::MaxFpsValue,
    ) {
        *slot = clamped;
    }
}

#[inline(always)]
pub(in crate::screens::options) fn graphics_show_software_threads(state: &State) -> bool {
    selected_video_renderer(state) == BackendType::Software
}

#[inline(always)]
pub(in crate::screens::options) fn graphics_show_present_mode(state: &State) -> bool {
    get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::VSync,
    ).is_some_and(|idx| !yes_no_from_choice(idx))
}

#[inline(always)]
pub(in crate::screens::options) fn graphics_show_max_fps(state: &State) -> bool {
    graphics_show_present_mode(state)
}

#[inline(always)]
pub(in crate::screens::options) fn max_fps_enabled(state: &State) -> bool {
    get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::MaxFps,
    ).is_some_and(yes_no_from_choice)
}

#[inline(always)]
pub(in crate::screens::options) fn graphics_show_max_fps_value(state: &State) -> bool {
    graphics_show_max_fps(state) && max_fps_enabled(state)
}

#[inline(always)]
pub(in crate::screens::options) fn graphics_show_high_dpi(state: &State) -> bool {
    cfg!(target_os = "macos") && selected_video_renderer(state) == BackendType::OpenGL
}

impl ChoiceEnum for FullscreenType {
    const ALL: &'static [Self] = &[Self::Exclusive, Self::Borderless];
    const DEFAULT: Self = Self::Exclusive;
}

pub(in crate::screens::options) fn selected_fullscreen_type(state: &State) -> FullscreenType {
    get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::FullscreenType,
    ).map_or(FullscreenType::Exclusive, FullscreenType::from_choice)
}

pub(in crate::screens::options) fn selected_display_mode(state: &State) -> DisplayMode {
    let display_choice = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayMode,
    ).unwrap_or(0);
    let windowed_idx = state.display_mode_choices.len().saturating_sub(1);
    if windowed_idx == 0 || display_choice >= windowed_idx {
        DisplayMode::Windowed
    } else {
        DisplayMode::Fullscreen(selected_fullscreen_type(state))
    }
}

pub(in crate::screens::options) fn selected_display_monitor(state: &State) -> usize {
    let display_choice = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayMode,
    ).unwrap_or(0);
    let windowed_idx = state.display_mode_choices.len().saturating_sub(1);
    if windowed_idx == 0 || display_choice >= windowed_idx {
        0
    } else {
        display_choice.min(windowed_idx.saturating_sub(1))
    }
}

pub(in crate::screens::options) fn selected_refresh_rate_millihertz(state: &State) -> u32 {
    let idx = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::RefreshRate,
    ).unwrap_or(0);
    state.refresh_rate_choices.get(idx).copied().unwrap_or(0)
}

pub(in crate::screens::options) fn max_fps_seed_value(state: &State, max_fps: u16) -> u16 {
    if max_fps != 0 {
        return clamped_max_fps(max_fps);
    }

    let selected_refresh_mhz = selected_refresh_rate_millihertz(state);
    let refresh_mhz = if selected_refresh_mhz != 0 {
        selected_refresh_mhz
    } else if let Some(spec) = state.monitor_specs.get(selected_display_monitor(state)) {
        if matches!(selected_display_mode(state), DisplayMode::Fullscreen(_)) {
            let (width, height) = selected_resolution(state);
            display::supported_refresh_rates(Some(spec), width, height)
                .into_iter()
                .max()
                .or_else(|| {
                    spec.modes
                        .iter()
                        .map(|mode| mode.refresh_rate_millihertz)
                        .max()
                })
                .unwrap_or(60_000)
        } else {
            spec.modes
                .iter()
                .map(|mode| mode.refresh_rate_millihertz)
                .max()
                .unwrap_or(60_000)
        }
    } else {
        60_000
    };

    clamped_max_fps(((refresh_mhz + 500) / 1000) as u16)
}

pub(in crate::screens::options) fn seed_max_fps_value_choice(state: &mut State, max_fps: u16) {
    let seeded = max_fps_seed_value(state, max_fps);
    let idx = max_fps_choice_index(&state.max_fps_choices, seeded);
    set_max_fps_value_choice_index(state, idx);
}

pub(in crate::screens::options) fn selected_max_fps(state: &State) -> u16 {
    if !max_fps_enabled(state) {
        return 0;
    }
    let idx = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::MaxFpsValue,
    ).unwrap_or(0);
    max_fps_from_choice(&state.max_fps_choices, idx)
}

pub(in crate::screens::options) fn ensure_display_mode_choices(state: &mut State) {
    state.display_mode_choices = build_display_mode_choices(&state.monitor_specs);
    // If current selection is out of bounds, reset it.
    if let Some(idx) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayMode,
    ) && *idx >= state.display_mode_choices.len()
    {
        *idx = 0;
    }
    if let Some(choice_idx) = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayMode,
    ) && let Some(cursor_idx) = get_choice_by_id_mut(
            &mut state.sub[SubmenuKind::Graphics].cursor_indices,
            GRAPHICS_OPTIONS_ROWS,
            SubRowId::DisplayMode,
        )
    {
        *cursor_idx = choice_idx;
    }
    // Also re-run logic that depends on the selected monitor.
    let current_res = selected_resolution(state);
    rebuild_resolution_choices(state, current_res.0, current_res.1);
}

pub fn update_monitor_specs(state: &mut State, specs: Vec<MonitorSpec>) {
    state.monitor_specs = specs;
    ensure_display_mode_choices(state);
    // Keep the Display Mode row aligned with the actual current mode after monitors refresh.
    set_display_mode_row_selection(
        state,
        state.monitor_specs.len(),
        state.display_mode_at_load,
        state.display_monitor_at_load,
    );
    if state.max_fps_at_load == 0 && !max_fps_enabled(state) {
        seed_max_fps_value_choice(state, 0);
    }
    clear_render_cache(state);
}

pub(in crate::screens::options) fn set_display_mode_row_selection(
    state: &mut State,
    _monitor_count: usize, // Ignored, we use stored monitor_specs now
    mode: DisplayMode,
    monitor: usize,
) {
    // Ensure choices are up to date.
    ensure_display_mode_choices(state);
    let windowed_idx = state.display_mode_choices.len().saturating_sub(1);
    let idx = match mode {
        DisplayMode::Windowed => windowed_idx,
        DisplayMode::Fullscreen(_) => {
            let max_idx = windowed_idx.saturating_sub(1);
            if max_idx == 0 {
                0
            } else {
                monitor.min(max_idx)
            }
        }
    };
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayMode,
    ) {
        *slot = idx;
    }
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].cursor_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayMode,
    ) {
        *slot = idx;
    }
    // Re-trigger resolution rebuild based on the potentially new monitor selection.
    let current_res = selected_resolution(state);
    rebuild_resolution_choices(state, current_res.0, current_res.1);
}

pub(in crate::screens::options) fn selected_aspect_label(state: &State) -> &'static str {
    let idx = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayAspectRatio,
    ).unwrap_or(0);
    DISPLAY_ASPECT_RATIO_CHOICES
        .get(idx)
        .or(Some(&DISPLAY_ASPECT_RATIO_CHOICES[0]))
        .and_then(|c| c.as_str_static())
        .unwrap_or("16:9")
}

pub(in crate::screens::options) fn inferred_aspect_choice(width: u32, height: u32) -> usize {
    if height == 0 {
        return 0;
    }

    if let Some(idx) = DISPLAY_ASPECT_RATIO_CHOICES.iter().position(|c| {
        c.as_str_static()
            .map_or(false, |label| aspect_matches(width, height, label))
    }) {
        return idx;
    }

    let ratio = width as f32 / height as f32;
    let mut best_idx = 0;
    let mut best_delta = f32::INFINITY;
    for (idx, choice) in DISPLAY_ASPECT_RATIO_CHOICES.iter().enumerate() {
        let Some(label) = choice.as_str_static() else {
            continue;
        };
        let target = match label {
            "16:9" => 16.0 / 9.0,
            "16:10" => 16.0 / 10.0,
            "4:3" => 4.0 / 3.0,
            "1:1" => 1.0,
            _ => continue,
        };
        let delta = (ratio - target).abs();
        if delta < best_delta {
            best_delta = delta;
            best_idx = idx;
        }
    }
    best_idx
}

pub(in crate::screens::options) fn sync_display_aspect_ratio(
    state: &mut State,
    width: u32,
    height: u32,
) {
    let idx = inferred_aspect_choice(width, height);
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayAspectRatio,
    ) {
        *slot = idx;
    }
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].cursor_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayAspectRatio,
    ) {
        *slot = idx;
    }
}

pub(in crate::screens::options) fn push_unique_resolution(
    target: &mut Vec<(u32, u32)>,
    width: u32,
    height: u32,
) {
    if !target.iter().any(|&(w, h)| w == width && h == height) {
        target.push((width, height));
    }
}

pub(in crate::screens::options) fn preset_resolutions_for_aspect(label: &str) -> Vec<(u32, u32)> {
    match label.to_ascii_lowercase().as_str() {
        "16:9" => vec![(1280, 720), (1600, 900), (1920, 1080)],
        "16:10" => vec![(1280, 800), (1440, 900), (1680, 1050), (1920, 1200)],
        "4:3" => vec![
            (640, 480),
            (800, 600),
            (1024, 768),
            (1280, 960),
            (1600, 1200),
        ],
        "1:1" => vec![(342, 342), (456, 456), (608, 608), (810, 810), (1080, 1080)],
        _ => DEFAULT_RESOLUTION_CHOICES.to_vec(),
    }
}

pub(in crate::screens::options) fn aspect_matches(width: u32, height: u32, label: &str) -> bool {
    let ratio = width as f32 / height as f32;
    match label {
        "16:9" => (ratio - 1.7777).abs() < 0.05,
        "16:10" => (ratio - 1.6).abs() < 0.05,
        "4:3" => (ratio - 1.3333).abs() < 0.05,
        "1:1" => (ratio - 1.0).abs() < 0.05,
        _ => true,
    }
}

pub(in crate::screens::options) fn selected_resolution(state: &State) -> (u32, u32) {
    let idx = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayResolution,
    ).unwrap_or(0);
    state
        .resolution_choices
        .get(idx)
        .copied()
        .or_else(|| state.resolution_choices.first().copied())
        .unwrap_or((state.display_width_at_load, state.display_height_at_load))
}

pub(in crate::screens::options) fn rebuild_refresh_rate_choices(state: &mut State) {
    if matches!(selected_display_mode(state), DisplayMode::Windowed) {
        state.refresh_rate_choices = vec![0];
        if let Some(slot) = get_choice_by_id_mut(
            &mut state.sub[SubmenuKind::Graphics].choice_indices,
            GRAPHICS_OPTIONS_ROWS,
            SubRowId::RefreshRate,
        ) {
            *slot = 0;
        }
        if let Some(slot) = get_choice_by_id_mut(
            &mut state.sub[SubmenuKind::Graphics].cursor_indices,
            GRAPHICS_OPTIONS_ROWS,
            SubRowId::RefreshRate,
        ) {
            *slot = 0;
        }
        return;
    }

    let (width, height) = selected_resolution(state);
    let mon_idx = selected_display_monitor(state);
    let mut rates = Vec::new();

    // Default choice is always available (0).
    rates.push(0);

    let supported_rates =
        display::supported_refresh_rates(state.monitor_specs.get(mon_idx), width, height);
    rates.extend(supported_rates);

    // Add common fallback rates if list is empty (besides Default)
    if rates.len() == 1 {
        rates.extend_from_slice(&[60000, 75000, 120000, 144000, 165000, 240000]);
    }

    // Preserve current selection if possible, else default to "Default".
    let current_rate = if let Some(idx) = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::RefreshRate,
    ) {
        state.refresh_rate_choices.get(idx).copied().unwrap_or(0)
    } else {
        0
    };

    state.refresh_rate_choices = rates;

    let next_idx = state
        .refresh_rate_choices
        .iter()
        .position(|&r| r == current_rate)
        .unwrap_or(0);
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::RefreshRate,
    ) {
        *slot = next_idx;
    }
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].cursor_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::RefreshRate,
    ) {
        *slot = next_idx;
    }
    if state.max_fps_at_load == 0 && !max_fps_enabled(state) {
        seed_max_fps_value_choice(state, 0);
    }
}

pub(in crate::screens::options) fn rebuild_resolution_choices(
    state: &mut State,
    width: u32,
    height: u32,
) {
    let aspect_label = selected_aspect_label(state);
    let mon_idx = selected_display_monitor(state);

    let mut list: Vec<(u32, u32)> =
        display::supported_resolutions(state.monitor_specs.get(mon_idx))
            .into_iter()
            .filter(|(w, h)| aspect_matches(*w, *h, aspect_label))
            .collect();

    // 2. If list is empty (e.g. no monitor data or Aspect filter too strict), use presets.
    if list.is_empty() {
        list = preset_resolutions_for_aspect(aspect_label);
    }

    // 3. Keep the current resolution only if it matches the selected aspect.
    if aspect_matches(width, height, aspect_label) {
        push_unique_resolution(&mut list, width, height);
    }

    // Sort descending by width then height (typical UI preference).
    list.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    state.resolution_choices = list;
    let next_idx = state
        .resolution_choices
        .iter()
        .position(|&(w, h)| w == width && h == height)
        .unwrap_or(0);
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayResolution,
    ) {
        *slot = next_idx;
    }
    if let Some(slot) = get_choice_by_id_mut(
        &mut state.sub[SubmenuKind::Graphics].cursor_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::DisplayResolution,
    ) {
        *slot = next_idx;
    }

    // Rebuild refresh rates since available rates depend on resolution.
    rebuild_refresh_rate_choices(state);
}
