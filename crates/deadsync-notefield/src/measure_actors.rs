use deadlib_present::actors::{
    Actor, FlatDraw, FlatPreparedU32, FlatSprite, InlineU32Text, SpriteSource, TextAlign,
    TextContent,
};
use deadlib_present::dsl::TextBuilder;
use deadlib_render_core::BlendMode;

/// Song-prewarmed decimal slots retained for visible edit measure labels.
///
/// The game thread owns this single-threaded, per-composition cursor; the
/// prepared text layouts themselves live in the screen-lifetime presentation
/// cache and are warmed when Practice starts. Capacity matches the edit-mode
/// field reserve envelope: 72 entries per player, with no eviction or pruning
/// on a live frame. A miss, oversized value, or saturation takes the exact
/// Actor path without inserting into the prepared cache. Entries are destroyed
/// with that presentation cache at screen teardown. Unit tests cover slot
/// saturation. Per-frame cost is bounded by the existing visible-measure
/// traversal.
pub const EDIT_MEASURE_TEXT_SLOTS_PER_PLAYER: u8 = 72;

#[derive(Clone, Copy, Debug)]
pub(crate) struct EditMeasureTextSlots {
    next: u16,
    end: u16,
}

impl EditMeasureTextSlots {
    pub(crate) fn new(base: u8) -> Self {
        let next = u16::from(base);
        Self {
            next,
            end: next + u16::from(EDIT_MEASURE_TEXT_SLOTS_PER_PLAYER),
        }
    }

    fn take(&mut self) -> Option<u8> {
        let slot = (self.next < self.end).then(|| u8::try_from(self.next).ok())??;
        self.next += 1;
        Some(slot)
    }
}

pub(crate) fn append_edit_measure_number(
    actors: &mut Vec<Actor>,
    draws: &mut Vec<FlatDraw>,
    slots: &mut EditMeasureTextSlots,
    edit_beat_bars: bool,
    measure_index: Option<i64>,
    x: f32,
    y: f32,
    field_zoom: f32,
    z_measure_lines: i16,
    font: &'static str,
) {
    let Some(measure) = measure_index else {
        return;
    };
    if !edit_beat_bars || measure < 0 {
        return;
    }

    let zoom = (field_zoom * 0.9).clamp(0.35, 0.75);
    if let Ok(value) = u32::try_from(measure)
        && let Some(slot) = slots.take()
    {
        draws.push(FlatDraw::PreparedU32(FlatPreparedU32 {
            align: [1.0, 0.5],
            offset: [x, y],
            color: [1.0; 4],
            font,
            text: InlineU32Text::new(value),
            slot,
            align_text: TextAlign::Right,
            z: z_measure_lines.saturating_add(1),
            scale: [zoom; 2],
            blend: BlendMode::Alpha,
            shadow_len: [2.0, -2.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
        }));
        return;
    }

    let mut text = TextBuilder::new();
    text.font(font);
    text.settext(edit_measure_text(measure as u64));
    text.align(1.0, 0.5);
    text.horizalign(TextAlign::Right);
    text.xy(x, y);
    text.zoom(zoom);
    text.shadowlength(2.0);
    text.diffuse([1.0, 1.0, 1.0, 1.0]);
    text.z(z_measure_lines.saturating_add(1));
    actors.push(text.build(0));
}

fn edit_measure_text(measure: u64) -> TextContent {
    u32::try_from(measure)
        .map(TextContent::inline_u32)
        .unwrap_or_else(|_| measure.to_string().into())
}

pub(crate) fn append_beat_bar(
    draws: &mut Vec<FlatDraw>,
    edit_beat_bars: bool,
    edit_bar_frame: u32,
    x_center: f32,
    y: f32,
    width: f32,
    field_zoom: f32,
    thickness: f32,
    alpha: f32,
    z_measure_lines: i16,
) {
    if edit_beat_bars {
        append_edit_beat_bar(
            draws,
            edit_bar_frame,
            x_center,
            y,
            width,
            field_zoom,
            thickness,
            alpha,
            z_measure_lines,
        );
    } else {
        append_measure_quad(
            draws,
            [0.5, 0.5],
            [x_center, y],
            [width, thickness],
            [1.0, 1.0, 1.0, alpha],
            z_measure_lines,
        );
    }
}

/// Colored measure cue line marking timing events such as BPM changes, stops,
/// delays, and scrolls.
pub(crate) fn append_cue_bar(
    draws: &mut Vec<FlatDraw>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    color: [f32; 3],
    alpha: f32,
    z_measure_lines: i16,
) {
    append_measure_quad(
        draws,
        [0.5, 0.5],
        [x_center, y],
        [width, thickness],
        [color[0], color[1], color[2], alpha],
        z_measure_lines,
    );
}

fn append_edit_beat_bar(
    draws: &mut Vec<FlatDraw>,
    frame: u32,
    x_center: f32,
    y: f32,
    width: f32,
    field_zoom: f32,
    thickness: f32,
    alpha: f32,
    z_measure_lines: i16,
) {
    match frame {
        0 | 1 => {
            append_edit_bar_segment(draws, x_center, y, width, thickness, alpha, z_measure_lines);
        }
        2 => append_dashed_edit_bar(
            draws,
            x_center,
            y,
            width,
            thickness,
            12.0 * field_zoom,
            8.0 * field_zoom,
            alpha,
            z_measure_lines,
        ),
        _ => append_dashed_edit_bar(
            draws,
            x_center,
            y,
            width,
            thickness,
            4.0 * field_zoom,
            6.0 * field_zoom,
            alpha,
            z_measure_lines,
        ),
    }
}

fn append_edit_bar_segment(
    draws: &mut Vec<FlatDraw>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    alpha: f32,
    z_measure_lines: i16,
) {
    append_measure_quad(
        draws,
        [0.5, 0.5],
        [x_center, y],
        [width, thickness],
        [1.0, 1.0, 1.0, alpha],
        z_measure_lines,
    );
}

fn append_dashed_edit_bar(
    draws: &mut Vec<FlatDraw>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    dash: f32,
    gap: f32,
    alpha: f32,
    z_measure_lines: i16,
) {
    let dash = dash.max(1.0);
    let step = (dash + gap).max(dash + 1.0);
    let left = width.mul_add(-0.5, x_center);
    let right = width.mul_add(0.5, x_center);
    let mut x = left;
    while x < right {
        let seg_w = dash.min(right - x);
        append_measure_quad(
            draws,
            [0.0, 0.5],
            [x, y],
            [seg_w, thickness],
            [1.0, 1.0, 1.0, alpha],
            z_measure_lines,
        );
        x += step;
    }
}

fn append_measure_quad(
    draws: &mut Vec<FlatDraw>,
    align: [f32; 2],
    xy: [f32; 2],
    size: [f32; 2],
    diffuse: [f32; 4],
    z: i16,
) {
    draws.push(FlatDraw::Sprite(FlatSprite {
        center: [
            (0.5 - align[0]).mul_add(size[0], xy[0]),
            (0.5 - align[1]).mul_add(size[1], xy[1]),
        ],
        world_z: 0.0,
        size,
        source: SpriteSource::Solid,
        tint: diffuse,
        glow: [1.0, 1.0, 1.0, 0.0],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        flip_x: false,
        flip_y: false,
        fade: [0.0; 4],
        blend: BlendMode::Alpha,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        z,
    }));
}
