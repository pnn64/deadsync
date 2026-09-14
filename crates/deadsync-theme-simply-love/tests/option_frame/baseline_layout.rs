// Frozen from fc570795b (0.5.1206); only imports/visibility adapted.
use super::*;
use std::borrow::Cow;

pub(in crate::screens::options) fn row_choices(
    state: &State,
    kind: SubmenuKind,
    rows: &[SubRow],
    row_idx: usize,
) -> Vec<Cow<'static, str>> {
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::Gameplay)
        && row.id == SubRowId::DefaultJudgmentPalette
    {
        return state
            .judgment_palettes
            .palettes
            .iter()
            .map(|entry| Cow::Owned(entry.name.clone()))
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
                    .map(Cow::Owned)
                    .collect();
            }
            SubRowId::DefaultScrollDirection => {
                return state
                    .system_scroll_direction_values
                    .iter()
                    .map(ToString::to_string)
                    .map(Cow::Owned)
                    .collect();
            }
            SubRowId::DefaultBackgroundFilter => {
                return state
                    .system_background_filter_values
                    .iter()
                    .map(|value| format!("{}%", value.percent()))
                    .map(Cow::Owned)
                    .collect();
            }
            SubRowId::DefaultNoteSkin => {
                return state
                    .system_noteskin_choices
                    .iter()
                    .cloned()
                    .map(Cow::Owned)
                    .collect();
            }
            _ => {}
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::SmxConfig)
    {
        if row.id == SubRowId::SmxBgPack {
            let default_label = tr("Common", "Default").to_string();
            let mut choices = vec![Cow::Owned(default_label)];
            choices.extend(state.smx_bg_pack_choices.iter().cloned().map(Cow::Owned));
            return choices;
        }
        if row.id == SubRowId::SmxJudgePack {
            let default_label = tr("Common", "Default").to_string();
            let mut choices = vec![Cow::Owned(default_label)];
            choices.extend(state.smx_judge_pack_choices.iter().cloned().map(Cow::Owned));
            return choices;
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::Graphics)
    {
        if row.id == SubRowId::SoftwareRendererThreads {
            return state
                .software_thread_labels
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
        if row.id == SubRowId::MaxFpsValue {
            return vec![Cow::Owned(selected_max_fps_label(state))];
        }
        if row.id == SubRowId::DisplayMode {
            return state
                .display_mode_choices
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
        if row.id == SubRowId::DisplayResolution {
            return state
                .resolution_choices
                .iter()
                .map(|&(w, h)| Cow::Owned(format!("{w}x{h}")))
                .collect();
        }
        if row.id == SubRowId::RefreshRate {
            return state
                .refresh_rate_choices
                .iter()
                .map(|&mhz| {
                    if mhz == 0 {
                        Cow::Owned(tr("Common", "Default").to_string())
                    } else {
                        // Format nicely: 60000 -> "60 Hz", 59940 -> "59.94 Hz"
                        let hz = mhz as f32 / 1000.0;
                        if (hz.fract()).abs() < 0.01 {
                            Cow::Owned(format!("{hz:.0}Hz"))
                        } else {
                            Cow::Owned(format!("{hz:.2}Hz"))
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
        return state
            .software_thread_labels
            .iter()
            .cloned()
            .map(Cow::Owned)
            .collect();
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::NullOrDieOptions)
        && row.id == SubRowId::PackSyncThreads
    {
        return state
            .software_thread_labels
            .iter()
            .cloned()
            .map(Cow::Owned)
            .collect();
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::Sound)
    {
        if row.id == SubRowId::SoundDevice {
            return state
                .sound_device_options
                .iter()
                .map(|opt| Cow::Owned(opt.label.clone()))
                .collect();
        }
        if row.id == SubRowId::AudioSampleRate {
            return sound_sample_rate_choices(state)
                .into_iter()
                .map(|rate| match rate {
                    None => Cow::Owned(tr("Common", "Auto").to_string()),
                    Some(hz) => Cow::Owned(format!("{hz} Hz")),
                })
                .collect();
        }
        #[cfg(target_os = "linux")]
        if row.id == SubRowId::LinuxAudioBackend {
            return state
                .linux_backend_choices
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::ScoreImport)
    {
        if row.id == SubRowId::ScoreImportProfile {
            return state
                .score_import_profile_choices
                .iter()
                .cloned()
                .map(Cow::Owned)
                .collect();
        }
        if row.id == SubRowId::ScoreImportPack {
            return vec![Cow::Owned(score_import_pack_summary(state))];
        }
    }
    if let Some(row) = rows.get(row_idx)
        && matches!(kind, SubmenuKind::SyncPacks)
        && row.id == SubRowId::SyncPackPack
    {
        return state
            .sync_pack_choices
            .iter()
            .cloned()
            .map(Cow::Owned)
            .collect();
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
        return vec![Cow::Owned(value.to_string())];
    }
    rows.get(row_idx)
        .map(|row| {
            row.choices
                .iter()
                .map(|c| Cow::Owned(c.get().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub(in crate::screens::options) fn submenu_display_choice_texts(
    state: &State,
    kind: SubmenuKind,
    rows: &[SubRow],
    row_idx: usize,
) -> Vec<Cow<'static, str>> {
    let mut choice_texts = row_choices(state, kind, rows, row_idx);
    let Some(row) = rows.get(row_idx) else {
        return choice_texts;
    };
    if choice_texts.is_empty() {
        return choice_texts;
    }
    if row.id == SubRowId::GlobalOffset {
        choice_texts[0] = Cow::Owned(format_ms(state.global_offset_ms));
    } else if row.id == SubRowId::MasterVolume {
        choice_texts[0] = Cow::Owned(format_percent(state.master_volume_pct));
    } else if row.id == SubRowId::SfxVolume {
        choice_texts[0] = Cow::Owned(format_percent(state.sfx_volume_pct));
    } else if row.id == SubRowId::AssistTickVolume {
        choice_texts[0] = Cow::Owned(format_percent(state.assist_tick_volume_pct));
    } else if row.id == SubRowId::MusicVolume {
        choice_texts[0] = Cow::Owned(format_percent(state.music_volume_pct));
    } else if row.id == SubRowId::SmxDefaultLightBrightness {
        choice_texts[0] = Cow::Owned(format_percent(state.smx_default_light_brightness_pct));
    } else if row.id == SubRowId::VisualDelay {
        choice_texts[0] = Cow::Owned(format_ms(state.visual_delay_ms));
    } else if row.id == SubRowId::Debounce {
        choice_texts[0] = Cow::Owned(format_ms(state.input_debounce_ms));
    } else if row.id == SubRowId::Fingerprint {
        choice_texts[0] = Cow::Owned(format_tenths_ms(state.null_or_die_fingerprint_tenths));
    } else if row.id == SubRowId::Window {
        choice_texts[0] = Cow::Owned(format_tenths_ms(state.null_or_die_window_tenths));
    } else if row.id == SubRowId::Step {
        choice_texts[0] = Cow::Owned(format_tenths_ms(state.null_or_die_step_tenths));
    } else if row.id == SubRowId::MagicOffset {
        choice_texts[0] = Cow::Owned(format_tenths_ms(state.null_or_die_magic_offset_tenths));
    }
    choice_texts
}

pub(in crate::screens::options) fn build_submenu_row_layout(
    state: &State,
    asset_manager: &AssetManager,
    kind: SubmenuKind,
    row_idx: usize,
) -> Option<SubmenuRowLayout> {
    let rows = submenu_rows(kind);
    let row = rows.get(row_idx)?;
    let choice_texts = submenu_display_choice_texts(state, kind, rows, row_idx);
    if choice_texts.is_empty() {
        return None;
    }
    let is_visual_style = row.id == SubRowId::VisualStyle;
    let is_color_choice = row.id == SubRowId::PreferredColor;
    let value_zoom = if is_visual_style {
        VISUAL_STYLE_VALUE_ZOOM
    } else {
        SUBMENU_VALUE_ZOOM
    };
    let inline_spacing = if is_visual_style {
        VISUAL_STYLE_INLINE_SPACING
    } else if is_color_choice {
        COLOR_CHOICE_INLINE_SPACING
    } else {
        INLINE_SPACING
    };
    let texts: Vec<Arc<str>> = choice_texts
        .iter()
        .map(|text| Arc::<str>::from(text.as_ref()))
        .collect();
    let mut widths: Vec<f32> = Vec::with_capacity(choice_texts.len());
    let mut text_h = 16.0_f32;
    if is_color_choice {
        let [width, height] = selected_visual_assets(state).select_color_size;
        let aspect = width as f32 / height.max(1) as f32;
        text_h = COLOR_CHOICE_ICON_H;
        widths.resize(texts.len(), COLOR_CHOICE_ICON_H * aspect);
    } else {
        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font("miso", |metrics_font| {
                text_h = (metrics_font.height as f32).max(1.0) * value_zoom;
                for text in &texts {
                    let mut w =
                        font::measure_line_width_logical(metrics_font, text.as_ref(), all_fonts)
                            as f32;
                    if !w.is_finite() || w <= 0.0 {
                        w = 1.0;
                    }
                    widths.push(w * value_zoom);
                }
            });
        });
    }
    if widths.len() != texts.len() {
        widths.clear();
        widths.extend(
            texts
                .iter()
                .map(|text| (text.chars().count().max(1) as f32) * 8.0 * value_zoom),
        );
    }
    let inline_row = row.inline && submenu_inline_widths_fit(&widths, inline_spacing);
    let mut x_positions: Vec<f32> = Vec::new();
    let mut centers: Vec<f32> = Vec::new();
    if inline_row {
        x_positions = Vec::with_capacity(widths.len());
        centers = Vec::with_capacity(widths.len());
        let mut x = 0.0_f32;
        for &draw_w in &widths {
            x_positions.push(x);
            centers.push(draw_w.mul_add(0.5, x));
            x += draw_w + inline_spacing;
        }
    }
    Some(SubmenuRowLayout {
        texts: Arc::from(texts),
        widths: Arc::from(widths),
        x_positions: Arc::from(x_positions),
        centers: Arc::from(centers),
        text_h,
        value_zoom,
        inline_spacing,
        inline_row,
    })
}
