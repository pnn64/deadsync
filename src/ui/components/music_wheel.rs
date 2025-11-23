use crate::act;
use crate::core::space::*;
use crate::core::space::{widescale};
use crate::screens::select_music::MusicWheelEntry;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::game::scores;
use std::collections::HashMap;

// --- Colors ---
fn col_music_wheel_box() -> [f32; 4] { color::rgba_hex("#0a141b") }
fn col_pack_header_box() -> [f32; 4] { color::rgba_hex("#4c565d") }
fn col_selected_song_box() -> [f32; 4] { color::rgba_hex("#272f35") }
fn col_selected_pack_header_box() -> [f32; 4] { color::rgba_hex("#5f686e") }

// --- Layout Constants ---
const NUM_WHEEL_ITEMS: usize = 17;
const CENTER_WHEEL_SLOT_INDEX: usize = NUM_WHEEL_ITEMS / 2;
const SELECTION_ANIMATION_CYCLE_DURATION: f32 = 1.0;

// Helper from select_music.rs
fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t,
    ]
}

pub struct MusicWheelParams<'a> {
    pub entries: &'a [MusicWheelEntry],
    pub selected_index: usize,
    pub selection_animation_timer: f32,
    pub pack_song_counts: &'a HashMap<String, usize>,
    pub preferred_difficulty_index: usize,
    pub selected_difficulty_index: usize,    
}

pub fn build(p: MusicWheelParams) -> Vec<Actor> {
    let mut actors = Vec::new();

    const WHEEL_WIDTH_DIVISOR: f32 = 2.125;
    let num_visible_items = NUM_WHEEL_ITEMS - 2; // 17 -> 15 visible

    // SL metrics-derived values
    let sl_shift                 = widescale(28.0, 33.0);                 // InitCommand shift in SL
    let highlight_w: f32         = screen_width() / WHEEL_WIDTH_DIVISOR;  // _screen.w/2.125
    let highlight_left_world: f32= screen_center_x() + sl_shift;          // left edge of the column
    let half_highlight: f32      = 0.5 * highlight_w;

    // Local Xs (container is LEFT-anchored at highlight_left_world)
    // In SL, titles are WideScale(75,111) from wheel center (no +sl_shift); cancel the container shift here.
    let title_x_local: f32       = widescale(75.0, 111.0) - sl_shift;
    let title_max_w_local: f32   = widescale(245.0, 350.0);

    // Pack name: visually centered in the column
    let pack_center_x_local: f32 = half_highlight - sl_shift + widescale(9.0, 10.0);
    let pack_name_max_w: f32     = widescale(240.0, 310.0);

    // Pack count
    let pack_count_x_local: f32 = screen_width() / 2.0 - widescale(9.0, 10.0) - sl_shift;

    // --- VERTICAL GEOMETRY (1:1 with Simply Love Lua) ---
    let slot_spacing: f32        = screen_height() / (num_visible_items as f32);
    let item_h_full: f32         = slot_spacing;
    let item_h_colored: f32      = slot_spacing - 1.0;
    let center_y: f32            = screen_center_y();
    let line_gap_units: f32      = 6.0;
    let half_item_h: f32         = item_h_full * 0.5; // NEW: Pre-calculate half height for centering children

    // Selection pulse
    let anim_t_unscaled = (p.selection_animation_timer / SELECTION_ANIMATION_CYCLE_DURATION)
        * std::f32::consts::PI * 2.0;
    let anim_t = (anim_t_unscaled.sin() + 1.0) / 2.0;
    
    let num_entries = p.entries.len();

    if num_entries > 0 {
        for i_slot in 0..NUM_WHEEL_ITEMS {
            let offset_from_center = i_slot as isize - CENTER_WHEEL_SLOT_INDEX as isize;
            let y_center_item      = center_y + (offset_from_center as f32) * slot_spacing;
            let is_selected_slot   = i_slot == CENTER_WHEEL_SLOT_INDEX;

            // The selected_index from the state now freely increments/decrements. We use it as a base
            // and apply the modulo here for safe list access.
            let list_index = ((p.selected_index as isize + offset_from_center + num_entries as isize)
                as usize) % num_entries;

            let (is_pack, bg_col, txt_col, title_str, subtitle_str, pack_name_opt) =
                match p.entries.get(list_index) {
                    Some(MusicWheelEntry::Song(info)) => {
                        let base = col_music_wheel_box();
                        let sel  = col_selected_song_box();
                        let bg   = if is_selected_slot { lerp_color(base, sel, anim_t) } else { base };
                        (false, bg, [1.0, 1.0, 1.0, 1.0], info.title.clone(), info.subtitle.clone(), None)
                    }
                    Some(MusicWheelEntry::PackHeader { name, original_index, .. }) => {
                        let base = col_pack_header_box();
                        let sel  = col_selected_pack_header_box();
                        let bg   = if is_selected_slot { lerp_color(base, sel, anim_t) } else { base };
                        let c    = color::simply_love_rgba(*original_index as i32);
                        (true, bg, [c[0], c[1], c[2], 1.0], name.clone(), String::new(), Some(name.clone()))
                    }
                    _ => (false, col_music_wheel_box(), [1.0; 4], String::new(), String::new(), None),
                };

            let has_subtitle = !subtitle_str.trim().is_empty();

            // Children local to container-left (highlight_left_world)
            let mut slot_children: Vec<Actor> = Vec::new();

            // Base black quad (full height)
            if is_pack {
                // Base black quad (full height) — only for packs
                slot_children.push(act!(quad:
                    align(0.0, 0.5):
                    xy(0.0, half_item_h):
                    zoomto(highlight_w, item_h_full):
                    diffuse(0.0, 0.0, 0.0, 1.0):
                    z(0)
                ));
            }
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
                        && *count > 0 {
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

                // --- Grade Sprite (Now with real data) ---
                let mut grade_actor = {
                    let grade_x = widescale(10.0, 17.0); // widescale(38.0, 50.0) - sl_shift
                    let grade_y = half_item_h;
                    let grade_zoom = widescale(0.18, 0.3);
                    
                    act!(sprite("grades/grades 1x19.png"):
                        align(0.5, 0.5): xy(grade_x, grade_y): zoom(grade_zoom): z(2): visible(false)
                    )
                };

                // Find the relevant chart to check for a grade
                if let Some(MusicWheelEntry::Song(info)) = p.entries.get(list_index) {
                    // For the selected item, use the *actual* selected difficulty.
                    // For all other items, use the player's *preferred* difficulty.
                    let difficulty_index_to_check = if is_selected_slot {
                        p.selected_difficulty_index
                    } else {
                        p.preferred_difficulty_index
                    };
                    
                    let difficulty_name = crate::ui::color::FILE_DIFFICULTY_NAMES[difficulty_index_to_check];

                    if let Some(chart) = info.charts.iter().find(|c| c.difficulty.eq_ignore_ascii_case(difficulty_name))
                        && let Some(cached_score) = scores::get_cached_score(&chart.short_hash)
                            && let Actor::Sprite { visible, cell, .. } = &mut grade_actor {
                                *visible = true;
                                *cell = Some((cached_score.grade.to_sprite_state(), u32::MAX));
                            }
                }

                slot_children.push(grade_actor);
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
        
        for i_slot in 0..NUM_WHEEL_ITEMS {
            let offset_from_center = i_slot as isize - CENTER_WHEEL_SLOT_INDEX as isize;
            let y_center_item      = center_y + (offset_from_center as f32) * slot_spacing;
            let is_selected_slot   = i_slot == CENTER_WHEEL_SLOT_INDEX;
            
            // Use pack header colors for the empty state
            let base = col_pack_header_box();
            let sel  = col_selected_pack_header_box();
            let bg_col   = if is_selected_slot { lerp_color(base, sel, anim_t) } else { base };

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
    
    actors
}
