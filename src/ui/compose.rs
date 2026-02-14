use crate::assets;
use crate::core::gfx as renderer;
use crate::core::gfx::{BlendMode, RenderList, RenderObject};
use crate::core::space::Metrics;
use crate::ui::actors::{self, Actor, SizeSpec};
use crate::ui::{anim, font};
use cgmath::{Matrix4, Rad, Vector2, Vector3};

/* ======================= RENDERER SCREEN BUILDER ======================= */

#[inline(always)]
pub fn build_screen<'a>(
    actors: &'a [actors::Actor],
    clear_color: [f32; 4],
    m: &Metrics,
    fonts: &std::collections::HashMap<&'static str, font::Font>,
    total_elapsed: f32,
) -> RenderList<'a> {
    let mut objects = Vec::with_capacity(estimate_object_count(actors));
    let mut cameras: Vec<Matrix4<f32>> = Vec::with_capacity(4);
    cameras.push(cgmath::ortho(m.left, m.right, m.bottom, m.top, -1.0, 1.0));
    let mut order_counter: u32 = 0;

    let root_rect = SmRect {
        x: 0.0,
        y: 0.0,
        w: m.right - m.left,
        h: m.top - m.bottom,
    };
    let parent_z: i16 = 0;
    let camera: u8 = 0;

    for actor in actors {
        build_actor_recursive(
            actor,
            root_rect,
            m,
            fonts,
            parent_z,
            camera,
            &mut cameras,
            &mut order_counter,
            &mut objects,
            total_elapsed,
        );
    }

    objects.sort_by_key(|o| (o.z, o.order));

    RenderList {
        clear_color,
        cameras,
        objects,
    }
}

#[inline(always)]
fn estimate_object_count(actors: &[Actor]) -> usize {
    let mut stack: Vec<&Actor> = Vec::with_capacity(actors.len());
    stack.extend(actors.iter());
    let mut total = 0usize;

    while let Some(a) = stack.pop() {
        match a {
            Actor::Sprite { visible, .. } => {
                if *visible {
                    total += 1;
                }
            }
            Actor::Text { content, .. } => {
                // Heuristic: each char is a glyph + potentially stroke.
                // 2 objects per char is a safe upper bound.
                total += content.len() * 2;
            }
            Actor::Mesh {
                visible, vertices, ..
            } => {
                if *visible && !vertices.is_empty() {
                    total += 1;
                }
            }
            Actor::TexturedMesh {
                visible, vertices, ..
            } => {
                if *visible && !vertices.is_empty() {
                    total += 1;
                }
            }
            Actor::Frame {
                children,
                background,
                ..
            } => {
                if background.is_some() {
                    total += 1;
                }
                stack.extend(children.iter());
            }
            Actor::Camera { children, .. } => {
                stack.extend(children.iter());
            }
            Actor::Shadow { child, .. } => {
                stack.push(child);
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
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn apply_effect_to_sprite(
    effect: anim::EffectState,
    elapsed: f32,
    tint: &mut [f32; 4],
    scale: &mut [f32; 2],
    rot_deg: &mut [f32; 3],
) {
    let beat = elapsed;
    if matches!(effect.mode, anim::EffectMode::Spin) {
        let units = anim::effect_clock_units(effect, elapsed, beat) - effect.offset;
        rot_deg[0] = (rot_deg[0] + effect.magnitude[0] * units).rem_euclid(360.0);
        rot_deg[1] = (rot_deg[1] + effect.magnitude[1] * units).rem_euclid(360.0);
        rot_deg[2] = (rot_deg[2] + effect.magnitude[2] * units).rem_euclid(360.0);
    }

    if let Some(mix) = anim::effect_mix(effect, elapsed, beat) {
        match effect.mode {
            anim::EffectMode::DiffuseRamp => {
                for (i, out) in tint.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color1[i], effect.color2[i], mix).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::Pulse => {
                scale[0] *= lerp_f32(1.0, effect.magnitude[0], mix).max(0.0);
                scale[1] *= lerp_f32(1.0, effect.magnitude[1], mix).max(0.0);
            }
            anim::EffectMode::GlowShift | anim::EffectMode::Spin | anim::EffectMode::None => {}
        }
    }

    tint[0] = tint[0].clamp(0.0, 1.0);
    tint[1] = tint[1].clamp(0.0, 1.0);
    tint[2] = tint[2].clamp(0.0, 1.0);
    tint[3] = tint[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
}

#[inline(always)]
fn apply_effect_to_text(
    effect: anim::EffectState,
    elapsed: f32,
    color: &mut [f32; 4],
    scale: &mut [f32; 2],
) {
    let beat = elapsed;
    if let Some(mix) = anim::effect_mix(effect, elapsed, beat) {
        match effect.mode {
            anim::EffectMode::DiffuseRamp => {
                for (i, out) in color.iter_mut().enumerate() {
                    let c = lerp_f32(effect.color1[i], effect.color2[i], mix).clamp(0.0, 1.0);
                    *out *= c;
                }
            }
            anim::EffectMode::Pulse => {
                scale[0] *= lerp_f32(1.0, effect.magnitude[0], mix).max(0.0);
                scale[1] *= lerp_f32(1.0, effect.magnitude[1], mix).max(0.0);
            }
            anim::EffectMode::GlowShift | anim::EffectMode::Spin | anim::EffectMode::None => {}
        }
    }

    color[0] = color[0].clamp(0.0, 1.0);
    color[1] = color[1].clamp(0.0, 1.0);
    color[2] = color[2].clamp(0.0, 1.0);
    color[3] = color[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
}

#[inline(always)]
fn build_actor_recursive<'a>(
    actor: &'a actors::Actor,
    parent: SmRect,
    m: &Metrics,
    fonts: &std::collections::HashMap<&'static str, font::Font>,
    base_z: i16,
    camera: u8,
    cameras: &mut Vec<Matrix4<f32>>,
    order_counter: &mut u32,
    out: &mut Vec<RenderObject<'a>>,
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
            glow: _,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            rot_z_deg,
            rot_x_deg,
            rot_y_deg,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            effect,
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
                let (cols, rows) = grid.unwrap_or_else(|| assets::sprite_sheet_dims(texture_name));
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

            let mut effect_tint = *tint;
            let mut effect_scale = *scale;
            let mut effect_rot = [*rot_x_deg, *rot_y_deg, *rot_z_deg];
            apply_effect_to_sprite(
                *effect,
                total_elapsed,
                &mut effect_tint,
                &mut effect_scale,
                &mut effect_rot,
            );

            let resolved_size = resolve_sprite_size_like_sm(
                *size,
                is_solid,
                texture_name,
                *uv_rect,
                chosen_cell,
                chosen_grid,
                effect_scale,
            );

            let rect = place_rect(parent, *align, *offset, resolved_size);

            let before = out.len();
            push_sprite(
                out,
                camera,
                rect,
                m,
                is_solid,
                texture_name,
                effect_tint,
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
                effect_rot[0],
                effect_rot[1],
                effect_rot[2],
                *texcoordvelocity,
                total_elapsed,
            );

            let layer = base_z.saturating_add(*z);
            for obj in out.iter_mut().skip(before) {
                obj.z = layer;
                obj.order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
        }

        actors::Actor::Mesh {
            align,
            offset,
            size,
            vertices,
            mode,
            visible,
            blend,
            z,
        } => {
            if !*visible || vertices.is_empty() {
                return;
            }

            let rect = place_rect(parent, *align, *offset, *size);
            let base_x = m.left + rect.x;
            let base_y = m.top - rect.y;

            let mut world: Vec<renderer::MeshVertex> = Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                world.push(renderer::MeshVertex {
                    pos: [base_x + v.pos[0], base_y - v.pos[1]],
                    color: v.color,
                });
            }

            let before = out.len();
            out.push(renderer::RenderObject {
                object_type: renderer::ObjectType::Mesh {
                    vertices: std::borrow::Cow::Owned(world),
                    mode: *mode,
                },
                transform: Matrix4::from_scale(1.0),
                blend: *blend,
                z: 0,
                order: 0,
                camera,
            });

            let layer = base_z.saturating_add(*z);
            for obj in out.iter_mut().skip(before) {
                obj.z = layer;
                obj.order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
        }

        actors::Actor::TexturedMesh {
            align,
            offset,
            size,
            texture,
            vertices,
            mode,
            visible,
            blend,
            z,
        } => {
            if !*visible || vertices.is_empty() {
                return;
            }

            let rect = place_rect(parent, *align, *offset, *size);
            let base_x = m.left + rect.x;
            let base_y = m.top - rect.y;

            let mut world: Vec<renderer::TexturedMeshVertex> = Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                world.push(renderer::TexturedMeshVertex {
                    pos: [base_x + v.pos[0], base_y - v.pos[1]],
                    uv: v.uv,
                    color: v.color,
                });
            }

            let before = out.len();
            out.push(renderer::RenderObject {
                object_type: renderer::ObjectType::TexturedMesh {
                    texture_id: std::borrow::Cow::Borrowed(texture.as_str()),
                    vertices: std::borrow::Cow::Owned(world),
                    mode: *mode,
                },
                transform: Matrix4::from_scale(1.0),
                blend: *blend,
                z: 0,
                order: 0,
                camera,
            });

            let layer = base_z.saturating_add(*z);
            for obj in out.iter_mut().skip(before) {
                obj.z = layer;
                obj.order = {
                    let o = *order_counter;
                    *order_counter += 1;
                    o
                };
            }
        }

        actors::Actor::Shadow { len, color, child } => {
            // Build the child first to push its objects; then duplicate those objects
            // with a pre-multiplied world translation and shadow tint at z-1.
            let start = out.len();
            build_actor_recursive(
                child,
                parent,
                m,
                fonts,
                base_z,
                camera,
                cameras,
                order_counter,
                out,
                total_elapsed,
            );

            // Prepare world-space translation matrix that matches StepMania's
            // DISPLAY->TranslateWorld behavior.
            let t_world = Matrix4::from_translation(Vector3::new(len[0], len[1], 0.0));

            // Duplicate each object produced for the child as a shadow pass.
            let end = out.len();
            for i in start..end {
                let obj = &out[i];
                let mut obj_type = obj.object_type.clone();
                match &mut obj_type {
                    renderer::ObjectType::Sprite { tint, .. } => {
                        // Multiply alpha like SM: shadow.a *= child_alpha
                        let mut shadow_tint = *color;
                        shadow_tint[3] *= (*tint)[3];
                        *tint = shadow_tint;
                    }
                    renderer::ObjectType::Mesh { vertices, .. } => {
                        let sc = *color;
                        let mut out = Vec::with_capacity(vertices.len());
                        for v in vertices.iter() {
                            out.push(renderer::MeshVertex {
                                pos: v.pos,
                                color: [
                                    v.color[0] * sc[0],
                                    v.color[1] * sc[1],
                                    v.color[2] * sc[2],
                                    v.color[3] * sc[3],
                                ],
                            });
                        }
                        *vertices = std::borrow::Cow::Owned(out);
                    }
                    renderer::ObjectType::TexturedMesh { vertices, .. } => {
                        let sc = *color;
                        let mut out = Vec::with_capacity(vertices.len());
                        for v in vertices.iter() {
                            out.push(renderer::TexturedMeshVertex {
                                pos: v.pos,
                                uv: v.uv,
                                color: [
                                    v.color[0] * sc[0],
                                    v.color[1] * sc[1],
                                    v.color[2] * sc[2],
                                    v.color[3] * sc[3],
                                ],
                            });
                        }
                        *vertices = std::borrow::Cow::Owned(out);
                    }
                }

                out.push(renderer::RenderObject {
                    object_type: obj_type,
                    transform: t_world * obj.transform,
                    blend: obj.blend,
                    // Draw behind the original to ensure correct order without
                    // having to rewind the global order counter.
                    z: obj.z.saturating_sub(1),
                    order: obj.order, // order doesn't matter since z is lower
                    camera: obj.camera,
                });
            }
        }

        actors::Actor::Camera {
            view_proj,
            children,
        } => {
            cameras.push(*view_proj);
            let id = cameras.len().saturating_sub(1).try_into().unwrap_or(0u8);
            for child in children {
                build_actor_recursive(
                    child,
                    parent,
                    m,
                    fonts,
                    base_z,
                    id,
                    cameras,
                    order_counter,
                    out,
                    total_elapsed,
                );
            }
        }

        actors::Actor::Text {
            align,
            offset,
            color,
            stroke_color,
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
            clip,
            blend,
            glow: _,
            effect,
        } => {
            if let Some(fm) = fonts.get(font) {
                let mut effect_color = *color;
                let mut effect_scale = *scale;
                apply_effect_to_text(*effect, total_elapsed, &mut effect_color, &mut effect_scale);
                let mut objects = layout_text(
                    fm,
                    fonts,
                    content.as_str(),
                    0.0, // _px_size unused
                    effect_scale,
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
                if let Some([x, y, w, h]) = *clip {
                    let clip_sm = SmRect {
                        x: parent.x + x,
                        y: parent.y + y,
                        w,
                        h,
                    };
                    let clip_world = sm_rect_to_world_edges(clip_sm, m);
                    clip_objects_to_world_rect(&mut objects, clip_world);
                }
                let layer = base_z.saturating_add(*z);
                let mut stroke_rgba = stroke_color.unwrap_or(fm.default_stroke_color);
                stroke_rgba[3] *= effect_color[3];
                if stroke_rgba[3] > 0.0 && !fm.stroke_texture_map.is_empty() {
                    let mut stroke_objects = Vec::with_capacity(objects.len());
                    for obj in &objects {
                        let renderer::ObjectType::Sprite { texture_id, .. } = &obj.object_type
                        else {
                            continue;
                        };
                        let Some(stroke_key) = fm.stroke_texture_map.get(texture_id.as_ref())
                        else {
                            continue;
                        };
                        let mut stroke_obj = obj.clone();
                        let renderer::ObjectType::Sprite {
                            texture_id, tint, ..
                        } = &mut stroke_obj.object_type
                        else {
                            continue;
                        };
                        *texture_id = std::borrow::Cow::Owned(stroke_key.clone());
                        *tint = stroke_rgba;
                        stroke_objects.push(stroke_obj);
                    }
                    for obj in &mut stroke_objects {
                        obj.z = layer;
                        obj.order = {
                            let o = *order_counter;
                            *order_counter += 1;
                            o
                        };
                        obj.blend = *blend;
                        obj.camera = camera;
                    }
                    out.extend(stroke_objects);
                }
                for obj in &mut objects {
                    obj.z = layer;
                    obj.order = {
                        let o = *order_counter;
                        *order_counter += 1;
                        o
                    };
                    obj.blend = *blend;
                    obj.camera = camera;
                    if let renderer::ObjectType::Sprite { tint, .. } = &mut obj.object_type {
                        *tint = effect_color;
                    }
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
                            camera,
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
                            0.0,
                            0.0,
                            None,
                            total_elapsed,
                        );
                        for obj in out.iter_mut().skip(before) {
                            obj.z = layer;
                            obj.order = {
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
                            camera,
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
                            0.0,
                            0.0,
                            None,
                            total_elapsed,
                        );
                        for obj in out.iter_mut().skip(before) {
                            obj.z = layer;
                            obj.order = {
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
                    camera,
                    cameras,
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
            let (gc, gr) = grid.unwrap_or_else(|| assets::sprite_sheet_dims(texture_name));
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
        x: ax.mul_add(-w, rx + offset[0]),
        y: ay.mul_add(-h, ry + offset[1]),
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
        let (gc, gr) = grid.unwrap_or_else(|| assets::sprite_sheet_dims(texture));
        let cols = gc.max(1);
        let rows = gr.max(1);
        let (col, row) = if cy == u32::MAX {
            let idx = cx;
            (idx % cols, (idx / cols).min(rows.saturating_sub(1)))
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
fn push_sprite<'a>(
    out: &mut Vec<renderer::RenderObject<'a>>,
    camera: u8,
    rect: SmRect,
    m: &Metrics,
    is_solid: bool,
    texture_id: &'a str,
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
    rot_x_deg: f32,
    rot_y_deg: f32,
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

    // StepMania parity: crop shifts geometry toward the un-cropped side(s).
    // (This matches Sprite::DrawTexture(), which moves quad vertices instead of the actor.)
    let center_x = ((cl - cr) * base_size.x).mul_add(0.5, base_center.x);
    let center_y = ((cb - ct) * base_size.y).mul_add(0.5, base_center.y);
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

    // StepMania parity (Sprite::DrawPrimitives edge-fade behavior):
    // - Fade distances are specified in the *pre-crop* [0..1] space.
    // - Visible (post-crop) fraction is `(1 - crop_a - crop_b)`.
    // - Negative crop values can "cancel" fade (used by Simply Love transitions).
    let mut fl_size = (fl + cropleft.min(0.0)).max(0.0);
    let mut fr_size = (fr + cropright.min(0.0)).max(0.0);
    let mut ft_size = (ft + croptop.min(0.0)).max(0.0);
    let mut fb_size = (fb + cropbottom.min(0.0)).max(0.0);

    let sum_x = fl_size + fr_size;
    if sum_x > 0.0 && sx_crop < sum_x {
        let s = sx_crop / sum_x;
        fl_size *= s;
        fr_size *= s;
    }

    let sum_y = ft_size + fb_size;
    if sum_y > 0.0 && sy_crop < sum_y {
        let s = sy_crop / sum_y;
        ft_size *= s;
        fb_size *= s;
    }

    let mut fl_eff = (fl_size / sx_crop).clamp(0.0, 1.0);
    let mut fr_eff = (fr_size / sx_crop).clamp(0.0, 1.0);
    let mut ft_eff = (ft_size / sy_crop).clamp(0.0, 1.0);
    let mut fb_eff = (fb_size / sy_crop).clamp(0.0, 1.0);

    if flip_x {
        std::mem::swap(&mut fl_eff, &mut fr_eff);
    }
    if flip_y {
        std::mem::swap(&mut ft_eff, &mut fb_eff);
    }

    // Matrix = T * R * S
    // SM->world flips Y, so rotationx sign flips; rotationy/z keep sign.
    let transform = {
        let rx = Matrix4::from_angle_x(Rad((-rot_x_deg).to_radians()));
        let ry = Matrix4::from_angle_y(Rad(rot_y_deg.to_radians()));
        let rz = Matrix4::from_angle_z(Rad(rot_z_deg.to_radians()));
        let r = rx * ry * rz;
        let s = Matrix4::from_nonuniform_scale(size_x, size_y, 1.0);
        let t = Matrix4::from_translation(Vector3::new(center_x, center_y, 0.0));
        t * r * s
    };

    let final_texture_id = if is_solid {
        std::borrow::Cow::Borrowed("__white")
    } else {
        std::borrow::Cow::Borrowed(texture_id)
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
        camera,
    });
}

#[inline(always)]
#[must_use]
const fn clamp_crop_fractions(l: f32, r: f32, t: f32, b: f32) -> (f32, f32, f32, f32) {
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
const fn quantize_up_even_i32(v: i32) -> i32 {
    if v <= 0 {
        0
    } else if (v & 1) != 0 {
        v + 1
    } else {
        v
    }
}

fn layout_text<'a>(
    font: &font::Font,
    fonts: &std::collections::HashMap<&'static str, font::Font>,
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
) -> Vec<RenderObject<'a>> {
    if text.is_empty() {
        return vec![];
    }
    // Optimization: Avoid allocating Vec for lines; iterate twice.
    let mut line_iter_check = text.lines();
    if line_iter_check.next().is_none() {
        return vec![];
    }

    #[inline(always)]
    fn advance_logical(glyph: &font::Glyph) -> i32 {
        lrint_ties_even(glyph.advance) as i32
    }

    // 1) Logical (integer) widths like SM: sum integer advances (default glyph if unmapped).
    let logical_line_widths: Vec<i32> = text
        .lines()
        .map(|line| {
            line.chars()
                .map(|c| font::find_glyph(font, c, fonts).map_or(0, advance_logical))
                .sum()
        })
        .collect();

    let max_logical_width_i = logical_line_widths.iter().copied().max().unwrap_or(0);
    let block_w_logical_even = quantize_up_even_i32(max_logical_width_i) as f32;

    // 2) Unscaled block cap height + line spacing in logical units
    let cap_height = if font.height > 0 {
        font.height as f32
    } else {
        font.line_spacing as f32
    };

    let num_lines = text.lines().count();
    let block_h_logical_i = if num_lines > 1 {
        font.height + ((num_lines - 1) as i32 * font.line_spacing)
    } else {
        font.height
    };
    let block_h_logical = if block_h_logical_i > 0 {
        block_h_logical_i as f32
    } else {
        cap_height
    };

    // 3) Fit scaling (zoomto...) preserves aspect ratio
    let s_w_fit = fit_width.map_or(f32::INFINITY, |w| {
        if block_w_logical_even > 0.0 {
            w / block_w_logical_even
        } else {
            1.0
        }
    });
    let s_h_fit = fit_height.map_or(f32::INFINITY, |h| {
        if block_h_logical > 0.0 {
            h / block_h_logical
        } else {
            1.0
        }
    });
    let fit_s = if s_w_fit.is_infinite() && s_h_fit.is_infinite() {
        1.0
    } else {
        s_w_fit.min(s_h_fit).max(0.0)
    };

    // 4) Reference sizes before/after zoom (but before max clamp)
    let width_before_zoom = block_w_logical_even * fit_s;
    let height_before_zoom = block_h_logical * fit_s;

    let width_after_zoom = width_before_zoom * scale[0];
    let height_after_zoom = height_before_zoom * scale[1];

    // 5) Decide the clamp denominators per axis based on order flags
    let denom_w_for_max = if max_w_pre_zoom {
        width_before_zoom
    } else {
        width_after_zoom
    };
    let denom_h_for_max = if max_h_pre_zoom {
        height_before_zoom
    } else {
        height_after_zoom
    };

    // 6) Compute per-axis extra downscale from max constraints
    let max_s_w = max_width.map_or(1.0, |mw| {
        if denom_w_for_max > mw {
            (mw / denom_w_for_max).max(0.0)
        } else {
            1.0
        }
    });
    let max_s_h = max_height.map_or(1.0, |mh| {
        if denom_h_for_max > mh {
            (mh / denom_h_for_max).max(0.0)
        } else {
            1.0
        }
    });

    // 7) Final per-axis scales: fit * zoom * (potential extra downscale)
    let sx = scale[0] * fit_s * max_s_w;
    let sy = scale[1] * fit_s * max_s_h;
    if sx.abs() < 1e-6 || sy.abs() < 1e-6 {
        return vec![];
    }

    // 8) Pixel rounding/snapping
    let block_w_px = block_w_logical_even * sx;
    let block_h_px = block_h_logical * sy;

    // 9) Place the block, compute baseline (unchanged)
    let block_left_sm = align[0].mul_add(-block_w_px, parent.x + offset[0]);
    let block_top_sm = align[1].mul_add(-block_h_px, parent.y + offset[1]);
    let block_center_x = 0.5f32.mul_add(block_w_px, block_left_sm);
    let block_center_y = 0.5f32.mul_add(block_h_px, block_top_sm);

    let mut pen_y_logical = lrint_ties_even(-(block_h_logical_i as f32) * 0.5) as i32;
    let line_padding = font.line_spacing - font.height;

    #[inline(always)]
    fn start_x_logical(align: actors::TextAlign, block_w_logical: f32, line_w_logical: f32) -> i32 {
        let align_value = match align {
            actors::TextAlign::Left => 0.0,
            actors::TextAlign::Center => 0.5,
            actors::TextAlign::Right => 1.0,
        };
        let start = (-0.5f32).mul_add(
            block_w_logical,
            align_value * (block_w_logical - line_w_logical),
        );
        lrint_ties_even(start) as i32
    }

    #[inline(always)]
    fn logical_to_world(center: f32, logical: f32, scale: f32) -> f32 {
        logical.mul_add(scale, center)
    }

    // Optimization: Use linear scan on simple vec instead of HashMap for texture dims cache
    let mut dims_cache: Vec<(&str, (f32, f32))> = Vec::with_capacity(4);

    let mut objects = Vec::new();

    for (i, line) in text.lines().enumerate() {
        pen_y_logical += font.height;
        let baseline_local_logical = pen_y_logical as f32;

        let line_w_logical = logical_line_widths[i] as f32;
        let mut pen_x_logical = start_x_logical(text_align, block_w_logical_even, line_w_logical);

        for ch in line.chars() {
            let glyph = match font::find_glyph(font, ch, fonts) {
                Some(g) => g,
                None => continue,
            };

            let quad_w = glyph.size[0] * sx;
            let quad_h = glyph.size[1] * sy;

            let draw_quad = !(ch == ' ' && !font.glyph_map.contains_key(&ch));
            if draw_quad && quad_w.abs() >= 1e-6 && quad_h.abs() >= 1e-6 {
                let quad_x_logical = pen_x_logical as f32 + glyph.offset[0];
                let quad_y_logical = baseline_local_logical + glyph.offset[1];

                let quad_x_sm = logical_to_world(block_center_x, quad_x_logical, sx);
                let quad_y_sm = logical_to_world(block_center_y, quad_y_logical, sy);

                let center_x = m.left + quad_x_sm + quad_w * 0.5;
                let center_y = m.top - (quad_y_sm + quad_h * 0.5);

                // Optimization: T * S manually
                // c0 = [w, 0, 0, 0]
                // c1 = [0, h, 0, 0]
                // c2 = [0, 0, 1, 0]
                // c3 = [tx, ty, 0, 1]
                let transform = Matrix4::new(
                    quad_w, 0.0, 0.0, 0.0, 0.0, quad_h, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, center_x,
                    center_y, 0.0, 1.0,
                );

                // Inline atlas_dims with linear scan
                let (tex_w, tex_h) = {
                    let key = &glyph.texture_key;
                    if let Some(&(_, d)) = dims_cache.iter().find(|(k, _)| k == key) {
                        d
                    } else {
                        let d = assets::texture_dims(key)
                            .map_or((1.0_f32, 1.0_f32), |meta| (meta.w as f32, meta.h as f32));
                        if dims_cache.len() < 8 {
                            dims_cache.push((key, d));
                        }
                        d
                    }
                };

                let uv_scale = [
                    (glyph.tex_rect[2] - glyph.tex_rect[0]) / tex_w,
                    (glyph.tex_rect[3] - glyph.tex_rect[1]) / tex_h,
                ];
                let uv_offset = [glyph.tex_rect[0] / tex_w, glyph.tex_rect[1] / tex_h];

                objects.push(RenderObject {
                    object_type: renderer::ObjectType::Sprite {
                        texture_id: std::borrow::Cow::Owned(glyph.texture_key.clone()),
                        tint: [1.0; 4],
                        uv_scale,
                        uv_offset,
                        edge_fade: [0.0; 4],
                    },
                    transform,
                    blend: BlendMode::Alpha,
                    z: 0,
                    order: 0,
                    camera: 0,
                });
            }

            pen_x_logical += advance_logical(glyph);
        }
        pen_y_logical += line_padding;
    }

    objects
}

#[inline(always)]
fn sm_rect_to_world_center_size(rect: SmRect, m: &Metrics) -> (Vector2<f32>, Vector2<f32>) {
    (
        Vector2::new(
            0.5f32.mul_add(rect.w, m.left + rect.x),
            m.top - 0.5f32.mul_add(rect.h, rect.y),
        ),
        Vector2::new(rect.w, rect.h),
    )
}

#[derive(Clone, Copy, Debug)]
struct WorldRect {
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
}

#[inline(always)]
fn sm_rect_to_world_edges(rect: SmRect, m: &Metrics) -> WorldRect {
    let left = m.left + rect.x;
    let right = rect.w.mul_add(1.0, left);

    let top = m.top - rect.y;
    let bottom = top - rect.h;

    WorldRect {
        left,
        right,
        bottom,
        top,
    }
}

fn clip_objects_to_world_rect(objects: &mut Vec<RenderObject<'_>>, clip: WorldRect) {
    if clip.left >= clip.right || clip.bottom >= clip.top {
        objects.clear();
        return;
    }

    let mut out = Vec::with_capacity(objects.len());
    for mut obj in objects.drain(..) {
        if clip_sprite_object_to_world_rect(&mut obj, clip) {
            out.push(obj);
        }
    }
    *objects = out;
}

fn clip_sprite_object_to_world_rect(obj: &mut RenderObject<'_>, clip: WorldRect) -> bool {
    let renderer::ObjectType::Sprite {
        uv_scale,
        uv_offset,
        ..
    } = &mut obj.object_type
    else {
        // Only sprite objects support clip-by-adjusting-UV today.
        return true;
    };

    let eps = 1e-6;
    let t = &obj.transform;
    if t.x.y.abs() > eps || t.y.x.abs() > eps || t.x.z.abs() > eps || t.y.z.abs() > eps {
        return true;
    }

    let w = t.x.x;
    let h = t.y.y;
    if w <= eps || h <= eps {
        return false;
    }

    let cx = t.w.x;
    let cy = t.w.y;

    let half_w = w * 0.5;
    let half_h = h * 0.5;

    let left = cx - half_w;
    let right = cx + half_w;
    let bottom = cy - half_h;
    let top = cy + half_h;

    let inter_left = left.max(clip.left);
    let inter_right = right.min(clip.right);
    let inter_bottom = bottom.max(clip.bottom);
    let inter_top = top.min(clip.top);
    if inter_left >= inter_right || inter_bottom >= inter_top {
        return false;
    }

    let inv_w = 1.0 / w;
    let inv_h = 1.0 / h;

    let cl = ((inter_left - left) * inv_w).clamp(0.0, 1.0);
    let cr = ((right - inter_right) * inv_w).clamp(0.0, 1.0);
    let cb = ((inter_bottom - bottom) * inv_h).clamp(0.0, 1.0);
    let ct = ((top - inter_top) * inv_h).clamp(0.0, 1.0);

    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= eps || sy_crop <= eps {
        return false;
    }

    uv_offset[0] += uv_scale[0] * cl;
    uv_offset[1] += uv_scale[1] * ct;
    uv_scale[0] *= sx_crop;
    uv_scale[1] *= sy_crop;

    let center_x = ((cl - cr) * w).mul_add(0.5, cx);
    let center_y = ((cb - ct) * h).mul_add(0.5, cy);
    let new_w = w * sx_crop;
    let new_h = h * sy_crop;

    obj.transform = Matrix4::new(
        new_w, 0.0, 0.0, 0.0, //
        0.0, new_h, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        center_x, center_y, 0.0, 1.0,
    );

    true
}
