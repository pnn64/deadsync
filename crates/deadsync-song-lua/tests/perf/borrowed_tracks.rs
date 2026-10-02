use super::*;
use std::sync::Arc;

#[path = "borrowed_tracks_baseline.rs"]
mod baseline;

#[test]
fn borrowed_track_comparisons_match_owned_values_for_all_targets_and_nan_colors() {
    let states = [
        SongLuaOverlayState::default(),
        SongLuaOverlayState {
            vertex_colors: Some([[0.0, -0.0, 0.5, 1.0]; 4]),
            x: -0.0,
            ..Default::default()
        },
        SongLuaOverlayState {
            vertex_colors: Some([[f32::from_bits(0x7fc00003), f32::INFINITY, -0.0, 1.0]; 4]),
            x: f32::NAN,
            ..Default::default()
        },
    ];
    // The list covers every current enum variant; no unsafe discriminant casts.
    let targets = [
        SongLuaOverlayUpdateTarget::X,
        SongLuaOverlayUpdateTarget::Y,
        SongLuaOverlayUpdateTarget::Z,
        SongLuaOverlayUpdateTarget::ZBias,
        SongLuaOverlayUpdateTarget::DrawOrder,
        SongLuaOverlayUpdateTarget::DrawByZPosition,
        SongLuaOverlayUpdateTarget::HAlign,
        SongLuaOverlayUpdateTarget::VAlign,
        SongLuaOverlayUpdateTarget::TextAlign,
        SongLuaOverlayUpdateTarget::Uppercase,
        SongLuaOverlayUpdateTarget::ShadowLen,
        SongLuaOverlayUpdateTarget::ShadowColor,
        SongLuaOverlayUpdateTarget::Glow,
        SongLuaOverlayUpdateTarget::Fov,
        SongLuaOverlayUpdateTarget::Vanishpoint,
        SongLuaOverlayUpdateTarget::Diffuse,
        SongLuaOverlayUpdateTarget::VertexColors,
        SongLuaOverlayUpdateTarget::Visible,
        SongLuaOverlayUpdateTarget::CropLeft,
        SongLuaOverlayUpdateTarget::CropRight,
        SongLuaOverlayUpdateTarget::CropTop,
        SongLuaOverlayUpdateTarget::CropBottom,
        SongLuaOverlayUpdateTarget::FadeLeft,
        SongLuaOverlayUpdateTarget::FadeRight,
        SongLuaOverlayUpdateTarget::FadeTop,
        SongLuaOverlayUpdateTarget::FadeBottom,
        SongLuaOverlayUpdateTarget::MaskSource,
        SongLuaOverlayUpdateTarget::MaskDest,
        SongLuaOverlayUpdateTarget::DepthTest,
        SongLuaOverlayUpdateTarget::Zoom,
        SongLuaOverlayUpdateTarget::ZoomX,
        SongLuaOverlayUpdateTarget::ZoomY,
        SongLuaOverlayUpdateTarget::ZoomZ,
        SongLuaOverlayUpdateTarget::BaseZoom,
        SongLuaOverlayUpdateTarget::BaseZoomX,
        SongLuaOverlayUpdateTarget::BaseZoomY,
        SongLuaOverlayUpdateTarget::BaseZoomZ,
        SongLuaOverlayUpdateTarget::RotationX,
        SongLuaOverlayUpdateTarget::RotationY,
        SongLuaOverlayUpdateTarget::RotationZ,
        SongLuaOverlayUpdateTarget::SkewX,
        SongLuaOverlayUpdateTarget::SkewY,
        SongLuaOverlayUpdateTarget::Blend,
        SongLuaOverlayUpdateTarget::Vibrate,
        SongLuaOverlayUpdateTarget::EffectMagnitude,
        SongLuaOverlayUpdateTarget::EffectClock,
        SongLuaOverlayUpdateTarget::EffectMode,
        SongLuaOverlayUpdateTarget::EffectColor1,
        SongLuaOverlayUpdateTarget::EffectColor2,
        SongLuaOverlayUpdateTarget::EffectPeriod,
        SongLuaOverlayUpdateTarget::EffectOffset,
        SongLuaOverlayUpdateTarget::EffectTiming,
        SongLuaOverlayUpdateTarget::Rainbow,
        SongLuaOverlayUpdateTarget::RainbowScroll,
        SongLuaOverlayUpdateTarget::TextJitter,
        SongLuaOverlayUpdateTarget::TextDistortion,
        SongLuaOverlayUpdateTarget::TextGlowMode,
        SongLuaOverlayUpdateTarget::MultAttrsWithDiffuse,
        SongLuaOverlayUpdateTarget::SpriteTexture,
        SongLuaOverlayUpdateTarget::SpriteAnimate,
        SongLuaOverlayUpdateTarget::SpriteLoop,
        SongLuaOverlayUpdateTarget::SpritePlaybackRate,
        SongLuaOverlayUpdateTarget::SpriteStateDelay,
        SongLuaOverlayUpdateTarget::SpriteStateIndex,
        SongLuaOverlayUpdateTarget::VertSpacing,
        SongLuaOverlayUpdateTarget::WrapWidthPixels,
        SongLuaOverlayUpdateTarget::MaxWidth,
        SongLuaOverlayUpdateTarget::MaxHeight,
        SongLuaOverlayUpdateTarget::MaxWPreZoom,
        SongLuaOverlayUpdateTarget::MaxHPreZoom,
        SongLuaOverlayUpdateTarget::MaxDimensionUsesZoom,
        SongLuaOverlayUpdateTarget::TextureFiltering,
        SongLuaOverlayUpdateTarget::TextureWrapping,
        SongLuaOverlayUpdateTarget::TexcoordOffset,
        SongLuaOverlayUpdateTarget::CustomTextureRect,
        SongLuaOverlayUpdateTarget::TexcoordVelocity,
        SongLuaOverlayUpdateTarget::Size,
        SongLuaOverlayUpdateTarget::StretchRect,
    ];
    for state in &states {
        for target in targets {
            let owned = overlay_state_update_value(state, target);
            for value in [
                owned.clone(),
                SongLuaOverlayUpdateValue::None,
                SongLuaOverlayUpdateValue::F32(0.0),
                SongLuaOverlayUpdateValue::Bool(false),
                SongLuaOverlayUpdateValue::VertexColors(Arc::new([[0.0, -0.0, 0.5, 1.0]; 4])),
            ] {
                assert_eq!(
                    overlay_state_matches_update_value(state, target, &value),
                    owned == value,
                    "target={target:?}"
                );
            }
        }
    }
    let state = states[1];
    let colors = overlay_state_update_value(&state, SongLuaOverlayUpdateTarget::VertexColors);
    crate::perf::assert_no_churn(|| {
        assert!(overlay_state_matches_update_value(
            &state,
            SongLuaOverlayUpdateTarget::VertexColors,
            &colors
        ))
    });
}

struct Fixture {
    lua: Lua,
    context: SongLuaCompileContext,
    overlays: Vec<SongLuaOverlayCompileActor<()>>,
    states: Vec<SongLuaOverlayState>,
    from: Vec<SongLuaOverlayState>,
    to: Vec<SongLuaOverlayState>,
    updates: Vec<SongLuaOverlayState>,
    scheduled: Vec<SongLuaScheduledOverlaySample>,
    tracks: Vec<SongLuaOverlayUpdateTrack>,
    indices: FxHashMap<(usize, SongLuaOverlayUpdateTarget), usize>,
    scratch: OverlaySampleScratch,
}

impl Fixture {
    fn new(count: usize, colors: bool) -> Self {
        let lua = Lua::new();
        let context = SongLuaCompileContext::new("", "Borrowed track comparisons");
        let states: Vec<_> = (0..count)
            .map(|i| SongLuaOverlayState {
                vertex_colors: colors.then_some([[i as f32, 0.25, -0.0, 1.0]; 4]),
                ..Default::default()
            })
            .collect();
        let overlays = states
            .iter()
            .enumerate()
            .map(|(i, state)| {
                let table = lua.create_table().unwrap();
                reset_actor_capture(&lua, &table).unwrap();
                SongLuaOverlayCompileActor {
                    table,
                    actor: crate::SongLuaOverlayActor {
                        kind: (),
                        name: Some(format!("actor{i}")),
                        parent_index: None,
                        initial_state: *state,
                        message_commands: vec![],
                    },
                    message_sounds: vec![],
                }
            })
            .collect();
        let mut tracks = Vec::new();
        let mut indices = FxHashMap::default();
        for (i, state) in states.iter().enumerate() {
            let target = if colors {
                SongLuaOverlayUpdateTarget::VertexColors
            } else {
                SongLuaOverlayUpdateTarget::X
            };
            indices.insert((i, target), tracks.len());
            tracks.push(SongLuaOverlayUpdateTrack {
                overlay_index: i,
                target,
                samples: vec![SongLuaOverlayUpdateSample {
                    beat: 0.0,
                    value: overlay_state_update_value(state, target),
                }],
            });
        }
        Self {
            lua,
            context,
            overlays,
            from: states.clone(),
            to: states.clone(),
            updates: states.clone(),
            scheduled: Vec::new(),
            states,
            tracks,
            indices,
            scratch: OverlaySampleScratch::default(),
        }
    }

    fn capture(
        &mut self,
        old: bool,
        beat: f32,
        from: &[SongLuaOverlayState],
        to: &mut [SongLuaOverlayState],
    ) {
        let mut updates = self.states.clone();
        let mut scheduled = Vec::new();
        if old {
            baseline::capture_update_overlay_samples(
                &self.lua,
                &self.context,
                &self.overlays,
                &self.states,
                from,
                &mut updates,
                to,
                &[],
                &mut self.tracks,
                &mut self.indices,
                beat,
                beat + 0.05,
                f64::from(beat),
                &mut scheduled,
                &mut self.scratch,
            )
        } else {
            capture_update_overlay_samples(
                &self.lua,
                &self.context,
                &self.overlays,
                &self.states,
                from,
                &mut updates,
                to,
                &[],
                &mut self.tracks,
                &mut self.indices,
                beat,
                beat + 0.05,
                f64::from(beat),
                &mut scheduled,
                &mut self.scratch,
            )
        }
        .unwrap();
    }

    fn frame(&mut self, old: bool, beat: f32) {
        if old {
            baseline::capture_update_overlay_samples(
                &self.lua,
                &self.context,
                &self.overlays,
                &self.states,
                &self.from,
                &mut self.updates,
                &mut self.to,
                &[],
                &mut self.tracks,
                &mut self.indices,
                beat,
                beat + 0.05,
                f64::from(beat),
                &mut self.scheduled,
                &mut self.scratch,
            )
        } else {
            capture_update_overlay_samples(
                &self.lua,
                &self.context,
                &self.overlays,
                &self.states,
                &self.from,
                &mut self.updates,
                &mut self.to,
                &[],
                &mut self.tracks,
                &mut self.indices,
                beat,
                beat + 0.05,
                f64::from(beat),
                &mut self.scheduled,
                &mut self.scratch,
            )
        }
        .unwrap();
    }

    fn steady_batch(&mut self, old: bool) {
        // Compiler state buffers and scratch storage already exist before the
        // measured work. Compare 64 frames without growing the output tracks.
        for tick in 0..64 {
            self.frame(old, tick as f32 * 0.05);
        }
    }

    fn changing_batch(&mut self, old: bool) {
        // This control emits owning color samples on every frame. A fresh
        // fixture keeps output growth identical in the two implementations.
        for tick in 0..16 {
            for state in &mut self.to {
                state.vertex_colors = Some([[tick as f32 + 1.0, 0.75, 0.5, 1.0]; 4]);
            }
            self.frame(old, tick as f32 * 0.05);
            self.from.copy_from_slice(&self.to);
        }
    }
}

#[test]
fn borrowed_track_capture_preserves_changes_clears_nan_and_captured_target_precedence() {
    let mut old = Fixture::new(3, true);
    let mut new = Fixture::new(3, true);
    let mut prior = old.states.clone();
    for (tick, colors) in [
        Some([[0.0, -0.0, 0.5, 1.0]; 4]),
        None,
        Some([[f32::from_bits(0x7fc00003), 0.0, 0.0, 1.0]; 4]),
        Some([[f32::from_bits(0x7fc00003), 0.0, 0.0, 1.0]; 4]),
        Some([[0.0, 0.0, 0.5, 1.0]; 4]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut changed = prior.clone();
        let prior_samples = new.tracks[0].samples.len();
        changed[0].vertex_colors = colors;
        changed[1].x = tick as f32;
        old.capture(true, tick as f32, &prior, &mut changed.clone());
        new.capture(false, tick as f32, &prior, &mut changed);
        assert_eq!(format!("{:?}", new.tracks), format!("{:?}", old.tracks));
        assert_eq!(new.indices, old.indices);
        assert!(new.tracks[0].samples.len() > prior_samples);
        for (a, b) in new.tracks.iter().zip(&old.tracks) {
            for (a, b) in a.samples.iter().zip(&b.samples) {
                if let (
                    SongLuaOverlayUpdateValue::VertexColors(a),
                    SongLuaOverlayUpdateValue::VertexColors(b),
                ) = (&a.value, &b.value)
                {
                    assert_eq!(
                        a.map(|c| c.map(f32::to_bits)),
                        b.map(|c| c.map(f32::to_bits))
                    );
                }
            }
        }
        prior = changed;
    }
    for fixture in [&mut old, &mut new] {
        crate::lua_util::begin_overlay_update_capture_from_indices(
            &fixture.lua,
            fixture
                .overlays
                .iter()
                .enumerate()
                .map(|(i, a)| (a.table.to_pointer() as usize, i)),
        );
        crate::capture_block_set_vertex_colors(
            &fixture.lua,
            &fixture.overlays[2].table,
            [[0.75; 4]; 4],
        )
        .unwrap();
    }
    let mut to = prior.clone();
    to[2].vertex_colors = Some([[0.25; 4]; 4]);
    old.capture(true, 8.0, &prior, &mut to.clone());
    new.capture(false, 8.0, &prior, &mut to);
    assert_eq!(format!("{:?}", new.tracks), format!("{:?}", old.tracks));
    let captured = new.indices[&(2, SongLuaOverlayUpdateTarget::VertexColors)];
    assert_eq!(
        new.tracks[captured].samples.last().unwrap().value,
        SongLuaOverlayUpdateValue::VertexColors(Arc::new([[0.75; 4]; 4]))
    );
}

#[test]
fn borrowed_track_steady_colors_reduce_allocations_and_scalar_controls_match() {
    for colors in [false, true] {
        let mut old = Fixture::new(32, colors);
        let mut new = Fixture::new(32, colors);
        old.steady_batch(true);
        new.steady_batch(false);
        assert_eq!(new.tracks, old.tracks);
        crate::perf::assert_no_churn(|| new.steady_batch(false));
        if !colors {
            crate::perf::assert_no_churn(|| old.steady_batch(true));
        }
        if colors {
            crate::perf::assert_reduced_churn(
                || old.steady_batch(true),
                || new.steady_batch(false),
            );
        }
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially"]
fn frame_capture_tracks_hot_path_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for count in [1, 32, 128] {
        for colors in [false, true] {
            for old in order {
                let mut fixture = Fixture::new(count, colors);
                fixture.steady_batch(old);
                crate::perf::measure_sampled(
                    &format!(
                        "track_compare_{count}_colors_{colors}/{}",
                        if old { "old" } else { "new" }
                    ),
                    128,
                    count * 64,
                    || fixture.steady_batch(old),
                );
            }
        }
    }
    for count in [1, 32] {
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!("track_changes_{count}/{}", if old { "old" } else { "new" }),
                32,
                count * 16,
                || {
                    let mut fixture = Fixture::new(count, true);
                    fixture.steady_batch(old);
                    fixture
                },
                |fixture| fixture.changing_batch(old),
            );
        }
    }
}
