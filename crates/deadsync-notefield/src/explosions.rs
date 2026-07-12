use crate::scale_effect_size;
use deadlib_present::actors::{Actor, SpriteSource};
use deadlib_present::dsl::SpriteBuilder;
use deadlib_render::BlendMode;
use deadsync_noteskin::{NoteskinSlot, TapExplosionLayer};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ExplosionRotation {
    Tap {
        rotation_y_deg: f32,
        extra_z_deg: f32,
    },
    Mine,
}

/// Canonical inputs for an ordered noteskin explosion layer sequence.
pub(crate) struct ExplosionComposeRequest<'a, S> {
    pub layers: &'a [TapExplosionLayer<S>],
    pub elapsed_s: f32,
    pub current_frame_beat: f32,
    pub relative_frame_beat: Option<f32>,
    pub uv_elapsed_s: f32,
    pub center: [f32; 2],
    pub field_zoom: f32,
    pub effect_zoom: f32,
    pub rotation: ExplosionRotation,
    pub z: i16,
}

/// Appends every visible explosion layer, preserving diffuse-before-glow order.
/// Concrete asset owners inject sprite sources so cached texture handles remain
/// outside the canonical notefield crate.
pub(crate) fn compose_explosion_layers<S, F>(
    actors: &mut Vec<Actor>,
    request: ExplosionComposeRequest<'_, S>,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    for layer in request.layers {
        let visual = layer.animation.state_at(request.elapsed_s);
        if !visual.visible {
            continue;
        }
        let slot = &layer.slot;
        let frame_beat = request
            .relative_frame_beat
            .filter(|_| slot.animation_is_beat_based())
            .unwrap_or(request.current_frame_beat);
        let frame = slot.frame_index(request.elapsed_s, frame_beat);
        let uv = slot.uv_for_frame_at(frame, request.uv_elapsed_s);
        let size = scale_effect_size(slot.logical_size(), request.field_zoom, request.effect_zoom);
        let (rotation_y_deg, rotation_z_deg) = match request.rotation {
            ExplosionRotation::Tap {
                rotation_y_deg,
                extra_z_deg,
            } => (
                rotation_y_deg,
                visual.rotation_z - slot.sprite_def().rotation_deg as f32 + extra_z_deg,
            ),
            ExplosionRotation::Mine => (0.0, -visual.rotation_z),
        };
        let blend = if layer.animation.blend_add {
            BlendMode::Add
        } else {
            BlendMode::Alpha
        };
        let draw = ExplosionActorDraw {
            center: request.center,
            size,
            zoom: visual.zoom,
            uv,
            tint: visual.diffuse,
            rotation_y_deg,
            rotation_z_deg,
            blend,
            z: request.z,
        };
        append_explosion_actor(actors, slot, sprite_source, draw);

        if visual.glow.iter().map(|channel| channel.abs()).sum::<f32>() > f32::EPSILON {
            append_explosion_actor(
                actors,
                slot,
                sprite_source,
                ExplosionActorDraw {
                    tint: visual.glow,
                    ..draw
                },
            );
        }
    }
}

#[derive(Clone, Copy)]
struct ExplosionActorDraw {
    center: [f32; 2],
    size: [f32; 2],
    zoom: f32,
    uv: [f32; 4],
    tint: [f32; 4],
    rotation_y_deg: f32,
    rotation_z_deg: f32,
    blend: BlendMode,
    z: i16,
}

fn append_explosion_actor<S, F>(
    actors: &mut Vec<Actor>,
    slot: &S,
    sprite_source: &F,
    draw: ExplosionActorDraw,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    let mut actor = SpriteBuilder::with_source(sprite_source(slot));
    actor.align(0.5, 0.5);
    actor.xy(draw.center[0], draw.center[1]);
    actor.size(draw.size[0], draw.size[1]);
    actor.zoom(draw.zoom);
    actor.customtexturerect(draw.uv);
    actor.diffuse(draw.tint);
    actor.rotationy(draw.rotation_y_deg);
    actor.rotationz(draw.rotation_z_deg);
    actor.blend(draw.blend);
    actor.z(draw.z);
    actors.push(actor.build(0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SizeSpec;
    use deadsync_noteskin::{
        ExplosionAnimation, ExplosionState, GlowEffect, ModelDrawState, ModelMesh, SpriteDefinition,
    };
    use std::sync::Arc;

    struct TestSlot {
        def: SpriteDefinition,
        texture: Arc<str>,
        beat_based: bool,
    }

    impl NoteskinSlot for TestSlot {
        fn sprite_def(&self) -> &SpriteDefinition {
            &self.def
        }

        fn source_size(&self) -> [i32; 2] {
            [10, 12]
        }

        fn texture_key_shared(&self) -> Arc<str> {
            Arc::clone(&self.texture)
        }

        fn model(&self) -> Option<&ModelMesh> {
            None
        }

        fn base_rot_sin_cos(&self) -> [f32; 2] {
            [0.0, 1.0]
        }

        fn animation_is_beat_based(&self) -> bool {
            self.beat_based
        }

        fn frame_index(&self, _time: f32, beat: f32) -> usize {
            beat.max(0.0).floor() as usize
        }

        fn frame_index_from_phase(&self, _phase: f32) -> usize {
            0
        }

        fn uv_for_frame_at(&self, frame_index: usize, _elapsed: f32) -> [f32; 4] {
            [frame_index as f32 * 0.1, 0.0, 1.0, 1.0]
        }

        fn model_draw_at(&self, _time: f32, _beat: f32) -> ModelDrawState {
            ModelDrawState::default()
        }

        fn model_glow_with_draw(
            &self,
            _draw: ModelDrawState,
            _time: f32,
            _beat: f32,
            _diffuse_alpha: f32,
        ) -> Option<[f32; 4]> {
            None
        }

        fn model_uv_params(&self, uv: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
            ([uv[2] - uv[0], uv[3] - uv[1]], [uv[0], uv[1]], [0.0; 2])
        }
    }

    fn layer(visible: bool, blend_add: bool) -> TapExplosionLayer<TestSlot> {
        TapExplosionLayer {
            slot: TestSlot {
                def: SpriteDefinition {
                    rotation_deg: 5,
                    ..SpriteDefinition::default()
                },
                texture: Arc::from("explosion"),
                beat_based: true,
            },
            animation: ExplosionAnimation {
                initial: ExplosionState {
                    zoom: 1.5,
                    color: [0.2, 0.3, 0.4, 0.5],
                    rotation_z: 20.0,
                    visible,
                },
                segments: Vec::new(),
                glow: Some(GlowEffect {
                    period: 1.0,
                    color1: [0.1, 0.2, 0.3, 0.4],
                    color2: [0.0; 4],
                }),
                blend_add,
            },
        }
    }

    #[test]
    fn tap_layers_emit_diffuse_then_glow_with_authored_transform() {
        let layers = [layer(true, true)];
        let mut actors = Vec::new();
        compose_explosion_layers(
            &mut actors,
            ExplosionComposeRequest {
                layers: &layers,
                elapsed_s: 0.0,
                current_frame_beat: 9.0,
                relative_frame_beat: Some(2.0),
                uv_elapsed_s: 3.0,
                center: [30.0, 40.0],
                field_zoom: 2.0,
                effect_zoom: 1.5,
                rotation: ExplosionRotation::Tap {
                    rotation_y_deg: 7.0,
                    extra_z_deg: 3.0,
                },
                z: 145,
            },
            &|slot| SpriteSource::Texture(Arc::clone(&slot.texture)),
        );

        assert_eq!(actors.len(), 2);
        let Actor::Sprite {
            size,
            scale,
            uv_rect,
            tint,
            glow,
            world_z,
            rot_y_deg,
            rot_z_deg,
            blend,
            z,
            ..
        } = &actors[0]
        else {
            panic!("diffuse explosion should emit a sprite");
        };
        let [SizeSpec::Px(width), SizeSpec::Px(height)] = size else {
            panic!("explosion size should be expressed in pixels");
        };
        assert!(
            (*width - 45.0).abs() <= f32::EPSILON,
            "unexpected width {width}"
        );
        assert!(
            (*height - 54.0).abs() <= f32::EPSILON,
            "unexpected height {height}"
        );
        assert_eq!(*scale, [1.0, 1.0]);
        assert_eq!(*uv_rect, Some([0.2, 0.0, 1.0, 1.0]));
        assert_eq!(*tint, [0.2, 0.3, 0.4, 0.5]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*world_z, 0.0);
        assert_eq!(*rot_y_deg, 7.0);
        assert_eq!(*rot_z_deg, 18.0);
        assert_eq!(*blend, BlendMode::Add);
        assert_eq!(*z, 145);
        let Actor::Sprite {
            tint,
            glow,
            world_z,
            ..
        } = &actors[1]
        else {
            unreachable!();
        };
        assert_eq!(*tint, [0.1, 0.2, 0.3, 0.2]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*world_z, 0.0);
    }

    #[test]
    fn mine_layers_invert_rotation_and_skip_hidden_layers() {
        let layers = [layer(false, false), layer(true, false)];
        let mut actors = Vec::new();
        compose_explosion_layers(
            &mut actors,
            ExplosionComposeRequest {
                layers: &layers,
                elapsed_s: 0.0,
                current_frame_beat: 0.0,
                relative_frame_beat: None,
                uv_elapsed_s: 0.0,
                center: [0.0; 2],
                field_zoom: 1.0,
                effect_zoom: 1.0,
                rotation: ExplosionRotation::Mine,
                z: 146,
            },
            &|slot| SpriteSource::Texture(Arc::clone(&slot.texture)),
        );

        assert_eq!(actors.len(), 2);
        let Actor::Sprite {
            rot_y_deg,
            rot_z_deg,
            blend,
            ..
        } = &actors[0]
        else {
            unreachable!();
        };
        assert_eq!(*rot_y_deg, 0.0);
        assert_eq!(*rot_z_deg, -20.0);
        assert_eq!(*blend, BlendMode::Alpha);
    }

    #[test]
    fn time_based_layer_keeps_current_beat_and_zero_alpha_diffuse() {
        let mut visible = layer(true, false);
        visible.slot.beat_based = false;
        visible.animation.initial.color[3] = 0.0;
        visible.animation.glow = None;
        let layers = [visible];
        let mut actors = Vec::new();
        compose_explosion_layers(
            &mut actors,
            ExplosionComposeRequest {
                layers: &layers,
                elapsed_s: 0.0,
                current_frame_beat: 7.0,
                relative_frame_beat: Some(2.0),
                uv_elapsed_s: 0.0,
                center: [0.0; 2],
                field_zoom: 1.0,
                effect_zoom: 1.0,
                rotation: ExplosionRotation::Tap {
                    rotation_y_deg: 0.0,
                    extra_z_deg: 0.0,
                },
                z: 1,
            },
            &|slot| SpriteSource::Texture(Arc::clone(&slot.texture)),
        );

        assert_eq!(actors.len(), 1);
        let Actor::Sprite { uv_rect, tint, .. } = &actors[0] else {
            unreachable!();
        };
        assert_eq!(*uv_rect, Some([0.7, 0.0, 1.0, 1.0]));
        assert_eq!(*tint, [0.2, 0.3, 0.4, 0.0]);
    }
}
