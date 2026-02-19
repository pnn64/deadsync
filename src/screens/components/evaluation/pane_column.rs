use crate::act;
use crate::assets::AssetManager;
use crate::core::gfx::BlendMode;
use crate::core::space::screen_center_y;
use crate::game::parsing::noteskin::{NUM_QUANTIZATIONS, Quantization};
use crate::game::profile;
use crate::screens::components::notefield::noteskin_model_actor;
use crate::screens::evaluation::{ColumnJudgments, ScoreInfo};
use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::font;

use super::utils::pane_origin_x;

pub fn build_column_judgments_pane(
    score_info: &ScoreInfo,
    controller: profile::PlayerSide,
    player_side: profile::PlayerSide,
    asset_manager: &AssetManager,
) -> Vec<Actor> {
    let num_cols = score_info.column_judgments.len();
    if num_cols == 0 {
        return vec![];
    }

    #[derive(Clone, Copy)]
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
        color: [f32; 4],
    }

    let show_fa_plus_rows = score_info.show_fa_plus_window && score_info.show_fa_plus_pane;
    let rows: Vec<RowInfo> = if show_fa_plus_rows {
        vec![
            RowInfo {
                kind: RowKind::FanW0,
                label: "FANTASTIC",
                color: color::JUDGMENT_RGBA[0],
            },
            RowInfo {
                kind: RowKind::FanW1,
                label: "FANTASTIC",
                color: color::JUDGMENT_FA_PLUS_WHITE_RGBA,
            },
            RowInfo {
                kind: RowKind::Ex,
                label: "EXCELLENT",
                color: color::JUDGMENT_RGBA[1],
            },
            RowInfo {
                kind: RowKind::Gr,
                label: "GREAT",
                color: color::JUDGMENT_RGBA[2],
            },
            RowInfo {
                kind: RowKind::Dec,
                label: "DECENT",
                color: color::JUDGMENT_RGBA[3],
            },
            RowInfo {
                kind: RowKind::Wo,
                label: "WAY OFF",
                color: color::JUDGMENT_RGBA[4],
            },
            RowInfo {
                kind: RowKind::Miss,
                label: "MISS",
                color: color::JUDGMENT_RGBA[5],
            },
        ]
    } else {
        vec![
            RowInfo {
                kind: RowKind::FanCombined,
                label: "FANTASTIC",
                color: color::JUDGMENT_RGBA[0],
            },
            RowInfo {
                kind: RowKind::Ex,
                label: "EXCELLENT",
                color: color::JUDGMENT_RGBA[1],
            },
            RowInfo {
                kind: RowKind::Gr,
                label: "GREAT",
                color: color::JUDGMENT_RGBA[2],
            },
            RowInfo {
                kind: RowKind::Dec,
                label: "DECENT",
                color: color::JUDGMENT_RGBA[3],
            },
            RowInfo {
                kind: RowKind::Wo,
                label: "WAY OFF",
                color: color::JUDGMENT_RGBA[4],
            },
            RowInfo {
                kind: RowKind::Miss,
                label: "MISS",
                color: color::JUDGMENT_RGBA[5],
            },
        ]
    };

    let cy = screen_center_y();
    let pane_origin_x = pane_origin_x(controller);

    // Pane3 geometry (Simply Love): 230x146 box, anchored near (-104, cy-40) within the P1 pane.
    let box_width: f32 = 230.0;
    let box_height: f32 = 146.0;
    let col_width = box_width / num_cols as f32;
    let row_height = box_height / rows.len() as f32;
    let base_x = pane_origin_x - 104.0;
    let base_y = cy - 40.0;

    // Judgment label column (Simply Love): frame at (50, cy-36), labels at x=-130 for P1 and -28 for P2.
    let labels_frame_x = (if player_side == profile::PlayerSide::P1 {
        50.0_f32
    } else {
        -50.0_f32
    })
    .mul_add(1.0_f32, pane_origin_x);
    let labels_frame_y = cy - 36.0;
    let labels_right_x = labels_frame_x
        + if player_side == profile::PlayerSide::P1 {
            -130.0
        } else {
            -28.0
        };

    let mut actors = Vec::new();

    let count_for = |cj: ColumnJudgments, kind: RowKind| -> (u32, Option<u32>) {
        match kind {
            RowKind::FanCombined => (cj.w0.saturating_add(cj.w1), None),
            RowKind::FanW0 => (cj.w0, None),
            RowKind::FanW1 => (cj.w1, None),
            RowKind::Ex => (cj.w2, None),
            RowKind::Gr => (cj.w3, None),
            RowKind::Dec => (cj.w4, Some(cj.early_w4)),
            RowKind::Wo => (cj.w5, Some(cj.early_w5)),
            RowKind::Miss => (cj.miss, None),
        }
    };

    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |miso_font| {
            let label_zoom: f32 = 0.8;
            let number_zoom: f32 = 0.9;
            let small_zoom: f32 = 0.65;
            let held_label_zoom: f32 = 0.6;

            // Row labels
            for (row_idx, row) in rows.iter().enumerate() {
                let y = labels_frame_y + (row_idx as f32 + 1.0).mul_add(row_height, 0.0);
                actors.push(act!(text: font("miso"): settext(row.label.to_string()):
                    align(1.0, 0.5):
                    xy(labels_right_x, y):
                    zoom(label_zoom):
                    maxwidth(65.0 / label_zoom):
                    horizalign(right):
                    diffuse(row.color[0], row.color[1], row.color[2], row.color[3]):
                    z(101)
                ));
            }

            // "HELD" label at the bottom, aligned relative to the MISS label width.
            let miss_label_width =
                font::measure_line_width_logical(miso_font, "MISS", all_fonts) as f32 * label_zoom;
            let held_label_x = labels_right_x - miss_label_width / 1.15;
            let held_y = labels_frame_y + 140.0;
            let miss_color = color::JUDGMENT_RGBA[5];
            actors.push(act!(text: font("miso"): settext("HELD".to_string()):
                align(1.0, 0.5):
                xy(held_label_x, held_y):
                zoom(held_label_zoom):
                horizalign(right):
                diffuse(miss_color[0], miss_color[1], miss_color[2], miss_color[3]):
                z(101)
            ));

            // Columns: arrows + per-row counts
            for col_idx in 0..num_cols {
                let cj = score_info.column_judgments[col_idx];
                let col_center_x = (col_idx as f32 + 1.0).mul_add(col_width, base_x);

                // Measure Miss number width for this column for alignment of early/held counts.
                let miss_str = cj.miss.to_string();
                let miss_width = font::measure_line_width_logical(miso_font, &miss_str, all_fonts)
                    as f32
                    * number_zoom;
                let right_edge_x = col_center_x - 1.0 - miss_width * 0.5;

                // Noteskin preview arrow (Tap Note, Q4th) above the column.
                if let Some(ns) = score_info.noteskin.as_ref() {
                    let note_idx = col_idx
                        .saturating_mul(NUM_QUANTIZATIONS)
                        .saturating_add(Quantization::Q4th as usize);
                    const TARGET_ARROW_PX: f32 = 64.0;
                    const PREVIEW_ZOOM: f32 = 0.4;
                    let elapsed = 0.0f32;
                    let beat = 0.0f32;
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
                        for (layer_idx, slot) in note_slots.iter().enumerate() {
                            let draw = slot.model_draw_at(elapsed, beat);
                            if !draw.visible {
                                continue;
                            }
                            let frame = slot.frame_index(elapsed, beat);
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
                            let color = draw.tint;
                            let blend = if draw.blend_add {
                                BlendMode::Add
                            } else {
                                BlendMode::Alpha
                            };
                            let z = 101 + layer_idx as i32;
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
                                actors.push(act!(sprite(slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(center[0], center[1]):
                                    setsize(size[0], size[1]):
                                    rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(add):
                                    z(z)
                                ));
                            } else {
                                actors.push(act!(sprite(slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(center[0], center[1]):
                                    setsize(size[0], size[1]):
                                    rotationz(draw.rot[2] - slot.def.rotation_deg as f32):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    diffuse(color[0], color[1], color[2], color[3]):
                                    blend(normal):
                                    z(z)
                                ));
                            }
                        }
                    } else if let Some(slot) = ns.notes.get(note_idx) {
                        let frame = slot.frame_index(elapsed, beat);
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
                            if let Some(model_actor) = noteskin_model_actor(
                                slot,
                                center,
                                final_size,
                                uv,
                                -slot.def.rotation_deg as f32,
                                elapsed,
                                beat,
                                [1.0, 1.0, 1.0, 1.0],
                                BlendMode::Alpha,
                                101,
                            ) {
                                actors.push(model_actor);
                            } else {
                                actors.push(act!(sprite(slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(center[0], center[1]):
                                    setsize(final_size[0], final_size[1]):
                                    rotationz(-slot.def.rotation_deg as f32):
                                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                    z(101)
                                ));
                            }
                        }
                    }
                }

                for (row_idx, row) in rows.iter().enumerate() {
                    let (count, early_opt) = count_for(cj, row.kind);
                    let y = labels_frame_y + (row_idx as f32 + 1.0).mul_add(row_height, 0.0);
                    actors.push(act!(text: font("miso"): settext(count.to_string()):
                        align(0.5, 0.5):
                        xy(col_center_x, y):
                        zoom(number_zoom):
                        horizalign(center):
                        z(101)
                    ));

                    if let Some(early) = early_opt {
                        let early_y = y - 10.0;
                        actors.push(act!(text: font("miso"): settext(early.to_string()):
                            align(1.0, 0.5):
                            xy(right_edge_x, early_y):
                            zoom(small_zoom):
                            horizalign(right):
                            z(101)
                        ));
                    }
                }

                // Held-miss count per column (MissBecauseHeld) at y=144, aligned like early counts.
                let held_str = cj.held_miss.to_string();
                actors.push(act!(text: font("miso"): settext(held_str):
                    align(1.0, 0.5):
                    xy(right_edge_x, base_y + 144.0):
                    zoom(small_zoom):
                    horizalign(right):
                    z(101)
                ));
            }
        })
    });

    actors
}
