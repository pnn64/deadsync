use super::*;

/// Returns `true` when the given submenu row should be treated as disabled
/// (non-interactive and visually dimmed). Add new cases here for any row
/// that should be conditionally locked based on runtime state.
pub(super) fn is_submenu_row_disabled(kind: SubmenuKind, id: SubRowId) -> bool {
    match (kind, id) {
        (SubmenuKind::InputBackend, SubRowId::MenuButtons) => {
            !crate::engine::input::any_player_has_dedicated_menu_buttons_for_mode(
                config::get().three_key_navigation,
            )
        }
        _ => false,
    }
}

pub(super) const fn submenu_rows(kind: SubmenuKind) -> &'static [SubRow] {
    match kind {
        SubmenuKind::System => SYSTEM_OPTIONS_ROWS,
        SubmenuKind::Graphics => GRAPHICS_OPTIONS_ROWS,
        SubmenuKind::Input => INPUT_OPTIONS_ROWS,
        SubmenuKind::InputBackend => INPUT_BACKEND_OPTIONS_ROWS,
        SubmenuKind::OnlineScoring => ONLINE_SCORING_OPTIONS_ROWS,
        SubmenuKind::NullOrDie => NULL_OR_DIE_MENU_ROWS,
        SubmenuKind::NullOrDieOptions => NULL_OR_DIE_OPTIONS_ROWS,
        SubmenuKind::SyncPacks => SYNC_PACK_OPTIONS_ROWS,
        SubmenuKind::Machine => MACHINE_OPTIONS_ROWS,
        SubmenuKind::Advanced => ADVANCED_OPTIONS_ROWS,
        SubmenuKind::Course => COURSE_OPTIONS_ROWS,
        SubmenuKind::Gameplay => GAMEPLAY_OPTIONS_ROWS,
        SubmenuKind::Sound => SOUND_OPTIONS_ROWS,
        SubmenuKind::SelectMusic => SELECT_MUSIC_OPTIONS_ROWS,
        SubmenuKind::GrooveStats => GROOVESTATS_OPTIONS_ROWS,
        SubmenuKind::ArrowCloud => ARROWCLOUD_OPTIONS_ROWS,
        SubmenuKind::ScoreImport => SCORE_IMPORT_OPTIONS_ROWS,
    }
}

pub(super) const fn submenu_items(kind: SubmenuKind) -> &'static [Item] {
    match kind {
        SubmenuKind::System => SYSTEM_OPTIONS_ITEMS,
        SubmenuKind::Graphics => GRAPHICS_OPTIONS_ITEMS,
        SubmenuKind::Input => INPUT_OPTIONS_ITEMS,
        SubmenuKind::InputBackend => INPUT_BACKEND_OPTIONS_ITEMS,
        SubmenuKind::OnlineScoring => ONLINE_SCORING_OPTIONS_ITEMS,
        SubmenuKind::NullOrDie => NULL_OR_DIE_MENU_ITEMS,
        SubmenuKind::NullOrDieOptions => NULL_OR_DIE_OPTIONS_ITEMS,
        SubmenuKind::SyncPacks => SYNC_PACK_OPTIONS_ITEMS,
        SubmenuKind::Machine => MACHINE_OPTIONS_ITEMS,
        SubmenuKind::Advanced => ADVANCED_OPTIONS_ITEMS,
        SubmenuKind::Course => COURSE_OPTIONS_ITEMS,
        SubmenuKind::Gameplay => GAMEPLAY_OPTIONS_ITEMS,
        SubmenuKind::Sound => SOUND_OPTIONS_ITEMS,
        SubmenuKind::SelectMusic => SELECT_MUSIC_OPTIONS_ITEMS,
        SubmenuKind::GrooveStats => GROOVESTATS_OPTIONS_ITEMS,
        SubmenuKind::ArrowCloud => ARROWCLOUD_OPTIONS_ITEMS,
        SubmenuKind::ScoreImport => SCORE_IMPORT_OPTIONS_ITEMS,
    }
}

pub(super) const fn submenu_title(kind: SubmenuKind) -> &'static str {
    match kind {
        SubmenuKind::System => "SYSTEM OPTIONS",
        SubmenuKind::Graphics => "GRAPHICS OPTIONS",
        SubmenuKind::Input => "INPUT OPTIONS",
        SubmenuKind::InputBackend => "INPUT OPTIONS",
        SubmenuKind::OnlineScoring => "ONLINE SCORE SERVICES",
        SubmenuKind::NullOrDie => "NULL-OR-DIE OPTIONS",
        SubmenuKind::NullOrDieOptions => "NULL-OR-DIE OPTIONS",
        SubmenuKind::SyncPacks => "SYNC PACKS",
        SubmenuKind::Machine => "MACHINE OPTIONS",
        SubmenuKind::Advanced => "ADVANCED OPTIONS",
        SubmenuKind::Course => "COURSE OPTIONS",
        SubmenuKind::Gameplay => "GAMEPLAY OPTIONS",
        SubmenuKind::Sound => "SOUND OPTIONS",
        SubmenuKind::SelectMusic => "SELECT MUSIC OPTIONS",
        SubmenuKind::GrooveStats => "GROOVESTATS OPTIONS",
        SubmenuKind::ArrowCloud => "ARROWCLOUD OPTIONS",
        SubmenuKind::ScoreImport => "SCORE IMPORT",
    }
}

pub(super) fn submenu_visible_row_indices(state: &State, kind: SubmenuKind, rows: &[SubRow]) -> Vec<usize> {
    match kind {
        SubmenuKind::Graphics => {
            let show_sw = graphics_show_software_threads(state);
            let show_present_mode = graphics_show_present_mode(state);
            let show_max_fps = graphics_show_max_fps(state);
            let show_max_fps_value = graphics_show_max_fps_value(state);
            let show_high_dpi = graphics_show_high_dpi(state);
            rows.iter()
                .enumerate()
                .filter_map(|(idx, row)| {
                    if row.id == SubRowId::SoftwareRendererThreads && !show_sw {
                        None
                    } else if row.id == SubRowId::PresentMode && !show_present_mode {
                        None
                    } else if row.id == SubRowId::MaxFps && !show_max_fps {
                        None
                    } else if row.id == SubRowId::MaxFpsValue && !show_max_fps_value {
                        None
                    } else if row.id == SubRowId::HighDpi && !show_high_dpi {
                        None
                    } else {
                        Some(idx)
                    }
                })
                .collect()
        }
        SubmenuKind::Advanced => rows.iter().enumerate().map(|(idx, _)| idx).collect(),
        SubmenuKind::SelectMusic => {
            let show_banners = state
                .sub_choice_indices_select_music
                .get(SELECT_MUSIC_SHOW_BANNERS_ROW_INDEX)
                .copied()
                .unwrap_or_else(|| yes_no_choice_index(true));
            let show_banners = yes_no_from_choice(show_banners);
            let show_breakdown = state
                .sub_choice_indices_select_music
                .get(SELECT_MUSIC_SHOW_BREAKDOWN_ROW_INDEX)
                .copied()
                .unwrap_or_else(|| yes_no_choice_index(true));
            let show_breakdown = yes_no_from_choice(show_breakdown);
            let show_previews = state
                .sub_choice_indices_select_music
                .get(SELECT_MUSIC_MUSIC_PREVIEWS_ROW_INDEX)
                .copied()
                .unwrap_or_else(|| yes_no_choice_index(true));
            let show_previews = yes_no_from_choice(show_previews);
            let show_scorebox = state
                .sub_choice_indices_select_music
                .get(SELECT_MUSIC_SHOW_SCOREBOX_ROW_INDEX)
                .copied()
                .unwrap_or_else(|| yes_no_choice_index(true));
            let show_scorebox = yes_no_from_choice(show_scorebox);
            rows.iter()
                .enumerate()
                .filter_map(|(idx, _)| {
                    if idx == SELECT_MUSIC_SHOW_VIDEO_BANNERS_ROW_INDEX && !show_banners {
                        None
                    } else if idx == SELECT_MUSIC_BREAKDOWN_STYLE_ROW_INDEX && !show_breakdown {
                        None
                    } else if idx == SELECT_MUSIC_PREVIEW_LOOP_ROW_INDEX && !show_previews {
                        None
                    } else if idx == SELECT_MUSIC_SCOREBOX_PLACEMENT_ROW_INDEX && !show_scorebox {
                        None
                    } else if idx == SELECT_MUSIC_SCOREBOX_CYCLE_ROW_INDEX && !show_scorebox {
                        None
                    } else {
                        Some(idx)
                    }
                })
                .collect()
        }
        SubmenuKind::Machine => {
            let show_preferred_style = state
                .sub_choice_indices_machine
                .get(MACHINE_SELECT_STYLE_ROW_INDEX)
                .copied()
                .unwrap_or(1)
                == 0;
            let show_preferred_mode = state
                .sub_choice_indices_machine
                .get(MACHINE_SELECT_PLAY_MODE_ROW_INDEX)
                .copied()
                .unwrap_or(1)
                == 0;
            rows.iter()
                .enumerate()
                .filter_map(|(idx, _)| {
                    if idx == MACHINE_PREFERRED_STYLE_ROW_INDEX && !show_preferred_style {
                        None
                    } else if idx == MACHINE_PREFERRED_MODE_ROW_INDEX && !show_preferred_mode {
                        None
                    } else {
                        Some(idx)
                    }
                })
                .collect()
        }
        #[cfg(target_os = "linux")]
        SubmenuKind::Sound => rows
            .iter()
            .enumerate()
            .filter_map(|(idx, row)| {
                if row.id == SubRowId::AlsaExclusive && !sound_show_alsa_exclusive(state) {
                    None
                } else {
                    Some(idx)
                }
            })
            .collect(),
        _ => (0..rows.len()).collect(),
    }
}

pub(super) fn submenu_total_rows(state: &State, kind: SubmenuKind) -> usize {
    let rows = submenu_rows(kind);
    submenu_visible_row_indices(state, kind, rows).len() + 1
}

pub(super) fn submenu_visible_row_to_actual(
    state: &State,
    kind: SubmenuKind,
    visible_row_idx: usize,
) -> Option<usize> {
    let rows = submenu_rows(kind);
    let visible_rows = submenu_visible_row_indices(state, kind, rows);
    visible_rows.get(visible_row_idx).copied()
}

#[cfg(target_os = "windows")]
const fn windows_backend_choice_index(backend: WindowsPadBackend) -> usize {
    match backend {
        WindowsPadBackend::Auto | WindowsPadBackend::RawInput => 0,
        WindowsPadBackend::Wgi => 1,
    }
}

#[cfg(target_os = "windows")]
const fn windows_backend_from_choice(idx: usize) -> WindowsPadBackend {
    match idx {
        0 => WindowsPadBackend::RawInput,
        _ => WindowsPadBackend::Wgi,
    }
}

pub(super) fn submenu_choice_indices(state: &State, kind: SubmenuKind) -> &[usize] {
    match kind {
        SubmenuKind::System => &state.sub_choice_indices_system,
        SubmenuKind::Graphics => &state.sub_choice_indices_graphics,
        SubmenuKind::Input => &state.sub_choice_indices_input,
        SubmenuKind::InputBackend => &state.sub_choice_indices_input_backend,
        SubmenuKind::OnlineScoring => &state.sub_choice_indices_online_scoring,
        SubmenuKind::NullOrDie => &state.sub_choice_indices_null_or_die,
        SubmenuKind::NullOrDieOptions => &state.sub_choice_indices_null_or_die_options,
        SubmenuKind::SyncPacks => &state.sub_choice_indices_sync_packs,
        SubmenuKind::Machine => &state.sub_choice_indices_machine,
        SubmenuKind::Advanced => &state.sub_choice_indices_advanced,
        SubmenuKind::Course => &state.sub_choice_indices_course,
        SubmenuKind::Gameplay => &state.sub_choice_indices_gameplay,
        SubmenuKind::Sound => &state.sub_choice_indices_sound,
        SubmenuKind::SelectMusic => &state.sub_choice_indices_select_music,
        SubmenuKind::GrooveStats => &state.sub_choice_indices_groovestats,
        SubmenuKind::ArrowCloud => &state.sub_choice_indices_arrowcloud,
        SubmenuKind::ScoreImport => &state.sub_choice_indices_score_import,
    }
}

pub(super) const fn submenu_choice_indices_mut(state: &mut State, kind: SubmenuKind) -> &mut Vec<usize> {
    match kind {
        SubmenuKind::System => &mut state.sub_choice_indices_system,
        SubmenuKind::Graphics => &mut state.sub_choice_indices_graphics,
        SubmenuKind::Input => &mut state.sub_choice_indices_input,
        SubmenuKind::InputBackend => &mut state.sub_choice_indices_input_backend,
        SubmenuKind::OnlineScoring => &mut state.sub_choice_indices_online_scoring,
        SubmenuKind::NullOrDie => &mut state.sub_choice_indices_null_or_die,
        SubmenuKind::NullOrDieOptions => &mut state.sub_choice_indices_null_or_die_options,
        SubmenuKind::SyncPacks => &mut state.sub_choice_indices_sync_packs,
        SubmenuKind::Machine => &mut state.sub_choice_indices_machine,
        SubmenuKind::Advanced => &mut state.sub_choice_indices_advanced,
        SubmenuKind::Course => &mut state.sub_choice_indices_course,
        SubmenuKind::Gameplay => &mut state.sub_choice_indices_gameplay,
        SubmenuKind::Sound => &mut state.sub_choice_indices_sound,
        SubmenuKind::SelectMusic => &mut state.sub_choice_indices_select_music,
        SubmenuKind::GrooveStats => &mut state.sub_choice_indices_groovestats,
        SubmenuKind::ArrowCloud => &mut state.sub_choice_indices_arrowcloud,
        SubmenuKind::ScoreImport => &mut state.sub_choice_indices_score_import,
    }
}

pub(super) fn submenu_cursor_indices(state: &State, kind: SubmenuKind) -> &[usize] {
    match kind {
        SubmenuKind::System => &state.sub_cursor_indices_system,
        SubmenuKind::Graphics => &state.sub_cursor_indices_graphics,
        SubmenuKind::Input => &state.sub_cursor_indices_input,
        SubmenuKind::InputBackend => &state.sub_cursor_indices_input_backend,
        SubmenuKind::OnlineScoring => &state.sub_cursor_indices_online_scoring,
        SubmenuKind::NullOrDie => &state.sub_cursor_indices_null_or_die,
        SubmenuKind::NullOrDieOptions => &state.sub_cursor_indices_null_or_die_options,
        SubmenuKind::SyncPacks => &state.sub_cursor_indices_sync_packs,
        SubmenuKind::Machine => &state.sub_cursor_indices_machine,
        SubmenuKind::Advanced => &state.sub_cursor_indices_advanced,
        SubmenuKind::Course => &state.sub_cursor_indices_course,
        SubmenuKind::Gameplay => &state.sub_cursor_indices_gameplay,
        SubmenuKind::Sound => &state.sub_cursor_indices_sound,
        SubmenuKind::SelectMusic => &state.sub_cursor_indices_select_music,
        SubmenuKind::GrooveStats => &state.sub_cursor_indices_groovestats,
        SubmenuKind::ArrowCloud => &state.sub_cursor_indices_arrowcloud,
        SubmenuKind::ScoreImport => &state.sub_cursor_indices_score_import,
    }
}

pub(super) const fn submenu_cursor_indices_mut(state: &mut State, kind: SubmenuKind) -> &mut Vec<usize> {
    match kind {
        SubmenuKind::System => &mut state.sub_cursor_indices_system,
        SubmenuKind::Graphics => &mut state.sub_cursor_indices_graphics,
        SubmenuKind::Input => &mut state.sub_cursor_indices_input,
        SubmenuKind::InputBackend => &mut state.sub_cursor_indices_input_backend,
        SubmenuKind::OnlineScoring => &mut state.sub_cursor_indices_online_scoring,
        SubmenuKind::NullOrDie => &mut state.sub_cursor_indices_null_or_die,
        SubmenuKind::NullOrDieOptions => &mut state.sub_cursor_indices_null_or_die_options,
        SubmenuKind::SyncPacks => &mut state.sub_cursor_indices_sync_packs,
        SubmenuKind::Machine => &mut state.sub_cursor_indices_machine,
        SubmenuKind::Advanced => &mut state.sub_cursor_indices_advanced,
        SubmenuKind::Course => &mut state.sub_cursor_indices_course,
        SubmenuKind::Gameplay => &mut state.sub_cursor_indices_gameplay,
        SubmenuKind::Sound => &mut state.sub_cursor_indices_sound,
        SubmenuKind::SelectMusic => &mut state.sub_cursor_indices_select_music,
        SubmenuKind::GrooveStats => &mut state.sub_cursor_indices_groovestats,
        SubmenuKind::ArrowCloud => &mut state.sub_cursor_indices_arrowcloud,
        SubmenuKind::ScoreImport => &mut state.sub_cursor_indices_score_import,
    }
}

pub(super) fn sync_submenu_cursor_indices(state: &mut State) {
    state.sub_cursor_indices_system = state.sub_choice_indices_system.clone();
    state.sub_cursor_indices_graphics = state.sub_choice_indices_graphics.clone();
    state.sub_cursor_indices_input = state.sub_choice_indices_input.clone();
    state.sub_cursor_indices_input_backend = state.sub_choice_indices_input_backend.clone();
    state.sub_cursor_indices_online_scoring = state.sub_choice_indices_online_scoring.clone();
    state.sub_cursor_indices_null_or_die = state.sub_choice_indices_null_or_die.clone();
    state.sub_cursor_indices_null_or_die_options =
        state.sub_choice_indices_null_or_die_options.clone();
    state.sub_cursor_indices_sync_packs = state.sub_choice_indices_sync_packs.clone();
    state.sub_cursor_indices_machine = state.sub_choice_indices_machine.clone();
    state.sub_cursor_indices_advanced = state.sub_choice_indices_advanced.clone();
    state.sub_cursor_indices_course = state.sub_choice_indices_course.clone();
    state.sub_cursor_indices_gameplay = state.sub_choice_indices_gameplay.clone();
    state.sub_cursor_indices_sound = state.sub_choice_indices_sound.clone();
    state.sub_cursor_indices_select_music = state.sub_choice_indices_select_music.clone();
    state.sub_cursor_indices_groovestats = state.sub_choice_indices_groovestats.clone();
    state.sub_cursor_indices_arrowcloud = state.sub_choice_indices_arrowcloud.clone();
    state.sub_cursor_indices_score_import = state.sub_choice_indices_score_import.clone();
}
