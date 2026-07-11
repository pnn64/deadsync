use crate::combo_actor_zoom;
use deadlib_present::actors::{Actor, SpriteSource, TextAlign};
use deadlib_present::dsl::{SpriteBuilder, TextBuilder};
use deadlib_render::BlendMode;
use deadsync_gameplay::{
    ActiveComboMilestone, COMBO_HUNDRED_MILESTONE_DURATION, COMBO_THOUSAND_MILESTONE_DURATION,
    ComboMilestoneKind,
};
use deadsync_theme::ComboFeedbackStyle;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ComboMilestoneAssets {
    pub burst: SpriteSource,
    pub hundred: SpriteSource,
    pub hundred_mini: SpriteSource,
    pub thousand: SpriteSource,
    pub hundred_zoom_scale: f32,
    pub hundred_mini_zoom_scale: f32,
    pub thousand_zoom_scale: f32,
}

pub struct ComboFeedbackRequest<'a> {
    pub style: ComboFeedbackStyle,
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
    pub number_text: fn(u32) -> Arc<str>,
}

/// Compose canonical combo numbers and hundred/thousand milestone feedback.
/// The caller supplies theme-selected sprite sources, colors, and font while
/// the notefield owns timing, ordering, and actor transforms.
pub fn compose_combo_feedback(actors: &mut Vec<Actor>, request: ComboFeedbackRequest<'_>) {
    if !request.show {
        return;
    }
    let zoom_mod = combo_actor_zoom(request.mini);
    if let Some(assets) = request.milestone_assets {
        for milestone in request.milestones {
            match milestone.kind {
                ComboMilestoneKind::Hundred => {
                    append_hundred(actors, &request, assets, milestone.elapsed, zoom_mod);
                }
                ComboMilestoneKind::Thousand => {
                    append_thousand(actors, &request, assets, milestone.elapsed, zoom_mod);
                }
            }
        }
    }
    append_combo_number(actors, &request, zoom_mod);
}

fn append_hundred(
    actors: &mut Vec<Actor>,
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
                actors,
                assets.burst.clone(),
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
        * assets.hundred_zoom_scale;
    append_sprite(
        actors,
        assets.hundred.clone(),
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
        * assets.hundred_mini_zoom_scale;
    append_sprite(
        actors,
        assets.hundred_mini.clone(),
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
    actors: &mut Vec<Actor>,
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
        * assets.thousand_zoom_scale;
    let alpha = style.thousand_start_alpha * (1.0 - progress);
    let x_offset = style.thousand_x_travel * progress * zoom_mod;
    for direction in [1.0_f32, -1.0] {
        append_sprite(
            actors,
            assets.thousand.clone(),
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

fn append_combo_number(actors: &mut Vec<Actor>, request: &ComboFeedbackRequest<'_>, zoom_mod: f32) {
    let Some(font) = request.font else { return };
    let (value, color) = if request.miss_combo >= request.style.threshold {
        (request.miss_combo, request.style.miss_color)
    } else if request.combo >= request.style.threshold {
        (request.combo, request.combo_color)
    } else {
        return;
    };

    let mut text = TextBuilder::new();
    text.font(font);
    text.settext((request.number_text)(value).into());
    text.align(0.5, 0.5);
    text.xy(request.number_xy[0], request.number_xy[1]);
    text.zoom(request.style.number_zoom * zoom_mod);
    text.horizalign(TextAlign::Center);
    text.shadowlength(request.style.shadow_len);
    text.diffuse(color);
    text.z(request.style.number_z);
    actors.push(text.build(0));
}

fn append_sprite(
    actors: &mut Vec<Actor>,
    source: SpriteSource,
    xy: [f32; 2],
    zoom: [f32; 2],
    rotation_deg: f32,
    color: [f32; 4],
    z: i16,
) {
    let mut sprite = SpriteBuilder::with_source(source);
    sprite.align(0.5, 0.5);
    sprite.xy(xy[0], xy[1]);
    sprite.zoomx(zoom[0]);
    sprite.zoomy(zoom[1]);
    sprite.rotationz(rotation_deg);
    sprite.diffuse(color);
    sprite.blend(BlendMode::Add);
    sprite.z(z);
    actors.push(sprite.build(0));
}

fn ease_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(2)
}

fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t
}

fn with_alpha(color: [f32; 4], alpha: f32) -> [f32; 4] {
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

    fn assets() -> ComboMilestoneAssets {
        ComboMilestoneAssets {
            burst: source("burst"),
            hundred: source("hundred"),
            hundred_mini: source("hundred-mini"),
            thousand: source("thousand"),
            hundred_zoom_scale: 2.0,
            hundred_mini_zoom_scale: 3.0,
            thousand_zoom_scale: 1.5,
        }
    }

    fn number_text(value: u32) -> Arc<str> {
        Arc::from(value.to_string())
    }

    fn request<'a>(
        milestones: &'a [ActiveComboMilestone],
        assets: Option<&'a ComboMilestoneAssets>,
    ) -> ComboFeedbackRequest<'a> {
        ComboFeedbackRequest {
            style: style(),
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
            number_text,
        }
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
        let mut actors = Vec::new();
        compose_combo_feedback(&mut actors, hidden);
        assert!(actors.is_empty());

        let mut below = request(&[], None);
        below.combo = 3;
        compose_combo_feedback(&mut actors, below);
        assert!(actors.is_empty());
    }

    #[test]
    fn miss_combo_precedes_normal_combo_number() {
        let mut request = request(&[], None);
        request.combo = 120;
        request.miss_combo = 4;
        let mut actors = Vec::new();
        compose_combo_feedback(&mut actors, request);

        assert_eq!(actors.len(), 1);
        match &actors[0] {
            Actor::Text {
                align,
                offset,
                color,
                font,
                content,
                align_text,
                z,
                scale,
                shadow_len,
                ..
            } => {
                assert_eq!(*align, [0.5, 0.5]);
                assert_eq!(*offset, [310.0, 265.0]);
                assert_eq!(*color, [1.0, 0.0, 0.0, 1.0]);
                assert_eq!(*font, "combo-font");
                assert_eq!(content.as_str(), "4");
                assert_eq!(*align_text, TextAlign::Center);
                assert_eq!(*z, 90);
                assert_eq!(*scale, [0.75, 0.75]);
                assert_eq!(*shadow_len, [1.0, -1.0]);
            }
            other => panic!("expected combo text, got {other:?}"),
        }
    }

    #[test]
    fn normal_combo_uses_resolved_color() {
        let mut request = request(&[], None);
        request.combo = 10;
        let mut actors = Vec::new();
        compose_combo_feedback(&mut actors, request);

        match &actors[0] {
            Actor::Text { color, content, .. } => {
                assert_eq!(*color, [0.1, 0.8, 0.3, 0.9]);
                assert_eq!(content.as_str(), "10");
            }
            other => panic!("expected combo text, got {other:?}"),
        }
    }

    #[test]
    fn hundred_milestone_actor_fingerprint_preserves_order() {
        let milestone_assets = assets();
        let milestones = [ActiveComboMilestone {
            kind: ComboMilestoneKind::Hundred,
            elapsed: 0.0,
        }];
        let mut request = request(&milestones, Some(&milestone_assets));
        request.font = None;
        let mut actors = Vec::new();
        compose_combo_feedback(&mut actors, request);

        assert_eq!(actors.len(), 4);
        let expected = [
            ("burst", [2.0, 2.0], 0.0, [1.0, 1.0, 1.0, 0.5]),
            ("burst", [2.0, 2.0], -0.0, [1.0, 1.0, 1.0, 0.5]),
            ("hundred", [0.5, 0.5], 10.0, [0.2, 0.4, 0.8, 0.6]),
            ("hundred-mini", [0.75, 0.75], 10.0, [0.2, 0.4, 0.8, 1.0]),
        ];
        for (actor, (key, scale, rotation, tint)) in actors.iter().zip(expected) {
            assert_sprite(actor, key, [320.0, 265.0], scale, false, rotation, tint);
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
        let mut actors = Vec::new();
        compose_combo_feedback(&mut actors, request);

        assert_eq!(actors.len(), 2);
        let zoom = 1.625 * 1.5;
        assert_sprite(
            &actors[0],
            "thousand",
            [370.0, 265.0],
            [zoom, zoom],
            false,
            0.0,
            [0.2, 0.4, 0.8, 0.35],
        );
        assert_sprite(
            &actors[1],
            "thousand",
            [270.0, 265.0],
            [zoom, zoom],
            true,
            0.0,
            [0.2, 0.4, 0.8, 0.35],
        );
    }

    fn assert_sprite(
        actor: &Actor,
        key: &str,
        offset: [f32; 2],
        scale: [f32; 2],
        flip_x: bool,
        rotation: f32,
        tint: [f32; 4],
    ) {
        match actor {
            Actor::Sprite {
                align,
                offset: actual_offset,
                source,
                tint: actual_tint,
                z,
                rot_z_deg,
                scale: actual_scale,
                blend,
                flip_x: actual_flip_x,
                ..
            } => {
                assert_eq!(*align, [0.5, 0.5]);
                assert_eq!(*actual_offset, offset);
                assert_eq!(source.texture_key(), Some(key));
                assert_eq!(*actual_tint, tint);
                assert_eq!(*z, 89);
                assert_eq!(*actual_flip_x, flip_x);
                assert!((*rot_z_deg - rotation).abs() <= 1e-6);
                assert!(
                    (actual_scale[0] - scale[0]).abs() <= 1e-6,
                    "unexpected x scale: actual={}, expected={}",
                    actual_scale[0],
                    scale[0]
                );
                assert!(
                    (actual_scale[1] - scale[1]).abs() <= 1e-6,
                    "unexpected y scale: actual={}, expected={}",
                    actual_scale[1],
                    scale[1]
                );
                assert_eq!(*blend, BlendMode::Add);
            }
            other => panic!("expected combo sprite, got {other:?}"),
        }
    }
}
