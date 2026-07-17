use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::select_music::MusicWheelEntry;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_simfile::song_search::{
    SongSearchCandidate, SongSearchCatalogEntry, build_song_search_candidates,
    song_search_difficulties_text,
};

use super::scroll_anim_dir;

pub const SONG_SEARCH_FOCUS_TWEEN_SECONDS: f32 = 0.1;
pub const SONG_SEARCH_INPUT_LOCK_SECONDS: f32 = 0.25;

const SONG_SEARCH_PROMPT_TITLE: &str = "Song Search";
const SONG_SEARCH_PROMPT_HINT: &str = "'pack/song' format will search for songs in specific packs\n'[###]' format will search for BPMs/Difficulties";
const SONG_SEARCH_PROMPT_MAX_LEN: usize = 30;
const SONG_SEARCH_TEXT_ENTRY_W: f32 = 620.0;
const SONG_SEARCH_TEXT_ENTRY_H: f32 = 190.0;
const SONG_SEARCH_TEXT_ENTRY_CURSOR_PERIOD: f32 = 1.0;
const SONG_SEARCH_TEXT_ENTRY_FOOTER: &str = "ENTER: SEARCH    ESC: CANCEL";
const SONG_SEARCH_PANE_W: f32 = 319.0;
const SONG_SEARCH_PANE_H: f32 = 319.0;
const SONG_SEARCH_PANE_BORDER: f32 = 2.0;
const SONG_SEARCH_TEXT_H: f32 = 15.0;
const SONG_SEARCH_ROW_SPACING: f32 = 30.0;
const SONG_SEARCH_RESULT_TITLE_W: f32 = 155.0;
const SONG_SEARCH_DETAIL_LABEL_W: f32 = 145.0;
const SONG_SEARCH_DETAIL_VALUE_W: f32 = 115.0;
const SONG_SEARCH_WHEEL_SLOTS: usize = 12;
const SONG_SEARCH_WHEEL_FOCUS_SLOT: usize = SONG_SEARCH_WHEEL_SLOTS / 2 - 1;

#[derive(Clone, Debug)]
pub struct SongSearchResultsState {
    pub search_text: String,
    pub chart_type: &'static str,
    pub candidates: Vec<SongSearchCandidate>,
    pub selected_index: usize,
    pub prev_selected_index: usize,
    pub last_move_dir: isize,
    pub focus_anim_elapsed: f32,
    pub input_lock: f32,
}

#[derive(Clone, Debug)]
pub struct SongSearchTextEntryState {
    pub query: String,
    pub blink_t: f32,
}

#[derive(Clone, Debug)]
pub enum SongSearchState {
    Hidden,
    TextEntry(SongSearchTextEntryState),
    Results(SongSearchResultsState),
}

pub fn begin_song_search_prompt() -> SongSearchState {
    SongSearchState::TextEntry(SongSearchTextEntryState {
        query: String::new(),
        blink_t: 0.0,
    })
}

pub fn begin_song_search_results(
    group_entries: &[MusicWheelEntry],
    search_text: String,
    chart_type: &'static str,
) -> SongSearchState {
    let trimmed = search_text.trim().to_string();
    if trimmed.is_empty() {
        return SongSearchState::Hidden;
    }
    let candidates = build_song_search_candidates(
        group_entries.iter().map(|entry| match entry {
            MusicWheelEntry::PackHeader { name, .. } => {
                SongSearchCatalogEntry::PackHeader(name.as_str())
            }
            MusicWheelEntry::Song(song) => SongSearchCatalogEntry::Song(song),
        }),
        &trimmed,
        chart_type,
    );
    SongSearchState::Results(SongSearchResultsState {
        search_text: trimmed,
        chart_type,
        candidates,
        selected_index: 0,
        prev_selected_index: 0,
        last_move_dir: 0,
        focus_anim_elapsed: SONG_SEARCH_FOCUS_TWEEN_SECONDS,
        input_lock: SONG_SEARCH_INPUT_LOCK_SECONDS,
    })
}

pub fn update_song_search(state: &mut SongSearchState, dt: f32) -> bool {
    let dt = dt.max(0.0);
    match state {
        SongSearchState::Hidden => false,
        SongSearchState::TextEntry(entry) => {
            entry.blink_t = (entry.blink_t + dt) % SONG_SEARCH_TEXT_ENTRY_CURSOR_PERIOD;
            true
        }
        SongSearchState::Results(results) => {
            results.input_lock = (results.input_lock - dt).max(0.0);
            if results.focus_anim_elapsed < SONG_SEARCH_FOCUS_TWEEN_SECONDS {
                results.focus_anim_elapsed =
                    (results.focus_anim_elapsed + dt).min(SONG_SEARCH_FOCUS_TWEEN_SECONDS);
            }
            true
        }
    }
}

pub fn song_search_add_text(entry: &mut SongSearchTextEntryState, text: &str) {
    let mut len = entry.query.chars().count();
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if len >= SONG_SEARCH_PROMPT_MAX_LEN {
            break;
        }
        entry.query.push(ch);
        len += 1;
    }
}

#[inline(always)]
pub fn song_search_backspace(entry: &mut SongSearchTextEntryState) {
    let _ = entry.query.pop();
}

#[inline(always)]
pub fn song_search_total_items(results: &SongSearchResultsState) -> usize {
    results.candidates.len() + 1
}

pub fn song_search_move(results: &mut SongSearchResultsState, delta: isize) -> bool {
    let len = song_search_total_items(results);
    if len == 0 || delta == 0 {
        return false;
    }
    let old = results.selected_index.min(len - 1);
    let next = ((old as isize + delta).rem_euclid(len as isize)) as usize;
    if next == old {
        return false;
    }
    results.prev_selected_index = old;
    results.last_move_dir = delta.signum();
    results.selected_index = next;
    results.focus_anim_elapsed = 0.0;
    true
}

#[inline(always)]
pub fn song_search_focused_candidate(
    results: &SongSearchResultsState,
) -> Option<&SongSearchCandidate> {
    results.candidates.get(results.selected_index)
}

pub fn build_song_search_overlay(
    state: &SongSearchState,
    active_color_index: i32,
    machine_font: MachineFont,
) -> Option<Vec<Actor>> {
    let mut actors = Vec::new();
    if matches!(state, SongSearchState::Hidden) {
        return None;
    }

    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.8):
        z(1450)
    ));

    match state {
        SongSearchState::Hidden => {}
        SongSearchState::TextEntry(entry) => {
            let cx = screen_center_x();
            let cy = screen_center_y();
            let panel_w = SONG_SEARCH_TEXT_ENTRY_W.min(screen_width() * 0.9);
            let panel_h = SONG_SEARCH_TEXT_ENTRY_H;
            let cursor = if entry.blink_t < SONG_SEARCH_TEXT_ENTRY_CURSOR_PERIOD * 0.5 {
                "▮"
            } else {
                " "
            };
            let mut value = entry.query.clone();
            if value.chars().count() < SONG_SEARCH_PROMPT_MAX_LEN {
                value.push_str(cursor);
            }
            let query_text = format!("> {value}");

            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(cx, cy):
                zoomto(panel_w + 2.0, panel_h + 2.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1451)
            ));
            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(cx, cy):
                zoomto(panel_w, panel_h):
                diffuse(0.12, 0.12, 0.12, 1.0):
                z(1452)
            ));
            actors.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Header)):
                settext(SONG_SEARCH_PROMPT_TITLE):
                align(0.5, 0.5):
                xy(cx, cy - panel_h * 0.5 + 22.0):
                zoom(0.42):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1453):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(SONG_SEARCH_PROMPT_HINT):
                align(0.5, 0.5):
                xy(cx, cy - 28.0):
                zoom(0.78):
                maxwidth(panel_w - 40.0):
                diffuse(0.8, 0.8, 0.8, 1.0):
                z(1453):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(query_text):
                align(0.5, 0.5):
                xy(cx, cy + 30.0):
                zoom(1.05):
                maxwidth(panel_w - 36.0):
                diffuse(0.4, 1.0, 0.4, 1.0):
                z(1453):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(SONG_SEARCH_TEXT_ENTRY_FOOTER):
                align(0.5, 0.5):
                xy(cx, cy + panel_h * 0.5 - 16.0):
                zoom(0.78):
                diffuse(0.75, 0.75, 0.75, 1.0):
                z(1453):
                horizalign(center)
            ));
        }
        SongSearchState::Results(results) => {
            let pane_cx = screen_center_x();
            let pane_cy = screen_center_y() + 40.0;
            let list_base_y = pane_cy - SONG_SEARCH_PANE_H * 0.5 - SONG_SEARCH_TEXT_H * 2.5;
            let list_x = pane_cx - SONG_SEARCH_PANE_W * 0.25;
            let list_clip = [
                pane_cx - SONG_SEARCH_PANE_W * 0.5,
                pane_cy - SONG_SEARCH_PANE_H * 0.5,
                SONG_SEARCH_PANE_W * 0.5,
                SONG_SEARCH_PANE_H,
            ];
            let selected_color = color::simply_love_rgba(active_color_index);
            let total_items = song_search_total_items(results).max(1);
            let focus_t = (results.focus_anim_elapsed / SONG_SEARCH_FOCUS_TWEEN_SECONDS.max(1e-6))
                .clamp(0.0, 1.0);
            let scroll_dir = scroll_anim_dir(
                total_items,
                results.prev_selected_index,
                results.selected_index,
                results.last_move_dir,
            ) as f32;
            let scroll_shift = scroll_dir
                * [1.0 - focus_t, 0.0]
                    [(results.focus_anim_elapsed >= SONG_SEARCH_FOCUS_TWEEN_SECONDS) as usize];

            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(pane_cx, pane_cy):
                zoomto(SONG_SEARCH_PANE_W + SONG_SEARCH_PANE_BORDER, SONG_SEARCH_PANE_H + SONG_SEARCH_PANE_BORDER):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1451)
            ));
            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(pane_cx, pane_cy):
                zoomto(SONG_SEARCH_PANE_W, SONG_SEARCH_PANE_H):
                diffuse(0.0, 0.0, 0.0, 1.0):
                z(1452)
            ));
            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(pane_cx, pane_cy):
                zoomto(SONG_SEARCH_PANE_BORDER, SONG_SEARCH_PANE_H - 10.0):
                diffuse(0.2, 0.2, 0.2, 1.0):
                z(1453)
            ));
            actors.push(act!(text:
                font("miso"):
                settext("Search Results For:"):
                align(0.5, 0.5):
                xy(pane_cx, pane_cy - SONG_SEARCH_PANE_H * 0.5 - SONG_SEARCH_TEXT_H * 5.0):
                zoom(0.8):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1454):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(format!("\"{}\"", results.search_text)):
                align(0.5, 0.5):
                xy(pane_cx, pane_cy - SONG_SEARCH_PANE_H * 0.5 - SONG_SEARCH_TEXT_H * 3.0):
                zoom(0.8):
                maxwidth(SONG_SEARCH_PANE_W):
                diffuse(0.4, 1.0, 0.4, 1.0):
                z(1454):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(format!("{} Results Found", results.candidates.len())):
                align(0.5, 0.5):
                xy(pane_cx, pane_cy - SONG_SEARCH_PANE_H * 0.5 - SONG_SEARCH_TEXT_H):
                zoom(0.8):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1454):
                horizalign(center)
            ));

            for slot_idx in 0..SONG_SEARCH_WHEEL_SLOTS {
                let offset = slot_idx as isize - SONG_SEARCH_WHEEL_FOCUS_SLOT as isize;
                let row_idx = ((results.selected_index as isize + offset)
                    .rem_euclid(total_items as isize)) as usize;
                let slot_pos = offset as f32 + scroll_shift;
                let y = (slot_pos + SONG_SEARCH_WHEEL_FOCUS_SLOT as f32 + 1.0)
                    .mul_add(SONG_SEARCH_ROW_SPACING, list_base_y);
                let focused = slot_pos.abs() < 0.5;
                let mut text = "Exit".to_string();
                let mut base_rgb = [1.0, 0.2, 0.2];
                if row_idx < results.candidates.len() {
                    let song = &results.candidates[row_idx].song;
                    text = song.display_title(false).to_string();
                    base_rgb = [1.0, 1.0, 1.0];
                }
                let focus_tint = if focused {
                    [selected_color[0], selected_color[1], selected_color[2]]
                } else {
                    [0.533, 0.533, 0.533]
                };
                let mut color_rgba = [
                    base_rgb[0] * focus_tint[0],
                    base_rgb[1] * focus_tint[1],
                    base_rgb[2] * focus_tint[2],
                    1.0,
                ];
                let alpha =
                    [0.0, 1.0][(slot_idx > 0 && slot_idx + 1 < SONG_SEARCH_WHEEL_SLOTS) as usize];
                color_rgba[3] *= alpha;
                let mut row = act!(text:
                    font("miso"):
                    settext(text):
                    align(0.5, 0.5):
                    xy(list_x, y):
                    maxwidth(SONG_SEARCH_RESULT_TITLE_W):
                    zoom(1.0):
                    diffuse(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3]):
                    z(1454):
                    horizalign(center)
                );
                super::set_text_clip_rect(&mut row, list_clip);
                actors.push(row);
            }

            if let Some(candidate) = song_search_focused_candidate(results) {
                let details = [
                    ("Pack", candidate.pack_name.clone()),
                    ("Song", candidate.song.display_title(false).to_string()),
                    (
                        "Subtitle",
                        candidate.song.display_subtitle(false).to_string(),
                    ),
                    ("BPMs", candidate.song.formatted_chart_display_bpm(None)),
                    (
                        "Difficulties",
                        song_search_difficulties_text(candidate.song.as_ref(), results.chart_type),
                    ),
                ];
                for (i, (label, value)) in details.iter().enumerate() {
                    let zoom = 0.8;
                    // Matches Simply Love's visible cap from `zoom(0.8):maxwidth(width / zoom)`.
                    let row_i = i as f32;
                    let label_row = row_i * 2.0 + 1.0;
                    let value_row = row_i * 2.0 + 2.0;
                    let label_y = pane_cy - SONG_SEARCH_PANE_H * 0.5
                        + SONG_SEARCH_TEXT_H * zoom * label_row
                        + 8.0 * label_row;
                    let value_y = pane_cy - SONG_SEARCH_PANE_H * 0.5
                        + SONG_SEARCH_TEXT_H * zoom * value_row
                        + 8.0 * value_row;
                    actors.push(act!(text:
                        font("miso"):
                        settext(format!("{label}:")):
                        align(0.0, 0.5):
                        xy(pane_cx + 10.0, label_y):
                        maxwidth(SONG_SEARCH_DETAIL_LABEL_W / zoom):
                        zoom(zoom):
                        diffuse(0.67, 0.67, 1.0, 1.0):
                        z(1454):
                        horizalign(left)
                    ));
                    actors.push(act!(text:
                        font("miso"):
                        settext(value.clone()):
                        align(0.0, 0.5):
                        xy(pane_cx + 40.0, value_y):
                        maxwidth(SONG_SEARCH_DETAIL_VALUE_W / zoom):
                        zoom(zoom):
                        diffuse(1.0, 1.0, 1.0, 1.0):
                        z(1454):
                        horizalign(left)
                    ));
                }
            }
        }
    }

    Some(actors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::SongData;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn test_song(title: &str, subtitle: &str) -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from("test.sm"),
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: "128".to_string(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 128.0,
            max_bpm: 128.0,
            normalized_bpms: "128".to_string(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        })
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn detail_text_uses_itgmania_visible_widths_after_zoom() {
        let pack = "Pack Name Long Enough To Need Horizontal Compression";
        let title = "Song Title Long Enough To Need Horizontal Compression";
        let subtitle = "Subtitle Long Enough To Need Horizontal Compression";
        let state = SongSearchState::Results(SongSearchResultsState {
            search_text: "long".to_string(),
            chart_type: "dance-single",
            candidates: vec![SongSearchCandidate {
                pack_name: pack.to_string(),
                song: test_song(title, subtitle),
            }],
            selected_index: 0,
            prev_selected_index: 0,
            last_move_dir: 0,
            focus_anim_elapsed: SONG_SEARCH_FOCUS_TWEEN_SECONDS,
            input_lock: 0.0,
        });

        let actors = build_song_search_overlay(&state, 0, MachineFont::Wendy)
            .expect("song search should render");
        let detail_x = screen_center_x() + 40.0;
        let mut found = 0;

        for actor in &actors {
            let Actor::Text {
                content,
                offset,
                scale,
                max_width,
                max_w_pre_zoom,
                ..
            } = actor
            else {
                continue;
            };
            if ![pack, title, subtitle].contains(&content.as_str())
                || (offset[0] - detail_x).abs() > 1e-6
            {
                continue;
            }

            found += 1;
            assert_close(scale[0], 0.8);
            assert_close(
                max_width.expect("detail value should cap width"),
                115.0 / 0.8,
            );
            assert!(*max_w_pre_zoom);
        }

        assert_eq!(found, 3);
    }
}
