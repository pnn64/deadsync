use super::*;
use crate::lua_util::SongLuaScheduledOverlayUpdate;
use std::hint::black_box;
use std::sync::Arc;

#[path = "capture_outputs_baseline.rs"]
mod baseline;

const TARGETS: [SongLuaOverlayUpdateTarget; 77] = [
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

type Indices = FxHashMap<(usize, SongLuaOverlayUpdateTarget), usize>;

fn value_bits(value: &SongLuaOverlayUpdateValue) -> (String, Vec<u32>) {
    use SongLuaOverlayUpdateValue as V;
    let bits = match value {
        V::F32(v) => vec![v.to_bits()],
        V::Vec2(v) => v.map(f32::to_bits).to_vec(),
        V::Vec3(v) => v.map(f32::to_bits).to_vec(),
        V::Vec4(v) => v.map(f32::to_bits).to_vec(),
        V::Vec5(v) => v.map(f32::to_bits).to_vec(),
        V::VertexColors(v) => v.iter().flatten().map(|v| v.to_bits()).collect(),
        _ => Vec::new(),
    };
    (format!("{value:?}"), bits)
}

fn assert_tracks(a: &[SongLuaOverlayUpdateTrack], b: &[SongLuaOverlayUpdateTrack]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(
            (a.overlay_index, a.target, a.samples.len()),
            (b.overlay_index, b.target, b.samples.len())
        );
        for (a, b) in a.samples.iter().zip(&b.samples) {
            assert_eq!(a.beat.to_bits(), b.beat.to_bits());
            assert_eq!(value_bits(&a.value), value_bits(&b.value));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push(
    old: bool,
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    indices: &mut Indices,
    actor: usize,
    target: SongLuaOverlayUpdateTarget,
    beat: f32,
    current: &SongLuaOverlayState,
    next_beat: f32,
    next: &SongLuaOverlayUpdateValue,
) -> usize {
    if old {
        baseline::push_update_overlay_value(
            tracks,
            indices,
            actor,
            target,
            beat,
            overlay_state_update_value(current, target),
            next_beat,
            next.clone(),
        )
    } else {
        push_captured_overlay_value(
            tracks, indices, actor, target, beat, current, next_beat, next,
        )
    }
}

#[test]
fn capture_sampling_lazy_values_preserve_steps_gaps_equal_writes_nan_and_value_identity() {
    let states = [
        SongLuaOverlayState::default(),
        SongLuaOverlayState {
            x: 17.0,
            vertex_colors: Some([[0.0, -0.0, 0.5, 1.0]; 4]),
            ..Default::default()
        },
        SongLuaOverlayState {
            x: f32::NAN,
            vertex_colors: Some([[f32::from_bits(0x7fc00003), 0.0, 0.0, 1.0]; 4]),
            ..Default::default()
        },
    ];
    let mut old = Vec::new();
    let mut new = Vec::new();
    let mut oi = Indices::default();
    let mut ni = Indices::default();
    let beats = [
        -1.0,
        -0.0,
        0.0,
        f32::EPSILON,
        2.0 * f32::EPSILON,
        1.0,
        1.0,
        -2.0,
        2.0,
        f32::NAN,
        f32::INFINITY,
    ];
    for target in TARGETS {
        for (tick, beat) in beats.into_iter().enumerate() {
            let current = &states[tick % states.len()];
            let next = overlay_state_update_value(&states[(tick + 1) % states.len()], target);
            assert_eq!(
                push(
                    false,
                    &mut new,
                    &mut ni,
                    tick % 3,
                    target,
                    beat,
                    current,
                    beat + 0.125,
                    &next
                ),
                push(
                    true,
                    &mut old,
                    &mut oi,
                    tick % 3,
                    target,
                    beat,
                    current,
                    beat + 0.125,
                    &next
                )
            );
            assert_tracks(&new, &old);
            assert_eq!(ni, oi);
        }
    }
    let colors = Arc::new([[0.25; 4]; 4]);
    let next = SongLuaOverlayUpdateValue::VertexColors(Arc::clone(&colors));
    let mut tracks = Vec::new();
    let mut indices = Indices::default();
    push(
        false,
        &mut tracks,
        &mut indices,
        0,
        SongLuaOverlayUpdateTarget::VertexColors,
        9.0,
        &states[0],
        10.0,
        &next,
    );
    assert_eq!(tracks[0].samples.len(), 1);
    assert_eq!(tracks[0].samples[0].beat, 10.0);
    let SongLuaOverlayUpdateValue::VertexColors(retained) = &tracks[0].samples[0].value else {
        panic!("missing colors")
    };
    assert!(Arc::ptr_eq(retained, &colors));
}

struct Values {
    states: Vec<SongLuaOverlayState>,
    writes: Vec<[SongLuaOverlayUpdateValue; 2]>,
    tracks: Vec<SongLuaOverlayUpdateTrack>,
    indices: Indices,
    target: SongLuaOverlayUpdateTarget,
}
impl Values {
    fn new(count: usize, colors: bool) -> Self {
        let states = vec![
            SongLuaOverlayState {
                vertex_colors: colors.then_some([[0.25; 4]; 4]),
                ..Default::default()
            };
            count
        ];
        let target = if colors {
            SongLuaOverlayUpdateTarget::VertexColors
        } else {
            SongLuaOverlayUpdateTarget::X
        };
        let writes: Vec<_> = states
            .iter()
            .map(|state| {
                [
                    overlay_state_update_value(state, target),
                    if colors {
                        SongLuaOverlayUpdateValue::VertexColors(Arc::new([[0.75; 4]; 4]))
                    } else {
                        SongLuaOverlayUpdateValue::F32(19.0)
                    },
                ]
            })
            .collect();
        let mut tracks = Vec::new();
        let mut indices = Indices::default();
        for (i, values) in writes.iter().enumerate() {
            indices.insert((i, target), i);
            let mut samples = Vec::with_capacity(40);
            samples.push(SongLuaOverlayUpdateSample {
                beat: 0.0,
                value: values[0].clone(),
            });
            tracks.push(SongLuaOverlayUpdateTrack {
                overlay_index: i,
                target,
                samples,
            });
        }
        Self {
            states,
            writes,
            tracks,
            indices,
            target,
        }
    }
    fn batch(&mut self, old: bool, changed: bool, gaps: bool) {
        for tick in 0..16 {
            let beat = if gaps {
                (tick + 1) as f32
            } else {
                tick as f32 * 0.125
            };
            let next_beat = beat + 0.125;
            for i in 0..self.states.len() {
                let next = &self.writes[i][if changed { (tick + 1) % 2 } else { 0 }];
                black_box(push(
                    old,
                    &mut self.tracks,
                    &mut self.indices,
                    i,
                    self.target,
                    beat,
                    &self.states[i],
                    next_beat,
                    next,
                ));
                set_overlay_state_update_value(&mut self.states[i], self.target, next);
            }
        }
    }
}

#[test]
fn capture_sampling_lazy_colors_have_zero_warm_churn_and_match_scalar_controls() {
    for colors in [false, true] {
        let mut old = Values::new(32, colors);
        let mut new = Values::new(32, colors);
        old.batch(true, false, false);
        new.batch(false, false, false);
        crate::perf::assert_no_churn(|| new.batch(false, false, false));
        if colors {
            crate::perf::assert_reduced_churn(
                || old.batch(true, false, false),
                || new.batch(false, false, false),
            );
        } else {
            crate::perf::assert_no_churn(|| old.batch(true, false, false));
        }
        assert_tracks(&new.tracks, &old.tracks);
    }
}

struct Capture {
    lua: Lua,
    context: SongLuaCompileContext,
    overlays: Vec<SongLuaOverlayCompileActor<()>>,
    states: Vec<SongLuaOverlayState>,
    from: Vec<SongLuaOverlayState>,
    update: Vec<SongLuaOverlayState>,
    to: Vec<SongLuaOverlayState>,
    tracks: Vec<SongLuaOverlayUpdateTrack>,
    indices: Indices,
    scheduled: Vec<SongLuaScheduledOverlaySample>,
    scratch: OverlaySampleScratch,
}
impl Capture {
    fn new(count: usize) -> Self {
        let lua = Lua::new();
        let states = vec![
            SongLuaOverlayState {
                vertex_colors: Some([[0.25; 4]; 4]),
                ..Default::default()
            };
            count
        ];
        let overlays: Vec<_> = states
            .iter()
            .map(|state| {
                let table = lua.create_table().unwrap();
                reset_actor_capture(&lua, &table).unwrap();
                SongLuaOverlayCompileActor {
                    table,
                    actor: crate::SongLuaOverlayActor {
                        kind: (),
                        name: None,
                        parent_index: None,
                        initial_state: *state,
                        message_commands: vec![],
                    },
                    message_sounds: vec![],
                }
            })
            .collect();
        crate::lua_util::begin_overlay_update_capture_from_indices(
            &lua,
            overlays
                .iter()
                .enumerate()
                .map(|(i, a)| (a.table.to_pointer() as usize, i)),
        );
        Self {
            lua,
            context: SongLuaCompileContext::new("", "Captured output comparison"),
            overlays,
            from: states.clone(),
            update: states.clone(),
            to: states.clone(),
            states,
            tracks: Vec::new(),
            indices: Indices::default(),
            scheduled: Vec::new(),
            scratch: OverlaySampleScratch::default(),
        }
    }
    fn frame(&mut self, old: bool, beat: f32, colors: [[f32; 4]; 4], restored: &[usize]) {
        for actor in self.overlays.iter().rev() {
            crate::capture_block_set_vertex_colors(&self.lua, &actor.table, colors).unwrap();
        }
        if old {
            baseline::capture_update_overlay_samples(
                &self.lua,
                &self.context,
                &self.overlays,
                &self.states,
                &self.from,
                &mut self.update,
                &mut self.to,
                restored,
                &mut self.tracks,
                &mut self.indices,
                beat,
                beat + 0.125,
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
                &mut self.update,
                &mut self.to,
                restored,
                &mut self.tracks,
                &mut self.indices,
                beat,
                beat + 0.125,
                f64::from(beat),
                &mut self.scheduled,
                &mut self.scratch,
            )
        }
        .unwrap();
    }
    fn batch(&mut self, old: bool) {
        for tick in 0..16 {
            self.frame(old, tick as f32 * 0.125, [[0.25; 4]; 4], &[]);
        }
    }
}

#[test]
fn capture_sampling_complete_capture_preserves_fallbacks_restored_messages_and_unchanged_write_precedence()
 {
    let mut old = Capture::new(3);
    let mut new = Capture::new(3);
    old.from.truncate(1);
    new.from.truncate(1);
    for (tick, colors) in [
        [[0.25; 4]; 4],
        [[0.25; 4]; 4],
        [[0.75; 4]; 4],
        [[f32::from_bits(0x7fc00003); 4]; 4],
        [[f32::from_bits(0x7fc00003); 4]; 4],
    ]
    .into_iter()
    .enumerate()
    {
        for fixture in [&mut old, &mut new] {
            fixture.to[2].vertex_colors = Some([[0.1; 4]; 4]);
        }
        old.frame(true, tick as f32, colors, &[2]);
        new.frame(false, tick as f32, colors, &[2]);
        assert_tracks(&new.tracks, &old.tracks);
        assert_eq!(new.indices, old.indices);
        for (a, b) in new
            .update
            .iter()
            .zip(&old.update)
            .chain(new.to.iter().zip(&old.to))
        {
            assert_eq!(
                value_bits(&overlay_state_update_value(
                    a,
                    SongLuaOverlayUpdateTarget::VertexColors
                )),
                value_bits(&overlay_state_update_value(
                    b,
                    SongLuaOverlayUpdateTarget::VertexColors
                ))
            );
        }
        assert_eq!(new.to[2].vertex_colors, Some([[0.1; 4]; 4]));
        assert!(new.scheduled.is_empty());
        assert!(old.scheduled.is_empty());
    }
    assert_eq!(
        new.tracks
            .iter()
            .map(|track| track.overlay_index)
            .collect::<Vec<_>>(),
        [2, 1, 0]
    );
}

fn context(segments: usize) -> SongLuaCompileContext {
    let mut context = SongLuaCompileContext::new("", "Scheduled timing comparison");
    context.song_music_rate = 1.25;
    context.song_timing_bpms = (0..segments)
        .map(|i| (i as f32 * 0.25, [120.0, 180.0, 90.0][i % 3]))
        .collect();
    context
}

fn updates(count: usize, shared: bool) -> Vec<SongLuaScheduledOverlayUpdate> {
    (0..count)
        .map(|i| SongLuaScheduledOverlayUpdate {
            dispatch_seconds: None,
            frame_advance: 0.0,
            initial_value: None,
            delay_seconds: if shared { 0.25 } else { i as f32 * 0.125 },
            duration_seconds: 0.5,
            easing: None,
            opt1: Some(-0.0),
            target: [
                SongLuaOverlayUpdateTarget::X,
                SongLuaOverlayUpdateTarget::Y,
                SongLuaOverlayUpdateTarget::ZoomX,
                SongLuaOverlayUpdateTarget::Visible,
            ][i % 4],
            value: if i % 4 == 3 {
                SongLuaOverlayUpdateValue::Bool(false)
            } else {
                SongLuaOverlayUpdateValue::F32(i as f32)
            },
        })
        .collect()
}

fn assert_scheduled(a: &[SongLuaScheduledOverlaySample], b: &[SongLuaScheduledOverlaySample]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(
            (a.overlay_index, a.target, &a.easing),
            (b.overlay_index, b.target, &b.easing)
        );
        assert_eq!(
            [a.start_seconds, a.end_seconds].map(f64::to_bits),
            [b.start_seconds, b.end_seconds].map(f64::to_bits)
        );
        assert_eq!(
            [a.start_beat, a.end_beat].map(f32::to_bits),
            [b.start_beat, b.end_beat].map(f32::to_bits)
        );
        assert_eq!(a.opt1.map(f32::to_bits), b.opt1.map(f32::to_bits));
        assert_eq!(value_bits(&a.from), value_bits(&b.from));
        assert_eq!(value_bits(&a.value), value_bits(&b.value));
    }
}

#[test]
fn capture_sampling_tween_clock_preserves_float_bits_time_changes_context_changes_and_prior_colors()
{
    let states = [SongLuaOverlayState {
        vertex_colors: Some([[0.0, -0.0, 0.5, 1.0]; 4]),
        ..Default::default()
    }];
    let pairs = [
        (0.0, 0.0),
        (-0.0, -0.0),
        (0.25, 0.5),
        (-1.0, 0.5),
        (f32::INFINITY, 0.0),
        (f32::NEG_INFINITY, f32::INFINITY),
        (f32::from_bits(0x7fc00003), 0.5),
        (0.0, f32::from_bits(0xffc00001)),
    ];
    let mut input = updates(128, true);
    for (i, update) in input.iter_mut().enumerate() {
        (update.delay_seconds, update.duration_seconds) = pairs[(i / 4) % pairs.len()];
        if i % 5 == 0 {
            update.target = SongLuaOverlayUpdateTarget::VertexColors;
            update.value = SongLuaOverlayUpdateValue::VertexColors(Arc::new(
                [[f32::from_bits(0x7fc00005); 4]; 4],
            ));
        }
        update.easing = (i % 3 == 0).then(|| "linear\0\u{e9}".into());
    }
    for segments in [0, 3, 128] {
        let mut context = context(segments);
        for rate in [0.5, 1.25, 2.0] {
            context.song_music_rate = rate;
            for actor in [0, usize::MAX] {
                for seconds in [-0.0, 0.0, 1.0, 48.0, f64::MAX, f64::INFINITY, f64::NAN] {
                    for count in [0, 1, input.len()] {
                        let mut old = Vec::new();
                        let mut new = Vec::new();
                        baseline::append_scheduled_overlay_updates(
                            &mut old,
                            &context,
                            &states,
                            actor,
                            &input[..count],
                            seconds,
                        );
                        append_scheduled_overlay_updates(
                            &mut new,
                            &context,
                            &states,
                            actor,
                            &input[..count],
                            seconds,
                        );
                        assert_scheduled(&new, &old);
                    }
                }
            }
        }
    }
    let mut output = Vec::new();
    append_scheduled_overlay_updates(&mut output, &context(3), &states, 0, &input, 2.0);
    let SongLuaOverlayUpdateValue::VertexColors(input_colors) = &input[0].value else {
        unreachable!()
    };
    let SongLuaOverlayUpdateValue::VertexColors(output_colors) = &output[0].value else {
        unreachable!()
    };
    assert!(Arc::ptr_eq(input_colors, output_colors));
}

#[test]
fn capture_sampling_tween_clock_reuses_output_without_allocation_churn() {
    let context = context(128);
    let states = [SongLuaOverlayState::default()];
    for shared in [true, false] {
        let input = updates(64, shared);
        let mut old = Vec::with_capacity(64);
        let mut new = Vec::with_capacity(64);
        baseline::append_scheduled_overlay_updates(&mut old, &context, &states, 0, &input, 48.0);
        append_scheduled_overlay_updates(&mut new, &context, &states, 0, &input, 48.0);
        assert_scheduled(&new, &old);
        crate::perf::assert_no_churn(|| {
            new.clear();
            append_scheduled_overlay_updates(&mut new, &context, &states, 0, &input, 48.0);
        });
        crate::perf::assert_no_churn(|| {
            old.clear();
            baseline::append_scheduled_overlay_updates(
                &mut old, &context, &states, 0, &input, 48.0,
            );
        });
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially"]
fn capture_sampling_outputs_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for count in [1, 32, 128] {
        for colors in [false, true] {
            for old in order {
                let mut fixture = Values::new(count, colors);
                fixture.batch(old, false, false);
                crate::perf::measure_sampled(
                    &format!(
                        "captured_values_{count}_colors_{colors}/{}",
                        if old { "old" } else { "new" }
                    ),
                    128,
                    count * 16,
                    || fixture.batch(old, false, false),
                );
            }
        }
    }
    for gaps in [false, true] {
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!(
                    "captured_changes_32_gaps_{gaps}/{}",
                    if old { "old" } else { "new" }
                ),
                128,
                32 * 16,
                || Values::new(32, true),
                |fixture| fixture.batch(old, true, gaps),
            );
        }
    }
    for count in [1, 16, 64] {
        for old in order {
            let mut fixture = Capture::new(count);
            fixture.batch(old);
            crate::perf::measure_sampled(
                &format!(
                    "complete_capture_{count}/{}",
                    if old { "old" } else { "new" }
                ),
                64,
                count * 16,
                || fixture.batch(old),
            );
        }
    }
    for segments in [0, 8, 128] {
        let context = context(segments);
        let states = [SongLuaOverlayState::default()];
        for count in [1, 4, 32] {
            for shared in [true, false] {
                let input = updates(count, shared);
                for old in order {
                    let mut output = Vec::with_capacity(count);
                    crate::perf::measure_sampled(
                        &format!(
                            "tween_clock_{segments}_{count}_shared_{shared}/{}",
                            if old { "old" } else { "new" }
                        ),
                        128,
                        count,
                        || {
                            output.clear();
                            if old {
                                baseline::append_scheduled_overlay_updates(
                                    &mut output,
                                    &context,
                                    &states,
                                    0,
                                    &input,
                                    48.0,
                                );
                            } else {
                                append_scheduled_overlay_updates(
                                    &mut output,
                                    &context,
                                    &states,
                                    0,
                                    &input,
                                    48.0,
                                );
                            }
                            black_box(&output);
                        },
                    );
                }
            }
        }
    }
}
