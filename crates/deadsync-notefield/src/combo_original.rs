// Frozen from 7becb497857a0296ea923f4b835b1eea2687d4ad.
use super::*;
pub(crate) fn compose_combo_milestones(
    draws: &mut Vec<FlatDraw>,
    request: &ComboFeedbackRequest<'_>,
) {
    if !request.show {
        return;
    }
    let zoom_mod = combo_actor_zoom(request.mini);
    if let Some(assets) = request.milestone_assets {
        for milestone in request.milestones {
            match milestone.kind {
                ComboMilestoneKind::Hundred => {
                    append_hundred(draws, request, assets, milestone.elapsed, zoom_mod);
                }
                ComboMilestoneKind::Thousand => {
                    append_thousand(draws, request, assets, milestone.elapsed, zoom_mod);
                }
            }
        }
    }
}

pub(crate) fn compose_combo_number(draws: &mut Vec<FlatDraw>, request: &ComboFeedbackRequest<'_>) {
    if request.show {
        append_combo_number(draws, request, combo_actor_zoom(request.mini));
    }
}

pub(super) fn append_hundred(
    draws: &mut Vec<FlatDraw>,
    request: &ComboFeedbackRequest<'_>,
    assets: &ComboMilestoneAssets,
    elapsed: f32,
    zoom_mod: f32,
) {
    let style = request.style;
    if elapsed <= style.burst_duration {
        let progress = (elapsed / style.burst_duration).clamp(0.0, 1.0);
        let zoom = lerp(style.burst_start_zoom, style.burst_end_zoom, progress) * zoom_mod;
        let alpha = style.burst_start_alpha * (1.0 - progress);
        for direction in [1.0_f32, -1.0] {
            append_sprite(
                draws,
                &assets.burst,
                request.milestone_xy,
                [zoom, zoom],
                style.burst_rotation_deg * direction * progress,
                [1.0, 1.0, 1.0, alpha],
                style.milestone_z,
            );
        }
    }

    if elapsed > COMBO_HUNDRED_MILESTONE_DURATION {
        return;
    }
    let progress = (elapsed / COMBO_HUNDRED_MILESTONE_DURATION).clamp(0.0, 1.0);
    let eased = ease_out_quad(progress);
    let zoom = lerp(style.hundred_start_zoom, style.hundred_end_zoom, eased)
        * zoom_mod
        * assets.hundred.zoom_scale;
    append_sprite(
        draws,
        &assets.hundred,
        request.milestone_xy,
        [zoom, zoom],
        style.hundred_start_rotation_deg * (1.0 - eased),
        with_alpha(
            request.player_color,
            style.hundred_start_alpha * (1.0 - eased),
        ),
        style.milestone_z,
    );

    if elapsed > style.mini_duration {
        return;
    }
    let progress = (elapsed / style.mini_duration).clamp(0.0, 1.0);
    let zoom = lerp(style.mini_start_zoom, style.mini_end_zoom, progress)
        * zoom_mod
        * assets.hundred_mini.zoom_scale;
    append_sprite(
        draws,
        &assets.hundred_mini,
        request.milestone_xy,
        [zoom, zoom],
        style.mini_start_rotation_deg * (1.0 - progress),
        with_alpha(
            request.player_color,
            style.mini_start_alpha * (1.0 - progress),
        ),
        style.milestone_z,
    );
}

fn append_thousand(
    draws: &mut Vec<FlatDraw>,
    request: &ComboFeedbackRequest<'_>,
    assets: &ComboMilestoneAssets,
    elapsed: f32,
    zoom_mod: f32,
) {
    if elapsed > COMBO_THOUSAND_MILESTONE_DURATION {
        return;
    }
    let style = request.style;
    let progress = (elapsed / COMBO_THOUSAND_MILESTONE_DURATION).clamp(0.0, 1.0);
    let zoom = lerp(style.thousand_start_zoom, style.thousand_end_zoom, progress)
        * zoom_mod
        * assets.thousand.zoom_scale;
    let alpha = style.thousand_start_alpha * (1.0 - progress);
    let x_offset = style.thousand_x_travel * progress * zoom_mod;
    for direction in [1.0_f32, -1.0] {
        append_sprite(
            draws,
            &assets.thousand,
            [
                request.milestone_xy[0] + x_offset * direction,
                request.milestone_xy[1],
            ],
            [zoom * direction, zoom],
            0.0,
            with_alpha(request.player_color, alpha),
            style.milestone_z,
        );
    }
}

fn append_combo_number(
    draws: &mut Vec<FlatDraw>,
    request: &ComboFeedbackRequest<'_>,
    zoom_mod: f32,
) {
    let Some(font) = request.font else { return };
    let (value, color) = if request.miss_combo >= request.style.threshold {
        (request.miss_combo, request.style.miss_color)
    } else if request.combo >= request.style.threshold {
        (request.combo, request.combo_color)
    } else {
        return;
    };

    let scale = request.style.number_zoom * zoom_mod;
    draws.push(FlatDraw::PreparedU32(FlatPreparedU32 {
        align: [0.5, 0.5],
        offset: request.number_xy,
        color,
        font,
        text: InlineU32Text::new(value),
        slot: request.number_text_slot,
        align_text: TextAlign::Center,
        z: request.style.number_z,
        scale: [scale, scale],
        blend: BlendMode::Alpha,
        shadow_len: [request.style.shadow_len, -request.style.shadow_len],
        shadow_color: [0.0, 0.0, 0.0, 0.5],
    }));
}

fn append_sprite(
    draws: &mut Vec<FlatDraw>,
    sprite: &ComboMilestoneSprite,
    xy: [f32; 2],
    zoom: [f32; 2],
    rotation_deg: f32,
    color: [f32; 4],
    z: i16,
) {
    draws.push(FlatDraw::Sprite(FlatSprite {
        center: xy,
        world_z: 0.0,
        size: [
            sprite.native_size[0] * zoom[0].abs(),
            sprite.native_size[1] * zoom[1].abs(),
        ],
        source: sprite.source.clone(),
        tint: color,
        glow: [1.0, 1.0, 1.0, 0.0],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        flip_x: zoom[0].is_sign_negative(),
        flip_y: zoom[1].is_sign_negative(),
        fade: [0.0; 4],
        blend: BlendMode::Add,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg: rotation_deg,
        z,
    }));
}

fn ease_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (1.0 - t).mul_add(-(1.0 - t), 1.0)
}

fn lerp(start: f32, end: f32, t: f32) -> f32 {
    (end - start).mul_add(t, start)
}

const fn with_alpha(color: [f32; 4], alpha: f32) -> [f32; 4] {
    [color[0], color[1], color[2], alpha]
}
