use super::*;
use crate::lua_util::SongLuaScheduledOverlayUpdate;
use std::hint::black_box;

#[path = "tween_replay_baseline.rs"]
mod baseline;

// Insert the complete enum order from the existing behavior fixture below.
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

fn fingerprint(state: &SongLuaOverlayState) -> Vec<(String, Vec<u32>)> {
    TARGETS
        .iter()
        .map(|&target| {
            let value = overlay_state_update_value(state, target);
            use SongLuaOverlayUpdateValue as V;
            let bits = match &value {
                V::F32(x) => vec![x.to_bits()],
                V::Vec2(x) => x.map(f32::to_bits).to_vec(),
                V::Vec3(x) => x.map(f32::to_bits).to_vec(),
                V::Vec4(x) => x.map(f32::to_bits).to_vec(),
                V::Vec5(x) => x.map(f32::to_bits).to_vec(),
                V::VertexColors(x) => x.iter().flatten().map(|x| x.to_bits()).collect(),
                _ => vec![],
            };
            (format!("{value:?}"), bits)
        })
        .collect()
}

fn sample(index: usize, easing: Option<&str>, shared: bool) -> SongLuaScheduledOverlaySample {
    SongLuaScheduledOverlaySample {
        overlay_index: index % 16,
        target: [
            SongLuaOverlayUpdateTarget::X,
            SongLuaOverlayUpdateTarget::Y,
            SongLuaOverlayUpdateTarget::ZoomX,
            SongLuaOverlayUpdateTarget::RotationZ,
        ][index % 4],
        start_seconds: if shared { 0.25 } else { index as f64 * 0.0001 },
        end_seconds: 2.0,
        start_beat: 0.5,
        end_beat: 4.0,
        easing: easing.map(str::to_owned),
        opt1: Some(0.37),
        from: SongLuaOverlayUpdateValue::F32(-0.0),
        value: SongLuaOverlayUpdateValue::F32(index as f32 + 1.0),
    }
}

fn replay(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<()>],
    states: &mut [SongLuaOverlayState],
    input: &[SongLuaScheduledOverlaySample],
    seconds: f64,
    old: bool,
) -> Result<(), String> {
    if old {
        baseline::apply_scheduled_overlay_states(lua, overlays, states, input, seconds)
    } else {
        apply_scheduled_overlay_states(lua, overlays, states, input, seconds)
    }
}

#[test]
fn tween_replay_factor_reuse_matches_frozen_curves_boundaries_bits_and_mixed_batches() {
    let lua = Lua::new();
    let mut input = Vec::new();
    for easing in [
        None,
        Some("linear"),
        Some("instant"),
        Some("inQuad"),
        Some("outQuad"),
        Some("inOutQuad"),
        Some("spring"),
        Some("outElastic"),
        Some("inBounce"),
        Some("outBounce"),
        Some("Spring"),
        Some("unknown\0\u{e9}"),
    ] {
        for (start, end) in [
            (0.25, 2.0),
            (-0.0, 0.0),
            (1.0, 1.0),
            (2.0, 1.0),
            (f64::NAN, 2.0),
            (0.0, f64::INFINITY),
            (f64::NEG_INFINITY, 2.0),
        ] {
            for opt in [
                None,
                Some(-0.0),
                Some(0.0),
                Some(0.37),
                Some(-1.0),
                Some(f32::NAN),
                Some(f32::INFINITY),
            ] {
                for _ in 0..3 {
                    let mut s = sample(input.len(), easing, true);
                    s.start_seconds = start;
                    s.end_seconds = end;
                    s.opt1 = opt;
                    input.push(s);
                }
            }
        }
    }
    // Start the mixed sequence with a reusable expensive pair so both the
    // cache-hit and cache-miss cases run in the same call.
    let start = input
        .iter()
        .position(|sample| sample.easing.as_deref() == Some("spring"))
        .unwrap();
    input.rotate_left(start);
    for seconds in [
        f64::NEG_INFINITY,
        -1.0,
        -0.0,
        0.0,
        0.25 - f64::EPSILON,
        0.25,
        0.5,
        1.0,
        2.0,
        f64::INFINITY,
        f64::NAN,
    ] {
        let mut a = vec![SongLuaOverlayState::default(); 16];
        let mut b = a.clone();
        replay(&lua, &[], &mut a, &input, seconds, true).unwrap();
        replay(&lua, &[], &mut b, &input, seconds, false).unwrap();
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(fingerprint(a), fingerprint(b));
        }
        // Check each curve before later writes can overwrite its result.
        for group in input.chunks(3) {
            let mut a = [SongLuaOverlayState::default(); 16];
            let mut b = a;
            replay(&lua, &[], &mut a, group, seconds, true).unwrap();
            replay(&lua, &[], &mut b, group, seconds, false).unwrap();
            for (a, b) in a.iter().zip(&b) {
                assert_eq!(
                    [a.x, a.y, a.zoom_x, a.rot_z_deg].map(f32::to_bits),
                    [b.x, b.y, b.zoom_x, b.rot_z_deg].map(f32::to_bits)
                );
            }
        }
    }
    // Random finite and non-finite keys, with repeated adjacent equal-bit keys.
    let mut seed = 0x0123456789abcdef_u64;
    for _ in 0..1024 {
        let mut s = sample(0, Some("outElastic"), true);
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        s.start_seconds = f64::from_bits(seed);
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        s.end_seconds = f64::from_bits(seed);
        s.opt1 = Some(f32::from_bits(seed as u32));
        let input = [s.clone(), s];
        let mut a = [SongLuaOverlayState::default()];
        let mut b = a;
        replay(&lua, &[], &mut a, &input, 0.75, true).unwrap();
        replay(&lua, &[], &mut b, &input, 0.75, false).unwrap();
        assert_eq!(fingerprint(&a[0]), fingerprint(&b[0]));
    }
}

fn overlays(lua: &Lua, count: usize) -> Vec<SongLuaOverlayCompileActor<()>> {
    (0..count)
        .map(|_| SongLuaOverlayCompileActor {
            table: lua.create_table().unwrap(),
            actor: crate::SongLuaOverlayActor {
                kind: (),
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: vec![],
            },
            message_sounds: vec![],
        })
        .collect()
}

#[test]
fn tween_replay_factor_reuse_preserves_getter_write_order_errors_and_partial_states() {
    for fail in [0, 2, 9] {
        let mut results = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let mut overlays = overlays(&lua, 1);
            lua.globals().set("fail", fail).unwrap();
            overlays[0].table = lua
                .load(
                    r#"log={};return setmetatable({}, {__newindex=function(t,k,v)
                log[#log+1]=k..':'..(type(v)=='table' and 'table' or tostring(v))
                if #log==fail then error('getter failed') end
            end})"#,
                )
                .eval()
                .unwrap();
            let mut input: Vec<_> = (0..16)
                .map(|i| sample(i, Some("outElastic"), true))
                .collect();
            for s in &mut input {
                s.overlay_index = 0;
            }
            input[1].target = SongLuaOverlayUpdateTarget::VertexColors;
            input[1].from =
                SongLuaOverlayUpdateValue::VertexColors(std::sync::Arc::new([[0.25; 4]; 4]));
            input[1].value = SongLuaOverlayUpdateValue::None;
            input[3].overlay_index = usize::MAX;
            let mut states = vec![SongLuaOverlayState::default()];
            let result = replay(&lua, &overlays, &mut states, &input, 1.0, old);
            let log: String = lua.load("return table.concat(log,'|')").eval().unwrap();
            results.push((
                result.err().map(|e| e.contains("getter failed")),
                log,
                fingerprint(&states[0]),
            ));
        }
        assert_eq!(results[0], results[1]);
        assert_eq!(results[0].0, if fail == 0 { None } else { Some(true) });
    }
}

#[test]
fn tween_replay_factor_reuse_keeps_state_only_and_warm_scalar_getters_allocation_free() {
    let lua = Lua::new();
    lua.gc_stop();
    let overlays = overlays(&lua, 16);
    for easing in [None, Some("spring"), Some("outElastic")] {
        let input: Vec<_> = (0..256).map(|i| sample(i, easing, true)).collect();
        let mut states = vec![SongLuaOverlayState::default(); 16];
        for actors in [&[][..], &overlays[..]] {
            replay(&lua, actors, &mut states, &input, 1.0, false).unwrap();
            crate::perf::assert_no_churn(|| {
                replay(&lua, actors, &mut states, &input, 1.0, false).unwrap();
            });
        }
    }
}

fn writes(count: usize, late: bool) -> Vec<SongLuaScheduledOverlayUpdate> {
    (0..count)
        .map(|i| SongLuaScheduledOverlayUpdate {
            initial_value: None,
            delay_seconds: 0.25,
            duration_seconds: 1.0,
            easing: None,
            opt1: None,
            target: TARGETS[if late { i / 16 % 77 } else { i % 77 }],
            value: SongLuaOverlayUpdateValue::F32(i as f32),
        })
        .collect()
}

#[test]
fn captured_final_target_mask_matches_frozen_loop_for_every_target_duplicates_and_thresholds() {
    let state = SongLuaOverlayState {
        x: 17.0,
        zoom: 2.0,
        vertex_colors: Some([[0.25, -0.0, f32::from_bits(0x7fc00003), 1.0]; 4]),
        ..Default::default()
    };
    let finals: Vec<_> = TARGETS
        .iter()
        .enumerate()
        .map(|(i, &target)| {
            use SongLuaOverlayUpdateTarget as T;
            use SongLuaOverlayUpdateValue as V;
            let value = match overlay_state_update_value(&state, target) {
                V::Bool(value) => V::Bool(!value),
                V::F32(_) => V::F32(i as f32 + 7.5),
                V::I32(_) => V::I32(i as i32 + 7),
                V::U32(_) => V::U32(i as u32 + 7),
                V::Vec2(_) => V::Vec2([7.5, -0.0]),
                V::Vec3(_) => V::Vec3([7.5, -0.0, 0.25]),
                V::Vec4(_) => V::Vec4([7.5, -0.0, 0.25, 1.0]),
                V::Vec5(_) => V::Vec5([7.5, -0.0, 0.25, 1.0, 0.5]),
                V::None => match target {
                    T::Fov | T::MaxWidth | T::MaxHeight => V::F32(7.5),
                    T::Vanishpoint | T::TexcoordOffset | T::TexcoordVelocity => {
                        V::Vec2([7.5, -0.0])
                    }
                    T::SpriteStateIndex => V::U32(7),
                    T::VertSpacing | T::WrapWidthPixels => V::I32(7),
                    T::EffectTiming => V::Vec5([0.25; 5]),
                    T::CustomTextureRect | T::StretchRect => V::Vec4([0.25; 4]),
                    _ => V::None,
                },
                value => value,
            };
            (target, value)
        })
        .collect();
    for count in [0, 1, 4, 5, 15, 16, 17, 77, 256, 1232] {
        for width in [0, 1, 4, 5, 15, 16, 17, 32, 77] {
            for index in [0, 1, 3, usize::MAX] {
                for restored in [&[][..], &[0, 2][..], &[1, 3, usize::MAX][..]] {
                    let scheduled = writes(count, true);
                    let mut a = vec![SongLuaOverlayState::default(); 3];
                    let mut b = a.clone();
                    let mut at = a.clone();
                    let mut bt = a.clone();
                    baseline::apply_captured_final_values(
                        &mut a,
                        &mut at,
                        index,
                        &scheduled,
                        &finals[..width],
                        restored,
                    );
                    apply_captured_final_values(
                        &mut b,
                        &mut bt,
                        index,
                        &scheduled,
                        &finals[..width],
                        restored,
                    );
                    for (a, b) in a.iter().zip(&b).chain(at.iter().zip(&bt)) {
                        assert_eq!(fingerprint(a), fingerprint(b));
                    }
                }
            }
        }
    }
    assert!(TARGETS.iter().enumerate().all(|(i, t)| i == *t as usize));
    let scheduled = writes(1232, true);
    let mut a = [SongLuaOverlayState::default()];
    let mut b = a;
    crate::perf::assert_no_churn(|| {
        apply_captured_final_values(&mut a, &mut b, 0, &scheduled, &finals, &[])
    });
}

const KEYS: [&str; 32] = [
    "x",
    "y",
    "z",
    "z_bias",
    "halign",
    "valign",
    "cropleft",
    "cropright",
    "croptop",
    "cropbottom",
    "fadeleft",
    "faderight",
    "fadetop",
    "fadebottom",
    "zoom",
    "zoom_x",
    "zoom_y",
    "zoom_z",
    "basezoom",
    "basezoom_x",
    "basezoom_y",
    "basezoom_z",
    "rot_x_deg",
    "rot_y_deg",
    "rot_z_deg",
    "skew_x",
    "skew_y",
    "effect_period",
    "effect_offset",
    "sprite_playback_rate",
    "sprite_state_delay",
    "text_distortion",
];

struct Capture {
    lua: Lua,
    overlays: Vec<SongLuaOverlayCompileActor<()>>,
    states: Vec<SongLuaOverlayState>,
    from: Vec<SongLuaOverlayState>,
    update: Vec<SongLuaOverlayState>,
    to: Vec<SongLuaOverlayState>,
    tracks: Vec<SongLuaOverlayUpdateTrack>,
    indices: FxHashMap<(usize, SongLuaOverlayUpdateTarget), usize>,
    pending: Vec<SongLuaScheduledOverlaySample>,
    scratch: OverlaySampleScratch,
    context: SongLuaCompileContext,
}
impl Capture {
    fn new(count: usize, width: usize, repeats: usize) -> Self {
        let lua = Lua::new();
        lua.gc_stop();
        let overlays = overlays(&lua, count);
        crate::lua_util::begin_overlay_update_capture_from_indices(
            &lua,
            overlays
                .iter()
                .enumerate()
                .map(|(i, a)| (a.table.to_pointer() as usize, i)),
        );
        for actor in &overlays {
            reset_actor_capture(&lua, &actor.table).unwrap();
            for key in &KEYS[..width] {
                crate::capture_block_set_f32(&lua, &actor.table, key, 2.0).unwrap();
            }
            actor
                .table
                .raw_set("__songlua_capture_duration", 1.0)
                .unwrap();
            for key in &KEYS[..width] {
                for tick in 0..repeats {
                    crate::capture_block_set_f32(&lua, &actor.table, key, tick as f32 + 3.0)
                        .unwrap();
                }
            }
            actor
                .table
                .raw_set("__songlua_capture_duration", 0.0)
                .unwrap();
            for key in &KEYS[..width] {
                crate::capture_block_set_f32(&lua, &actor.table, key, 9.0).unwrap();
            }
        }
        let states = vec![SongLuaOverlayState::default(); count];
        Self {
            lua,
            overlays,
            from: states.clone(),
            update: states.clone(),
            to: states.clone(),
            states,
            tracks: Vec::new(),
            indices: FxHashMap::default(),
            pending: Vec::new(),
            scratch: OverlaySampleScratch::default(),
            context: SongLuaCompileContext::new("", "Final target reconciliation"),
        }
    }
    fn run(&mut self, old: bool, restored: &[usize]) {
        let call = if old {
            baseline::capture_update_overlay_samples
        } else {
            capture_update_overlay_samples
        };
        call(
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
            0.0,
            0.125,
            1.0,
            &mut self.pending,
            &mut self.scratch,
        )
        .unwrap();
        black_box((&self.tracks, &self.pending, &self.update, &self.to));
    }
}

#[test]
fn captured_final_target_mask_preserves_complete_capture_tween_precedence_and_restore() {
    for (count, width, repeats) in [(1, 1, 0), (3, 4, 1), (3, 5, 2), (3, 32, 16)] {
        let mut a = Capture::new(count, width, repeats);
        let mut b = Capture::new(count, width, repeats);
        a.from.truncate(1);
        b.from.truncate(1);
        a.run(true, &[1]);
        b.run(false, &[1]);
        assert_eq!(a.indices, b.indices);
        assert_eq!(format!("{:?}", a.tracks), format!("{:?}", b.tracks));
        assert_eq!(a.pending.len(), b.pending.len());
        for (a, b) in a.update.iter().zip(&b.update).chain(a.to.iter().zip(&b.to)) {
            assert_eq!(fingerprint(a), fingerprint(b));
        }
        for (a, b) in a.pending.iter().zip(&b.pending) {
            assert_eq!(
                (
                    a.overlay_index,
                    a.target,
                    a.start_seconds.to_bits(),
                    a.end_seconds.to_bits()
                ),
                (
                    b.overlay_index,
                    b.target,
                    b.start_seconds.to_bits(),
                    b.end_seconds.to_bits()
                )
            );
            assert_eq!(
                (&a.from, &a.value, &a.easing),
                (&b.from, &b.value, &b.easing)
            );
        }
        let len = b.pending.len();
        b.run(false, &[1]);
        assert_eq!(b.pending.len(), len);
    }
}

#[test]
#[ignore = "manual old/new CPU, throughput and allocator benchmark"]
fn tween_replay_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    let lua = Lua::new();
    lua.gc_stop();
    let actors = overlays(&lua, 16);
    for (name, ease, shared) in [
        ("linear", None, true),
        ("spring", Some("spring"), true),
        ("elastic", Some("outElastic"), true),
        ("distinct", Some("outElastic"), false),
    ] {
        for count in [0, 1, 4, 32, 256] {
            for getters in [false, true] {
                let input: Vec<_> = (0..count).map(|i| sample(i, ease, shared)).collect();
                for old in order {
                    let mut states = vec![SongLuaOverlayState::default(); 16];
                    let mode = if old { "old" } else { "new" };
                    let label = if getters { "getters" } else { "states" };
                    crate::perf::measure_sampled(
                        &format!("replay/{name}/{count}/{label}/{mode}"),
                        4096,
                        count.max(1),
                        || {
                            replay(
                                &lua,
                                if getters { &actors } else { &[] },
                                &mut states,
                                black_box(&input),
                                black_box(1.0),
                                old,
                            )
                            .unwrap();
                            black_box(&states);
                        },
                    );
                }
            }
        }
    }
    for (width, count, late) in [
        (1, 0, false),
        (1, 1, false),
        (4, 4, false),
        (5, 5, false),
        (15, 15, false),
        (16, 16, false),
        (17, 17, false),
        (32, 0, false),
        (32, 32, false),
        (77, 77, false),
        (77, 1232, true),
    ] {
        let input = writes(count, late);
        let state = SongLuaOverlayState::default();
        let finals: Vec<_> = TARGETS[..width]
            .iter()
            .map(|&t| (t, overlay_state_update_value(&state, t)))
            .collect();
        for old in order {
            let mut states = [SongLuaOverlayState::default()];
            let mut to = states;
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("reconcile/{width}/{count}/{mode}"),
                4096,
                width,
                || {
                    if old {
                        baseline::apply_captured_final_values(
                            &mut states,
                            &mut to,
                            0,
                            black_box(&input),
                            black_box(&finals),
                            &[],
                        )
                    } else {
                        apply_captured_final_values(
                            &mut states,
                            &mut to,
                            0,
                            black_box(&input),
                            black_box(&finals),
                            &[],
                        )
                    }
                    black_box((&states, &to));
                },
            );
        }
    }
    for (width, repeats) in [(1, 0), (4, 1), (5, 1), (32, 1), (32, 16)] {
        for old in order {
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled_with_setup(
                &format!("capture/{width}/{repeats}/{mode}"),
                128,
                4 * width,
                || Capture::new(4, width, repeats),
                |f| f.run(old, &[1]),
            );
        }
    }
}
