use crate::combo_actor_zoom;
use deadlib_present::actors::{
    FlatDraw, FlatPreparedInline, FlatPreparedU32, FlatSprite, InlineText, InlineU32Text,
    SpriteSource, TextAlign,
};
use deadlib_render_core::BlendMode;
use deadsync_core::hud_style::{ComboFeedbackStyle, FieldFeedbackStyle};
use deadsync_gameplay::{
    ActiveComboMilestone, COMBO_HUNDRED_MILESTONE_DURATION, COMBO_THOUSAND_MILESTONE_DURATION,
    ComboMilestoneKind,
};
#[cfg(test)]
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ComboMilestoneSprite {
    pub source: SpriteSource,
    pub native_size: [f32; 2],
    pub zoom_scale: f32,
}

#[derive(Clone, Debug)]
pub struct ComboMilestoneAssets {
    pub burst: ComboMilestoneSprite,
    pub hundred: ComboMilestoneSprite,
    pub hundred_mini: ComboMilestoneSprite,
    pub thousand: ComboMilestoneSprite,
}

pub(crate) struct ComboFeedbackRequest<'a> {
    pub style: ComboFeedbackStyle,
    pub field_feedback: Option<&'static FieldFeedbackStyle>,
    pub show: bool,
    pub milestone_assets: Option<&'a ComboMilestoneAssets>,
    pub milestones: &'a [ActiveComboMilestone],
    pub combo: u32,
    pub miss_combo: u32,
    pub number_xy: [f32; 2],
    pub milestone_xy: [f32; 2],
    pub mini: f32,
    pub player_color: [f32; 4],
    pub combo_color: [f32; 4],
    pub font: Option<&'static str>,
    pub number_text_slot: u8,
    pub label_text_slot: u8,
    pub recent_tap_age: Option<f32>,
}

/// Compose canonical hundred/thousand milestone feedback as resolved sprites.
pub(crate) fn compose_combo_milestones(
    draws: &mut Vec<FlatDraw>,
    request: &ComboFeedbackRequest<'_>,
) {
    if request
        .field_feedback
        .and_then(|style| style.combo_artwork)
        .is_some_and(|artwork| !artwork.show_milestones)
    {
        return;
    }
    if !request.show {
        return;
    }
    let Some(assets) = request.milestone_assets else {
        return;
    };
    if request.milestones.is_empty() {
        return;
    }
    let zoom_mod = combo_actor_zoom(request.mini);
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

/// Compose the prepared numeric half after the milestone sprites.
#[inline(always)]
pub(crate) fn compose_combo_number(draws: &mut Vec<FlatDraw>, request: &ComboFeedbackRequest<'_>) {
    if !request.show {
        return;
    }
    let Some(mut font) = request.font else { return };
    if request
        .field_feedback
        .and_then(|style| style.combo_artwork)
        .is_some_and(|artwork| !artwork.show_miss_combo)
        && request.miss_combo >= request.style.threshold
    {
        return;
    }
    let (value, mut color) = if request.miss_combo >= request.style.threshold {
        (request.miss_combo, request.style.miss_color)
    } else if request.combo >= request.style.threshold {
        (request.combo, request.combo_color)
    } else {
        return;
    };

    let mini = combo_actor_zoom(request.mini);
    let mut scale = request.style.number_zoom * mini;
    let mut align = [0.5, 0.5];
    let mut number_xy = request.number_xy;
    let mut number_shadow = request.style.shadow_len;
    if let Some(style) = request.field_feedback {
        let pulse = request.recent_tap_age.map_or(1.0, |age| {
            let progress = (age / style.combo_pulse_duration).clamp(0.0, 1.0);
            (style.combo_pulse_scale - 1.0).mul_add(1.0 - progress, 1.0)
        });
        scale = (value.min(style.combo_number_zoom_limit) as f32)
            .mul_add(style.combo_number_zoom_step, style.combo_number_zoom)
            * mini
            * pulse;
        align[1] = 1.0;
        if let Some(artwork) = style.combo_artwork {
            number_shadow = artwork.shadow_len;
            font = artwork.number_font;
            color = artwork.tint;
            align = artwork.number_align;
            for (axis, offset) in number_xy.iter_mut().zip(artwork.number_offset) {
                *axis += offset * mini;
            }
            let zoom = mini * pulse;
            let size = artwork.label_size.map(|axis| axis * zoom);
            let center = std::array::from_fn(|axis| {
                request.number_xy[axis]
                    + artwork.label_offset[axis] * mini
                    + (0.5 - artwork.label_align[axis]) * size[axis]
            });
            let label = FlatSprite {
                center,
                world_z: 0.0,
                size,
                source: SpriteSource::TextureStatic(artwork.label_texture),
                tint: artwork.tint,
                vertex_color_encoding: Default::default(),
                glow: [0.0; 4],
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                flip_x: false,
                flip_y: false,
                fade: [0.0; 4],
                blend: BlendMode::Alpha,
                rot_x_deg: 0.0,
                rot_y_deg: 0.0,
                rot_z_deg: 0.0,
                z: request.style.number_z,
            };
            if artwork.shadow_len > 0.0 {
                let mut shadow = label.clone();
                shadow.center[0] += artwork.shadow_len;
                shadow.center[1] += artwork.shadow_len;
                shadow.tint = [0.0, 0.0, 0.0, 0.5];
                shadow.z -= 1;
                draws.push(FlatDraw::Sprite(shadow));
            }
            draws.push(FlatDraw::Sprite(label));
        } else if let Some(text) = InlineText::copy_from(style.combo_label) {
            draws.push(FlatDraw::PreparedInline(FlatPreparedInline {
                align: [0.5, 0.0],
                offset: [
                    request.number_xy[0],
                    request.number_xy[1] + style.combo_label_y,
                ],
                color,
                font: style.combo_label_font,
                text,
                slot: request.label_text_slot,
                align_text: TextAlign::Center,
                z: request.style.number_z,
                scale: [style.combo_label_zoom * mini * pulse; 2],
                blend: BlendMode::Alpha,
                shadow_len: [request.style.shadow_len, -request.style.shadow_len],
                shadow_color: [0.0, 0.0, 0.0, 0.5],
            }));
        }
    }
    draws.push(FlatDraw::PreparedU32(FlatPreparedU32 {
        align,
        offset: number_xy,
        color,
        font,
        text: InlineU32Text::new(value),
        slot: request.number_text_slot,
        align_text: TextAlign::Center,
        z: request.style.number_z,
        scale: [scale, scale],
        blend: BlendMode::Alpha,
        shadow_len: [number_shadow, -number_shadow],
        shadow_color: [0.0, 0.0, 0.0, 0.5],
    }));
}

fn append_hundred(
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
        vertex_color_encoding: Default::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> ComboFeedbackStyle {
        ComboFeedbackStyle {
            threshold: 4,
            milestone_z: 89,
            number_z: 90,
            number_zoom: 0.75,
            shadow_len: 1.0,
            miss_color: [1.0, 0.0, 0.0, 1.0],
            burst_duration: 0.5,
            burst_start_zoom: 2.0,
            burst_end_zoom: 1.0,
            burst_start_alpha: 0.5,
            burst_rotation_deg: 90.0,
            hundred_start_zoom: 0.25,
            hundred_end_zoom: 2.0,
            hundred_start_alpha: 0.6,
            hundred_start_rotation_deg: 10.0,
            mini_duration: 0.4,
            mini_start_zoom: 0.25,
            mini_end_zoom: 1.8,
            mini_start_alpha: 1.0,
            mini_start_rotation_deg: 10.0,
            thousand_start_zoom: 0.25,
            thousand_end_zoom: 3.0,
            thousand_start_alpha: 0.7,
            thousand_x_travel: 100.0,
        }
    }

    fn source(name: &str) -> SpriteSource {
        SpriteSource::Texture(Arc::from(name))
    }

    fn sprite(name: &str, native_size: [f32; 2], zoom_scale: f32) -> ComboMilestoneSprite {
        ComboMilestoneSprite {
            source: source(name),
            native_size,
            zoom_scale,
        }
    }

    fn assets() -> ComboMilestoneAssets {
        ComboMilestoneAssets {
            burst: sprite("burst", [10.0, 20.0], 1.0),
            hundred: sprite("hundred", [30.0, 40.0], 2.0),
            hundred_mini: sprite("hundred-mini", [50.0, 60.0], 3.0),
            thousand: sprite("thousand", [70.0, 80.0], 1.5),
        }
    }

    fn request<'a>(
        milestones: &'a [ActiveComboMilestone],
        assets: Option<&'a ComboMilestoneAssets>,
    ) -> ComboFeedbackRequest<'a> {
        ComboFeedbackRequest {
            style: style(),
            field_feedback: None,
            show: true,
            milestone_assets: assets,
            milestones,
            combo: 0,
            miss_combo: 0,
            number_xy: [310.0, 265.0],
            milestone_xy: [320.0, 265.0],
            mini: 0.0,
            player_color: [0.2, 0.4, 0.8, 0.25],
            combo_color: [0.1, 0.8, 0.3, 0.9],
            font: Some("combo-font"),
            number_text_slot: 0,
            label_text_slot: 0,
            recent_tap_age: None,
        }
    }

    fn compose_feedback(draws: &mut Vec<FlatDraw>, request: ComboFeedbackRequest<'_>) {
        compose_combo_milestones(draws, &request);
        compose_combo_number(draws, &request);
    }

    #[test]
    fn bitmap_combo_style_replaces_text_font_and_optional_milestones() {
        use deadsync_core::hud_style::ComboArtworkStyle;
        static FEEDBACK: FieldFeedbackStyle = FieldFeedbackStyle {
            combo_artwork: Some(&ComboArtworkStyle {
                number_font: "fixture-number-font",
                number_offset: [0.0, 20.0],
                number_align: [1.0, 1.0],
                label_texture: "fixture-combo-label",
                label_size: [96.0, 32.0],
                label_offset: [6.0, 20.0],
                label_align: [0.0, 1.0],
                tint: [1.0; 4],
                show_miss_combo: false,
                show_milestones: false,
                shadow_len: 4.0,
            }),
            judgment_y: [0.0; 2],
            judgment_centered_add: [0.0; 2],
            combo_y: [0.0; 2],
            combo_centered_add: [0.0; 2],
            combo_label: "old label",
            combo_label_font: "old font",
            combo_label_y: 0.0,
            combo_number_zoom: 0.5,
            combo_number_zoom_step: 0.0,
            combo_number_zoom_limit: 0,
            combo_label_zoom: 1.0,
            combo_pulse_duration: 0.05,
            combo_pulse_scale: 1.2,
        };
        let assets = assets();
        let milestones = [ActiveComboMilestone {
            kind: ComboMilestoneKind::Hundred,
            elapsed: 0.1,
        }];
        let mut req = request(&milestones, Some(&assets));
        req.field_feedback = Some(&FEEDBACK);
        req.combo = 100;
        let mut draws = Vec::new();
        compose_feedback(&mut draws, req);
        assert_eq!(draws.len(), 3);
        assert!(
            matches!(&draws[1], FlatDraw::Sprite(sprite) if sprite.source.texture_key() == Some("fixture-combo-label") && sprite.blend == BlendMode::Alpha)
        );
        assert!(
            matches!(&draws[2], FlatDraw::PreparedU32(number) if number.font == "fixture-number-font" && number.align == [1.0, 1.0] && number.offset == [310.0, 285.0] && number.color == [1.0; 4])
        );
        let mut missed = request(&milestones, Some(&assets));
        missed.field_feedback = Some(&FEEDBACK);
        missed.miss_combo = 5;
        draws.clear();
        compose_feedback(&mut draws, missed);
        assert!(draws.is_empty());
    }

    #[test]
    fn hidden_and_below_threshold_feedback_emit_nothing() {
        let milestone_assets = assets();
        let milestones = [ActiveComboMilestone {
            kind: ComboMilestoneKind::Hundred,
            elapsed: 0.0,
        }];
        let mut hidden = request(&milestones, Some(&milestone_assets));
        hidden.show = false;
        let mut draws = Vec::new();
        compose_feedback(&mut draws, hidden);
        assert!(draws.is_empty());

        let mut below = request(&[], None);
        below.combo = 3;
        compose_feedback(&mut draws, below);
        assert!(draws.is_empty());
    }

    #[test]
    fn miss_combo_precedes_normal_combo_number() {
        let mut request = request(&[], None);
        request.combo = 120;
        request.miss_combo = 4;
        let mut draws = Vec::new();
        compose_feedback(&mut draws, request);

        assert_eq!(draws.len(), 1);
        match &draws[0] {
            FlatDraw::PreparedU32(FlatPreparedU32 {
                align,
                offset,
                color,
                font,
                text,
                align_text,
                z,
                scale,
                shadow_len,
                ..
            }) => {
                assert_eq!(*align, [0.5, 0.5]);
                assert_eq!(*offset, [310.0, 265.0]);
                assert_eq!(*color, [1.0, 0.0, 0.0, 1.0]);
                assert_eq!(*font, "combo-font");
                assert_eq!(text.as_str(), "4");
                assert_eq!(*align_text, TextAlign::Center);
                assert_eq!(*z, 90);
                assert_eq!(*scale, [0.75, 0.75]);
                assert_eq!(*shadow_len, [1.0, -1.0]);
            }
            other => panic!("expected prepared combo text, got {other:?}"),
        }
    }

    #[test]
    fn normal_combo_uses_resolved_color() {
        let mut request = request(&[], None);
        request.combo = 10;
        let mut draws = Vec::new();
        compose_feedback(&mut draws, request);

        match &draws[0] {
            FlatDraw::PreparedU32(FlatPreparedU32 { color, text, .. }) => {
                assert_eq!(*color, [0.1, 0.8, 0.3, 0.9]);
                assert_eq!(text.as_str(), "10");
            }
            other => panic!("expected prepared combo text, got {other:?}"),
        }
    }

    #[test]
    fn hundred_milestone_draw_fingerprint_preserves_order() {
        let milestone_assets = assets();
        let milestones = [ActiveComboMilestone {
            kind: ComboMilestoneKind::Hundred,
            elapsed: 0.0,
        }];
        let mut request = request(&milestones, Some(&milestone_assets));
        request.font = None;
        let mut draws = Vec::new();
        compose_feedback(&mut draws, request);

        assert_eq!(draws.len(), 4);
        let expected = [
            ("burst", [20.0, 40.0], 0.0, [1.0, 1.0, 1.0, 0.5]),
            ("burst", [20.0, 40.0], -0.0, [1.0, 1.0, 1.0, 0.5]),
            ("hundred", [15.0, 20.0], 10.0, [0.2, 0.4, 0.8, 0.6]),
            ("hundred-mini", [37.5, 45.0], 10.0, [0.2, 0.4, 0.8, 1.0]),
        ];
        for (draw, (key, size, rotation, tint)) in draws.iter().zip(expected) {
            assert_sprite(draw, key, [320.0, 265.0], size, false, rotation, tint);
        }
    }

    #[test]
    fn thousand_milestone_mirrors_halfway_swooshes() {
        let milestone_assets = assets();
        let milestones = [ActiveComboMilestone {
            kind: ComboMilestoneKind::Thousand,
            elapsed: COMBO_THOUSAND_MILESTONE_DURATION * 0.5,
        }];
        let mut request = request(&milestones, Some(&milestone_assets));
        request.font = None;
        let mut draws = Vec::new();
        compose_feedback(&mut draws, request);

        assert_eq!(draws.len(), 2);
        let zoom = 1.625 * 1.5;
        assert_sprite(
            &draws[0],
            "thousand",
            [370.0, 265.0],
            [70.0 * zoom, 80.0 * zoom],
            false,
            0.0,
            [0.2, 0.4, 0.8, 0.35],
        );
        assert_sprite(
            &draws[1],
            "thousand",
            [270.0, 265.0],
            [70.0 * zoom, 80.0 * zoom],
            true,
            0.0,
            [0.2, 0.4, 0.8, 0.35],
        );
    }

    #[test]
    fn unique_active_milestones_emit_at_most_six_draws() {
        let milestone_assets = assets();
        let milestones = [
            ActiveComboMilestone {
                kind: ComboMilestoneKind::Hundred,
                elapsed: 0.0,
            },
            ActiveComboMilestone {
                kind: ComboMilestoneKind::Thousand,
                elapsed: 0.0,
            },
        ];
        let mut request = request(&milestones, Some(&milestone_assets));
        request.font = None;
        let mut draws = Vec::new();
        compose_feedback(&mut draws, request);

        assert_eq!(draws.len(), 6);
    }

    fn assert_sprite(
        draw: &FlatDraw,
        key: &str,
        center: [f32; 2],
        size: [f32; 2],
        flip_x: bool,
        rotation: f32,
        tint: [f32; 4],
    ) {
        match draw {
            FlatDraw::Sprite(FlatSprite {
                center: actual_center,
                size: actual_size,
                source,
                tint: actual_tint,
                z,
                rot_z_deg,
                blend,
                flip_x: actual_flip_x,
                uv_rect,
                glow,
                fade,
                ..
            }) => {
                assert_eq!(*actual_center, center);
                assert_eq!(*actual_size, size);
                assert_eq!(source.texture_key(), Some(key));
                assert_eq!(*actual_tint, tint);
                assert_eq!(*z, 89);
                assert_eq!(*actual_flip_x, flip_x);
                assert!((*rot_z_deg - rotation).abs() <= 1e-6);
                assert_eq!(*blend, BlendMode::Add);
                assert_eq!(*uv_rect, [0.0, 0.0, 1.0, 1.0]);
                assert_eq!(*glow, [1.0, 1.0, 1.0, 0.0]);
                assert_eq!(*fade, [0.0; 4]);
            }
            other => panic!("expected combo sprite draw, got {other:?}"),
        }
    }
}
