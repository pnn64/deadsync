use crate::*;
use deadlib_present::actors::{Actor, SpriteSource};
use deadlib_present::dsl::SpriteBuilder;
use deadlib_render::BlendMode;
use deadsync_noteskin::{
    NoteskinSlot, ReceptorGlowBehavior, ReceptorPulse, ReceptorReverseBehavior,
};
use deadsync_theme::ReceptorStyle;

pub fn receptor_row_center(
    field_center_x: f32,
    local_col: usize,
    receptor_y: f32,
    beat_factor_value: f32,
    elapsed: f32,
    col_offsets: &[f32],
    invert: &[f32],
    tornado: &[TornadoBounds],
    move_x: &[f32],
    move_y: &[f32],
    params: NoteXParams,
    tiny_zoom: f32,
    tipsy: f32,
) -> [f32; 2] {
    let x = field_center_x
        + note_x_offset(
            local_col,
            0.0,
            beat_factor_value,
            elapsed,
            col_offsets,
            invert,
            tornado,
            move_x,
            params,
            tiny_zoom,
        );
    let y =
        receptor_y + move_col_extra(move_y, local_col) + tipsy_y_extra(local_col, elapsed, tipsy);
    [x, y]
}

pub(crate) fn hold_indicator_column_x(
    field_center_x: f32,
    local_col: usize,
    beat_factor_value: f32,
    elapsed: f32,
    col_offsets: &[f32],
    invert: &[f32],
    tornado: &[TornadoBounds],
    move_x: &[f32],
    params: NoteXParams,
    tiny_zoom: f32,
) -> f32 {
    field_center_x
        + note_x_offset(
            local_col,
            0.0,
            beat_factor_value,
            elapsed,
            col_offsets,
            invert,
            tornado,
            move_x,
            params,
            tiny_zoom,
        )
}

/// Per-lane canonical receptor inputs supplied by a concrete gameplay screen.
pub struct ReceptorActorsRequest<'a, S> {
    /// Resolved lazily by the adapter so hidden targets preserve the old
    /// short-circuit behavior and never index the concrete noteskin slots.
    pub target_slot: Option<&'a S>,
    pub target_reverse: Option<ReceptorReverseBehavior>,
    pub hold_slot: Option<&'a S>,
    pub center: [f32; 2],
    pub hidden: bool,
    pub hide_targets: bool,
    pub reverse: bool,
    pub bop_zoom: f32,
    pub effect_zoom: f32,
    pub confusion_rotation_deg: f32,
    pub elapsed: f32,
    pub beat: f32,
    pub receptor_alpha: f32,
    pub field_zoom: f32,
    pub rotation_y_deg: f32,
    pub pulse: &'a ReceptorPulse,
    pub press_behavior: ReceptorGlowBehavior,
    pub style: ReceptorStyle,
}

/// Lazily resolved press-glow inputs, read only after hold composition succeeds.
pub struct ReceptorPress<'a, S> {
    pub slot: &'a S,
    pub reverse: Option<ReceptorReverseBehavior>,
    pub visual: (f32, f32),
}

struct ReceptorSpriteDraw {
    align: [f32; 2],
    center: [f32; 2],
    size: [f32; 2],
    zoom: [f32; 2],
    tint: [f32; 4],
    rotation_y_deg: f32,
    rotation_z_deg: f32,
    uv: [f32; 4],
    blend: BlendMode,
    z: i16,
}

/// Appends one lane's target, hold explosion, and press glow in canonical order.
pub fn compose_receptor_actors<'a, S, F, P>(
    actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    request: ReceptorActorsRequest<'a, S>,
    resolve_press: P,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
    P: FnOnce() -> Option<ReceptorPress<'a, S>>,
{
    let targets_visible =
        !request.hidden && !request.hide_targets && request.receptor_alpha > f32::EPSILON;
    if targets_visible {
        let slot = request
            .target_slot
            .expect("visible receptor target must have a noteskin slot");
        let reverse = request
            .target_reverse
            .unwrap_or_default()
            .state(request.reverse);
        let rotation = slot.sprite_def().rotation_deg as f32 + reverse.base_rotation_z();
        let frame = slot.frame_index(request.elapsed, request.beat);
        let uv = slot.uv_for_frame_at(frame, request.elapsed);
        let draw = slot.model_draw_at(request.elapsed, request.beat);
        let base_size = effect_size(slot, request.field_zoom, request.effect_zoom);
        let size = [base_size[0] * draw.zoom[0], base_size[1] * draw.zoom[1]];
        let color = request.pulse.color_for_beat(request.beat);
        let alpha = color[3] * draw.tint[3] * request.receptor_alpha;
        if draw.visible && alpha > f32::EPSILON && size[0] > f32::EPSILON && size[1] > f32::EPSILON
        {
            let center = draw_center(
                slot,
                request.center,
                draw.pos,
                request.field_zoom * request.effect_zoom,
            );
            append_receptor_sprite(
                actors,
                slot,
                sprite_source,
                ReceptorSpriteDraw {
                    align: [0.5, reverse.vert_align()],
                    center,
                    size,
                    zoom: mirrored_zoom(slot, request.bop_zoom),
                    tint: [
                        color[0] * draw.tint[0],
                        color[1] * draw.tint[1],
                        color[2] * draw.tint[2],
                        alpha,
                    ],
                    rotation_y_deg: request.rotation_y_deg,
                    rotation_z_deg: draw.rot[2] - rotation + request.confusion_rotation_deg,
                    uv,
                    blend: BlendMode::Alpha,
                    z: request.style.target_z,
                },
            );
        }
    }

    if let Some(slot) = request.hold_slot {
        let draw = song_lua_note_model_draw(
            slot.model_draw_at(request.elapsed, request.beat),
            request.rotation_y_deg,
        );
        let frame = slot.frame_index(request.elapsed, request.beat);
        let uv = slot.uv_for_frame_at(frame, request.elapsed);
        let base_size = effect_size(slot, request.field_zoom, request.effect_zoom);
        let size = [
            base_size[0] * draw.zoom[0].max(0.0),
            base_size[1] * draw.zoom[1].max(0.0),
        ];
        if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
            return;
        }
        let final_rotation =
            slot.sprite_def().rotation_deg as f32 - draw.rot[2] - request.confusion_rotation_deg;
        let color = draw.tint;
        let glow = slot.model_glow_with_draw(draw, request.elapsed, request.beat, color[3]);
        let blend = if draw.blend_add {
            BlendMode::Add
        } else {
            BlendMode::Alpha
        };
        if let Some(actor) = noteskin_model_actor_from_draw_cached(
            slot,
            draw,
            request.center,
            size,
            uv,
            -final_rotation,
            color,
            blend,
            request.style.hold_explosion_z,
            model_cache,
        ) {
            actors.push(actor);
            if let Some(glow) = glow
                && let Some(actor) = noteskin_model_actor_from_draw_cached(
                    slot,
                    draw,
                    request.center,
                    size,
                    uv,
                    -final_rotation,
                    glow,
                    blend,
                    request.style.hold_explosion_z,
                    model_cache,
                )
            {
                actors.push(actor);
            }
        } else {
            append_receptor_sprite(
                actors,
                slot,
                sprite_source,
                ReceptorSpriteDraw {
                    align: [0.5, 0.5],
                    center: request.center,
                    size,
                    zoom: [1.0, 1.0],
                    tint: color,
                    rotation_y_deg: 0.0,
                    rotation_z_deg: -final_rotation,
                    uv,
                    blend,
                    z: request.style.hold_explosion_z,
                },
            );
            if let Some(glow) = glow {
                append_receptor_sprite(
                    actors,
                    slot,
                    sprite_source,
                    ReceptorSpriteDraw {
                        align: [0.5, 0.5],
                        center: request.center,
                        size,
                        zoom: [1.0, 1.0],
                        tint: glow,
                        rotation_y_deg: 0.0,
                        rotation_z_deg: -final_rotation,
                        uv,
                        blend,
                        z: request.style.hold_explosion_z,
                    },
                );
            }
        }
    }

    if targets_visible && let Some(press) = resolve_press() {
        let (alpha, zoom) = press.visual;
        let slot = press.slot;
        let alpha = alpha * request.receptor_alpha;
        if alpha > f32::EPSILON {
            let frame = slot.frame_index(request.elapsed, request.beat);
            let uv = slot.uv_for_frame_at(frame, request.elapsed);
            let draw = slot.model_draw_at(request.elapsed, request.beat);
            let base_size = effect_size(slot, request.field_zoom, request.effect_zoom);
            let reverse = press.reverse.unwrap_or_default().state(request.reverse);
            let rotation = slot.sprite_def().rotation_deg as f32 + reverse.base_rotation_z();
            let size = [
                base_size[0] * zoom * draw.zoom[0],
                base_size[1] * zoom * draw.zoom[1],
            ];
            if draw.visible && size[0] > f32::EPSILON && size[1] > f32::EPSILON {
                let center = draw_center(
                    slot,
                    request.center,
                    draw.pos,
                    request.field_zoom * request.effect_zoom,
                );
                append_receptor_sprite(
                    actors,
                    slot,
                    sprite_source,
                    ReceptorSpriteDraw {
                        align: [0.5, reverse.vert_align()],
                        center,
                        size,
                        zoom: mirrored_zoom(slot, request.bop_zoom),
                        tint: [
                            draw.tint[0],
                            draw.tint[1],
                            draw.tint[2],
                            alpha * draw.tint[3],
                        ],
                        rotation_y_deg: request.rotation_y_deg,
                        rotation_z_deg: draw.rot[2] - rotation + request.confusion_rotation_deg,
                        uv,
                        blend: if request.press_behavior.blend_add {
                            BlendMode::Add
                        } else {
                            BlendMode::Alpha
                        },
                        z: request.style.press_glow_z,
                    },
                );
            }
        }
    }
}

fn effect_size<S: NoteskinSlot>(slot: &S, field_zoom: f32, effect_zoom: f32) -> [f32; 2] {
    let size = slot.logical_size();
    [
        size[0] * field_zoom * effect_zoom,
        size[1] * field_zoom * effect_zoom,
    ]
}

fn draw_center<S: NoteskinSlot>(
    slot: &S,
    center: [f32; 2],
    position: [f32; 3],
    scale: f32,
) -> [f32; 2] {
    let [sin_r, cos_r] = slot.base_rot_sin_cos();
    let offset = [
        position[0] * scale * cos_r - position[1] * scale * sin_r,
        position[0] * scale * sin_r + position[1] * scale * cos_r,
    ];
    [center[0] + offset[0], center[1] + offset[1]]
}

fn mirrored_zoom<S: NoteskinSlot>(slot: &S, zoom: f32) -> [f32; 2] {
    [
        if slot.sprite_def().mirror_h {
            -zoom
        } else {
            zoom
        },
        if slot.sprite_def().mirror_v {
            -zoom
        } else {
            zoom
        },
    ]
}

fn append_receptor_sprite<S, F>(
    actors: &mut Vec<Actor>,
    slot: &S,
    sprite_source: &F,
    draw: ReceptorSpriteDraw,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    let mut actor = SpriteBuilder::with_source(sprite_source(slot));
    actor.align(draw.align[0], draw.align[1]);
    actor.xy(draw.center[0], draw.center[1]);
    actor.size(draw.size[0], draw.size[1]);
    actor.zoomx(draw.zoom[0]);
    actor.zoomy(draw.zoom[1]);
    actor.diffuse(draw.tint);
    actor.rotationy(draw.rotation_y_deg);
    actor.rotationz(draw.rotation_z_deg);
    actor.customtexturerect(draw.uv);
    actor.blend(draw.blend);
    actor.z(draw.z);
    actors.push(actor.build(0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::SizeSpec;
    use deadsync_noteskin::{
        ModelDrawState, ModelMesh, ModelVertex, ReceptorReverseState, SpriteDefinition,
    };
    use std::cell::Cell;
    use std::sync::Arc;

    struct TestSlot {
        def: SpriteDefinition,
        logical_size: [f32; 2],
        model: Option<ModelMesh>,
        draw: ModelDrawState,
        glow: Option<[f32; 4]>,
        texture: Arc<str>,
    }

    impl TestSlot {
        fn sprite(key: &str) -> Self {
            Self {
                def: SpriteDefinition {
                    size: [64, 64],
                    ..SpriteDefinition::default()
                },
                logical_size: [64.0, 64.0],
                model: None,
                draw: ModelDrawState::default(),
                glow: None,
                texture: Arc::from(key),
            }
        }

        fn model(key: &str) -> Self {
            let mut slot = Self::sprite(key);
            slot.model = Some(ModelMesh {
                vertices: Arc::from([ModelVertex {
                    pos: [0.0, 0.0, 0.0],
                    uv: [0.0, 0.0],
                    tex_matrix_scale: [1.0, 1.0],
                }]),
                bounds: [0.0, 0.0, 0.0, 64.0, 64.0, 0.0],
            });
            slot
        }
    }

    impl NoteskinSlot for TestSlot {
        fn sprite_def(&self) -> &SpriteDefinition {
            &self.def
        }

        fn source_size(&self) -> [i32; 2] {
            [self.logical_size[0] as i32, self.logical_size[1] as i32]
        }

        fn logical_size(&self) -> [f32; 2] {
            self.logical_size
        }

        fn texture_key_shared(&self) -> Arc<str> {
            self.texture.clone()
        }

        fn model(&self) -> Option<&ModelMesh> {
            self.model.as_ref()
        }

        fn base_rot_sin_cos(&self) -> [f32; 2] {
            [0.0, 1.0]
        }

        fn frame_index(&self, _time: f32, _beat: f32) -> usize {
            0
        }

        fn frame_index_from_phase(&self, _phase: f32) -> usize {
            0
        }

        fn uv_for_frame_at(&self, _frame_index: usize, _elapsed: f32) -> [f32; 4] {
            [0.1, 0.2, 0.8, 0.9]
        }

        fn model_draw_at(&self, _time: f32, _beat: f32) -> ModelDrawState {
            self.draw
        }

        fn model_glow_with_draw(
            &self,
            _draw: ModelDrawState,
            _time: f32,
            _beat: f32,
            _diffuse_alpha: f32,
        ) -> Option<[f32; 4]> {
            self.glow
        }

        fn model_uv_params(&self, uv: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
            ([uv[2] - uv[0], uv[3] - uv[1]], [uv[0], uv[1]], [0.0, 0.0])
        }
    }

    fn pulse() -> ReceptorPulse {
        ReceptorPulse {
            effect_color1: [1.0; 4],
            effect_color2: [1.0; 4],
            effect_period: 1.0,
            ramp_to_half: 0.0,
            hold_at_half: 0.0,
            ramp_to_full: 0.0,
            hold_at_full: 0.0,
            hold_at_zero: 0.0,
            effect_offset: 0.0,
        }
    }

    fn style() -> ReceptorStyle {
        ReceptorStyle {
            target_z: 100,
            press_glow_z: 105,
            hold_explosion_z: 145,
        }
    }

    fn request<'a>(
        target: Option<&'a TestSlot>,
        hold: Option<&'a TestSlot>,
        pulse: &'a ReceptorPulse,
    ) -> ReceptorActorsRequest<'a, TestSlot> {
        ReceptorActorsRequest {
            target_slot: target,
            target_reverse: None,
            hold_slot: hold,
            center: [10.0, 20.0],
            hidden: false,
            hide_targets: false,
            reverse: false,
            bop_zoom: 1.0,
            effect_zoom: 1.0,
            confusion_rotation_deg: 0.0,
            elapsed: 2.0,
            beat: 3.0,
            receptor_alpha: 1.0,
            field_zoom: 1.0,
            rotation_y_deg: 0.0,
            pulse,
            press_behavior: ReceptorGlowBehavior::default(),
            style: style(),
        }
    }

    fn texture_source(slot: &TestSlot) -> SpriteSource {
        SpriteSource::TextureHandle {
            key: slot.texture.clone(),
            handle: 77,
            generation: 9,
        }
    }

    fn assert_sprite(actor: &Actor, key: &str, z: i16, blend: BlendMode) {
        let Actor::Sprite {
            source,
            z: actual_z,
            blend: actual_blend,
            ..
        } = actor
        else {
            panic!("expected sprite actor, got {actor:?}");
        };
        assert_eq!(*actual_z, z);
        assert_eq!(*actual_blend, blend);
        assert!(matches!(
            source,
            SpriteSource::TextureHandle {
                key: actual_key,
                handle: 77,
                generation: 9,
            } if actual_key.as_ref() == key
        ));
    }

    #[test]
    fn lane_sequence_preserves_actor_order_z_and_cached_sources() {
        let target = TestSlot::sprite("target");
        let mut hold = TestSlot::sprite("hold");
        hold.draw.blend_add = true;
        hold.glow = Some([0.2, 0.3, 0.4, 0.5]);
        let press = TestSlot::sprite("press");
        let pulse = pulse();
        let mut actors = Vec::new();

        compose_receptor_actors(
            &mut actors,
            &mut ModelMeshCache::default(),
            request(Some(&target), Some(&hold), &pulse),
            || {
                Some(ReceptorPress {
                    slot: &press,
                    reverse: None,
                    visual: (0.75, 1.25),
                })
            },
            &texture_source,
        );

        assert_eq!(actors.len(), 4);
        assert_sprite(&actors[0], "target", 100, BlendMode::Alpha);
        assert_sprite(&actors[1], "hold", 145, BlendMode::Add);
        assert_sprite(&actors[2], "hold", 145, BlendMode::Add);
        assert_sprite(&actors[3], "press", 105, BlendMode::Add);
    }

    #[test]
    fn target_preserves_reverse_mirror_bop_and_authored_transform() {
        let mut target = TestSlot::sprite("target");
        target.def.rotation_deg = 30;
        target.def.mirror_h = true;
        target.def.mirror_v = true;
        target.logical_size = [64.0, 32.0];
        target.draw.pos = [2.0, 3.0, 0.0];
        target.draw.rot[2] = 7.0;
        target.draw.zoom = [1.5, 0.5, 1.0];
        let pulse = pulse();
        let mut request = request(Some(&target), None, &pulse);
        request.target_reverse = Some(ReceptorReverseBehavior {
            reverse_on: ReceptorReverseState {
                base_rotation_z: Some(180.0),
                vert_align: Some(1.0),
            },
            ..ReceptorReverseBehavior::default()
        });
        request.reverse = true;
        request.bop_zoom = 1.25;
        request.field_zoom = 2.0;
        request.effect_zoom = 0.5;
        request.confusion_rotation_deg = 4.0;
        request.rotation_y_deg = 12.0;
        let mut actors = Vec::new();

        compose_receptor_actors(
            &mut actors,
            &mut ModelMeshCache::default(),
            request,
            || None,
            &texture_source,
        );

        let Actor::Sprite {
            align,
            offset,
            size,
            flip_x,
            flip_y,
            rot_y_deg,
            rot_z_deg,
            scale,
            ..
        } = &actors[0]
        else {
            panic!("expected target sprite");
        };
        assert_eq!(*align, [0.5, 1.0]);
        assert_eq!(*offset, [12.0, 23.0]);
        assert!(matches!(size, [SizeSpec::Px(w), SizeSpec::Px(h)] if *w == 120.0 && *h == 20.0));
        assert!(*flip_x && *flip_y);
        assert_eq!(*scale, [1.0, 1.0]);
        assert_eq!(*rot_y_deg, 12.0);
        assert_eq!(*rot_z_deg, -199.0);
    }

    #[test]
    fn press_preserves_reverse_geometry_tint_and_normal_blend() {
        let target = TestSlot::sprite("target");
        let mut press = TestSlot::sprite("press");
        press.def.rotation_deg = 20;
        press.def.mirror_h = true;
        press.logical_size = [10.0, 20.0];
        press.draw.pos = [4.0, -2.0, 0.0];
        press.draw.rot[2] = 5.0;
        press.draw.zoom = [0.5, 2.0, 1.0];
        press.draw.tint = [0.2, 0.4, 0.6, 0.8];
        let pulse = pulse();
        let mut request = request(Some(&target), None, &pulse);
        request.bop_zoom = 0.8;
        request.field_zoom = 1.5;
        request.effect_zoom = 2.0;
        request.confusion_rotation_deg = 3.0;
        request.rotation_y_deg = 7.0;
        request.receptor_alpha = 0.6;
        request.reverse = true;
        request.press_behavior.blend_add = false;
        let mut actors = Vec::new();

        compose_receptor_actors(
            &mut actors,
            &mut ModelMeshCache::default(),
            request,
            || {
                Some(ReceptorPress {
                    slot: &press,
                    reverse: Some(ReceptorReverseBehavior {
                        reverse_on: ReceptorReverseState {
                            base_rotation_z: Some(90.0),
                            vert_align: Some(0.25),
                        },
                        ..ReceptorReverseBehavior::default()
                    }),
                    visual: (0.5, 1.25),
                })
            },
            &texture_source,
        );

        let Actor::Sprite {
            align,
            offset,
            size,
            source,
            tint,
            uv_rect,
            flip_x,
            flip_y,
            blend,
            rot_y_deg,
            rot_z_deg,
            ..
        } = &actors[1]
        else {
            panic!("expected press sprite");
        };
        assert_eq!(*align, [0.5, 0.25]);
        assert_eq!(*offset, [22.0, 14.0]);
        assert!(matches!(size, [SizeSpec::Px(w), SizeSpec::Px(h)] if *w == 15.0 && *h == 120.0));
        assert!(matches!(
            source,
            SpriteSource::TextureHandle {
                key,
                handle: 77,
                generation: 9,
            } if key.as_ref() == "press"
        ));
        assert_eq!(*tint, [0.2, 0.4, 0.6, 0.24000001]);
        assert_eq!(*uv_rect, Some([0.1, 0.2, 0.8, 0.9]));
        assert!(*flip_x);
        assert!(!*flip_y);
        assert_eq!(*blend, BlendMode::Alpha);
        assert_eq!(*rot_y_deg, 7.0);
        assert_eq!(*rot_z_deg, -102.0);
    }

    #[test]
    fn zero_size_hold_returns_before_resolving_press() {
        let target = TestSlot::sprite("target");
        let mut hold = TestSlot::sprite("hold");
        hold.draw.zoom[0] = 0.0;
        let pulse = pulse();
        let resolved = Cell::new(0);
        let mut actors = Vec::new();

        compose_receptor_actors(
            &mut actors,
            &mut ModelMeshCache::default(),
            request(Some(&target), Some(&hold), &pulse),
            || {
                resolved.set(resolved.get() + 1);
                None
            },
            &texture_source,
        );

        assert_eq!(resolved.get(), 0);
        assert_eq!(actors.len(), 1);
        assert_sprite(&actors[0], "target", 100, BlendMode::Alpha);
    }

    #[test]
    fn hold_model_reuses_geometry_and_preserves_squared_tint() {
        let target = TestSlot::sprite("target");
        let mut hold = TestSlot::model("hold-model");
        hold.draw.tint = [0.5, 0.25, 0.75, 0.8];
        hold.glow = Some([0.2, 0.4, 0.6, 0.5]);
        let pulse = pulse();
        let mut actors = Vec::new();
        let mut cache = ModelMeshCache::default();

        compose_receptor_actors(
            &mut actors,
            &mut cache,
            request(Some(&target), Some(&hold), &pulse),
            || None,
            &texture_source,
        );

        assert_eq!(actors.len(), 3);
        let (Actor::TexturedMesh { tint: diffuse, .. }, Actor::TexturedMesh { tint: glow, .. }) =
            (&actors[1], &actors[2])
        else {
            panic!("expected diffuse and glow model actors");
        };
        for (actual, expected) in diffuse.iter().zip([0.25, 0.0625, 0.5625, 0.64]) {
            assert!((actual - expected).abs() <= 1e-6);
        }
        for (actual, expected) in glow.iter().zip([0.1, 0.1, 0.45, 0.4]) {
            assert!((actual - expected).abs() <= 1e-6);
        }
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 1,
                misses: 1,
                saturated_misses: 0,
            }
        );
    }

    #[test]
    fn invisible_empty_model_falls_through_to_cached_sprite_source() {
        let target = TestSlot::sprite("target");
        let mut hold = TestSlot::model("hold-fallback");
        hold.model.as_mut().expect("model").vertices = Arc::from([]);
        hold.draw.visible = false;
        let pulse = pulse();
        let mut actors = Vec::new();

        compose_receptor_actors(
            &mut actors,
            &mut ModelMeshCache::default(),
            request(Some(&target), Some(&hold), &pulse),
            || None,
            &texture_source,
        );

        assert_eq!(actors.len(), 2);
        assert_sprite(&actors[1], "hold-fallback", 145, BlendMode::Alpha);
    }
}
