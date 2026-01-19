use crate::act;
use crate::core::space::widescale;
use crate::core::space::*;
use crate::game::scores;
use crate::screens::select_music::MusicWheelEntry;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use std::collections::HashMap;

// --- Colors ---
fn col_music_wheel_box() -> [f32; 4] {
    color::rgba_hex("#0a141b")
}
fn col_pack_header_box() -> [f32; 4] {
    color::rgba_hex("#4c565d")
}

// --- Layout Constants ---
// Simply Love theme metric: [MusicWheel] NumWheelItems=17.
// StepMania/ITGmania WheelBase allocates `ceil(NumWheelItems+2)` internal items so that
// extra off-screen items can slide in during scroll and avoid exposing gaps.
const NUM_WHEEL_ITEMS_TO_DRAW: usize = 17;
const NUM_VISIBLE_WHEEL_ITEMS: usize = NUM_WHEEL_ITEMS_TO_DRAW - 2; // 17 -> 15 visible on-screen
const NUM_WHEEL_SLOTS: usize = NUM_WHEEL_ITEMS_TO_DRAW + 2; // 17 -> 19 internal
const CENTER_WHEEL_SLOT_INDEX: usize = NUM_WHEEL_SLOTS / 2;
const WHEEL_DRAW_RADIUS: f32 = (NUM_WHEEL_ITEMS_TO_DRAW as f32) * 0.5; // 8.5
const SELECTION_ANIMATION_CYCLE_DURATION: f32 = 1.0;
const LAMP_PULSE_PERIOD: f32 = 0.8;
const LAMP_PULSE_LERP_TO_WHITE: f32 = 0.70;

fn col_quint_lamp() -> [f32; 4] {
    // zmod quint color: color("1,0.2,0.406,1")
    [1.0, 0.2, 0.406, 1.0]
}
fn col_clear_lamp() -> [f32; 4] {
    // zmod clear lamp
    color::rgba_hex("#0000CC")
}
fn col_fail_lamp() -> [f32; 4] {
    // zmod fail lamp
    color::rgba_hex("#990000")
}

fn lamp_judge_count_color(lamp_index: u8) -> [f32; 4] {
    // zmod uses SL.JudgmentColors["FA+"][lamp+1] for the single-digit overlay.
    match lamp_index {
        1 => color::rgba_hex(color::JUDGMENT_FA_PLUS_WHITE_HEX),
        2 => color::rgba_hex(color::JUDGMENT_HEX[1]),
        3 => color::rgba_hex(color::JUDGMENT_HEX[2]),
        4 => color::rgba_hex(color::JUDGMENT_HEX[3]),
        _ => [1.0; 4],
    }
}

// Helper from select_music.rs
fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

pub struct MusicWheelParams<'a> {
    pub entries: &'a [MusicWheelEntry],
    pub selected_index: usize,
    pub position_offset_from_selection: f32,
    pub selection_animation_timer: f32,
    pub pack_song_counts: &'a HashMap<String, usize>,
    pub preferred_difficulty_index: usize,
    pub selected_difficulty_index: usize,
}

pub fn build(p: MusicWheelParams) -> Vec<Actor> {
    let mut actors = Vec::new();
    let translated_titles = crate::config::get().translated_titles;
    let target_chart_type = crate::game::profile::get_session_play_style().chart_type();

    const WHEEL_WIDTH_DIVISOR: f32 = 2.125;
    let num_visible_items = NUM_VISIBLE_WHEEL_ITEMS;

    // SL metrics-derived values
    let sl_shift = widescale(28.0, 33.0); // InitCommand shift in SL
    let highlight_w: f32 = screen_width() / WHEEL_WIDTH_DIVISOR; // _screen.w/2.125
    let highlight_left_world: f32 = screen_center_x() + sl_shift; // left edge of the column
    let half_highlight: f32 = 0.5 * highlight_w;

    // Local Xs (container is LEFT-anchored at highlight_left_world)
    // In SL, titles are WideScale(75,111) from wheel center (no +sl_shift); cancel the container shift here.
    let title_x_local: f32 = widescale(75.0, 111.0) - sl_shift;
    let title_max_w_local: f32 = widescale(245.0, 350.0);

    // Pack name: visually centered in the column
    let pack_center_x_local: f32 = half_highlight - sl_shift + widescale(9.0, 10.0);
    let pack_name_max_w: f32 = widescale(240.0, 310.0);

    // Pack count
    let pack_count_x_local: f32 = screen_width() / 2.0 - widescale(9.0, 10.0) - sl_shift;

    // "Has Edit" icon (Simply Love: Graphics/MusicWheelItem Song NormalPart/default.lua)
    let has_edit_right_x_local: f32 = screen_width() / widescale(2.15, 2.14) - 8.0;

    // --- VERTICAL GEOMETRY (1:1 with Simply Love Lua) ---
    let slot_spacing: f32 = screen_height() / (num_visible_items as f32);
    let item_h_full: f32 = slot_spacing;
    let item_h_colored: f32 = slot_spacing - 1.0;
    let center_y: f32 = screen_center_y();
    let line_gap_units: f32 = 6.0;
    let half_item_h: f32 = item_h_full * 0.5; // NEW: Pre-calculate half height for centering children

    // Selection pulse
    let anim_t_unscaled = (p.selection_animation_timer / SELECTION_ANIMATION_CYCLE_DURATION)
        * std::f32::consts::PI
        * 2.0;
    let anim_t = (anim_t_unscaled.sin() + 1.0) / 2.0;

    let lamp_pulse_t_unscaled = (p.selection_animation_timer / LAMP_PULSE_PERIOD)
        * std::f32::consts::PI
        * 2.0;
    let lamp_pulse_t = (lamp_pulse_t_unscaled.sin() + 1.0) / 2.0;

    let num_entries = p.entries.len();

    if num_entries > 0 {
        for i_slot in 0..NUM_WHEEL_SLOTS {
            let offset_from_center = i_slot as isize - CENTER_WHEEL_SLOT_INDEX as isize;
            let offset_from_center_f =
                offset_from_center as f32 + p.position_offset_from_selection;
            if offset_from_center_f.abs() > WHEEL_DRAW_RADIUS {
                continue;
            }
            let y_center_item = center_y + offset_from_center_f * slot_spacing;
            let is_selected_slot = i_slot == CENTER_WHEEL_SLOT_INDEX;

            // The selected_index from the state now freely increments/decrements. We use it as a base
            // and apply the modulo here for safe list access.
            let list_index =
                ((p.selected_index as isize + offset_from_center + num_entries as isize) as usize)
                    % num_entries;

            let (is_pack, bg_col, txt_col, title_str, subtitle_str, pack_name_opt, has_edit) =
                match p.entries.get(list_index) {
                    Some(MusicWheelEntry::Song(info)) => {
                        let has_edit = info.charts.iter().any(|c| {
                            c.chart_type.eq_ignore_ascii_case(target_chart_type)
                                && c.difficulty.eq_ignore_ascii_case("edit")
                                && !c.notes.is_empty()
                        });
                        (
                            false,
                            col_music_wheel_box(),
                            [1.0, 1.0, 1.0, 1.0],
                            info.display_title(translated_titles).to_string(),
                            info.display_subtitle(translated_titles).to_string(),
                            None,
                            has_edit,
                        )
                    }
                    Some(MusicWheelEntry::PackHeader {
                        name,
                        original_index,
                        ..
                    }) => {
                        let c = color::simply_love_rgba(*original_index as i32);
                        (
                            true,
                            col_pack_header_box(),
                            [c[0], c[1], c[2], 1.0],
                            name.clone(),
                            String::new(),
                            Some(name.clone()),
                            false,
                        )
                    }
                    _ => (
                        false,
                        col_music_wheel_box(),
                        [1.0; 4],
                        String::new(),
                        String::new(),
                        None,
                        false,
                    ),
                };

            let has_subtitle = !subtitle_str.trim().is_empty();

            // Children local to container-left (highlight_left_world)
            let mut slot_children: Vec<Actor> = Vec::new();

            // Base quad (full height) for the 1px gap effect.
            // Simply Love uses a solid black base for pack headers, and a dark translucent base for songs.
            let base_full_col = if is_pack {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [0.0, 10.0 / 255.0, 17.0 / 255.0, 0.5]
            };
            slot_children.push(act!(quad:
                align(0.0, 0.5):
                xy(0.0, half_item_h):
                zoomto(highlight_w, item_h_full):
                diffuse(base_full_col[0], base_full_col[1], base_full_col[2], base_full_col[3]):
                z(0)
            ));
            // Colored quad (height - 1)
            slot_children.push(act!(quad:
                align(0.0, 0.5):
                xy(0.0, half_item_h):
                zoomto(highlight_w, item_h_colored):
                diffuse(bg_col[0], bg_col[1], bg_col[2], bg_col[3]):
                z(1)
            ));

            if is_pack {
                // PACK name — centered with slight right bias
                slot_children.push(act!(text:
                    font("miso"):
                    settext(title_str.clone()):
                    align(0.5, 0.5):
                    xy(pack_center_x_local, half_item_h): // FIX: Center vertically
                    maxwidth(pack_name_max_w):
                    zoom(1.0):
                    diffuse(txt_col[0], txt_col[1], txt_col[2], txt_col[3]):
                    z(2)
                ));

                // PACK count — right-aligned, inset from edge
                if let Some(pack_name) = pack_name_opt
                    && let Some(count) = p.pack_song_counts.get(&pack_name)
                    && *count > 0
                {
                    slot_children.push(act!(text:
                        font("miso"):
                        settext(format!("{}", count)):
                        align(1.0, 0.5):
                        xy(pack_count_x_local, half_item_h): // FIX: Center vertically
                        zoom(0.75):
                        horizalign(right):
                        diffuse(1.0, 1.0, 1.0, 1.0):
                        z(2)
                    ));
                }
            } else {
                // SONG title/subtitle — subtract sl_shift to avoid double offset
                let subtitle_y_offset = if has_subtitle { -line_gap_units } else { 0.0 };
                slot_children.push(act!(text:
                    font("miso"):
                    settext(title_str.clone()):
                    align(0.0, 0.5):
                    xy(title_x_local, half_item_h + subtitle_y_offset): // FIX: Center vertically
                    maxwidth(title_max_w_local):
                    zoom(0.85):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(2)
                ));
                if has_subtitle {
                    slot_children.push(act!(text:
                        font("miso"):
                        settext(subtitle_str.clone()):
                        align(0.0, 0.5):
                        xy(title_x_local, half_item_h + line_gap_units): // FIX: Center vertically
                        maxwidth(title_max_w_local):
                        zoom(0.7):
                        diffuse(1.0, 1.0, 1.0, 1.0):
                        z(2)
                    ));
                }

                if has_edit {
                    slot_children.push(act!(sprite("has_edit.png"):
                        align(1.0, 0.5):
                        xy(has_edit_right_x_local, half_item_h):
                        // Simply Love uses `Has Edit (doubleres).png` at `zoom(0.375)`;
                        // our asset is untagged, so scale down by 0.5 for parity.
                        zoom(0.1875):
                        z(2)
                    ));
                }

                // --- Grade Sprite + Lamp (using cached scores) ---
                let grade_x = widescale(10.0, 17.0); // widescale(38.0, 50.0) - sl_shift
                let grade_y = half_item_h;
                let grade_zoom = widescale(0.18, 0.3);

                let mut grade_actor = act!(sprite("grades/grades 1x19.png"):
                    align(0.5, 0.5):
                    xy(grade_x, grade_y):
                    zoom(grade_zoom):
                    z(2):
                    visible(false)
                );

                // Optional lamp quad, positioned to the left of the grade sprite.
                let mut lamp_actor: Option<Actor> = None;
                let mut judge_actor: Option<Actor> = None;

                // Find the relevant chart to check for a grade (and lamp).
                if let Some(MusicWheelEntry::Song(info)) = p.entries.get(list_index) {
                    // For the selected item, use the *actual* selected difficulty.
                    // For all other items, use the player's *preferred* difficulty.
                    let difficulty_index_to_check = if is_selected_slot {
                        p.selected_difficulty_index
                    } else {
                        p.preferred_difficulty_index
                    };

                    let difficulty_name =
                        crate::ui::color::FILE_DIFFICULTY_NAMES[difficulty_index_to_check];

                    if let Some(chart) = info
                        .charts
                        .iter()
                        .find(|c| c.difficulty.eq_ignore_ascii_case(difficulty_name))
                    {
                        if let Some(cached_score) = scores::get_cached_score(&chart.short_hash) {
                            let has_score = cached_score.grade != scores::Grade::Failed
                                || cached_score.score_percent > 0.0;
                            if has_score {
                                if let Actor::Sprite { visible, cell, .. } = &mut grade_actor {
                                    *visible = true;
                                    *cell = Some((cached_score.grade.to_sprite_state(), u32::MAX));
                                }

                                // Position and size mirror Simply Love/zmod's lamp quad.
                                let lamp_x = grade_x - widescale(13.0, 20.0);
                                let lamp_w = widescale(5.0, 6.0);
                                let lamp_h = 31.0;

                                // zmod: show a clear/fail lamp if no StageAward-like lamp exists.
                                // In deadsync today, that means:
                                // - `lamp_index=Some(..)` => FC lamp tier (pulse)
                                // - `lamp_index=None`     => clear lamp (solid) for any non-FC score
                                // - `grade=Failed`        => fail lamp (solid) if a real fail score exists
                                let (lamp_color, lamp_pulsing, lamp_index) =
                                    match cached_score.lamp_index {
                                        Some(0) => (col_quint_lamp(), true, Some(0u8)),
                                        Some(idx @ 1..=4) => {
                                            let color_index = (idx - 1) as usize;
                                            let base = color::rgba_hex(
                                                color::JUDGMENT_HEX[color_index.min(5)],
                                            );
                                            (base, true, Some(idx))
                                        }
                                        Some(_) => (col_clear_lamp(), false, None),
                                        None if cached_score.grade == scores::Grade::Failed => {
                                            (col_fail_lamp(), false, None)
                                        }
                                        None => (col_clear_lamp(), false, None),
                                    };

                                let lamp_color_final = if lamp_pulsing {
                                    let lamp_color2 = lerp_color(
                                        [1.0; 4],
                                        lamp_color,
                                        LAMP_PULSE_LERP_TO_WHITE,
                                    );
                                    lerp_color(lamp_color, lamp_color2, lamp_pulse_t)
                                } else {
                                    lamp_color
                                };

                                lamp_actor = Some(act!(quad:
                                    align(0.5, 0.5):
                                    xy(lamp_x, grade_y):
                                    zoomto(lamp_w, lamp_h):
                                    diffuse(lamp_color_final[0], lamp_color_final[1], lamp_color_final[2], lamp_color_final[3]):
                                    z(2)
                                ));

                                if let Some(lamp_index) = lamp_index
                                    && let Some(count) = cached_score.lamp_judge_count
                                    && count < 10
                                {
                                    let judge_x = grade_x - widescale(7.0, 13.0);
                                    let judge_y = grade_y + 10.0;
                                    let judge_col = lamp_judge_count_color(lamp_index);
                                    judge_actor = Some(act!(text:
                                        font("wendy_screenevaluation"):
                                        settext(format!("{}", count)):
                                        align(0.5, 0.5):
                                        horizalign(center):
                                        xy(judge_x, judge_y):
                                        zoom(0.15):
                                        diffuse(judge_col[0], judge_col[1], judge_col[2], judge_col[3]):
                                        z(10)
                                    ));
                                }
                            }
                        }
                    }
                }

                slot_children.push(grade_actor);
                if let Some(lamp) = lamp_actor {
                    slot_children.push(lamp);
                }
                if let Some(judge) = judge_actor {
                    slot_children.push(judge);
                }
            }

            // Container: left-anchored at SL highlight-left
            actors.push(Actor::Frame {
                align: [0.0, 0.5], // left-center
                offset: [highlight_left_world, y_center_item],
                size: [SizeSpec::Px(highlight_w), SizeSpec::Px(item_h_full)],
                background: None,
                z: 51,
                children: slot_children,
            });
        }
    } else {
        // Handle the case where there are no songs or packs loaded.
        let empty_text = "- EMPTY -";
        let text_color = color::decorative_rgba(0); // Red

        for i_slot in 0..NUM_WHEEL_SLOTS {
            let offset_from_center = i_slot as isize - CENTER_WHEEL_SLOT_INDEX as isize;
            let offset_from_center_f =
                offset_from_center as f32 + p.position_offset_from_selection;
            if offset_from_center_f.abs() > WHEEL_DRAW_RADIUS {
                continue;
            }
            let y_center_item = center_y + offset_from_center_f * slot_spacing;

            // Use pack header colors for the empty state
            let bg_col = col_pack_header_box();

            let mut slot_children: Vec<Actor> = Vec::new();

            // Add black background for 1px gap effect, just like real pack headers
            slot_children.push(act!(quad:
                align(0.0, 0.5):
                xy(0.0, half_item_h):
                zoomto(highlight_w, item_h_full):
                diffuse(0.0, 0.0, 0.0, 1.0):
                z(0)
            ));

            // Colored (gray) quad background for the slot
            slot_children.push(act!(quad:
                align(0.0, 0.5):
                xy(0.0, half_item_h):
                zoomto(highlight_w, item_h_colored):
                diffuse(bg_col[0], bg_col[1], bg_col[2], bg_col[3]):
                z(1)
            ));

            // "- EMPTY -" text, centered like a pack header
            slot_children.push(act!(text:
                font("miso"):
                settext(empty_text):
                align(0.5, 0.5):
                xy(pack_center_x_local, half_item_h):
                maxwidth(pack_name_max_w):
                zoom(1.0):
                diffuse(text_color[0], text_color[1], text_color[2], text_color[3]):
                z(2)
            ));

            // Container frame for the slot
            actors.push(Actor::Frame {
                align: [0.0, 0.5], // left-center
                offset: [highlight_left_world, y_center_item],
                size: [SizeSpec::Px(highlight_w), SizeSpec::Px(item_h_full)],
                background: None,
                z: 51,
                children: slot_children,
            });
        }
    }

    // Selection highlight overlay (Simply Love: Graphics/MusicWheel highlight.lua + [MusicWheel] HighlightOnCommand)
    let highlight_c1: [f32; 4] = [0.8, 0.8, 0.8, 0.15];
    let highlight_c2: [f32; 4] = [0.8, 0.8, 0.8, 0.05];
    let highlight_col = lerp_color(highlight_c1, highlight_c2, anim_t);
    actors.push(act!(quad:
        align(0.0, 0.5):
        xy(highlight_left_world, center_y):
        zoomto(highlight_w, item_h_colored):
        diffuse(highlight_col[0], highlight_col[1], highlight_col[2], highlight_col[3]):
        z(62)
    ));

    actors
}
