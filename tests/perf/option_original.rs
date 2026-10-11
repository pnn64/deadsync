use super::*;
const DEFAULT_RESOLUTIONS: &[(u32, u32)] = &[
    (1920, 1080),
    (1600, 900),
    (1280, 720),
    (1024, 768),
    (800, 600),
];

pub(super) fn sound_sample_rate_choices(state: &State) -> Vec<Option<u32>> {
    let device_idx =
        selected_sound_device_choice(state).min(state.sound_device_options.len().saturating_sub(1));
    if let Some(option) = state.sound_device_options.get(device_idx) {
        return audio_sample_rate_choices(&option.sample_rates_hz);
    }
    audio_sample_rate_choices(&[])
}

pub(super) fn audio_sample_rate_choices(sample_rates_hz: &[u32]) -> Vec<Option<u32>> {
    let mut choices = Vec::with_capacity(sample_rates_hz.len() + 1);
    choices.push(None);
    for &hz in sample_rates_hz {
        let rate = Some(hz);
        if !choices.contains(&rate) {
            choices.push(rate);
        }
    }
    if choices.len() == 1 {
        choices.extend([Some(44_100), Some(48_000)]);
    }
    choices
}

pub(super) fn sample_rate_choice_index(state: &State, rate: Option<u32>) -> usize {
    sound_sample_rate_choices(state)
        .iter()
        .position(|&value| value == rate)
        .unwrap_or(0)
}

pub(super) fn sample_rate_from_choice(state: &State, idx: usize) -> Option<u32> {
    sound_sample_rate_choices(state).get(idx).copied().flatten()
}

pub(super) fn aspect_ratio_matches(ratio: f32, label: &str) -> bool {
    match label {
        "16:9" => (ratio - 1.7777).abs() < 0.05,
        "16:10" => (ratio - 1.6).abs() < 0.05,
        "4:3" => (ratio - 1.3333).abs() < 0.05,
        "1:1" => (ratio - 1.0).abs() < 0.05,
        _ => true,
    }
}

pub(super) fn aspect_matches(width: u32, height: u32, label: &str) -> bool {
    if height == 0 {
        return false;
    }
    aspect_ratio_matches(width as f32 / height as f32, label)
}

pub(super) fn preset_resolutions_for_aspect(label: &str) -> Vec<(u32, u32)> {
    match label {
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
        _ => DEFAULT_RESOLUTIONS.to_vec(),
    }
}

pub(super) fn push_unique_resolution(target: &mut Vec<(u32, u32)>, width: u32, height: u32) {
    if !target.contains(&(width, height)) {
        target.push((width, height));
    }
}

pub(super) fn supported_resolutions(spec: Option<&GraphicsMonitorView>) -> Vec<(u32, u32)> {
    let mut values = spec.map_or_else(Vec::new, |spec| {
        spec.modes
            .iter()
            .map(|mode| (mode.width, mode.height))
            .collect()
    });
    values.sort_unstable();
    values.dedup();
    values
}

pub(super) fn supported_refresh_rates(
    spec: Option<&GraphicsMonitorView>,
    width: u32,
    height: u32,
) -> Vec<u32> {
    let mut values = spec.map_or_else(Vec::new, |spec| {
        spec.modes
            .iter()
            .filter(|mode| mode.width == width && mode.height == height)
            .map(|mode| mode.refresh_rate_millihertz)
            .collect()
    });
    values.sort_unstable();
    values.dedup();
    values
}

pub(super) fn max_fps_seed_value(state: &State, max_fps: u16) -> u16 {
    if max_fps != 0 {
        return clamped_max_fps(max_fps);
    }

    let selected_refresh_mhz = selected_refresh_rate_millihertz(state);
    let refresh_mhz = if selected_refresh_mhz != 0 {
        selected_refresh_mhz
    } else if let Some(spec) = state.monitor_specs.get(selected_display_monitor(state)) {
        if matches!(
            selected_display_mode(state),
            DisplayModeChoice::Fullscreen(_)
        ) {
            let (width, height) = selected_resolution(state);
            supported_refresh_rates(Some(spec), width, height)
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

    let recommended = ((refresh_mhz + 500) / 1000).saturating_mul(3);
    clamped_max_fps(recommended.min(u32::from(u16::MAX)) as u16)
}

pub(super) fn rebuild_refresh_rate_choices(state: &mut State) {
    if matches!(selected_display_mode(state), DisplayModeChoice::Windowed) {
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

    let supported_rates = supported_refresh_rates(state.monitor_specs.get(mon_idx), width, height);
    rates.extend(supported_rates);

    // ITGmania keeps the nearest advertised rate within 10 Hz, otherwise Default.
    let current_rate = if let Some(idx) = get_choice_by_id(
        &state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        SubRowId::RefreshRate,
    ) {
        state
            .refresh_rate_choices
            .get(idx)
            .copied()
            .unwrap_or(state.refresh_rate_at_load)
    } else {
        state.refresh_rate_at_load
    };

    state.refresh_rate_choices = rates;

    let next_idx = state
        .refresh_rate_choices
        .iter()
        .enumerate()
        .min_by_key(|(_, rate)| rate.abs_diff(current_rate))
        .filter(|(_, rate)| rate.abs_diff(current_rate) < 10_000)
        .map_or(0, |(idx, _)| idx);
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

pub(super) fn rebuild_resolution_choices(state: &mut State, width: u32, height: u32) {
    let aspect_label = selected_aspect_label(state);
    let mon_idx = selected_display_monitor(state);

    let mut list: Vec<(u32, u32)> = supported_resolutions(state.monitor_specs.get(mon_idx))
        .into_iter()
        .filter(|(w, h)| aspect_matches(*w, *h, aspect_label))
        .collect();

    // 2. If list is empty (e.g. no monitor data or Aspect filter too strict), use presets.
    if list.is_empty() {
        list = preset_resolutions_for_aspect(aspect_label);
    }

    // Keep the selected physical mode even when it uses non-square pixels and
    // therefore does not match the configured display aspect ratio.
    if width > 0 && height > 0 {
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

pub(super) fn collect_row_choices<T: ChoiceText>(
    state: &State,
    kind: SubmenuKind,
    rows: &[SubRow],
    row_idx: usize,
) -> Vec<T> {
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::Gameplay)
        && row.id == SubRowId::DefaultJudgmentPalette
    {
        return state
            .judgment_palettes
            .palettes
            .iter()
            .map(|entry| T::borrowed(entry.name.as_str()))
            .collect();
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::System)
    {
        match row.id {
            SubRowId::DefaultScrollSpeed => {
                return state
                    .system_scroll_speed_values
                    .iter()
                    .map(ToString::to_string)
                    .map(T::owned)
                    .collect();
            }
            SubRowId::DefaultScrollDirection => {
                return state
                    .system_scroll_direction_values
                    .iter()
                    .map(ToString::to_string)
                    .map(T::owned)
                    .collect();
            }
            SubRowId::DefaultBackgroundFilter => {
                return state
                    .system_background_filter_values
                    .iter()
                    .map(|value| format!("{}%", value.percent()))
                    .map(T::owned)
                    .collect();
            }
            SubRowId::DefaultNoteSkin => {
                return string_choice_texts(&state.system_noteskin_choices);
            }
            _ => {}
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::SmxConfig)
    {
        if row.id == SubRowId::SmxBgPack {
            let default_label = T::shared(tr("Common", "Default"));
            let mut choices = vec![default_label];
            choices.extend(
                state
                    .smx_bg_pack_choices
                    .iter()
                    .map(|text| T::borrowed(text.as_str())),
            );
            return choices;
        }
        if row.id == SubRowId::SmxJudgePack {
            let default_label = T::shared(tr("Common", "Default"));
            let mut choices = vec![default_label];
            choices.extend(
                state
                    .smx_judge_pack_choices
                    .iter()
                    .map(|text| T::borrowed(text.as_str())),
            );
            return choices;
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::Graphics)
    {
        if row.id == SubRowId::SoftwareRendererThreads {
            return string_choice_texts(&state.software_thread_labels);
        }
        if row.id == SubRowId::MaxFpsValue {
            return vec![T::owned(selected_max_fps_label(state))];
        }
        if row.id == SubRowId::DisplayMode {
            return string_choice_texts(&state.display_mode_choices);
        }
        if row.id == SubRowId::DisplayResolution {
            return state
                .resolution_choices
                .iter()
                .map(|&(w, h)| T::owned(format!("{w}x{h}")))
                .collect();
        }
        if row.id == SubRowId::RefreshRate {
            return state
                .refresh_rate_choices
                .iter()
                .map(|&mhz| {
                    if mhz == 0 {
                        T::shared(tr("Common", "Default"))
                    } else {
                        // Format nicely: 60000 -> "60 Hz", 59940 -> "59.94 Hz"
                        let hz = mhz as f32 / 1000.0;
                        if (hz.fract()).abs() < 0.01 {
                            T::owned(format!("{hz:.0}Hz"))
                        } else {
                            T::owned(format!("{hz:.2}Hz"))
                        }
                    }
                })
                .collect();
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::Advanced)
        && row.id == SubRowId::SongParsingThreads
    {
        return string_choice_texts(&state.software_thread_labels);
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::NullOrDieOptions)
        && row.id == SubRowId::PackSyncThreads
    {
        return string_choice_texts(&state.software_thread_labels);
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::Sound)
    {
        if row.id == SubRowId::SoundDevice {
            return state
                .sound_device_options
                .iter()
                .map(|opt| T::borrowed(opt.label.as_str()))
                .collect();
        }
        if row.id == SubRowId::AudioSampleRate {
            return sound_sample_rate_choices(state)
                .into_iter()
                .map(|rate| match rate {
                    None => T::shared(tr("Common", "Auto")),
                    Some(hz) => T::owned(format!("{hz} Hz")),
                })
                .collect();
        }
        #[cfg(target_os = "linux")]
        if row.id == SubRowId::LinuxAudioBackend {
            return string_choice_texts(&state.linux_backend_choices);
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::ScoreImport)
    {
        if row.id == SubRowId::ScoreImportProfile {
            return string_choice_texts(&state.score_import_profile_choices);
        }
        if row.id == SubRowId::ScoreImportPack {
            return vec![T::owned(score_import_pack_summary(state))];
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::SyncPacks)
        && row.id == SubRowId::SyncPackPack
    {
        return string_choice_texts(&state.sync_pack_choices);
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::Bookkeeping)
    {
        let value = match row.id {
            SubRowId::CoinsInserted => state.bookkeeping.coins_inserted,
            SubRowId::CreditsSpent => state.bookkeeping.credits_spent,
            SubRowId::PlaysStarted => state.bookkeeping.plays_started,
            SubRowId::StagesPlayed => state.bookkeeping.stages_played,
            _ => 0,
        };
        return vec![T::owned(value.to_string())];
    }
    rows.get(row_idx)
        .map(|row| choice_texts(row.choices))
        .unwrap_or_default()
}

pub(super) fn resolution_with_current_refresh(state: &mut State, width: u32, height: u32) {
    let aspect_label = selected_aspect_label(state);
    let mon_idx = selected_display_monitor(state);

    let mut list: Vec<(u32, u32)> = supported_resolutions(state.monitor_specs.get(mon_idx))
        .into_iter()
        .filter(|(w, h)| aspect_matches(*w, *h, aspect_label))
        .collect();

    // 2. If list is empty (e.g. no monitor data or Aspect filter too strict), use presets.
    if list.is_empty() {
        list = preset_resolutions_for_aspect(aspect_label);
    }

    // Keep the selected physical mode even when it uses non-square pixels and
    // therefore does not match the configured display aspect ratio.
    if width > 0 && height > 0 {
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
    super::rebuild_refresh_rate_choices(state);
}
