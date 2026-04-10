use crate::act;
use crate::engine::present::actors::Actor;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};

use super::{Action, Item, scroll_dir, set_text_clip_rect};

const WIDTH: f32 = 210.0;
const HEIGHT: f32 = 160.0;
const HEADER_Y_OFFSET: f32 = -92.0;
const ITEM_SPACING: f32 = 36.0;
const ITEM_TOP_Y_OFFSET: f32 = -15.0;
const ITEM_BOTTOM_Y_OFFSET: f32 = 10.0;
const TOP_TEXT_BASE_ZOOM: f32 = 1.15;
const BOTTOM_TEXT_BASE_ZOOM: f32 = 0.85;
const UNFOCUSED_ROW_ZOOM: f32 = 0.5;
const FOCUSED_ROW_ZOOM: f32 = 0.6;
const DIM_ALPHA: f32 = 0.8;
const HINT_Y_OFFSET: f32 = 100.0;
const HINT_TEXT: &str = "PRESS &SELECT; TO CANCEL";
const WHEEL_SLOTS: usize = 7;
const WHEEL_FOCUS_SLOT: usize = WHEEL_SLOTS / 2;
const VISIBLE_ROWS: usize = WHEEL_SLOTS - 2;
const FONT_TOP: &str = "miso";
const FONT_BOTTOM: &str = "wendy";

pub const FOCUS_TWEEN_SECONDS: f32 = 0.15;

const SORTS_INACTIVE_COLOR: [f32; 4] = crate::engine::present::color::rgba_hex("#005D7F");
const SORTS_ACTIVE_COLOR: [f32; 4] = crate::engine::present::color::rgba_hex("#0030A8");

pub struct RenderParams<'a> {
    pub items: &'a [Item],
    pub selected_index: usize,
    pub prev_selected_index: usize,
    pub focus_anim_elapsed: f32,
    pub selected_color: [f32; 4],
}

pub fn build_overlay(p: RenderParams<'_>) -> Vec<Actor> {
    let mut actors = Vec::new();
    let cx = screen_center_x();
    let cy = screen_center_y();
    let clip_rect = [cx - WIDTH * 0.5, cy - HEIGHT * 0.5, WIDTH, HEIGHT];
    let selected_index = p.selected_index.min(p.items.len().saturating_sub(1));

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, DIM_ALPHA):
        z(1450)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy + HEADER_Y_OFFSET):
        zoomto(WIDTH + 2.0, 22.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1451)
    ));
    actors.push(act!(text:
        font(FONT_BOTTOM):
        settext("OPTIONS"):
        align(0.5, 0.5):
        xy(cx, cy + HEADER_Y_OFFSET):
        zoom(0.4):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1452):
        horizalign(center)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(WIDTH + 2.0, HEIGHT + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1451)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cx, cy):
        zoomto(WIDTH, HEIGHT):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1452)
    ));

    if !p.items.is_empty() {
        let focus_t = (p.focus_anim_elapsed / FOCUS_TWEEN_SECONDS.max(1e-6)).clamp(0.0, 1.0);
        let scroll_dir = scroll_dir(
            p.items.len(),
            p.prev_selected_index.min(p.items.len() - 1),
            selected_index,
        ) as f32;
        let scroll_shift = scroll_dir
            * [1.0 - focus_t, 0.0][(p.focus_anim_elapsed >= FOCUS_TWEEN_SECONDS) as usize];
        let selected_rgba = [
            p.selected_color[0],
            p.selected_color[1],
            p.selected_color[2],
            1.0,
        ];
        let mut draw_row = |item_idx: usize, slot_pos: f32| {
            let focus_lerp = (1.0 - slot_pos.abs()).clamp(0.0, 1.0);
            let row_zoom =
                (FOCUSED_ROW_ZOOM - UNFOCUSED_ROW_ZOOM).mul_add(focus_lerp, UNFOCUSED_ROW_ZOOM);
            let row_alpha = (3.0 - slot_pos.abs()).clamp(0.0, 1.0);
            let row_tint = [
                (selected_rgba[0] - 0.533).mul_add(focus_lerp, 0.533),
                (selected_rgba[1] - 0.533).mul_add(focus_lerp, 0.533),
                (selected_rgba[2] - 0.533).mul_add(focus_lerp, 0.533),
            ];
            let top_color = [row_tint[0], row_tint[1], row_tint[2], row_alpha];
            let y = slot_pos.mul_add(ITEM_SPACING, cy);
            let item = &p.items[item_idx];
            let bottom_color = match item.action {
                Action::OpenSorts => [
                    (SORTS_ACTIVE_COLOR[0] - SORTS_INACTIVE_COLOR[0])
                        .mul_add(focus_lerp, SORTS_INACTIVE_COLOR[0]),
                    (SORTS_ACTIVE_COLOR[1] - SORTS_INACTIVE_COLOR[1])
                        .mul_add(focus_lerp, SORTS_INACTIVE_COLOR[1]),
                    (SORTS_ACTIVE_COLOR[2] - SORTS_INACTIVE_COLOR[2])
                        .mul_add(focus_lerp, SORTS_INACTIVE_COLOR[2]),
                    row_alpha,
                ],
                Action::BackToMain => [row_tint[0], 0.0, 0.0, row_alpha],
                _ => [row_tint[0], row_tint[1], row_tint[2], row_alpha],
            };

            let mut top = act!(text:
                font(FONT_TOP):
                settext(item.top_label):
                align(0.5, 0.5):
                xy(cx, y + ITEM_TOP_Y_OFFSET * row_zoom):
                zoom(TOP_TEXT_BASE_ZOOM * row_zoom):
                diffuse(top_color[0], top_color[1], top_color[2], top_color[3]):
                z(1454):
                horizalign(center)
            );
            set_text_clip_rect(&mut top, clip_rect);
            actors.push(top);

            let mut bottom = act!(text:
                font(FONT_BOTTOM):
                settext(item.bottom_label):
                align(0.5, 0.5):
                xy(cx, y + ITEM_BOTTOM_Y_OFFSET * row_zoom):
                maxwidth(405.0):
                zoom(BOTTOM_TEXT_BASE_ZOOM * row_zoom):
                diffuse(
                    bottom_color[0],
                    bottom_color[1],
                    bottom_color[2],
                    bottom_color[3]
                ):
                z(1454):
                horizalign(center)
            );
            set_text_clip_rect(&mut bottom, clip_rect);
            actors.push(bottom);
        };

        if p.items.len() <= VISIBLE_ROWS {
            let span = p.items.len();
            let first_offset = -((span as isize).saturating_sub(1) / 2);
            for i in 0..span {
                let offset = first_offset + i as isize;
                let item_idx = ((selected_index as isize + offset)
                    .rem_euclid(p.items.len() as isize)) as usize;
                let slot_pos = offset as f32 + scroll_shift;
                draw_row(item_idx, slot_pos);
            }
        } else {
            for slot_idx in 0..WHEEL_SLOTS {
                let offset = slot_idx as isize - WHEEL_FOCUS_SLOT as isize;
                let item_idx = ((selected_index as isize + offset)
                    .rem_euclid(p.items.len() as isize)) as usize;
                let slot_pos = offset as f32 + scroll_shift;
                draw_row(item_idx, slot_pos);
            }
        }
    }

    actors.push(act!(text:
        font(FONT_BOTTOM):
        settext(HINT_TEXT):
        align(0.5, 0.5):
        xy(cx, cy + HINT_Y_OFFSET):
        zoom(0.26):
        diffuse(0.7, 0.7, 0.7, 1.0):
        z(1454):
        horizalign(center)
    ));
    actors
}
