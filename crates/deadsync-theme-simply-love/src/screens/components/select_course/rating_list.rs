use crate::act;
use crate::fonts::machine_font_key;
use deadlib_present::actors::Actor;
use deadsync_config::theme::MachineFont;
use deadsync_theme::FontRole;
use std::sync::Arc;

const SCROLL_STEP_SECONDS: f32 = 0.5;
const SCROLL_END_PAUSE_SECONDS: f32 = 0.5;
const SCROLL_MIN_ENTRIES: usize = 6;

pub(crate) trait RatingListEntry {
    fn title(&self) -> &Arc<str>;
    fn difficulty(&self) -> &str;
    fn meter_text(&self) -> &Arc<str>;
}

pub(crate) trait RatingSelectorEntry {
    fn difficulty_name(&self) -> &str;
    fn meter_text(&self) -> &Arc<str>;
}

pub(crate) struct Params<'a, E, R> {
    pub layout: super::layout::CourseRatingListLayout,
    pub entries: Option<&'a [E]>,
    pub ratings: Option<&'a [Option<R>]>,
    pub selected_rating_index: usize,
    pub no_course_text: &'a Arc<str>,
    pub elapsed: f32,
    pub panel_color: [f32; 4],
    pub machine_font: MachineFont,
    pub selection_animation_beat: f32,
    pub global_offset_seconds: f32,
    pub is_p2_single: bool,
}

pub(crate) fn push<E, R, F>(actors: &mut Vec<Actor>, params: Params<'_, E, R>, difficulty_color: F)
where
    E: RatingListEntry,
    R: RatingSelectorEntry,
    F: Fn(&str) -> [f32; 4],
{
    let Params {
        layout,
        entries,
        ratings,
        selected_rating_index,
        no_course_text,
        elapsed,
        panel_color,
        machine_font,
        selection_animation_beat,
        global_offset_seconds,
        is_p2_single,
    } = params;

    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(
            layout.panel_left + layout.panel_width * 0.5,
            layout.panel_top + layout.panel_height * 0.5
        ):
        setsize(layout.panel_width, layout.panel_height):
        z(120):
        diffuse(panel_color[0], panel_color[1], panel_color[2], panel_color[3])
    ));

    if let Some(entries) = entries.filter(|entries| !entries.is_empty()) {
        let visible_rows = entries.len().clamp(1, layout.visible_rows);
        let (start_idx, frac) = tracklist_scroll(entries.len(), visible_rows, elapsed);
        let rows_to_draw = visible_rows + 2;
        let title_maxwidth = (layout.panel_width
            - (layout.list_title_x - layout.list_left_x)
            - layout.list_right_pad)
            .max(40.0 * layout.scale);
        for row in 0..rows_to_draw {
            let idx = start_idx + row;
            if idx >= entries.len() {
                break;
            }
            let entry = &entries[idx];
            let y = frac.mul_add(
                -layout.row_spacing,
                (row as f32).mul_add(layout.row_spacing, layout.list_start_y),
            );
            if y > layout.panel_bottom + layout.row_spacing {
                break;
            }
            let color = difficulty_color(entry.difficulty());
            let mut meter_actor = act!(text:
                font("miso"):
                settext(Arc::clone(entry.meter_text())):
                align(0.0, 0.0):
                xy(layout.list_left_x, y):
                zoomtoheight(layout.text_height):
                maxwidth(34.0 * layout.scale):
                z(121):
                diffuse(color[0], color[1], color[2], 1.0)
            );
            if let Actor::Text { clip, .. } = &mut meter_actor {
                *clip = Some(layout.list_clip);
            }
            actors.push(meter_actor);

            let mut title_actor = act!(text:
                font("miso"):
                settext(Arc::clone(entry.title())):
                align(0.0, 0.0):
                xy(layout.list_title_x, y):
                zoomtoheight(layout.text_height):
                maxwidth(title_maxwidth):
                z(121):
                diffuse(1.0, 1.0, 1.0, 1.0)
            );
            if let Actor::Text { clip, .. } = &mut title_actor {
                *clip = Some(layout.list_clip);
            }
            actors.push(title_actor);
        }
    } else {
        let mut no_course_actor = act!(text:
            font("miso"):
            settext(Arc::clone(no_course_text)):
            align(0.0, 0.0):
            xy(layout.list_left_x, layout.list_start_y):
            zoom(0.72 * layout.scale):
            maxwidth(layout.panel_width - 16.0 * layout.scale):
            z(121):
            diffuse(1.0, 1.0, 1.0, 1.0)
        );
        if let Actor::Text { clip, .. } = &mut no_course_actor {
            *clip = Some(layout.list_clip);
        }
        actors.push(no_course_actor);
    }

    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(layout.rating_box_center_x, layout.rating_box_center_y):
        setsize(layout.rating_box_width, layout.rating_box_height):
        z(120):
        diffuse(panel_color[0], panel_color[1], panel_color[2], panel_color[3])
    ));

    let rating_len = ratings.map_or(0, <[_]>::len);
    let rating_top_index = if rating_len > layout.rating_visible_slots {
        selected_rating_index
            .saturating_sub(layout.rating_visible_slots - 1)
            .min(rating_len - layout.rating_visible_slots)
    } else {
        0
    };
    if let Some(ratings) = ratings {
        for slot in 0..layout.rating_visible_slots {
            let y = layout.rating_slot_y(slot);
            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(layout.rating_box_center_x, y):
                setsize(layout.rating_slot_size, layout.rating_slot_size):
                z(121):
                diffuse(0.059, 0.059, 0.059, 1.0)
            ));
            let idx = rating_top_index + slot;
            let Some(rating) = ratings.get(idx).and_then(Option::as_ref) else {
                continue;
            };
            let color = difficulty_color(rating.difficulty_name());
            actors.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Header)):
                settext(Arc::clone(rating.meter_text())):
                align(0.5, 0.5):
                xy(layout.rating_box_center_x, y):
                zoom(layout.rating_meter_zoom):
                z(122):
                diffuse(color[0], color[1], color[2], 1.0)
            ));
        }
    }

    if rating_len > 0 {
        let selected_slot = (selected_rating_index.saturating_sub(rating_top_index))
            .min(layout.rating_visible_slots - 1);
        let arrow_y = layout.rating_arrow_y(selected_slot);
        let bounce = arrow_bounce01(selection_animation_beat, global_offset_seconds);
        let (arrow_x_offset, arrow_dx, arrow_rotation) = if is_p2_single {
            (
                layout.rating_arrow_p2_x_offset,
                bounce * layout.rating_arrow_bounce_distance,
                180.0,
            )
        } else {
            (
                layout.rating_arrow_p1_x_offset,
                -bounce * layout.rating_arrow_bounce_distance,
                0.0,
            )
        };
        actors.push(act!(sprite("meter_arrow.png"):
            align(0.0, 0.5):
            xy(
                layout.rating_box_center_x + arrow_x_offset + arrow_dx,
                arrow_y
            ):
            rotationz(arrow_rotation):
            zoom(layout.rating_arrow_zoom):
            z(122)
        ));
    }
}

#[inline(always)]
fn arrow_bounce01(selection_beat: f32, global_offset_seconds: f32) -> f32 {
    let effect_offset = -10.0 * global_offset_seconds;
    let t = (selection_beat + effect_offset).rem_euclid(1.0);
    (t * std::f32::consts::PI).sin().clamp(0.0, 1.0)
}

fn tracklist_scroll(entry_count: usize, visible_rows: usize, elapsed: f32) -> (usize, f32) {
    if entry_count == 0
        || visible_rows == 0
        || entry_count <= SCROLL_MIN_ENTRIES
        || entry_count <= visible_rows
    {
        return (0, 0.0);
    }
    let max_start = entry_count - visible_rows;
    let step = SCROLL_STEP_SECONDS.max(1e-3);
    let pause = SCROLL_END_PAUSE_SECONDS.max(0.0);
    let sweep = max_start as f32 * step;
    let cycle = pause + sweep + pause + sweep;
    if cycle <= f32::EPSILON {
        return (0, 0.0);
    }

    let mut t = elapsed.max(0.0).rem_euclid(cycle);
    let pos = if t < pause {
        0.0
    } else {
        t -= pause;
        if t < sweep {
            t / step
        } else {
            t -= sweep;
            if t < pause {
                max_start as f32
            } else {
                t -= pause;
                (max_start as f32 - t / step).max(0.0)
            }
        }
    }
    .clamp(0.0, max_start as f32);

    let start = pos.floor() as usize;
    (start, pos - start as f32)
}
