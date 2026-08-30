use crate::act;
use crate::assets::{self, AssetManager};
use crate::screens::evaluation::{ColumnJudgments, ScoreInfo};
use deadlib_present::actors::{
    Actor, InlineU32Text, SharedActorFrameScratch, SizeSpec, TextContent,
};
use deadlib_present::color;
use deadlib_present::color::{JudgmentColorRole as Role, JudgmentPalette};
use deadlib_present::font;
use deadlib_present::space::screen_center_y;
use deadlib_render_core::{BlendMode, SamplerDesc};
use deadsync_assets::noteskin::SpriteSlot;
use deadsync_notefield::noteskin_model_actor;
use deadsync_noteskin::{NUM_QUANTIZATIONS, Quantization};
use deadsync_profile as profile_data;
use image::{Rgba, RgbaImage};
use std::cell::RefCell;
use std::hash::Hasher;
use std::sync::Arc;
use twox_hash::XxHash64;

use super::utils::{arrow_breakdown_rgba, pane3_origin_x};

const DISABLED_WINDOW_RGBA: [f32; 4] = color::JUDGMENT_FA_PLUS_WHITE_EVAL_DIM_RGBA;
const PANE3_SINGLE_WIDTH: f32 = 230.0;
const PANE3_DOUBLE_WIDTH: f32 = 520.0;

#[derive(Clone, Copy)]
struct ColumnPaneInputs<'a> {
    columns: &'a [ColumnJudgments],
    noteskin: Option<&'a deadsync_assets::noteskin::Noteskin>,
    show_fa_plus_rows: bool,
    track_early_judgments: bool,
    disabled_timing_windows: [bool; 5],
}

impl<'a> ColumnPaneInputs<'a> {
    fn from_score(score_info: &'a ScoreInfo) -> Self {
        Self {
            columns: &score_info.column_judgments,
            noteskin: score_info.noteskin.as_deref(),
            show_fa_plus_rows: score_info.show_fa_plus_window && score_info.show_fa_plus_pane,
            track_early_judgments: score_info.track_early_judgments,
            disabled_timing_windows: score_info.disabled_timing_windows,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ColumnPanePresentation {
    scratch: RefCell<SharedActorFrameScratch>,
}

impl ColumnPanePresentation {
    pub(crate) fn new(score_info: &ScoreInfo) -> Self {
        Self::from_inputs(ColumnPaneInputs::from_score(score_info))
    }

    fn from_inputs(inputs: ColumnPaneInputs<'_>) -> Self {
        Self {
            scratch: RefCell::new(SharedActorFrameScratch::with_capacity(
                column_pane_actor_capacity(inputs),
            )),
        }
    }
}

impl Clone for ColumnPanePresentation {
    fn clone(&self) -> Self {
        Self {
            scratch: RefCell::new(SharedActorFrameScratch::with_capacity(
                self.scratch.borrow().capacity(),
            )),
        }
    }
}

#[inline(always)]
const fn pane3_width(num_cols: usize) -> f32 {
    if matches!(num_cols, 8 | 10) {
        PANE3_DOUBLE_WIDTH
    } else {
        PANE3_SINGLE_WIDTH
    }
}

const PANE3_SOLID_ARROW_MASK_KEY_PREFIX: &str = "__eval_pane3_arrow_mask_";

struct Pane3SolidArrowMaskKey {
    bytes: [u8; 64],
    len: usize,
}

impl Pane3SolidArrowMaskKey {
    #[inline(always)]
    fn as_str(&self) -> &str {
        // SAFETY: construction copies an ASCII prefix and lowercase hex digits.
        unsafe { std::str::from_utf8_unchecked(&self.bytes[..self.len]) }
    }
}

#[inline(always)]
fn pane3_solid_arrow_mask_key(texture_key: &str) -> Pane3SolidArrowMaskKey {
    let mut hasher = XxHash64::default();
    hasher.write(texture_key.as_bytes());
    let hash = hasher.finish();
    let prefix = PANE3_SOLID_ARROW_MASK_KEY_PREFIX.as_bytes();
    let mut bytes = [0_u8; 64];
    bytes[..prefix.len()].copy_from_slice(prefix);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for digit in 0..16 {
        let shift = (15 - digit) * 4;
        bytes[prefix.len() + digit] = HEX[((hash >> shift) & 0xF) as usize];
    }
    Pane3SolidArrowMaskKey {
        bytes,
        len: prefix.len() + 16,
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn pane3_solid_arrow_mask_key_legacy(texture_key: &str) -> String {
    let mut hasher = XxHash64::default();
    hasher.write(texture_key.as_bytes());
    format!(
        "{PANE3_SOLID_ARROW_MASK_KEY_PREFIX}{:016x}",
        hasher.finish()
    )
}

fn pane3_solid_arrow_texture(texture_key: &str) -> Arc<str> {
    let key = pane3_solid_arrow_mask_key(texture_key);
    if let Some(key) = assets::generated_texture_shared_key(key.as_str()) {
        return key;
    }

    let Ok(src) = deadsync_assets::open_bundled_image(&format!("assets/{texture_key}"))
        .map(|img| img.to_rgba8())
    else {
        return Arc::from(texture_key);
    };

    let (w, h) = (src.width(), src.height());
    let mut mask = RgbaImage::new(w, h);
    for (x, y, px) in src.enumerate_pixels() {
        let a = px[3];
        let out = if a == 0 {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([255, 255, 255, a])
        };
        mask.put_pixel(x, y, out);
    }
    assets::register_generated_texture(key.as_str(), mask, SamplerDesc::default());
    assets::generated_texture_shared_key(key.as_str())
        .expect("generated pane arrow texture was just registered")
}

#[inline(always)]
const fn pane3_zoom_x(slot: &SpriteSlot) -> f32 {
    if slot.def.mirror_h { -1.0 } else { 1.0 }
}

#[inline(always)]
const fn pane3_zoom_y(slot: &SpriteSlot) -> f32 {
    if slot.def.mirror_v { -1.0 } else { 1.0 }
}

#[inline(always)]
fn pane3_retexture_model_actor(mut actor: Actor, texture: &str) -> Actor {
    if let Actor::TexturedMesh {
        texture: mesh_tex, ..
    } = &mut actor
    {
        *mesh_tex = texture.into();
    }
    actor
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    FanCombined,
    FanW0,
    FanW1,
    Ex,
    Gr,
    Dec,
    Wo,
    Miss,
}

#[derive(Clone, Copy)]
struct RowInfo {
    kind: RowKind,
    label: &'static str,
    role: Role,
    show_early: bool,
}

const FA_PLUS_ROWS: [RowInfo; 7] = [
    RowInfo {
        kind: RowKind::FanW0,
        label: "FANTASTIC",
        role: Role::FantasticBlue,
        show_early: false,
    },
    RowInfo {
        kind: RowKind::FanW1,
        label: "FANTASTIC",
        role: Role::FantasticWhite,
        show_early: true,
    },
    RowInfo {
        kind: RowKind::Ex,
        label: "EXCELLENT",
        role: Role::Excellent,
        show_early: true,
    },
    RowInfo {
        kind: RowKind::Gr,
        label: "GREAT",
        role: Role::Great,
        show_early: true,
    },
    RowInfo {
        kind: RowKind::Dec,
        label: "DECENT",
        role: Role::Decent,
        show_early: true,
    },
    RowInfo {
        kind: RowKind::Wo,
        label: "WAY OFF",
        role: Role::WayOff,
        show_early: true,
    },
    RowInfo {
        kind: RowKind::Miss,
        label: "MISS",
        role: Role::Miss,
        show_early: false,
    },
];

const STANDARD_ROWS: [RowInfo; 6] = [
    RowInfo {
        kind: RowKind::FanCombined,
        label: "FANTASTIC",
        role: Role::FantasticBlue,
        show_early: false,
    },
    RowInfo {
        kind: RowKind::Ex,
        label: "EXCELLENT",
        role: Role::Excellent,
        show_early: true,
    },
    RowInfo {
        kind: RowKind::Gr,
        label: "GREAT",
        role: Role::Great,
        show_early: true,
    },
    RowInfo {
        kind: RowKind::Dec,
        label: "DECENT",
        role: Role::Decent,
        show_early: true,
    },
    RowInfo {
        kind: RowKind::Wo,
        label: "WAY OFF",
        role: Role::WayOff,
        show_early: true,
    },
    RowInfo {
        kind: RowKind::Miss,
        label: "MISS",
        role: Role::Miss,
        show_early: false,
    },
];

fn column_pane_actor_capacity(inputs: ColumnPaneInputs<'_>) -> usize {
    let rows = if inputs.show_fa_plus_rows {
        FA_PLUS_ROWS.as_slice()
    } else {
        STANDARD_ROWS.as_slice()
    };
    let early_rows = rows.iter().filter(|row| row.show_early).count();
    let label_annotations = if inputs.track_early_judgments {
        early_rows + 2
    } else {
        0
    };
    let per_column_annotations = if inputs.track_early_judgments {
        early_rows
    } else {
        0
    };
    let previews = inputs.noteskin.map_or(0, |noteskin| {
        (0..inputs.columns.len())
            .map(|col_idx| pane3_arrow_preview_capacity(noteskin, col_idx))
            .sum()
    });
    rows.len()
        + label_annotations
        + 1
        + inputs.columns.len() * (rows.len() + per_column_annotations + 2 + 1)
        + previews
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RowCounts {
    count: u32,
    early: Option<u32>,
    early_all: Option<u32>,
}

#[inline(always)]
const fn column_row_counts(cj: ColumnJudgments, kind: RowKind) -> RowCounts {
    match kind {
        RowKind::FanCombined => RowCounts {
            count: cj.w0.saturating_add(cj.w1),
            early: None,
            early_all: None,
        },
        RowKind::FanW0 => RowCounts {
            count: cj.w0,
            early: None,
            early_all: None,
        },
        RowKind::FanW1 => RowCounts {
            count: cj.w1,
            early: Some(cj.early_w1),
            early_all: None,
        },
        RowKind::Ex => RowCounts {
            count: cj.w2,
            early: Some(cj.early_w2),
            early_all: None,
        },
        RowKind::Gr => RowCounts {
            count: cj.w3,
            early: Some(cj.early_w3),
            early_all: None,
        },
        RowKind::Dec => RowCounts {
            count: cj.w4,
            early: Some(cj.early_w4),
            early_all: Some(cj.early_total_w4),
        },
        RowKind::Wo => RowCounts {
            count: cj.w5,
            early: Some(cj.early_w5),
            early_all: Some(cj.early_total_w5),
        },
        RowKind::Miss => RowCounts {
            count: cj.miss,
            early: None,
            early_all: None,
        },
    }
}

#[inline(always)]
const fn row_disabled(disabled_windows: [bool; 5], kind: RowKind) -> bool {
    match kind {
        RowKind::FanCombined | RowKind::FanW0 | RowKind::FanW1 => disabled_windows[0],
        RowKind::Ex => disabled_windows[1],
        RowKind::Gr => disabled_windows[2],
        RowKind::Dec => disabled_windows[3],
        RowKind::Wo => disabled_windows[4],
        RowKind::Miss => false,
    }
}

#[must_use]
pub fn build_column_judgments_pane(
    score_info: &ScoreInfo,
    controller: profile_data::PlayerSide,
    player_side: profile_data::PlayerSide,
    asset_manager: &AssetManager,
    preview_elapsed: f32,
    arrow_glow_active: bool,
) -> Vec<Actor> {
    build_column_judgments_pane_with_palette(
        score_info,
        controller,
        player_side,
        asset_manager,
        preview_elapsed,
        arrow_glow_active,
        JudgmentPalette::default(),
    )
}

#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn build_column_judgments_pane_with_palette(
    score_info: &ScoreInfo,
    controller: profile_data::PlayerSide,
    player_side: profile_data::PlayerSide,
    asset_manager: &AssetManager,
    preview_elapsed: f32,
    arrow_glow_active: bool,
    palette: JudgmentPalette,
) -> Vec<Actor> {
    build_column_judgments_pane_from_inputs(
        ColumnPaneInputs::from_score(score_info),
        controller,
        player_side,
        asset_manager,
        preview_elapsed,
        arrow_glow_active,
        palette,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_column_judgments_pane_from_inputs(
    inputs: ColumnPaneInputs<'_>,
    controller: profile_data::PlayerSide,
    player_side: profile_data::PlayerSide,
    asset_manager: &AssetManager,
    preview_elapsed: f32,
    arrow_glow_active: bool,
    palette: JudgmentPalette,
) -> Vec<Actor> {
    let mut actors = Vec::new();
    push_column_judgments_pane_from_inputs(
        &mut actors,
        inputs,
        controller,
        player_side,
        asset_manager,
        preview_elapsed,
        arrow_glow_active,
        palette,
    );
    actors
}

#[allow(clippy::too_many_arguments)]
fn push_column_judgments_pane_from_inputs(
    actors: &mut Vec<Actor>,
    inputs: ColumnPaneInputs<'_>,
    controller: profile_data::PlayerSide,
    player_side: profile_data::PlayerSide,
    asset_manager: &AssetManager,
    preview_elapsed: f32,
    arrow_glow_active: bool,
    palette: JudgmentPalette,
) {
    let num_cols = inputs.columns.len();
    if num_cols == 0 {
        return;
    }

    let rows = if inputs.show_fa_plus_rows {
        FA_PLUS_ROWS.as_slice()
    } else {
        STANDARD_ROWS.as_slice()
    };

    let cy = screen_center_y();
    let pane_origin_x = pane3_origin_x(controller, num_cols);

    // Pane3 geometry (SL/zmod): 230x146 normally; one-player/two-sides styles
    // (dance-double and pump-double) expand to 520px.
    let box_width = pane3_width(num_cols);
    let box_height: f32 = 146.0;
    let col_width = box_width / num_cols as f32;
    let row_height = box_height / rows.len() as f32;
    let base_x = pane_origin_x - 104.0;
    let base_y = cy - 40.0;

    // Judgment label column (Simply Love): frame at (50, cy-36), labels at x=-130 for P1 and -28 for P2.
    let labels_frame_x = (if player_side == profile_data::PlayerSide::P1 {
        50.0_f32
    } else {
        -50.0_f32
    })
    .mul_add(1.0_f32, pane_origin_x);
    let labels_frame_y = cy - 36.0;
    let labels_right_x = labels_frame_x
        + if player_side == profile_data::PlayerSide::P1 {
            -130.0
        } else {
            -28.0
        };

    const PREVIEW_BPM: f32 = 120.0;
    let preview_time = preview_elapsed.max(0.0);
    let preview_beat = preview_time * (PREVIEW_BPM / 60.0);

    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |miso_font| {
            let label_zoom: f32 = 0.8;
            let number_zoom: f32 = 0.9;
            let small_zoom: f32 = 0.6;
            let held_label_zoom: f32 = 0.6;
            let early_y_offset: f32 = -5.0;
            let all_y_offset: f32 = -10.0;

            // Row labels
            for (row_idx, row) in rows.iter().enumerate() {
                let y = labels_frame_y + (row_idx as f32 + 1.0).mul_add(row_height, 0.0);
                let row_color = if row_disabled(inputs.disabled_timing_windows, row.kind) {
                    DISABLED_WINDOW_RGBA
                } else {
                    palette.color(row.role)
                };
                actors.push(act!(text: font("miso"): settext(row.label):
                    align(1.0, 0.5):
                    xy(labels_right_x, y):
                    zoom(label_zoom):
                    maxwidth(65.0 / label_zoom):
                    horizalign(right):
                    diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
                    z(101)
                ));

                if inputs.track_early_judgments && row.show_early {
                    let label_width =
                        font::measure_line_width_logical(miso_font, row.label, all_fonts) as f32
                            * label_zoom;
                    let early_x = if matches!(row.kind, RowKind::FanW1 | RowKind::Ex) {
                        labels_right_x - label_width / 1.15
                    } else {
                        (labels_right_x - label_width - 4.0).max(labels_frame_x - 190.0)
                    };
                    actors.push(act!(text: font("miso"): settext("Early"):
                        align(1.0, 0.5):
                        xy(early_x, y + early_y_offset):
                        zoom(small_zoom):
                        horizalign(right):
                        diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
                        z(101)
                    ));

                    if matches!(row.kind, RowKind::Dec | RowKind::Wo) {
                        actors.push(act!(text: font("miso"): settext("(All)"):
                            align(1.0, 0.5):
                            xy(labels_right_x, y + all_y_offset):
                            zoom(small_zoom):
                            horizalign(right):
                            diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
                            z(101)
                        ));
                    }
                }
            }

            // "HELD" label at the bottom, aligned relative to the MISS label width.
            let miss_label_width =
                font::measure_line_width_logical(miso_font, "MISS", all_fonts) as f32 * label_zoom;
            let held_label_x = labels_right_x - miss_label_width - 4.0;
            let held_y = base_y + 144.0;
            let miss_color = palette.color(Role::Miss);
            actors.push(act!(text: font("miso"): settext("HELD"):
                align(1.0, 0.5):
                xy(held_label_x, held_y):
                zoom(held_label_zoom):
                horizalign(right):
                diffuse(miss_color[0], miss_color[1], miss_color[2], miss_color[3]):
                z(101)
            ));

            // Columns: arrows + per-row counts
            for col_idx in 0..num_cols {
                let cj = inputs.columns[col_idx];
                let col_center_x = (col_idx as f32 + 1.0).mul_add(col_width, base_x);

                // Measure the widest main count so side annotations clear every row.
                let mut max_count_width: f32 = 0.0;
                for row in rows {
                    let counts = column_row_counts(cj, row.kind);
                    let count_text = InlineU32Text::new(counts.count);
                    let w = font::measure_line_width_logical(
                        miso_font,
                        count_text.as_str(),
                        all_fonts,
                    ) as f32
                        * number_zoom;
                    if w > max_count_width {
                        max_count_width = w;
                    }
                }
                let right_edge_x = max_count_width.mul_add(-0.5, col_center_x - 1.0);

                let arrow_color = arrow_glow_active.then(|| arrow_breakdown_rgba(col_idx));

                // Noteskin preview arrow (Tap Note, Q4th) above the column.
                if let Some(ns) = inputs.noteskin {
                    let note_idx = col_idx
                        .saturating_mul(NUM_QUANTIZATIONS)
                        .saturating_add(Quantization::Q4th as usize);
                    const TARGET_ARROW_PX: f32 = 64.0;
                    const PREVIEW_ZOOM: f32 = 0.4;
                    let elapsed = preview_time;
                    let beat = preview_beat;
                    let note_uv_phase = ns.tap_note_uv_phase(elapsed, beat, 0.0);
                    if let Some(note_slots) = ns.note_layers.get(note_idx) {
                        let primary_h = note_slots
                            .first()
                            .map(|slot| slot.logical_size()[1].max(1.0))
                            .unwrap_or(1.0);
                        let note_scale = if primary_h > f32::EPSILON {
                            (TARGET_ARROW_PX * PREVIEW_ZOOM) / primary_h
                        } else {
                            PREVIEW_ZOOM
                        };
                        let mut solid_arrow_drawn = false;
                        for (layer_idx, slot) in note_slots.iter().enumerate() {
                            let draw = slot.model_draw_at(elapsed, beat);
                            if !draw.visible {
                                continue;
                            }
                            let frame = slot.frame_index_from_phase(note_uv_phase);
                            let uv_elapsed = if slot.model.is_some() {
                                note_uv_phase
                            } else {
                                elapsed
                            };
                            let uv = slot.uv_for_frame_at(frame, uv_elapsed);
                            let raw = slot.logical_size();
                            let base_size = [raw[0] * note_scale, raw[1] * note_scale];
                            let rot_rad = (-slot.def.rotation_deg as f32).to_radians();
                            let (sin_r, cos_r) = rot_rad.sin_cos();
                            let ox = draw.pos[0] * note_scale;
                            let oy = draw.pos[1] * note_scale;
                            let center = [
                                col_center_x + ox * cos_r - oy * sin_r,
                                base_y + ox * sin_r + oy * cos_r,
                            ];
                            let size = [
                                base_size[0] * draw.zoom[0].max(0.0),
                                base_size[1] * draw.zoom[1].max(0.0),
                            ];
                            if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                                continue;
                            }
                            let z = 101 + layer_idx as i32;
                            if let Some(arrow_rgba) = arrow_color {
                                if solid_arrow_drawn {
                                    continue;
                                }
                                let solid_tex = pane3_solid_arrow_texture(slot.texture_key());
                                if let Some(model_actor) = noteskin_model_actor(
                                    slot,
                                    center,
                                    size,
                                    uv,
                                    -slot.def.rotation_deg as f32,
                                    elapsed,
                                    beat,
                                    arrow_rgba,
                                    BlendMode::Alpha,
                                    z as i16,
                                ) {
                                    actors
                                        .push(pane3_retexture_model_actor(model_actor, &solid_tex));
                                } else {
                                    actors.push(act!(sprite(solid_tex):
                                        align(0.5, 0.5):
                                        xy(center[0], center[1]):
                                        setsize(size[0], size[1]):
                                        zoomx(pane3_zoom_x(slot)):
                                        zoomy(pane3_zoom_y(slot)):
                                        rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                        diffuse(arrow_rgba[0], arrow_rgba[1], arrow_rgba[2], arrow_rgba[3]):
                                        blend(normal):
                                        z(z)
                                    ));
                                }
                                solid_arrow_drawn = true;
                                continue;
                            }

                            let color = draw.tint;
                            let blend = if draw.blend_add {
                                BlendMode::Add
                            } else {
                                BlendMode::Alpha
                            };
                            if let Some(model_actor) = noteskin_model_actor(
                                slot,
                                center,
                                size,
                                uv,
                                -slot.def.rotation_deg as f32,
                                elapsed,
                                beat,
                                color,
                                blend,
                                z as i16,
                            ) {
                                actors.push(model_actor);
                            } else if draw.blend_add {
                                actors.push(act!(sprite(slot.texture_key_shared()):
                                    align(0.5, 0.5):
                                    xy(center[0], center[1]):
                                    setsize(size[0], size[1]):
                                    zoomx(pane3_zoom_x(slot)):
                                    zoomy(pane3_zoom_y(slot)):
                                    rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(add):
                                    z(z)
                                ));
                            } else {
                                actors.push(act!(sprite(slot.texture_key_shared()):
                                    align(0.5, 0.5):
                                    xy(center[0], center[1]):
                                    setsize(size[0], size[1]):
                                    zoomx(pane3_zoom_x(slot)):
                                    zoomy(pane3_zoom_y(slot)):
                                    rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(normal):
                                    z(z)
                                ));
                            }
                        }
                    } else if let Some(slot) = ns.notes.get(note_idx) {
                        let draw = slot.model_draw_at(elapsed, beat);
                        if draw.visible {
                        let frame = slot.frame_index_from_phase(note_uv_phase);
                            let uv_elapsed = if slot.model.is_some() {
                                note_uv_phase
                            } else {
                                elapsed
                            };
                            let uv = slot.uv_for_frame_at(frame, uv_elapsed);
                            let size = slot.logical_size();
                            let w = size[0].max(0.0);
                            let h = size[1].max(0.0);
                            if w > 0.0 && h > 0.0 {
                                let scale = (TARGET_ARROW_PX * PREVIEW_ZOOM) / h.max(1.0);
                                let final_size = [w * scale, h * scale];
                                let center = [col_center_x, base_y];
                                if let Some(arrow_rgba) = arrow_color {
                                    let solid_tex = pane3_solid_arrow_texture(slot.texture_key());
                                    if let Some(model_actor) = noteskin_model_actor(
                                        slot,
                                        center,
                                        final_size,
                                        uv,
                                        -slot.def.rotation_deg as f32,
                                        elapsed,
                                        beat,
                                        arrow_rgba,
                                        BlendMode::Alpha,
                                        101,
                                    ) {
                                        actors
                                            .push(pane3_retexture_model_actor(model_actor, &solid_tex));
                                    } else {
                                        actors.push(act!(sprite(solid_tex):
                                            align(0.5, 0.5):
                                            xy(center[0], center[1]):
                                            setsize(final_size[0], final_size[1]):
                                            zoomx(pane3_zoom_x(slot)):
                                            zoomy(pane3_zoom_y(slot)):
                                            rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                            diffuse(arrow_rgba[0], arrow_rgba[1], arrow_rgba[2], arrow_rgba[3]):
                                            blend(normal):
                                            z(101)
                                        ));
                                    }
                                } else {
                                    let color = draw.tint;
                                    if let Some(model_actor) = noteskin_model_actor(
                                        slot,
                                        center,
                                        final_size,
                                        uv,
                                        -slot.def.rotation_deg as f32,
                                        elapsed,
                                        beat,
                                        color,
                                        BlendMode::Alpha,
                                        101,
                                    ) {
                                        actors.push(model_actor);
                                    } else {
                                        actors.push(act!(sprite(slot.texture_key_shared()):
                                            align(0.5, 0.5):
                                            xy(center[0], center[1]):
                                            setsize(final_size[0], final_size[1]):
                                            zoomx(pane3_zoom_x(slot)):
                                            zoomy(pane3_zoom_y(slot)):
                                            rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                            diffuse(color[0], color[1], color[2], color[3]):
                                            blend(normal):
                                            z(101)
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }

                for (row_idx, row) in rows.iter().enumerate() {
                    let counts = column_row_counts(cj, row.kind);
                    let y = labels_frame_y + (row_idx as f32 + 1.0).mul_add(row_height, 0.0);
                    let row_color = if row_disabled(inputs.disabled_timing_windows, row.kind) {
                        DISABLED_WINDOW_RGBA
                    } else {
                        [1.0; 4]
                    };
                    actors.push(act!(text: font("miso"): settext(TextContent::inline_u32(counts.count)):
                        align(0.5, 0.5):
                        xy(col_center_x, y):
                        zoom(number_zoom):
                        horizalign(center):
                        diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
                        z(101)
                    ));

                    if inputs.track_early_judgments
                        && let Some(early) = counts.early
                    {
                        actors.push(act!(text: font("miso"): settext(TextContent::inline_u32(early)):
                            align(1.0, 0.5):
                            xy(right_edge_x, y + early_y_offset):
                            zoom(small_zoom):
                            horizalign(right):
                            diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
                            z(101)
                        ));
                    }

                    if let Some(early_all) = counts.early_all {
                        if inputs.track_early_judgments {
                            actors.push(act!(text: font("miso"): settext(TextContent::inline_u32(early_all)):
                                align(-1.0, 0.5):
                                xy(col_center_x - 1.0, y + all_y_offset):
                                zoom(small_zoom):
                                horizalign(left):
                                diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
                                z(101)
                            ));
                        } else {
                            actors.push(act!(text: font("miso"): settext(TextContent::inline_u32(early_all)):
                                align(1.0, 0.5):
                                xy(right_edge_x, y + all_y_offset):
                                zoom(small_zoom):
                                horizalign(right):
                                diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
                                z(101)
                            ));
                        }
                    }
                }

                // Held-miss count per column (MissBecauseHeld), aligned with the HELD label.
                actors.push(act!(text: font("miso"): settext(TextContent::inline_u32(cj.held_miss)):
                    align(1.0, 0.5):
                    xy(right_edge_x, held_y):
                    zoom(small_zoom):
                    horizalign(right):
                    z(101)
                ));
            }
        })
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn push_cached_column_judgments_pane_with_palette(
    out: &mut Vec<Actor>,
    presentation: &ColumnPanePresentation,
    score_info: &ScoreInfo,
    controller: profile_data::PlayerSide,
    player_side: profile_data::PlayerSide,
    asset_manager: &AssetManager,
    preview_elapsed: f32,
    arrow_glow_active: bool,
    palette: JudgmentPalette,
) {
    push_cached_column_judgments_pane_from_inputs(
        out,
        presentation,
        ColumnPaneInputs::from_score(score_info),
        controller,
        player_side,
        asset_manager,
        preview_elapsed,
        arrow_glow_active,
        palette,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_cached_column_judgments_pane_from_inputs(
    out: &mut Vec<Actor>,
    presentation: &ColumnPanePresentation,
    inputs: ColumnPaneInputs<'_>,
    controller: profile_data::PlayerSide,
    player_side: profile_data::PlayerSide,
    asset_manager: &AssetManager,
    preview_elapsed: f32,
    arrow_glow_active: bool,
    palette: JudgmentPalette,
) {
    let source = presentation
        .scratch
        .borrow_mut()
        .refill([0.0, 0.0], |actors| {
            push_column_judgments_pane_from_inputs(
                actors,
                inputs,
                controller,
                player_side,
                asset_manager,
                preview_elapsed,
                arrow_glow_active,
                palette,
            );
        });
    if let Some(children) = source {
        out.push(Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            children,
            background: None,
            z: 0,
            tint: [1.0; 4],
            blend: None,
        });
    }
}

const PANE3_PREVIEW_BPM: f32 = 120.0;
const PANE3_PREVIEW_TARGET_ARROW_PX: f32 = 64.0;
const PANE3_PREVIEW_ZOOM: f32 = 0.4;

#[inline]
pub(crate) fn pane3_arrow_preview_capacity(
    noteskin: &deadsync_assets::noteskin::Noteskin,
    col_idx: usize,
) -> usize {
    let note_idx = col_idx
        .saturating_mul(NUM_QUANTIZATIONS)
        .saturating_add(Quantization::Q4th as usize);
    noteskin.note_layers.get(note_idx).map_or_else(
        || usize::from(noteskin.notes.get(note_idx).is_some()),
        |layers| layers.len(),
    )
}

/// Appends a single column's tap-note preview (4th-quantized) using the
/// supplied noteskin. Lives next to the pane-3 implementation so the
/// per-arrow timing pane can show identical icons without duplicating the
/// rendering logic. `arrow_color` of `None` uses the noteskin's natural
/// tints; `Some(rgba)` recolors the arrow to a solid tint (used by the
/// glow-on-judge effect in pane 3).
pub(crate) fn push_pane3_arrow_preview(
    actors: &mut Vec<Actor>,
    noteskin: &deadsync_assets::noteskin::Noteskin,
    col_idx: usize,
    center: [f32; 2],
    arrow_color: Option<[f32; 4]>,
    preview_elapsed: f32,
    scale_multiplier: f32,
) {
    let preview_time = preview_elapsed.max(0.0);
    let preview_beat = preview_time * (PANE3_PREVIEW_BPM / 60.0);

    let note_idx = col_idx
        .saturating_mul(NUM_QUANTIZATIONS)
        .saturating_add(Quantization::Q4th as usize);
    let elapsed = preview_time;
    let beat = preview_beat;
    let note_uv_phase = noteskin.tap_note_uv_phase(elapsed, beat, 0.0);
    let (cx, cy) = (center[0], center[1]);
    let effective_zoom = PANE3_PREVIEW_ZOOM * scale_multiplier.max(0.0);

    if let Some(note_slots) = noteskin.note_layers.get(note_idx) {
        let primary_h = note_slots
            .first()
            .map(|slot| slot.logical_size()[1].max(1.0))
            .unwrap_or(1.0);
        let note_scale = if primary_h > f32::EPSILON {
            (PANE3_PREVIEW_TARGET_ARROW_PX * effective_zoom) / primary_h
        } else {
            effective_zoom
        };
        let mut solid_arrow_drawn = false;
        for (layer_idx, slot) in note_slots.iter().enumerate() {
            let draw = slot.model_draw_at(elapsed, beat);
            if !draw.visible {
                continue;
            }
            let frame = slot.frame_index_from_phase(note_uv_phase);
            let uv_elapsed = if slot.model.is_some() {
                note_uv_phase
            } else {
                elapsed
            };
            let uv = slot.uv_for_frame_at(frame, uv_elapsed);
            let raw = slot.logical_size();
            let base_size = [raw[0] * note_scale, raw[1] * note_scale];
            let rot_rad = (-slot.def.rotation_deg as f32).to_radians();
            let (sin_r, cos_r) = rot_rad.sin_cos();
            let ox = draw.pos[0] * note_scale;
            let oy = draw.pos[1] * note_scale;
            let pos = [cx + ox * cos_r - oy * sin_r, cy + ox * sin_r + oy * cos_r];
            let size = [
                base_size[0] * draw.zoom[0].max(0.0),
                base_size[1] * draw.zoom[1].max(0.0),
            ];
            if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                continue;
            }
            let z = 101 + layer_idx as i32;
            if let Some(arrow_rgba) = arrow_color {
                if solid_arrow_drawn {
                    continue;
                }
                let solid_tex = pane3_solid_arrow_texture(slot.texture_key());
                if let Some(model_actor) = noteskin_model_actor(
                    slot,
                    pos,
                    size,
                    uv,
                    -slot.def.rotation_deg as f32,
                    elapsed,
                    beat,
                    arrow_rgba,
                    BlendMode::Alpha,
                    z as i16,
                ) {
                    actors.push(pane3_retexture_model_actor(model_actor, &solid_tex));
                } else {
                    actors.push(act!(sprite(solid_tex):
                        align(0.5, 0.5):
                        xy(pos[0], pos[1]):
                        setsize(size[0], size[1]):
                        zoomx(pane3_zoom_x(slot)):
                        zoomy(pane3_zoom_y(slot)):
                        rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                        diffuse(arrow_rgba[0], arrow_rgba[1], arrow_rgba[2], arrow_rgba[3]):
                        blend(normal):
                        z(z)
                    ));
                }
                solid_arrow_drawn = true;
                continue;
            }

            let color = draw.tint;
            let blend = if draw.blend_add {
                BlendMode::Add
            } else {
                BlendMode::Alpha
            };
            if let Some(model_actor) = noteskin_model_actor(
                slot,
                pos,
                size,
                uv,
                -slot.def.rotation_deg as f32,
                elapsed,
                beat,
                color,
                blend,
                z as i16,
            ) {
                actors.push(model_actor);
            } else if draw.blend_add {
                actors.push(act!(sprite(slot.texture_key_shared()):
                    align(0.5, 0.5):
                    xy(pos[0], pos[1]):
                    setsize(size[0], size[1]):
                    zoomx(pane3_zoom_x(slot)):
                    zoomy(pane3_zoom_y(slot)):
                    rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                    diffuse(color[0], color[1], color[2], color[3]):
                    blend(add):
                    z(z)
                ));
            } else {
                actors.push(act!(sprite(slot.texture_key_shared()):
                    align(0.5, 0.5):
                    xy(pos[0], pos[1]):
                    setsize(size[0], size[1]):
                    zoomx(pane3_zoom_x(slot)):
                    zoomy(pane3_zoom_y(slot)):
                    rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                    diffuse(color[0], color[1], color[2], color[3]):
                    blend(normal):
                    z(z)
                ));
            }
        }
    } else if let Some(slot) = noteskin.notes.get(note_idx) {
        let draw = slot.model_draw_at(elapsed, beat);
        if !draw.visible {
            return;
        }
        let frame = slot.frame_index_from_phase(note_uv_phase);
        let uv_elapsed = if slot.model.is_some() {
            note_uv_phase
        } else {
            elapsed
        };
        let uv = slot.uv_for_frame_at(frame, uv_elapsed);
        let size = slot.logical_size();
        let w = size[0].max(0.0);
        let h = size[1].max(0.0);
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let scale = (PANE3_PREVIEW_TARGET_ARROW_PX * effective_zoom) / h.max(1.0);
        let final_size = [w * scale, h * scale];
        if let Some(arrow_rgba) = arrow_color {
            let solid_tex = pane3_solid_arrow_texture(slot.texture_key());
            if let Some(model_actor) = noteskin_model_actor(
                slot,
                [cx, cy],
                final_size,
                uv,
                -slot.def.rotation_deg as f32,
                elapsed,
                beat,
                arrow_rgba,
                BlendMode::Alpha,
                101,
            ) {
                actors.push(pane3_retexture_model_actor(model_actor, &solid_tex));
            } else {
                actors.push(act!(sprite(solid_tex):
                    align(0.5, 0.5):
                    xy(cx, cy):
                    setsize(final_size[0], final_size[1]):
                    zoomx(pane3_zoom_x(slot)):
                    zoomy(pane3_zoom_y(slot)):
                    rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                    diffuse(arrow_rgba[0], arrow_rgba[1], arrow_rgba[2], arrow_rgba[3]):
                    blend(normal):
                    z(101)
                ));
            }
        } else {
            let color = draw.tint;
            if let Some(model_actor) = noteskin_model_actor(
                slot,
                [cx, cy],
                final_size,
                uv,
                -slot.def.rotation_deg as f32,
                elapsed,
                beat,
                color,
                BlendMode::Alpha,
                101,
            ) {
                actors.push(model_actor);
            } else {
                actors.push(act!(sprite(slot.texture_key_shared()):
                    align(0.5, 0.5):
                    xy(cx, cy):
                    setsize(final_size[0], final_size[1]):
                    zoomx(pane3_zoom_x(slot)):
                    zoomy(pane3_zoom_y(slot)):
                    rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                    diffuse(color[0], color[1], color[2], color[3]):
                    blend(normal):
                    z(101)
                ));
            }
        }
    }
}

#[cfg(any(test, feature = "bench-support"))]
pub(crate) fn build_pane3_arrow_preview(
    noteskin: &deadsync_assets::noteskin::Noteskin,
    col_idx: usize,
    center: [f32; 2],
    arrow_color: Option<[f32; 4]>,
    preview_elapsed: f32,
    scale_multiplier: f32,
) -> Vec<Actor> {
    let mut actors = Vec::new();
    push_pane3_arrow_preview(
        &mut actors,
        noteskin,
        col_idx,
        center,
        arrow_color,
        preview_elapsed,
        scale_multiplier,
    );
    actors
}

#[cfg(any(test, feature = "bench-support"))]
pub struct ColumnPaneCacheBenchmark {
    columns: [ColumnJudgments; 4],
    noteskin: deadsync_assets::noteskin::Noteskin,
    presentation: ColumnPanePresentation,
    asset_manager: AssetManager,
}

#[cfg(any(test, feature = "bench-support"))]
impl ColumnPaneCacheBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let columns = std::array::from_fn(|index| ColumnJudgments {
            w0: 100 + index as u32,
            w1: 200 + index as u32,
            w2: 30 + index as u32,
            w3: 4 + index as u32,
            w4: 2,
            w5: 1,
            miss: index as u32,
            early_w1: 80,
            early_w2: 12,
            early_w3: 2,
            early_w4: 1,
            early_w5: 1,
            early_total_w4: 3,
            early_total_w5: 2,
            held_miss: index as u32,
            ..ColumnJudgments::default()
        });
        let noteskin = deadsync_assets::noteskin::load_itg_default(&deadsync_noteskin::Style {
            num_cols: 4,
            num_players: 1,
        })
        .expect("bundled dance noteskin should load");
        let inputs = ColumnPaneInputs {
            columns: &columns,
            noteskin: Some(&noteskin),
            show_fa_plus_rows: true,
            track_early_judgments: true,
            disabled_timing_windows: [false; 5],
        };
        let presentation = ColumnPanePresentation::from_inputs(inputs);
        let fixture = Self {
            columns,
            noteskin,
            presentation,
            asset_manager: super::benchmark_asset_manager(),
        };
        let mut warm = Vec::with_capacity(1);
        let _ = fixture.retained_frame(&mut warm);
        fixture
    }

    fn inputs(&self) -> ColumnPaneInputs<'_> {
        ColumnPaneInputs {
            columns: &self.columns,
            noteskin: Some(&self.noteskin),
            show_fa_plus_rows: true,
            track_early_judgments: true,
            disabled_timing_windows: [false; 5],
        }
    }

    fn benchmark_texture_key(&self) -> &str {
        self.noteskin
            .note_layers
            .iter()
            .flat_map(|layers| layers.iter())
            .next()
            .expect("benchmark noteskin should contain a tap-note layer")
            .texture_key()
    }

    #[must_use]
    pub fn legacy_mask_key(&self) -> Arc<str> {
        let key = pane3_solid_arrow_mask_key_legacy(self.benchmark_texture_key());
        assert!(
            assets::texture_dims(&key).is_some(),
            "benchmark generated mask should be primed"
        );
        Arc::from(key)
    }

    #[must_use]
    pub fn shared_mask_key(&self) -> Arc<str> {
        pane3_solid_arrow_texture(self.benchmark_texture_key())
    }

    #[must_use]
    pub fn direct_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        out.extend(build_column_judgments_pane_from_inputs(
            self.inputs(),
            profile_data::PlayerSide::P1,
            profile_data::PlayerSide::P1,
            &self.asset_manager,
            1.25,
            false,
            JudgmentPalette::default(),
        ));
        std::hint::black_box(&*out);
        column_actor_count(out)
    }

    #[must_use]
    pub fn retained_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_cached_column_judgments_pane_from_inputs(
            out,
            &self.presentation,
            self.inputs(),
            profile_data::PlayerSide::P1,
            profile_data::PlayerSide::P1,
            &self.asset_manager,
            1.25,
            false,
            JudgmentPalette::default(),
        );
        std::hint::black_box(&*out);
        column_actor_count(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for ColumnPaneCacheBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn column_actor_count(actors: &[Actor]) -> u64 {
    actors
        .iter()
        .map(|actor| match actor {
            Actor::Frame { children, .. } => column_actor_count(children),
            Actor::SharedFrame { children, .. } => column_actor_count(children),
            _ => 1,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::super::utils::{arrow_breakdown_rgba, pane3_origin_x};
    use super::{
        ColumnPaneCacheBenchmark, FA_PLUS_ROWS, PANE3_DOUBLE_WIDTH, PANE3_SINGLE_WIDTH, RowCounts,
        RowKind, STANDARD_ROWS, build_pane3_arrow_preview, column_row_counts,
        pane3_solid_arrow_mask_key, pane3_solid_arrow_mask_key_legacy, pane3_width, row_disabled,
    };
    use crate::screens::evaluation::ColumnJudgments;
    use deadlib_present::actors::Actor;
    use deadlib_present::color;
    use deadsync_assets::noteskin::load_itg_default;
    use deadsync_noteskin::Style;
    use deadsync_profile as profile_data;
    use std::sync::Arc;

    #[test]
    fn stack_arrow_mask_keys_match_legacy_formatting() {
        for texture_key in [
            "noteskins/pump/default/DownLeft Tap Note 8x4.png",
            "noteskins/dance/love/Left Tap Note.png",
            "short.png",
            "",
        ] {
            assert_eq!(
                pane3_solid_arrow_mask_key(texture_key).as_str(),
                pane3_solid_arrow_mask_key_legacy(texture_key)
            );
        }
    }

    #[test]
    fn static_column_rows_preserve_judgment_order_and_labels() {
        assert_eq!(
            STANDARD_ROWS.map(|row| (row.kind, row.label, row.show_early)),
            [
                (RowKind::FanCombined, "FANTASTIC", false),
                (RowKind::Ex, "EXCELLENT", true),
                (RowKind::Gr, "GREAT", true),
                (RowKind::Dec, "DECENT", true),
                (RowKind::Wo, "WAY OFF", true),
                (RowKind::Miss, "MISS", false),
            ]
        );
        assert_eq!(
            FA_PLUS_ROWS.map(|row| (row.kind, row.label, row.show_early)),
            [
                (RowKind::FanW0, "FANTASTIC", false),
                (RowKind::FanW1, "FANTASTIC", true),
                (RowKind::Ex, "EXCELLENT", true),
                (RowKind::Gr, "GREAT", true),
                (RowKind::Dec, "DECENT", true),
                (RowKind::Wo, "WAY OFF", true),
                (RowKind::Miss, "MISS", false),
            ]
        );
    }

    #[test]
    fn column_counts_expose_arrowcloud_all_bad_rescores() {
        let cj = ColumnJudgments {
            w4: 3,
            w5: 4,
            early_w4: 1,
            early_w5: 2,
            early_total_w4: 5,
            early_total_w5: 6,
            ..Default::default()
        };

        assert_eq!(
            column_row_counts(cj, RowKind::Dec),
            RowCounts {
                count: 3,
                early: Some(1),
                early_all: Some(5),
            }
        );
        assert_eq!(
            column_row_counts(cj, RowKind::Wo),
            RowCounts {
                count: 4,
                early: Some(2),
                early_all: Some(6),
            }
        );
    }

    #[test]
    fn column_counts_keep_rescore_all_counts_off_other_rows() {
        let cj = ColumnJudgments {
            w0: 1,
            w1: 2,
            w2: 3,
            w3: 4,
            miss: 5,
            early_w1: 6,
            early_w2: 7,
            early_w3: 8,
            early_total_w2: 9,
            early_total_w3: 10,
            ..Default::default()
        };

        assert_eq!(
            column_row_counts(cj, RowKind::FanCombined),
            RowCounts {
                count: 3,
                early: None,
                early_all: None,
            }
        );
        assert_eq!(
            column_row_counts(cj, RowKind::Ex),
            RowCounts {
                count: 3,
                early: Some(7),
                early_all: None,
            }
        );
        assert_eq!(
            column_row_counts(cj, RowKind::Gr),
            RowCounts {
                count: 4,
                early: Some(8),
                early_all: None,
            }
        );
        assert_eq!(
            column_row_counts(cj, RowKind::Miss),
            RowCounts {
                count: 5,
                early: None,
                early_all: None,
            }
        );
    }

    #[test]
    fn column_rows_map_to_disabled_timing_windows() {
        let disabled = [false, false, false, true, true];
        assert!(row_disabled(disabled, RowKind::Dec));
        assert!(row_disabled(disabled, RowKind::Wo));
        assert!(!row_disabled(disabled, RowKind::Gr));
        assert!(!row_disabled(disabled, RowKind::Miss));
    }

    #[test]
    fn pane3_doubles_layout_uses_full_width_slot() {
        assert_eq!(pane3_width(4), PANE3_SINGLE_WIDTH);
        assert_eq!(pane3_width(5), PANE3_SINGLE_WIDTH);
        assert_eq!(pane3_width(8), PANE3_DOUBLE_WIDTH);
        assert_eq!(pane3_width(10), PANE3_DOUBLE_WIDTH);
        assert_eq!(
            pane3_origin_x(profile_data::PlayerSide::P1, 8),
            pane3_origin_x(profile_data::PlayerSide::P2, 8)
        );
        assert_eq!(
            pane3_origin_x(profile_data::PlayerSide::P1, 10),
            pane3_origin_x(profile_data::PlayerSide::P2, 10)
        );
    }

    #[test]
    fn pane3_arrow_glow_colors_tint_doubles_p2_columns() {
        assert_eq!(arrow_breakdown_rgba(0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(arrow_breakdown_rgba(1), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(arrow_breakdown_rgba(2), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(arrow_breakdown_rgba(3), [1.0, 1.0, 0.0, 1.0]);
        assert_eq!(arrow_breakdown_rgba(4), color::rgba_hex("#B54DFF"));
        assert_eq!(arrow_breakdown_rgba(5), color::rgba_hex("#FF8A00"));
        assert_eq!(arrow_breakdown_rgba(6), color::rgba_hex("#00D7FF"));
        assert_eq!(arrow_breakdown_rgba(7), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(arrow_breakdown_rgba(8), [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn pump_evaluation_preview_preserves_right_panel_mirroring() {
        let noteskin = load_itg_default(&Style {
            num_cols: 5,
            num_players: 1,
        })
        .expect("bundled pump default noteskin should load");
        let left = build_pane3_arrow_preview(&noteskin, 0, [0.0, 0.0], None, 0.0, 1.0);
        let right = build_pane3_arrow_preview(&noteskin, 3, [0.0, 0.0], None, 0.0, 1.0);

        let first_flip_x = |actors: &[Actor]| {
            actors.iter().find_map(|actor| match actor {
                Actor::Sprite { flip_x, .. } => Some(*flip_x),
                _ => None,
            })
        };
        assert_eq!(first_flip_x(&left), Some(false));
        assert_eq!(first_flip_x(&right), Some(true));
    }

    #[test]
    fn pump_evaluation_preview_advances_noteskin_animation_phase() {
        let noteskin = load_itg_default(&Style {
            num_cols: 5,
            num_players: 1,
        })
        .expect("bundled pump default noteskin should load");
        let at_start = build_pane3_arrow_preview(&noteskin, 0, [0.0, 0.0], None, 0.0, 1.0);
        let at_next_frame = build_pane3_arrow_preview(&noteskin, 0, [0.0, 0.0], None, 0.2, 1.0);

        let first_uv = |actors: &[Actor]| {
            actors.iter().find_map(|actor| match actor {
                Actor::Sprite { uv_rect, .. } => Some(*uv_rect),
                _ => None,
            })
        };
        assert_ne!(first_uv(&at_start), first_uv(&at_next_frame));
    }

    #[test]
    fn retained_column_pane_matches_direct_and_reuses_its_buffer() {
        let fixture = ColumnPaneCacheBenchmark::new();
        let mut direct = Vec::new();
        let mut retained = Vec::with_capacity(1);
        assert_eq!(
            fixture.direct_frame(&mut direct),
            fixture.retained_frame(&mut retained),
        );
        let [Actor::SharedFrame { children, .. }] = retained.as_slice() else {
            panic!("expected retained column pane in one shared frame");
        };
        let [
            Actor::Frame {
                children: retained_actors,
                ..
            },
        ] = children.as_ref()
        else {
            panic!("expected reusable identity frame inside column pane");
        };
        assert_eq!(format!("{direct:#?}"), format!("{retained_actors:#?}"));

        let source_ptr = Arc::as_ptr(children).cast::<()>() as usize;
        retained.clear();
        let _ = fixture.retained_frame(&mut retained);
        let [
            Actor::SharedFrame {
                children: repeated, ..
            },
        ] = retained.as_slice()
        else {
            panic!("expected repeated retained column pane");
        };
        assert_eq!(source_ptr, Arc::as_ptr(repeated).cast::<()>() as usize);
        let stats = fixture.presentation.scratch.borrow().stats();
        assert_eq!(stats.growths, 0);
        assert_eq!(stats.replacements, 0);
    }
}
