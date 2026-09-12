// Frozen from fd056aa05 (0.5.1168); implementation bodies are unchanged.
use super::*;

#[inline(always)]
fn song_lua_fold_x_around_pivot(x: f32, pivot_x: f32, cos_y: f32) -> f32 {
    (x - pivot_x).mul_add(cos_y, pivot_x)
}

pub fn song_lua_player_y_fold_actor(actor: Actor, pivot_x: f32, rotation_y_deg: f32) -> Actor {
    if !pivot_x.is_finite() || !rotation_y_deg.is_finite() || rotation_y_deg.abs() <= f32::EPSILON {
        return actor;
    }
    let cos_y = rotation_y_deg.to_radians().cos();
    match actor {
        Actor::Sprite {
            align,
            mut offset,
            world_z,
            size,
            source,
            tint,
            glow,
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
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            skew,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color,
            effect,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Sprite {
                align,
                offset,
                world_z,
                size,
                source,
                tint,
                glow,
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
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                blend,
                mask_source,
                mask_dest,
                rot_x_deg,
                rot_y_deg,
                rot_z_deg,
                skew,
                local_offset,
                local_offset_rot_sin_cos,
                texcoordvelocity,
                animate,
                state_delay,
                scale,
                shadow_len,
                shadow_color,
                effect,
            }
        }
        Actor::Text {
            align,
            mut offset,
            local_transform,
            color,
            stroke_color,
            glow,
            font,
            content,
            attributes,
            align_text,
            z,
            mut scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend,
            shadow_len,
            shadow_color,
            effect,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            scale[0] *= cos_y;
            Actor::Text {
                align,
                offset,
                local_transform,
                color,
                stroke_color,
                glow,
                font,
                content,
                attributes,
                align_text,
                z,
                scale,
                fit_width,
                fit_height,
                line_spacing,
                wrap_width_pixels,
                max_width,
                max_height,
                max_w_pre_zoom,
                max_h_pre_zoom,
                jitter,
                distortion,
                clip,
                mask_dest,
                blend,
                shadow_len,
                shadow_color,
                effect,
            }
        }
        Actor::Mesh {
            align,
            mut offset,
            size,
            tint,
            vertices,
            visible,
            blend,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Mesh {
                align,
                offset,
                size,
                tint,
                vertices,
                visible,
                blend,
                z,
            }
        }
        Actor::ReusableMesh {
            align,
            mut offset,
            size,
            tint,
            vertices,
            visible,
            blend,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::ReusableMesh {
                align,
                offset,
                size,
                tint,
                vertices,
                visible,
                blend,
                z,
            }
        }
        Actor::TexturedMesh {
            align,
            mut offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            glow,
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::TexturedMesh {
                align,
                offset,
                world_z,
                size,
                local_transform,
                texture,
                tint,
                glow,
                vertices,
                geom_cache_key,
                uv_scale,
                uv_offset,
                uv_tex_shift,
                depth_test,
                visible,
                blend,
                z,
            }
        }
        Actor::ReusableTexturedMesh {
            align,
            mut offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            glow,
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::ReusableTexturedMesh {
                align,
                offset,
                world_z,
                size,
                local_transform,
                texture,
                tint,
                glow,
                vertices,
                geom_cache_key,
                uv_scale,
                uv_offset,
                uv_tex_shift,
                depth_test,
                visible,
                blend,
                z,
            }
        }
        Actor::Frame {
            mut offset,
            children,
            align,
            size,
            background,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Frame {
                align,
                offset,
                size,
                children: children
                    .into_iter()
                    .map(|child| song_lua_player_y_fold_actor(child, pivot_x, rotation_y_deg))
                    .collect(),
                background,
                z,
            }
        }
        Actor::SharedFrame {
            mut offset,
            children,
            align,
            size,
            background,
            z,
            tint,
            blend,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::SharedFrame {
                align,
                offset,
                size,
                children,
                background,
                z,
                tint,
                blend,
            }
        }
        Actor::RetainedFrame {
            align,
            mut offset,
            size,
            frame,
            z,
            tint,
            blend,
            visible,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::RetainedFrame {
                align,
                offset,
                size,
                frame,
                z,
                tint,
                blend,
                visible,
            }
        }
        actor @ Actor::SharedTransform { .. } => actor,
        Actor::Camera {
            view_proj,
            children,
        } => Actor::Camera {
            view_proj,
            children: children
                .into_iter()
                .map(|child| song_lua_player_y_fold_actor(child, pivot_x, rotation_y_deg))
                .collect(),
        },
        Actor::CameraPush { view_proj } => Actor::CameraPush { view_proj },
        Actor::CameraPop => Actor::CameraPop,
        Actor::Shadow {
            len,
            color,
            mut child,
        } => {
            let actor = std::mem::replace(child.as_mut(), Actor::CameraPop);
            *child = song_lua_player_y_fold_actor(actor, pivot_x, rotation_y_deg);
            Actor::Shadow { len, color, child }
        }
    }
}

pub(super) fn song_lua_capture_tint(color: [f32; 4], tint: [f32; 4]) -> [f32; 4] {
    [
        color[0] * tint[0],
        color[1] * tint[1],
        color[2] * tint[2],
        color[3] * tint[3],
    ]
}

pub(super) fn song_lua_add_z(z: i16, delta: i16) -> i16 {
    (i32::from(z) + i32::from(delta)).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

pub(super) fn song_lua_style_capture_actor(
    actor: Actor,
    capture_tint: [f32; 4],
    blend: Option<BlendMode>,
    z_shift: i16,
) -> Actor {
    match actor {
        Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint: actor_tint,
            glow,
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
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend: actor_blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            skew,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color,
            effect,
        } => Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            glow: song_lua_capture_tint(glow, capture_tint),
            z: song_lua_add_z(z, z_shift),
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
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend: blend.unwrap_or(actor_blend),
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            skew,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color: song_lua_capture_tint(shadow_color, capture_tint),
            effect,
        },
        Actor::Text {
            align,
            offset,
            local_transform,
            color,
            stroke_color,
            glow,
            font,
            content,
            attributes,
            align_text,
            z,
            scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend: actor_blend,
            shadow_len,
            shadow_color,
            effect,
        } => Actor::Text {
            align,
            offset,
            local_transform,
            color: song_lua_capture_tint(color, capture_tint),
            stroke_color: stroke_color.map(|color| song_lua_capture_tint(color, capture_tint)),
            glow: song_lua_capture_tint(glow, capture_tint),
            font,
            content,
            attributes,
            align_text,
            z: song_lua_add_z(z, z_shift),
            scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend: blend.unwrap_or(actor_blend),
            shadow_len,
            shadow_color: song_lua_capture_tint(shadow_color, capture_tint),
            effect,
        },
        Actor::Mesh {
            align,
            offset,
            size,
            tint: actor_tint,
            vertices,
            visible,
            blend: actor_blend,
            z,
        } => Actor::Mesh {
            align,
            offset,
            size,
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            vertices,
            visible,
            blend: blend.unwrap_or(actor_blend),
            z: song_lua_add_z(z, z_shift),
        },
        Actor::ReusableMesh {
            align,
            offset,
            size,
            tint,
            vertices,
            visible,
            blend: actor_blend,
            z,
        } => Actor::ReusableMesh {
            align,
            offset,
            size,
            tint: song_lua_capture_tint(tint, capture_tint),
            vertices,
            visible,
            blend: blend.unwrap_or(actor_blend),
            z: song_lua_add_z(z, z_shift),
        },
        Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint: actor_tint,
            glow,
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend: actor_blend,
            z,
        } => Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            glow: song_lua_capture_tint(glow, capture_tint),
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend: blend.unwrap_or(actor_blend),
            z: song_lua_add_z(z, z_shift),
        },
        Actor::ReusableTexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint: actor_tint,
            glow,
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend: actor_blend,
            z,
        } => Actor::ReusableTexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            glow: song_lua_capture_tint(glow, capture_tint),
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend: blend.unwrap_or(actor_blend),
            z: song_lua_add_z(z, z_shift),
        },
        Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        } => Actor::Frame {
            align,
            offset,
            size,
            children: children
                .into_iter()
                .map(|child| song_lua_style_capture_actor(child, capture_tint, blend, z_shift))
                .collect(),
            background,
            z: song_lua_add_z(z, z_shift),
        },
        Actor::SharedFrame {
            align,
            offset,
            size,
            children,
            background,
            z,
            tint: actor_tint,
            blend: actor_blend,
        } => Actor::SharedFrame {
            align,
            offset,
            size,
            children,
            background,
            z: song_lua_add_z(z, z_shift),
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            blend: blend.or(actor_blend),
        },
        Actor::SharedTransform {
            transform,
            source_view_proj,
            children,
            z,
            tint: actor_tint,
            blend: actor_blend,
        } => Actor::SharedTransform {
            transform,
            source_view_proj,
            children,
            z: song_lua_add_z(z, z_shift),
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            blend: blend.or(actor_blend),
        },
        Actor::RetainedFrame {
            align,
            offset,
            size,
            frame,
            z,
            tint: actor_tint,
            blend: actor_blend,
            visible,
        } => Actor::RetainedFrame {
            align,
            offset,
            size,
            frame,
            z: song_lua_add_z(z, z_shift),
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            blend: blend.or(actor_blend),
            visible,
        },
        Actor::Camera {
            view_proj,
            children,
        } => Actor::Camera {
            view_proj,
            children: children
                .into_iter()
                .map(|child| song_lua_style_capture_actor(child, capture_tint, blend, z_shift))
                .collect(),
        },
        Actor::CameraPush { view_proj } => Actor::CameraPush { view_proj },
        Actor::CameraPop => Actor::CameraPop,
        Actor::Shadow {
            len,
            color,
            mut child,
        } => {
            let actor = std::mem::replace(child.as_mut(), Actor::CameraPop);
            *child = song_lua_style_capture_actor(actor, capture_tint, blend, z_shift);
            Actor::Shadow {
                len,
                color: song_lua_capture_tint(color, capture_tint),
                child,
            }
        }
    }
}

pub(super) fn song_lua_proxy_expand_retained(children: &mut Vec<Actor>) {
    let mut index = 0;
    while index < children.len() {
        let frame = match &children[index] {
            Actor::RetainedFrame {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                frame,
                z: 0,
                tint: [1.0, 1.0, 1.0, 1.0],
                blend: None,
                visible: true,
            } => Some(Arc::clone(frame)),
            _ => None,
        };
        let Some(frame) = frame else {
            index += 1;
            continue;
        };
        children.remove(index);
        for child in frame.children().iter().rev() {
            children.insert(index, child.clone());
        }
        // Inspect the inserted actors too in case a retained static fragment
        // contains another identity retained frame.
    }
}

pub(super) fn song_lua_proxy_local_children_in_place(children: &mut Vec<Actor>) {
    // Static gameplay fragments retain their authored absolute z values behind
    // an identity wrapper. ActorProxy establishes a new draw plane, so expose
    // those children before sorting and zeroing their local z values.
    song_lua_proxy_expand_retained(children);
    let mut run_start = 0;
    for index in 0..children.len() {
        if matches!(children[index], Actor::CameraPush { .. } | Actor::CameraPop) {
            song_lua_proxy_local_run(&mut children[run_start..index]);
            song_lua_proxy_zero_local_z(&mut children[index]);
            run_start = index + 1;
        }
    }
    song_lua_proxy_local_run(&mut children[run_start..]);
}

pub(super) fn song_lua_proxy_local_run(children: &mut [Actor]) {
    if children.len() <= 8 || children.len() > PLAYER_ACTOR_SCRATCH_CAPACITY {
        // Tiny and overflow runs avoid auxiliary setup while retaining stable
        // equal-z ordering and a hard zero-allocation fallback.
        for index in 1..children.len() {
            let z = song_lua_proxy_actor_z(&children[index]);
            let mut insert = index;
            while insert > 0 && song_lua_proxy_actor_z(&children[insert - 1]) > z {
                children.swap(insert - 1, insert);
                insert -= 1;
            }
        }
    } else {
        // Sort compact indices by (z, original position), then apply the
        // permutation to the large Actor values. This is stable and uses no
        // heap-backed merge buffer.
        let mut order = [0u16; PLAYER_ACTOR_SCRATCH_CAPACITY];
        let mut target = [0u16; PLAYER_ACTOR_SCRATCH_CAPACITY];
        for (index, slot) in order[..children.len()].iter_mut().enumerate() {
            *slot = index as u16;
        }
        order[..children.len()].sort_unstable_by_key(|&index| {
            (song_lua_proxy_actor_z(&children[index as usize]), index)
        });
        for (new_index, &old_index) in order[..children.len()].iter().enumerate() {
            target[old_index as usize] = new_index as u16;
        }
        for index in 0..children.len() {
            while target[index] as usize != index {
                let swap_index = target[index] as usize;
                children.swap(index, swap_index);
                target.swap(index, swap_index);
            }
        }
    }
    for child in children {
        song_lua_proxy_zero_local_z(child);
    }
}

pub(super) fn song_lua_proxy_zero_local_z(actor: &mut Actor) {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. }
        | Actor::RetainedFrame { z, .. } => *z = 0,
        Actor::Frame { z, children, .. } => {
            *z = 0;
            song_lua_proxy_local_children_in_place(children);
        }
        Actor::SharedFrame { z, children, .. } | Actor::SharedTransform { z, children, .. } => {
            *z = 0;
            *children = song_lua_proxy_source_segment_owned(children);
        }
        Actor::Camera { children, .. } => song_lua_proxy_local_children_in_place(children),
        Actor::Shadow { child, .. } => song_lua_proxy_zero_local_z(child),
        Actor::CameraPush { .. } | Actor::CameraPop => {}
    }
}

pub(super) fn song_lua_proxy_source_segment_owned(segment: &Arc<[Actor]>) -> Arc<[Actor]> {
    if !segment.iter().any(song_lua_proxy_actor_has_z) {
        return Arc::clone(segment);
    }
    let (offset, actors) = song_lua_proxy_segment_actors(segment);
    let mut children = Vec::with_capacity(actors.len());
    song_lua_proxy_local_children_into(actors.iter().cloned(), &mut children);
    if offset == [0.0, 0.0] {
        Arc::from(children)
    } else {
        Arc::from([Actor::Frame {
            align: [0.0, 0.0],
            offset,
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children,
            background: None,
            z: 0,
        }])
    }
}

pub(super) fn song_lua_proxy_segment_actors(segment: &[Actor]) -> ([f32; 2], &[Actor]) {
    let [
        Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        },
    ] = segment
    else {
        return ([0.0, 0.0], segment);
    };
    if *align == [0.0, 0.0]
        && matches!(*size, [SizeSpec::Fill, SizeSpec::Fill])
        && background.is_none()
        && *z == 0
    {
        (*offset, children)
    } else {
        ([0.0, 0.0], segment)
    }
}

pub(super) fn song_lua_proxy_actor_has_z(actor: &Actor) -> bool {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. } => *z != 0,
        Actor::Frame { z, children, .. } => {
            *z != 0 || children.iter().any(song_lua_proxy_actor_has_z)
        }
        Actor::SharedFrame { z, children, .. } | Actor::SharedTransform { z, children, .. } => {
            *z != 0 || children.iter().any(song_lua_proxy_actor_has_z)
        }
        Actor::RetainedFrame { z, frame, .. } => {
            *z != 0 || frame.children().iter().any(song_lua_proxy_actor_has_z)
        }
        Actor::Camera { children, .. } => children.iter().any(song_lua_proxy_actor_has_z),
        Actor::Shadow { child, .. } => song_lua_proxy_actor_has_z(child),
        Actor::CameraPush { .. } | Actor::CameraPop => false,
    }
}

pub(super) fn song_lua_proxy_actor_z(actor: &Actor) -> i16 {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. }
        | Actor::Frame { z, .. }
        | Actor::SharedFrame { z, .. }
        | Actor::SharedTransform { z, .. }
        | Actor::RetainedFrame { z, .. } => *z,
        Actor::Shadow { child, .. } => song_lua_proxy_actor_z(child),
        Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop => 0,
    }
}

pub(super) fn song_lua_proxy_local_children_into(
    children: impl Iterator<Item = Actor>,
    out: &mut Vec<Actor>,
) {
    out.extend(children);
    song_lua_proxy_local_children_in_place(out);
}
