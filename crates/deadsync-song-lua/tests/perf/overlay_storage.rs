use super::*;
use crate::lua_util::SongLuaScheduledOverlayUpdate;
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;

#[path = "overlay_storage_baseline.rs"]
mod baseline;

// Exercises each discriminant used by the fixed target lookup.
const TARGETS: &[SongLuaOverlayUpdateTarget] = &[
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

fn value(index: usize) -> SongLuaOverlayUpdateValue {
    use SongLuaOverlayUpdateValue as V;
    match index % 8 {
        0 => V::F32(f32::from_bits(0x7fc00003)),
        1 => V::Vec2([-0.0, f32::INFINITY]),
        2 => V::Vec3([1.0, -2.0, f32::NEG_INFINITY]),
        3 => V::Vec4([0.0, 0.5, 0.75, 1.0]),
        4 => V::Vec5([1.0, 2.0, 3.0, 4.0, 5.0]),
        5 => V::VertexColors(Arc::new([[0.0, -0.0, 0.5, 1.0]; 4])),
        6 => V::Bool(false),
        _ => V::None,
    }
}

fn assert_value(a: &SongLuaOverlayUpdateValue, b: &SongLuaOverlayUpdateValue) {
    use SongLuaOverlayUpdateValue as V;
    let bits = |v: &V| -> Option<Vec<u32>> {
        Some(match v {
            V::F32(v) => vec![v.to_bits()],
            V::Vec2(v) => v.map(f32::to_bits).to_vec(),
            V::Vec3(v) => v.map(f32::to_bits).to_vec(),
            V::Vec4(v) => v.map(f32::to_bits).to_vec(),
            V::Vec5(v) => v.map(f32::to_bits).to_vec(),
            V::VertexColors(v) => v.iter().flatten().map(|v| v.to_bits()).collect(),
            _ => return None,
        })
    };
    assert_eq!(std::mem::discriminant(a), std::mem::discriminant(b));
    if let Some(bits_a) = bits(a) {
        assert_eq!(Some(bits_a), bits(b));
    } else {
        assert_eq!(a, b);
    }
}

fn assert_scheduled(
    actual: &[SongLuaScheduledOverlaySample],
    expected: &[SongLuaScheduledOverlaySample],
) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
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
        assert_value(&a.from, &b.from);
        assert_value(&a.value, &b.value);
    }
}

fn updates(count: usize, distinct: usize, colors: bool) -> Vec<SongLuaScheduledOverlayUpdate> {
    (0..count)
        .map(|index| SongLuaScheduledOverlayUpdate {
            delay_seconds: index as f32 * 0.125,
            duration_seconds: 0.5,
            easing: None,
            opt1: None,
            target: TARGETS[index % distinct],
            value: if colors {
                SongLuaOverlayUpdateValue::VertexColors(Arc::new([[index as f32; 4]; 4]))
            } else {
                SongLuaOverlayUpdateValue::F32(index as f32)
            },
        })
        .collect()
}

#[test]
fn scheduled_lookup_matches_parent_for_every_target_duplicates_missing_states_and_float_bits() {
    let mut context = SongLuaCompileContext::new(".", "Overlay storage");
    context.song_timing_bpms = vec![(0.0, 120.0), (2.0, 240.0), (4.0, 90.0)];
    context.song_music_rate = 1.25;
    let states = [SongLuaOverlayState {
        x: 19.0,
        vertex_colors: Some([[0.25; 4]; 4]),
        ..Default::default()
    }; 2];
    for count in [0, 1, 16, 77, 256] {
        for seed in 0..8 {
            let mut input = updates(count, TARGETS.len(), false);
            for (index, update) in input.iter_mut().enumerate() {
                update.target = TARGETS[(index * 31 + seed) % TARGETS.len()];
                update.value = value(index + seed);
                update.delay_seconds = [-0.0, -1.0, 0.25, f32::INFINITY, f32::NAN][index % 5];
                update.duration_seconds = [0.0, 0.5, -1.0][index % 3];
                update.easing = (index % 2 == 0).then(|| "linear".to_owned());
                update.opt1 = Some(f32::from_bits(0xffc00001));
            }
            let mut old = Vec::new();
            let mut new = Vec::new();
            for actor in [0, 1, usize::MAX, 0] {
                baseline::append_scheduled_overlay_updates(
                    &mut old, &context, &states, actor, &input, -0.25,
                );
                append_scheduled_overlay_updates(&mut new, &context, &states, actor, &input, -0.25);
                assert_scheduled(&new, &old);
            }
        }
    }
}

#[test]
fn scheduled_lookup_keeps_prior_values_per_actor_and_reuses_output_without_churn() {
    let context = SongLuaCompileContext::new(".", "Overlay storage");
    let states = [SongLuaOverlayState {
        x: 19.0,
        ..Default::default()
    }];
    let input = updates(128, 1, false);
    let mut output = Vec::with_capacity(256);
    crate::perf::assert_no_churn(|| {
        for _ in 0..16 {
            output.clear();
            append_scheduled_overlay_updates(&mut output, &context, &states, 0, &input, 1.0);
            append_scheduled_overlay_updates(&mut output, &context, &states, 0, &input, 2.0);
        }
    });
    assert_eq!(output[0].from, SongLuaOverlayUpdateValue::F32(19.0));
    assert_eq!(output[1].from, SongLuaOverlayUpdateValue::F32(0.0));
    assert_eq!(output[128].from, SongLuaOverlayUpdateValue::F32(19.0));
    let colors = updates(2, 1, true);
    output.clear();
    append_scheduled_overlay_updates(&mut output, &context, &states, 0, &colors, 0.0);
    let SongLuaOverlayUpdateValue::VertexColors(original) = &colors[0].value else {
        unreachable!()
    };
    let SongLuaOverlayUpdateValue::VertexColors(from) = &output[1].from else {
        panic!("missing prior colors")
    };
    assert!(Arc::ptr_eq(original, from));
    assert_eq!(Arc::strong_count(original), 3);
}

fn command(message: &str, x: f32, duration: f32) -> crate::SongLuaOverlayMessageCommand {
    crate::SongLuaOverlayMessageCommand {
        message: message.to_owned(),
        aux: Some(x),
        blocks: vec![crate::SongLuaOverlayCommandBlock {
            start: 0.0,
            duration,
            easing: None,
            opt1: None,
            opt2: None,
            delta: crate::SongLuaOverlayStateDelta {
                x: Some(x),
                ..Default::default()
            },
        }],
    }
}

struct Actors {
    lua: Lua,
    context: SongLuaCompileContext,
    overlays: Vec<SongLuaOverlayCompileActor<()>>,
    states: Vec<SongLuaOverlayState>,
}

impl Actors {
    fn new(count: usize) -> Self {
        let lua = Lua::new();
        let overlays = (0..count)
            .map(|index| {
                let table = lua.create_table().unwrap();
                set_actor_overlay_getter_state(&lua, &table, SongLuaOverlayState::default())
                    .unwrap();
                reset_actor_capture(&lua, &table).unwrap();
                SongLuaOverlayCompileActor {
                    table,
                    actor: crate::SongLuaOverlayActor {
                        kind: (),
                        name: None,
                        parent_index: None,
                        initial_state: SongLuaOverlayState::default(),
                        message_commands: vec![
                            command("A", index as f32 + 10.0, 0.25),
                            command("B", -(index as f32), 0.0),
                            command("A", 999.0, 0.0),
                        ],
                    },
                    message_sounds: vec![],
                }
            })
            .collect();
        Self {
            lua,
            context: SongLuaCompileContext::new(".", "Overlay replay"),
            overlays,
            states: vec![SongLuaOverlayState::default(); count],
        }
    }
}

fn assert_actors(actual: &Actors, expected: &Actors) {
    assert_eq!(actual.states, expected.states);
    for (a, b) in actual.overlays.iter().zip(&expected.overlays) {
        assert_eq!(
            actor_overlay_initial_state(&a.table).unwrap(),
            actor_overlay_initial_state(&b.table).unwrap()
        );
        assert_eq!(
            a.table.get::<Option<f32>>("__songlua_aux").unwrap(),
            b.table.get::<Option<f32>>("__songlua_aux").unwrap()
        );
    }
}

fn messages(entries: &[(f32, &str)]) -> Vec<SongLuaMessageEvent> {
    entries
        .iter()
        .map(|&(beat, message)| SongLuaMessageEvent {
            beat,
            message: message.to_owned(),
            persists: true,
        })
        .collect()
}

#[test]
fn replay_matches_parent_for_order_ties_boundaries_rewinds_and_first_matching_command() {
    let events = messages(&[
        (2.0, "A"),
        (0.0, "B"),
        (1.0, "A"),
        (0.0, "A"),
        (1.0, "missing"),
        (f32::EPSILON, "B"),
    ]);
    let mut old = Actors::new(4);
    let mut new = Actors::new(4);
    // Different actor command sets make first-touch order differ from index order.
    for actors in [&mut old, &mut new] {
        actors.overlays[1]
            .actor
            .message_commands
            .retain(|command| command.message == "A");
        actors.overlays[2]
            .actor
            .message_commands
            .retain(|command| command.message == "B");
        actors.states.truncate(3);
    }
    let mut old_replay = baseline::SongLuaPerframeMessageReplay::new(&events, 4);
    let mut new_replay = SongLuaPerframeMessageReplay::new(&events, 4);
    for beat in [-1.0, 0.0, 0.125, 0.5, 1.0, 1.25, 2.0, 2.5, -0.5] {
        let old_started = old_replay
            .advance(
                &old.lua,
                &old.context,
                &mut old.overlays,
                &mut old.states,
                beat,
            )
            .unwrap();
        let new_started = new_replay
            .advance(
                &new.lua,
                &new.context,
                &mut new.overlays,
                &mut new.states,
                beat,
            )
            .unwrap();
        assert_eq!(new_started, old_started);
        if beat == 0.0 {
            assert_eq!(new_started, &[0, 1, 2, 3]);
        }
        assert_actors(&new, &old);
    }
    assert!(new.states[0].x < 999.0);
}

#[test]
fn replay_clears_partial_result_after_lua_error_and_preserves_retry_behavior() {
    let events = messages(&[(0.0, "A")]);
    let mut old = Actors::new(2);
    let mut new = Actors::new(2);
    for actors in [&mut old, &mut new] {
        let meta = actors.lua.create_table().unwrap();
        let fail: Function = actors
            .lua
            .load("return function() error('injected replay error', 0) end")
            .eval()
            .unwrap();
        meta.set("__newindex", fail).unwrap();
        actors.overlays[1].table.set_metatable(Some(meta)).unwrap();
    }
    let mut old_replay = baseline::SongLuaPerframeMessageReplay::new(&events, 2);
    let mut new_replay = SongLuaPerframeMessageReplay::new(&events, 2);
    let old_error = old_replay
        .advance(
            &old.lua,
            &old.context,
            &mut old.overlays,
            &mut old.states,
            0.0,
        )
        .unwrap_err();
    let new_error = new_replay
        .advance(
            &new.lua,
            &new.context,
            &mut new.overlays,
            &mut new.states,
            0.0,
        )
        .unwrap_err();
    assert_eq!(new_error, old_error);
    assert!(new_error.contains("injected replay error"));
    assert_actors(&new, &old);
    old.overlays[1].table.set_metatable(None).unwrap();
    new.overlays[1].table.set_metatable(None).unwrap();
    let old_started = old_replay
        .advance(
            &old.lua,
            &old.context,
            &mut old.overlays,
            &mut old.states,
            0.0,
        )
        .unwrap();
    let new_started = new_replay
        .advance(
            &new.lua,
            &new.context,
            &mut new.overlays,
            &mut new.states,
            0.0,
        )
        .unwrap();
    assert_eq!(new_started, old_started);
    assert_eq!(new_started, &[0, 1]);
    assert_actors(&new, &old);
}

fn old_indices(input: &[usize]) -> Vec<usize> {
    // Result-collection statements from the frozen advance method above.
    let mut started = Vec::new();
    for &index in input {
        started.push(index);
    }
    started.sort_unstable();
    started.dedup();
    started
}

fn collect_indices<'a>(
    scratch: &'a mut StartedOverlayIndices,
    count: usize,
    input: &[usize],
) -> &'a [usize] {
    scratch.clear();
    for &index in input {
        scratch.insert(index, count);
    }
    scratch.as_sorted_slice()
}

#[test]
fn replay_result_indices_match_parent_for_duplicate_batches_and_reuse_storage() {
    for actors in [0, 1, 16, 128, 1024] {
        let mut scratch = StartedOverlayIndices::default();
        for seed in 0..16 {
            let input: Vec<_> = (0..actors * 8).map(|i| (i * 31 + seed) % actors).collect();
            assert_eq!(
                collect_indices(&mut scratch, actors, &input),
                old_indices(&input)
            );
            assert!(collect_indices(&mut scratch, actors, &[]).is_empty());
        }
        if actors > 0 {
            let input: Vec<_> = (0..actors * 8).map(|i| (i * 31) % actors).collect();
            collect_indices(&mut scratch, actors, &input);
            crate::perf::assert_no_churn(|| {
                for _ in 0..16 {
                    black_box(collect_indices(&mut scratch, actors, &input));
                }
            });
            assert_eq!(scratch.indices.len(), actors);
        } else {
            assert_eq!(scratch.indices.capacity(), 0);
            assert_eq!(scratch.seen.capacity(), 0);
        }
    }
}

#[test]
fn scheduled_capture_matches_parent_across_actor_boundaries_and_message_precedence() {
    let mut old = Actors::new(3);
    let mut new = Actors::new(3);
    let baseline_states = vec![SongLuaOverlayState::default(); 3];
    for actors in [&old, &new] {
        crate::lua_util::begin_overlay_update_capture(
            &actors.lua,
            actors
                .overlays
                .iter()
                .enumerate()
                .map(|(i, actor)| (actor.table.to_pointer() as usize, i))
                .collect(),
        );
    }
    let mut old_scratch = OverlaySampleScratch::default();
    let mut new_scratch = OverlaySampleScratch::default();
    let mut old_tracks = Vec::new();
    let mut new_tracks = Vec::new();
    let mut old_indices = HashMap::new();
    let mut new_indices = HashMap::new();
    let mut old_scheduled = Vec::new();
    let mut new_scheduled = Vec::new();
    let mut old_next = baseline_states.clone();
    let mut new_next = baseline_states.clone();
    for tick in 0..5 {
        for actors in [&old, &new] {
            for overlay in &actors.overlays {
                overlay
                    .table
                    .set("__songlua_capture_duration", 0.5)
                    .unwrap();
                for (key, value) in [("x", 3.0), ("y", 8.0), ("x", 12.0)] {
                    crate::lua_util::capture_block_set_f32(
                        &actors.lua,
                        &overlay.table,
                        key,
                        value + tick as f32,
                    )
                    .unwrap();
                }
            }
        }
        baseline::capture_update_overlay_samples(
            &old.lua,
            &old.context,
            &old.overlays,
            &baseline_states,
            &baseline_states,
            &mut old.states,
            &mut old_next,
            &[1],
            &mut old_tracks,
            &mut old_indices,
            tick as f32,
            tick as f32,
            tick as f64,
            &mut old_scheduled,
            &mut old_scratch,
        )
        .unwrap();
        capture_update_overlay_samples(
            &new.lua,
            &new.context,
            &new.overlays,
            &baseline_states,
            &baseline_states,
            &mut new.states,
            &mut new_next,
            &[1],
            &mut new_tracks,
            &mut new_indices,
            tick as f32,
            tick as f32,
            tick as f64,
            &mut new_scheduled,
            &mut new_scratch,
        )
        .unwrap();
        assert_scheduled(&new_scheduled, &old_scheduled);
        assert_eq!(new_tracks, old_tracks);
        assert_eq!(new_indices, old_indices);
        assert_eq!(new_next, old_next);
        assert_actors(&new, &old);
    }
}

fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        crate::perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
        crate::perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
    } else {
        crate::perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
        crate::perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn overlay_storage_bench_perframe() {
    let context = SongLuaCompileContext::new(".", "Overlay storage benchmark");
    let states = [SongLuaOverlayState::default()];
    for (count, distinct, colors) in [
        (0, 1, false),
        (1, 1, false),
        (16, 1, false),
        (128, 1, false),
        (128, 16, false),
        (512, 77, false),
        (128, 1, true),
        (128, 16, true),
    ] {
        let input = updates(count, distinct, colors);
        let mut old = Vec::with_capacity(count);
        let mut new = Vec::with_capacity(count);
        baseline::append_scheduled_overlay_updates(&mut old, &context, &states, 0, &input, 0.0);
        append_scheduled_overlay_updates(&mut new, &context, &states, 0, &input, 0.0);
        assert_scheduled(&new, &old);
        pair(
            &format!("scheduled_{count}_targets_{distinct}_colors_{colors}"),
            512,
            count.max(1),
            || {
                old.clear();
                baseline::append_scheduled_overlay_updates(
                    &mut old,
                    black_box(&context),
                    &states,
                    0,
                    black_box(&input),
                    0.0,
                );
                black_box(&old);
            },
            || {
                new.clear();
                append_scheduled_overlay_updates(
                    &mut new,
                    black_box(&context),
                    &states,
                    0,
                    black_box(&input),
                    0.0,
                );
                black_box(&new);
            },
        );
    }
    for (actors, events) in [(0, 0), (1, 1), (16, 1), (128, 1), (128, 16), (1024, 16)] {
        let input: Vec<_> = (0..actors * events).map(|i| i % actors).collect();
        let mut scratch = StartedOverlayIndices::default();
        assert_eq!(
            collect_indices(&mut scratch, actors, &input),
            old_indices(&input)
        );
        pair(
            &format!("replay_indices_{actors}x{events}"),
            256,
            input.len().max(1),
            || old_indices(black_box(&input)),
            || {
                black_box(collect_indices(&mut scratch, actors, black_box(&input)));
            },
        );
    }
    for (actor_count, event_count) in [(0, 16), (4, 1), (4, 16)] {
        let events = messages(&vec![(0.0, "A"); event_count]);
        let mut old = Actors::new(actor_count);
        let mut new = Actors::new(actor_count);
        pair(
            &format!("replay_lua_{actor_count}x{event_count}"),
            8,
            (actor_count * event_count).max(1),
            || {
                old.states.fill(SongLuaOverlayState::default());
                let mut replay = baseline::SongLuaPerframeMessageReplay::new(&events, actor_count);
                black_box(
                    replay
                        .advance(
                            &old.lua,
                            &old.context,
                            &mut old.overlays,
                            &mut old.states,
                            0.0,
                        )
                        .unwrap(),
                );
            },
            || {
                new.states.fill(SongLuaOverlayState::default());
                let mut replay = SongLuaPerframeMessageReplay::new(&events, actor_count);
                black_box(
                    replay
                        .advance(
                            &new.lua,
                            &new.context,
                            &mut new.overlays,
                            &mut new.states,
                            0.0,
                        )
                        .unwrap(),
                );
            },
        );
        assert_actors(&new, &old);
    }
}
