use crate::core::gfx as renderer;
use crate::core::gfx::{BlendMode, RenderList, RenderObject};
use crate::core::space::Metrics;
use crate::assets;
use crate::ui::font; // CHANGED
use crate::ui::actors::{self, Actor, SizeSpec};
use cgmath::{Deg, Matrix4, Vector2, Vector3};

/* ======================= RENDERER SCREEN BUILDER ======================= */

#[inline(always)]
pub fn build_screen(
    actors: &[actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &std::collections::HashMap<&'static str, font::Font>,
    total_elapsed: f32,
) -> RenderList {
    let mut objects = Vec::with_capacity(estimate_object_count(actors, fonts));
    let mut order_counter: u32 = 0;

    let root_rect = SmRect {
        x: 0.0,
        y: 0.0,
        w: m.right - m.left,
        h: m.top - m.bottom,
    };
    let parent_z: i16 = 0;

    for actor in actors {
        build_actor_recursive(
            actor,
            root_rect,
            m,
            fonts,
            parent_z,
            &mut order_counter,
            &mut objects,
            total_elapsed,
        );
    }

    objects.sort_by_key(|o| (o.z, o.order));

    RenderList { clear_color, objects }
}

#[inline(always)]
fn estimate_object_count(
    actors: &[Actor],
    fonts: &std::collections::HashMap<&'static str, font::Font>,
) -> usize {
    let mut stack: Vec<&Actor> = Vec::with_capacity(actors.len());
    stack.extend(actors.iter());
    let mut total = 0usize;

    while let Some(a) = stack.pop() {
        match a {
            Actor::Sprite { visible, tint, .. } => {
                if *visible && tint[3] > 0.0 {
                    total += 1;
                }
            }
            Actor::Text { content, font, .. } => {
                if let Some(fm) = fonts.get(font) {
                    total += content
                        .chars()
                        .filter(|&c| {
                            if c == '\n' {
                                return false;
                            }
                            // Count if explicitly mapped OR we have a default glyph.
                            // (Previously skipped unmapped SPACE; SM still draws advance/default.)
                            let mapped = fm.glyph_map.contains_key(&c);
                            mapped || fm.default_glyph.is_some()
                        })
                        .count();
                } else {
                    total += content.chars().filter(|&ch| ch != '\n').count();
                }
            }
            Actor::Frame {
                children, background, ..
            } => {
                if background.is_some() {
                    total += 1;
                }
                stack.extend(children.iter());
            }
        }
    }
    total
}

/* ======================= ACTOR -> OBJECT CONVERSION ======================= */

#[derive(Clone, Copy)]
struct SmRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[inline(always)]
fn build_actor_recursive(
    actor: &actors::Actor,
    parent: SmRect,
    m: &Metrics,
    fonts: &std::collections::HashMap<&'static str, font::Font>,
    base_z: i16,
    order_counter: &mut u32,
    out: &mut Vec<RenderObject>,
    total_elapsed: f32,
) {
    match actor {
        actors::Actor::Sprite {
            align,
            offset,
            size,
            source,
            tint,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            blend,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            rot_z_deg,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
        } => {
            if !*visible {
                return;
            }

            let (is_solid, texture_name) = match source {
                actors::SpriteSource::Solid => (true, "__white"),
                actors::SpriteSource::Texture(name) => (false, name.as_str()),
            };

            let mut chosen_cell = *cell;
            let mut chosen_grid = *grid;

            if !is_solid && uv_rect.is_none() {
                let (cols, rows) = grid.unwrap_or_else(|| assets::parse_sprite_sheet_dims(texture_name));
                let total = cols.saturating_mul(rows).max(1);

                let start_linear: u32 = match *cell {
                    Some((cx, cy)) if cy != u32::MAX => {
                        let cx = cx.min(cols.saturating_sub(1));
                        let cy = cy.min(rows.saturating_sub(1));
                        cy.saturating_mul(cols).saturating_add(cx)
                    }
                    Some((i, _)) => i,
                    None => 0,
                };

                if *animate && *state_delay > 0.0 && total > 1 {
                    let steps = (total_elapsed / *state_delay).floor().max(0.0) as u32;
                    let idx = (start_linear + (steps % total)) % total;
                    chosen_cell = Some((idx, u32::MAX));
                    chosen_grid = Some((cols, rows));
                } else if chosen_cell.is_none() && total > 1 {
                    chosen_cell = Some((0, u32::MAX));
                    chosen_grid = Some((cols, rows));
                }
            }

            let resolved_size = resolve_sprite_size_like_sm(
                *size,
                is_solid,
                texture_name,
                *uv_rect,
                chosen_cell,
                chosen_grid,
                *scale,
            );

            let rect = place_rect(parent, *align, *offset, resolved_size);

            let before = out.len();
            push_sprite(
                out,
                rect,
                m,
                is_solid,
                texture_name,
                *tint,
                *uv_rect,
                chosen_cell,
                chosen_grid,
                *flip_x,
                *flip_y,
                *cropleft,
                *cropright,
                *croptop,
                *cropbottom,
                *fadeleft,
                *faderight,
                *fadetop,
                *fadebottom,
                *blend,
                *rot_z_deg,
                *texcoordvelocity,
                total_elapsed,
            );

            let layer = base_z.saturating_add(*z);
            for i in before..out.len() {
                out[i].z = layer;
                out[i].order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
        }

        actors::Actor::Text {
            align,
            offset,
            color,
            font,
            content,
            align_text,
            z,
            scale,
            fit_width,
            fit_height,
            max_width,
            max_height,
            // NEW:
            max_w_pre_zoom,
            max_h_pre_zoom,
            blend,
        } => {
            if let Some(fm) = fonts.get(font) {
                let mut objects = layout_text(
                    fm,
                    content,
                    0.0,                 // _px_size unused
                    *scale,
                    *fit_width,
                    *fit_height,
                    *max_width,
                    *max_height,
                    // NEW flags:
                    *max_w_pre_zoom,
                    *max_h_pre_zoom,
                    parent,
                    *align,
                    *offset,
                    *align_text,
                    m,
                );
                let layer = base_z.saturating_add(*z);
                for obj in &mut objects {
                    obj.z = layer;
                    obj.order = {
                        let o = *order_counter;
                        *order_counter += 1;
                        o
                    };
                    obj.blend = *blend;
                    let renderer::ObjectType::Sprite { tint, .. } = &mut obj.object_type;
                    *tint = *color;
                }
                out.extend(objects);
            }
        }

        actors::Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        } => {
            let rect = place_rect(parent, *align, *offset, *size);
            let layer = base_z.saturating_add(*z);

            if let Some(bg) = background {
                match bg {
                    actors::Background::Color(c) => {
                        let before = out.len();
                        push_sprite(
                            out,
                            rect,
                            m,
                            true,
                            "__white",
                            *c,
                            None,
                            None,
                            None,
                            false,
                            false,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            BlendMode::Alpha,
                            0.0,
                            None,
                            total_elapsed,
                        );
                        for i in before..out.len() {
                            out[i].z = layer;
                            out[i].order = {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            };
                        }
                    }
                    actors::Background::Texture(tex) => {
                        let before = out.len();
                        push_sprite(
                            out,
                            rect,
                            m,
                            false,
                            tex,
                            [1.0; 4],
                            None,
                            None,
                            None,
                            false,
                            false,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            0.0,
                            BlendMode::Alpha,
                            0.0,
                            None,
                            total_elapsed,
                        );
                        for i in before..out.len() {
                            out[i].z = layer;
                            out[i].order = {
                                let o = *order_counter;
                                *order_counter += 1;
                                o
                            };
                        }
                    }
                }
            }

            for child in children {
                build_actor_recursive(
                    child,
                    rect,
                    m,
                    fonts,
                    layer,
                    order_counter,
                    out,
                    total_elapsed,
                );
            }
        }
    }
}

/* ======================= LAYOUT HELPERS ======================= */

#[inline(always)]
fn resolve_sprite_size_like_sm(
    size: [SizeSpec; 2],
    is_solid: bool,
    texture_name: &str,
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    scale: [f32; 2],
) -> [SizeSpec; 2] {
    use SizeSpec::Px;

    #[inline(always)]
    fn native_dims(
        is_solid: bool,
        texture_name: &str,
        uv: Option<[f32; 4]>,
        cell: Option<(u32, u32)>,
        grid: Option<(u32, u32)>,
    ) -> (f32, f32) {
        if is_solid {
            return (1.0, 1.0);
        }
        let Some(meta) = assets::texture_dims(texture_name) else {
            return (0.0, 0.0);
        };
        let (mut tw, mut th) = (meta.w as f32, meta.h as f32);
        if let Some([u0, v0, u1, v1]) = uv {
            tw *= (u1 - u0).abs().max(1e-6);
            th *= (v1 - v0).abs().max(1e-6);
        } else if cell.is_some() {
            let (gc, gr) = grid.unwrap_or_else(|| assets::parse_sprite_sheet_dims(texture_name));
            let cols = gc.max(1);
            let rows = gr.max(1);
            tw /= cols as f32;
            th /= rows as f32;
        }
        (tw, th)
    }

    let (nw, nh) = native_dims(is_solid, texture_name, uv_rect, cell, grid);
    let aspect = if nw > 0.0 && nh > 0.0 { nh / nw } else { 1.0 };

    match (size[0], size[1]) {
        (Px(w), Px(h)) if w == 0.0 && h == 0.0 => [Px(nw * scale[0]), Px(nh * scale[1])],
        (Px(w), Px(h)) if w > 0.0 && h == 0.0 => [Px(w), Px(w * aspect)],
        (Px(w), Px(h)) if w == 0.0 && h > 0.0 => {
            let inv_aspect = if aspect > 0.0 { 1.0 / aspect } else { 1.0 };
            [Px(h * inv_aspect), Px(h)]
        }
        _ => size,
    }
}

#[inline(always)]
fn place_rect(parent: SmRect, align: [f32; 2], offset: [f32; 2], size: [SizeSpec; 2]) -> SmRect {
    let w = match size[0] {
        SizeSpec::Px(w) => w,
        SizeSpec::Fill => parent.w,
    };
    let h = match size[1] {
        SizeSpec::Px(h) => h,
        SizeSpec::Fill => parent.h,
    };
    let rx = parent.x;
    let ry = parent.y;
    let ax = align[0];
    let ay = align[1];
    SmRect {
        x: rx + offset[0] - ax * w,
        y: ry + offset[1] - ay * h,
        w,
        h,
    }
}

#[inline(always)]
fn calculate_uvs(
    texture: &str,
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    flip_x: bool,
    flip_y: bool,
    cl: f32,
    cr: f32,
    ct: f32,
    cb: f32,
    texcoordvelocity: Option<[f32; 2]>,
    total_elapsed: f32,
) -> ([f32; 2], [f32; 2]) {
    let (mut uv_scale, mut uv_offset) = if let Some([u0, v0, u1, v1]) = uv_rect {
        let du = (u1 - u0).abs().max(1e-6);
        let dv = (v1 - v0).abs().max(1e-6);
        ([du, dv], [u0.min(u1), v0.min(v1)])
    } else if let Some((cx, cy)) = cell {
        let (gc, gr) = grid.unwrap_or_else(|| assets::parse_sprite_sheet_dims(texture));
        let cols = gc.max(1);
        let rows = gr.max(1);
        let (col, row) = if cy == u32::MAX {
            let idx = cx;
            (
                idx % cols,
                (idx / cols).min(rows.saturating_sub(1)),
            )
        } else {
            (
                cx.min(cols.saturating_sub(1)),
                cy.min(rows.saturating_sub(1)),
            )
        };
        let s = [1.0 / cols as f32, 1.0 / rows as f32];
        let o = [col as f32 * s[0], row as f32 * s[1]];
        (s, o)
    } else {
        ([1.0, 1.0], [0.0, 0.0])
    };

    uv_offset[0] += uv_scale[0] * cl;
    uv_offset[1] += uv_scale[1] * ct;
    uv_scale[0] *= (1.0 - cl - cr).max(0.0);
    uv_scale[1] *= (1.0 - ct - cb).max(0.0);

    if flip_x {
        uv_offset[0] += uv_scale[0];
        uv_scale[0] = -uv_scale[0];
    }
    if flip_y {
        uv_offset[1] += uv_scale[1];
        uv_scale[1] = -uv_scale[1];
    }

    if let Some(vel) = texcoordvelocity {
        uv_offset[0] += vel[0] * total_elapsed;
        uv_offset[1] += vel[1] * total_elapsed;
    }

    (uv_scale, uv_offset)
}

#[inline(always)]
fn push_sprite(
    out: &mut Vec<renderer::RenderObject>,
    rect: SmRect,
    m: &Metrics,
    is_solid: bool,
    texture_id: &str,
    tint: [f32; 4],
    uv_rect: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    flip_x: bool,
    flip_y: bool,
    cropleft: f32,
    cropright: f32,
    croptop: f32,
    cropbottom: f32,
    fadeleft: f32,
    faderight: f32,
    fadetop: f32,
    fadebottom: f32,
    blend: BlendMode,
    rot_z_deg: f32,
    texcoordvelocity: Option<[f32; 2]>,
    total_elapsed: f32,
) {
    if tint[3] <= 0.0 {
        return;
    }

    let (cl, cr, ct, cb) = clamp_crop_fractions(cropleft, cropright, croptop, cropbottom);

    let (base_center, base_size) = sm_rect_to_world_center_size(rect, m);
    if base_size.x <= 0.0 || base_size.y <= 0.0 {
        return;
    }

    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= 0.0 || sy_crop <= 0.0 {
        return;
    }

    let center_x = base_center.x;
    let center_y = base_center.y;
    let size_x = base_size.x * sx_crop;
    let size_y = base_size.y * sy_crop;

    let (uv_scale, uv_offset) = if is_solid {
        ([1.0, 1.0], [0.0, 0.0])
    } else {
        calculate_uvs(
            texture_id,
            uv_rect,
            cell,
            grid,
            flip_x,
            flip_y,
            cl,
            cr,
            ct,
            cb,
            texcoordvelocity,
            total_elapsed,
        )
    };

    let fl = fadeleft.clamp(0.0, 1.0);
    let fr = faderight.clamp(0.0, 1.0);
    let ft = fadetop.clamp(0.0, 1.0);
    let fb = fadebottom.clamp(0.0, 1.0);

    let mut fl_eff = ((fl - cl).max(0.0) / sx_crop).clamp(0.0, 1.0);
    let mut fr_eff = ((fr - cr).max(0.0) / sx_crop).clamp(0.0, 1.0);
    let mut ft_eff = ((ft - ct).max(0.0) / sy_crop).clamp(0.0, 1.0);
    let mut fb_eff = ((fb - cb).max(0.0) / sy_crop).clamp(0.0, 1.0);

    if flip_x {
        std::mem::swap(&mut fl_eff, &mut fr_eff);
    }
    if flip_y {
        std::mem::swap(&mut ft_eff, &mut fb_eff);
    }

    let transform = Matrix4::from_translation(Vector3::new(center_x, center_y, 0.0))
        * Matrix4::from_angle_z(Deg(rot_z_deg))
        * Matrix4::from_nonuniform_scale(size_x, size_y, 1.0);

    let final_texture_id = if is_solid {
        "__white".to_string()
    } else {
        texture_id.to_string()
    };

    out.push(renderer::RenderObject {
        object_type: renderer::ObjectType::Sprite {
            texture_id: final_texture_id,
            tint,
            uv_scale,
            uv_offset,
            edge_fade: [fl_eff, fr_eff, ft_eff, fb_eff],
        },
        transform,
        blend,
        z: 0,
        order: 0,
    });
}

#[inline(always)]
#[must_use]
fn clamp_crop_fractions(l: f32, r: f32, t: f32, b: f32) -> (f32, f32, f32, f32) {
    (
        l.clamp(0.0, 1.0),
        r.clamp(0.0, 1.0),
        t.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
    )
}

#[inline(always)]
#[must_use]
fn lrint_ties_even(v: f32) -> f32 {
    if !v.is_finite() {
        return 0.0;
    }
    // Fast path: already an integer (including -0.0)
    if v.fract() == 0.0 {
        return v;
    }

    let floor = v.floor();
    let frac = v - floor;

    if frac < 0.5 {
        floor
    } else if frac > 0.5 {
        floor + 1.0
    } else {
        // frac == 0.5 exactly: ties-to-even
        // Use i64 for parity check to avoid edge overflow on extreme values.
        let f_even = ((floor as i64) & 1) == 0;
        if f_even { floor } else { floor + 1.0 }
    }
}

#[inline(always)]
#[must_use]
fn quantize_up_even_px(v: f32) -> f32 {
    if !v.is_finite() || v <= 0.0 {
        return 0.0;
    }
    let mut n = v.ceil() as i32;
    if (n & 1) != 0 {
        n += 1;
    }
    n as f32
}

fn layout_text(
    font: &font::Font,
    text: &str,
    _px_size: f32,
    scale: [f32; 2],
    fit_width: Option<f32>,
    fit_height: Option<f32>,
    max_width: Option<f32>,
    max_height: Option<f32>,
    // NEW: StepMania order semantics (per axis)
    max_w_pre_zoom: bool,
    max_h_pre_zoom: bool,
    parent: SmRect,
    align: [f32; 2],
    offset: [f32; 2],
    text_align: actors::TextAlign,
    m: &Metrics,
) -> Vec<RenderObject> {
    if text.is_empty() {
        return vec![];
    }
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return vec![];
    }

    // 1) Logical (integer) widths like SM: sum integer advances (default glyph if unmapped).
    let logical_line_widths: Vec<i32> = lines.iter().map(|l| font::measure_line_width_logical(font, l)).collect();
    let max_logical_width = logical_line_widths.iter().copied().max().unwrap_or(0) as f32;

    // 2) Unscaled block cap height + line spacing in logical units
    let cap_height = if font.height > 0 { font.height as f32 } else { font.line_spacing as f32 };
    let num_lines = lines.len();
    let unscaled_block_height = if num_lines > 1 {
        cap_height + ((num_lines - 1) as f32 * font.line_spacing as f32)
    } else {
        cap_height
    };

    // 3) Fit scaling (zoomto...) preserves aspect ratio
    use std::f32::INFINITY;
    let s_w_fit = fit_width.map_or(INFINITY, |w| if max_logical_width > 0.0 { w / max_logical_width } else { 1.0 });
    let s_h_fit = fit_height.map_or(INFINITY, |h| if unscaled_block_height > 0.0 { h / unscaled_block_height } else { 1.0 });
    let fit_s = if s_w_fit.is_infinite() && s_h_fit.is_infinite() { 1.0 } else { s_w_fit.min(s_h_fit).max(0.0) };

    // 4) Reference sizes before/after zoom (but before max clamp)
    let width_before_zoom  = max_logical_width     * fit_s;
    let height_before_zoom = unscaled_block_height * fit_s;

    let width_after_zoom   = width_before_zoom  * scale[0];
    let height_after_zoom  = height_before_zoom * scale[1];

    // 5) Decide the clamp denominators per axis based on order flags
    // If a zoom occurred AFTER the last max for that axis, SM semantics = clamp BEFORE that zoom.
    // Otherwise clamp AFTER zoom.
    let denom_w_for_max = if max_w_pre_zoom { width_before_zoom } else { width_after_zoom };
    let denom_h_for_max = if max_h_pre_zoom { height_before_zoom } else { height_after_zoom };

    // 6) Compute per-axis extra downscale from max constraints
    let max_s_w = max_width.map_or(1.0, |mw| {
        if denom_w_for_max > mw { (mw / denom_w_for_max).max(0.0) } else { 1.0 }
    });
    let max_s_h = max_height.map_or(1.0, |mh| {
        if denom_h_for_max > mh { (mh / denom_h_for_max).max(0.0) } else { 1.0 }
    });

    // 7) Final per-axis scales: fit * zoom * (potential extra downscale)
    let sx = scale[0] * fit_s * max_s_w;
    let sy = scale[1] * fit_s * max_s_h;
    if sx.abs() < 1e-6 || sy.abs() < 1e-6 {
        return vec![];
    }

    // 8) Pixel rounding/snapping (unchanged)
    let line_widths_px: Vec<f32> = logical_line_widths.iter().map(|w| lrint_ties_even((*w as f32) * sx)).collect();
    let max_line_width_px = line_widths_px.iter().fold(0.0_f32, |a, &b| a.max(b));
    let block_w_px = quantize_up_even_px(max_line_width_px);
    let block_h_px = unscaled_block_height * sy;

    // 9) Place the block, compute baseline (unchanged)
    let block_left_sm = parent.x + offset[0] - align[0] * block_w_px;
    let block_top_sm  = parent.y + offset[1] - align[1] * block_h_px;
    let block_center_y = block_top_sm  + 0.5 * block_h_px;

    let i_y0 = lrint_ties_even(-block_h_px * 0.5);
    let baseline_local = i_y0 + (font.height as f32) * sy;
    let mut baseline_sm = lrint_ties_even(block_center_y + baseline_local);

    #[inline(always)]
    fn start_x_px(align: actors::TextAlign, block_left_px: f32, block_w_px: f32, line_w_px: f32) -> f32 {
        match align {
            actors::TextAlign::Left   => block_left_px,
            actors::TextAlign::Center => lrint_ties_even(block_left_px + 0.5 * (block_w_px - line_w_px)),
            actors::TextAlign::Right  => lrint_ties_even(block_left_px + (block_w_px - line_w_px)),
        }
    }

    use std::collections::HashMap;
    let mut dims_cache: HashMap<&str, (f32, f32)> = HashMap::new();

    #[inline(always)]
    fn atlas_dims<'a>(cache: &mut HashMap<&'a str, (f32, f32)>, key: &'a str) -> (f32, f32) {
        if let Some(&d) = cache.get(key) {
            return d;
        }
        let d = assets::texture_dims(key)
            .map_or((1.0_f32, 1.0_f32), |meta| (meta.w as f32, meta.h as f32));
        cache.insert(key, d);
        d
    }

    let mut objects = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let line_w_px = line_widths_px[i];
        let pen_start_x = start_x_px(text_align, block_left_sm, block_w_px, line_w_px);

        let mut pen_ux: i32 = 0;

        for ch in line.chars() {
            let mapped = font.glyph_map.get(&ch);
            let glyph = match mapped.or(font.default_glyph.as_ref()) {
                Some(g) => g,
                None => continue, // no glyph and no default; skip entirely
            };

            let quad_w = glyph.size[0] * sx;
            let quad_h = glyph.size[1] * sy;

            let draw_quad = !(ch == ' ' && mapped.is_none());

            let pen_x_draw = pen_start_x + (pen_ux as f32) * sx;

            if draw_quad && quad_w.abs() >= 1e-6 && quad_h.abs() >= 1e-6 {
                let quad_x_sm = pen_x_draw + glyph.offset[0] * sx;
                let quad_y_sm = lrint_ties_even(baseline_sm + glyph.offset[1] * sy);

                let center_x = m.left + quad_x_sm + quad_w * 0.5;
                let center_y = m.top  - (quad_y_sm + quad_h * 0.5);

                let transform = Matrix4::from_translation(Vector3::new(center_x, center_y, 0.0))
                    * Matrix4::from_nonuniform_scale(quad_w, quad_h, 1.0);

                let (tex_w, tex_h) = atlas_dims(&mut dims_cache, &glyph.texture_key);
                let uv_scale = [
                    (glyph.tex_rect[2] - glyph.tex_rect[0]) / tex_w,
                    (glyph.tex_rect[3] - glyph.tex_rect[1]) / tex_h,
                ];
                let uv_offset = [glyph.tex_rect[0] / tex_w, glyph.tex_rect[1] / tex_h];

                objects.push(RenderObject {
                    object_type: renderer::ObjectType::Sprite {
                        texture_id: glyph.texture_key.clone(),
                        tint: [1.0; 4],
                        uv_scale,
                        uv_offset,
                        edge_fade: [0.0; 4],
                    },
                    transform,
                    blend: BlendMode::Alpha,
                    z: 0,
                    order: 0,
                });
            }

            pen_ux += glyph.advance as i32;
        }

        baseline_sm = lrint_ties_even(baseline_sm + (font.line_spacing as f32) * sy);
    }

    objects
}

#[inline(always)]
fn sm_rect_to_world_center_size(rect: SmRect, m: &Metrics) -> (Vector2<f32>, Vector2<f32>) {
    (
        Vector2::new(m.left + rect.x + 0.5 * rect.w, m.top - (rect.y + 0.5 * rect.h)),
        Vector2::new(rect.w, rect.h),
    )
}
