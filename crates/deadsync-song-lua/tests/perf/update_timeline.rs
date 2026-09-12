use super::*;
use std::collections::HashMap;
use std::hint::black_box;

#[path = "update_timeline_baseline.rs"]
mod baseline;

const MOD_KEYS: &[&str] = &[
    "boost",
    "brake",
    "wave",
    "expand",
    "boomerang",
    "drunk",
    "dizzy",
    "confusion",
    "confusionoffset",
    "flip",
    "invert",
    "tornado",
    "tipsy",
    "bumpy",
    "bumpyoffset",
    "bumpyperiod",
    "pulseinner",
    "pulseouter",
    "pulseperiod",
    "pulseoffset",
    "beat",
    "randomspeed",
    "hidden",
    "sudden",
    "suddenoffset",
    "stealth",
    "blink",
    "dark",
    "xmod",
    "cmod",
    "mmod",
    "z",
];

fn options(lua: &Lua, count: usize, speedmods: bool) -> Table {
    let table = lua.create_table().unwrap();
    if count > 0 {
        let state = lua.create_table().unwrap();
        for (index, key) in MOD_KEYS.iter().cycle().take(count).enumerate() {
            state
                .set(format!("{key}{index}"), index as f32 * 0.125)
                .unwrap();
        }
        table.set("__songlua_player_option_state", state).unwrap();
    }
    if speedmods {
        for (key, value) in [
            ("__songlua_speedmod_xmod", 1.5),
            ("__songlua_speedmod_cmod", 600.0),
            ("__songlua_speedmod_mmod", 750.0),
        ] {
            table.set(key, value).unwrap();
        }
    }
    table
}

#[test]
fn option_sampling_preserves_values_overrides_and_lua_errors() {
    let lua = Lua::new();
    for count in [0, 1, 16, 64] {
        for speeds in [false, true] {
            let table = options(&lua, count, speeds);
            assert_eq!(
                player_option_sample(&table),
                baseline::player_option_sample(&table)
            );
        }
    }
    let table: Table = lua
        .load(
            r#"return {
        __songlua_player_option_state = {
            boost = true, brake = false, drunk = "0.25", tiny = {},
            xmod = 2, cmod = 400, mmod = 500,
        },
        __songlua_speedmod_xmod = 3,
        __songlua_speedmod_cmod = "600",
    }"#,
        )
        .eval()
        .unwrap();
    let actual = player_option_sample(&table).unwrap();
    assert_eq!(actual, baseline::player_option_sample(&table).unwrap());
    assert_eq!(actual["boost"], 1.0);
    assert_eq!(actual["brake"], 0.0);
    assert_eq!(actual["xmod"], 3.0);
    assert_eq!(actual["cmod"], 600.0);
    assert_eq!(actual["mmod"], 500.0);
    assert!(!actual.contains_key("tiny"));
    for field in [
        "__songlua_player_option_state",
        "__songlua_speedmod_xmod",
        "__songlua_speedmod_cmod",
        "__songlua_speedmod_mmod",
    ] {
        let table = lua.create_table().unwrap();
        table.set(field, "invalid value").unwrap();
        let expected = baseline::player_option_sample(&table);
        assert!(expected.is_err());
        assert_eq!(player_option_sample(&table), expected);
    }
}

#[test]
fn option_sampling_keeps_metamethod_lookup_order_and_short_circuiting() {
    let lua = Lua::new();
    for fail in [false, true] {
        let make = || -> Table {
            lua.load(
                r#"
                local fail = ...
                return setmetatable({ seen = {} }, { __index = function(t, k)
                    table.insert(t.seen, k)
                    if fail and k == "__songlua_speedmod_cmod" then error("lookup failed") end
                    if k == "__songlua_speedmod_xmod" then return 2 end
                    if k == "__songlua_speedmod_cmod" then return 600 end
                end })
            "#,
            )
            .call(fail)
            .unwrap()
        };
        let old_table = make();
        let new_table = make();
        let old = baseline::player_option_sample(&old_table);
        let new = player_option_sample(&new_table);
        // Lua stack traces include the Rust caller's load-site line, so compare
        // the message prefix and separately assert the observable lookup log.
        if fail {
            assert!(old.unwrap_err().contains("lookup failed"));
            assert!(new.unwrap_err().contains("lookup failed"));
        } else {
            assert_eq!(old, new);
        }
        let log = |table: &Table| -> Vec<String> {
            table
                .get::<Table>("seen")
                .unwrap()
                .sequence_values()
                .collect::<mlua::Result<_>>()
                .unwrap()
        };
        let expected: Vec<_> = [
            "__songlua_speedmod_xmod",
            "__songlua_speedmod_cmod",
            "__songlua_speedmod_mmod",
        ]
        .into_iter()
        .take(if fail { 2 } else { 3 })
        .map(str::to_owned)
        .collect();
        assert_eq!(log(&new_table), expected);
        assert_eq!(log(&old_table), expected);
    }
}

#[test]
fn empty_option_sampling_has_no_rust_heap_churn() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    player_option_sample(&table).unwrap();
    crate::perf::assert_no_churn(|| {
        for _ in 0..64 {
            assert!(player_option_sample(&table).unwrap().is_empty());
        }
    });
}

type ModStates = [SongLuaUpdateModState; LUA_PLAYERS];
type ModIndex = BTreeMap<(usize, String), usize>;

fn mod_states(count: usize, value: f32) -> ModStates {
    std::array::from_fn(|player| {
        MOD_KEYS
            .iter()
            .take(count)
            .enumerate()
            .map(|(i, key)| (key.to_string(), value + (player * count + i) as f32 / 64.0))
            .collect()
    })
}

fn assert_windows(actual: &[SongLuaEaseWindow], expected: &[SongLuaEaseWindow]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert_eq!(
            a.approach_speed.map(f32::to_bits),
            b.approach_speed.map(f32::to_bits)
        );
        assert_eq!(
            (
                a.start.to_bits(),
                a.limit.to_bits(),
                a.from.to_bits(),
                a.to.to_bits()
            ),
            (
                b.start.to_bits(),
                b.limit.to_bits(),
                b.from.to_bits(),
                b.to.to_bits()
            )
        );
        assert_eq!(
            (
                &a.target,
                &a.easing,
                a.player,
                a.unit,
                a.span_mode,
                a.sustain,
                a.opt1,
                a.opt2
            ),
            (
                &b.target,
                &b.easing,
                b.player,
                b.unit,
                b.span_mode,
                b.sustain,
                b.opt1,
                b.opt2
            )
        );
    }
}

fn next(rng: &mut u64) -> usize {
    *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*rng >> 32) as usize
}

#[test]
fn mod_windows_match_parent_across_state_changes_and_stale_indices() {
    let keys: Vec<_> = MOD_KEYS
        .iter()
        .copied()
        .chain([
            "rotationx",
            "rotationy",
            "rotationz",
            "zoom",
            "zoomx",
            "zoomy",
            "zoomz",
            "bumpy1",
            "bumpy16",
            "bumpy17",
            "tiny01",
            "movex3",
            "movey16",
            "confusionoffset8",
            "x",
            "y",
            "unknown",
            "",
        ])
        .collect();
    let values = [
        -0.0,
        0.0,
        0.25,
        1.0,
        -1.0,
        f32::EPSILON,
        f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];
    for seed in 1..=24 {
        let mut rng = seed;
        let mut old = Vec::new();
        let mut new = Vec::new();
        let mut public = Vec::new();
        let mut old_index = ModIndex::new();
        let mut new_index = ModIndex::new();
        let mut public_index = ModIndex::new();
        let mut lookup = (0, String::new());
        for tick in 0..32 {
            let mut make = || -> ModStates {
                std::array::from_fn(|_| {
                    keys.iter()
                        .filter_map(|&key| {
                            if next(&mut rng).is_multiple_of(4) {
                                None
                            } else {
                                Some((key.to_owned(), values[next(&mut rng) % values.len()]))
                            }
                        })
                        .collect()
                })
            };
            let from = if tick % 13 == 0 {
                ModStates::default()
            } else {
                make()
            };
            let to = make();
            let base = make();
            let speeds = make();
            if tick % 7 == 0 {
                // Public callers may clear output or provide stale cache slots.
                old.clear();
                new.clear();
                public.clear();
                for map in [&mut old_index, &mut new_index, &mut public_index] {
                    map.insert((0, "unknown".to_owned()), 0);
                    map.insert((1, "xmod".to_owned()), usize::MAX);
                }
            }
            let start = if tick % 5 == 0 { -0.0 } else { tick as f32 };
            let end = if tick % 9 == 0 {
                start - 1.0
            } else {
                start + 0.5
            };
            baseline::push_update_mod_targets(
                &mut old,
                start,
                end,
                &from,
                &to,
                &base,
                &speeds,
                &mut old_index,
            );
            push_update_mod_targets_with_key(
                &mut new,
                start,
                end,
                &from,
                &to,
                &base,
                &speeds,
                &mut new_index,
                &mut lookup,
            );
            push_update_mod_targets(
                &mut public,
                start,
                end,
                &from,
                &to,
                &base,
                &speeds,
                &mut public_index,
            );
            assert_windows(&new, &old);
            assert_windows(&public, &old);
            assert_eq!(new_index, old_index);
            assert_eq!(public_index, old_index);
        }
    }
}

#[test]
fn unchanged_mod_windows_extend_without_heap_churn_after_warmup() {
    let states = mod_states(32, 1.0);
    let speeds = mod_states(32, 0.5);
    let base = ModStates::default();
    let mut out = Vec::new();
    let mut index = ModIndex::new();
    let mut lookup = (0, String::new());
    push_update_mod_targets_with_key(
        &mut out,
        0.0,
        1.0,
        &states,
        &states,
        &base,
        &speeds,
        &mut index,
        &mut lookup,
    );
    assert_eq!(out.len(), 64);
    crate::perf::assert_no_churn(|| {
        for tick in 1..600 {
            push_update_mod_targets_with_key(
                &mut out,
                tick as f32,
                tick as f32 + 1.0,
                &states,
                &states,
                &base,
                &speeds,
                &mut index,
                &mut lookup,
            );
        }
    });
    assert_eq!(out.len(), 64);
    assert!(
        out.iter()
            .all(|window| window.start == 0.0 && window.limit == 600.0)
    );
    assert!(out.iter().any(
        |window| window.target == SongLuaEaseTarget::Mod("xmod".into())
            && window.to == states[0]["xmod"]
    ));
    assert!(out.iter().any(
        |window| window.target == SongLuaEaseTarget::Mod("boost".into())
            && window.to == states[0]["boost"] * 100.0
    ));
}

fn scheduled(index: usize, overlay: usize, start: f32, end: f32) -> SongLuaScheduledOverlaySample {
    SongLuaScheduledOverlaySample {
        overlay_index: overlay,
        target: SongLuaOverlayUpdateTarget::X,
        start_seconds: f64::from(start),
        end_seconds: f64::from(end),
        start_beat: start,
        end_beat: end,
        easing: None,
        opt1: None,
        from: SongLuaOverlayUpdateValue::F32(-1.0),
        value: SongLuaOverlayUpdateValue::F32(index as f32),
    }
}

type TrackIndex = HashMap<(usize, SongLuaOverlayUpdateTarget), usize>;

fn assert_tracks(actual: &[SongLuaOverlayUpdateTrack], expected: &[SongLuaOverlayUpdateTrack]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert_eq!(
            (a.overlay_index, a.target, a.samples.len()),
            (b.overlay_index, b.target, b.samples.len())
        );
        for (a, b) in a.samples.iter().zip(&b.samples) {
            assert_eq!(a.beat.to_bits(), b.beat.to_bits());
            assert_eq!(a.value, b.value);
        }
    }
}

#[test]
fn scheduled_merges_match_parent_for_overlap_ties_and_nonfinite_beats() {
    let beats = [
        -2.0,
        -0.0,
        0.0,
        f32::EPSILON,
        2.0 * f32::EPSILON,
        3.0 * f32::EPSILON,
        0.5,
        1.0,
        1.0 + f32::EPSILON,
        2.0,
        8.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::from_bits(0xffc00001),
    ];
    let base = [SongLuaOverlayState::default(); 3];
    for seed in 1..=64 {
        let mut rng = seed;
        let mut old = Vec::new();
        let mut new = Vec::new();
        let mut old_index = TrackIndex::new();
        let mut new_index = TrackIndex::new();
        for batch in 0..8 {
            let samples: Vec<_> = (0..(seed as usize % 33))
                .map(|i| {
                    let start = beats[next(&mut rng) % beats.len()];
                    let end = beats[next(&mut rng) % beats.len()];
                    let mut sample = scheduled(batch * 100 + i, next(&mut rng) % 3, start, end);
                    if next(&mut rng).is_multiple_of(2) {
                        sample.target = SongLuaOverlayUpdateTarget::Visible;
                        sample.value = SongLuaOverlayUpdateValue::Bool(i % 2 == 0);
                    }
                    sample
                })
                .collect();
            baseline::merge_scheduled_overlay_samples(
                &mut old,
                &mut old_index,
                &base,
                samples.clone(),
            );
            merge_scheduled_overlay_samples(&mut new, &mut new_index, &base, samples);
            assert_tracks(&new, &old);
            assert_eq!(new_index, old_index);
        }
    }
}

#[test]
fn ordered_appends_keep_last_timestamp_value_and_arc_ownership() {
    let base = [SongLuaOverlayState::default(); 1];
    let old_owner = std::sync::Arc::new([[0.0; 4]; 4]);
    let old_weak = std::sync::Arc::downgrade(&old_owner);
    let kept = std::sync::Arc::new([[1.0; 4]; 4]);
    let mut samples = Vec::new();
    for (i, owner) in [old_owner, kept.clone(), kept.clone()]
        .into_iter()
        .enumerate()
    {
        let mut sample = scheduled(i, 0, i as f32 * f32::EPSILON, i as f32 * f32::EPSILON);
        sample.target = SongLuaOverlayUpdateTarget::VertexColors;
        sample.value = SongLuaOverlayUpdateValue::VertexColors(owner);
        samples.push(sample);
    }
    let mut tracks = Vec::new();
    merge_scheduled_overlay_samples(&mut tracks, &mut TrackIndex::new(), &base, samples);
    assert!(old_weak.upgrade().is_none());
    assert_eq!(tracks[0].samples.len(), 1);
    assert_eq!(
        tracks[0].samples[0].beat.to_bits(),
        (2.0 * f32::EPSILON).to_bits()
    );
    assert_eq!(
        tracks[0].samples[0].value,
        SongLuaOverlayUpdateValue::VertexColors(kept.clone())
    );
    assert_eq!(std::sync::Arc::strong_count(&kept), 2);
}

#[test]
fn scheduled_merges_preserve_prefilled_unsorted_and_empty_tracks() {
    let base = [SongLuaOverlayState::default(); 2];
    let original = vec![
        SongLuaOverlayUpdateTrack {
            overlay_index: 0,
            target: SongLuaOverlayUpdateTarget::X,
            samples: [4.0, 1.0, -0.0, 1.0, 3.0]
                .into_iter()
                .enumerate()
                .map(|(i, beat)| SongLuaOverlayUpdateSample {
                    beat,
                    value: SongLuaOverlayUpdateValue::F32(i as f32 + 100.0),
                })
                .collect(),
        },
        SongLuaOverlayUpdateTrack {
            overlay_index: 1,
            target: SongLuaOverlayUpdateTarget::X,
            samples: Vec::new(),
        },
    ];
    let indices: TrackIndex = original
        .iter()
        .enumerate()
        .map(|(i, t)| ((t.overlay_index, t.target), i))
        .collect();
    for count in [0, 1, 16, 256] {
        let samples: Vec<_> = (0..count)
            .map(|i| scheduled(i, i % 2, i as f32 * 0.5, i as f32 * 0.5 + 0.25))
            .collect();
        let mut old = original.clone();
        let mut new = original.clone();
        let mut old_index = indices.clone();
        let mut new_index = indices.clone();
        baseline::merge_scheduled_overlay_samples(&mut old, &mut old_index, &base, samples.clone());
        merge_scheduled_overlay_samples(&mut new, &mut new_index, &base, samples);
        assert_tracks(&new, &old);
        assert_eq!(new_index, old_index);
    }
}

fn measure_pair<O, N, A, B>(name: &str, iterations: usize, units: usize, mut old: O, mut new: N)
where
    O: FnMut() -> A,
    N: FnMut() -> B,
{
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        crate::perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
        crate::perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
    } else {
        crate::perf::measure_sampled(&format!("{name}/old"), iterations, units, &mut old);
        crate::perf::measure_sampled(&format!("{name}/new"), iterations, units, &mut new);
    }
}

fn mod_batch(
    old: bool,
    frames: usize,
    from: &ModStates,
    other: &ModStates,
    speeds: &ModStates,
    changing: bool,
) -> (Vec<SongLuaEaseWindow>, ModIndex) {
    let mut out = Vec::new();
    let mut index = ModIndex::new();
    let mut lookup = (0, String::new());
    let base = ModStates::default();
    for tick in 0..frames {
        let states = if changing && tick % 2 == 1 {
            other
        } else {
            from
        };
        if old {
            baseline::push_update_mod_targets(
                &mut out,
                tick as f32,
                tick as f32 + 1.0,
                states,
                states,
                &base,
                speeds,
                &mut index,
            );
        } else {
            push_update_mod_targets_with_key(
                &mut out,
                tick as f32,
                tick as f32 + 1.0,
                states,
                states,
                &base,
                speeds,
                &mut index,
                &mut lookup,
            );
        }
    }
    (out, index)
}

#[test]
#[ignore = "manual old/new release benchmark; run serially with --nocapture"]
fn update_timeline_bench() {
    let lua = Lua::new();
    for (count, speeds) in [(0, false), (0, true), (16, false), (16, true), (64, true)] {
        let table = options(&lua, count, speeds);
        player_option_sample(&table).unwrap();
        lua.gc_stop();
        measure_pair(
            &format!("options_{count}_speed_{speeds}"),
            4096,
            1,
            || baseline::player_option_sample(black_box(&table)).unwrap(),
            || player_option_sample(black_box(&table)).unwrap(),
        );
        lua.gc_restart();
        lua.gc_collect().unwrap();
    }
    for (keys, frames, changing, authored_speed) in [
        (1, 1, false, true),
        (1, 600, false, true),
        (16, 600, false, true),
        (32, 600, false, true),
        (16, 128, true, true),
        (32, 128, true, false),
        (0, 600, false, true),
    ] {
        let from = mod_states(keys, 1.0);
        let other = mod_states(keys, 2.0);
        let speeds = if authored_speed {
            mod_states(keys, 0.5)
        } else {
            ModStates::default()
        };
        let old = mod_batch(true, frames, &from, &other, &speeds, changing);
        let new = mod_batch(false, frames, &from, &other, &speeds, changing);
        assert_windows(&new.0, &old.0);
        assert_eq!(new.1, old.1);
        measure_pair(
            &format!("mods_{keys}x{frames}_change_{changing}_speed_{authored_speed}"),
            if frames == 1 { 4096 } else { 32 },
            frames,
            || mod_batch(true, frames, black_box(&from), &other, &speeds, changing),
            || mod_batch(false, frames, black_box(&from), &other, &speeds, changing),
        );
    }
    for count in [1, 16, 32] {
        let states = mod_states(count, 1.0);
        let speeds = mod_states(count, 0.5);
        let base = ModStates::default();
        let (mut old, mut old_index) = mod_batch(true, 1, &states, &states, &speeds, false);
        let (mut new, mut new_index) = mod_batch(false, 1, &states, &states, &speeds, false);
        let mut lookup = (0, String::new());
        push_update_mod_targets_with_key(
            &mut new,
            1.0,
            2.0,
            &states,
            &states,
            &base,
            &speeds,
            &mut new_index,
            &mut lookup,
        );
        measure_pair(
            &format!("mods_warm_{count}"),
            2048,
            count * LUA_PLAYERS,
            || {
                baseline::push_update_mod_targets(
                    black_box(&mut old),
                    1.0,
                    2.0,
                    &states,
                    &states,
                    &base,
                    &speeds,
                    &mut old_index,
                );
                black_box(&old);
            },
            || {
                push_update_mod_targets_with_key(
                    black_box(&mut new),
                    1.0,
                    2.0,
                    &states,
                    &states,
                    &base,
                    &speeds,
                    &mut new_index,
                    &mut lookup,
                );
                black_box(&new);
            },
        );
    }
    for (count, overlays, overlap, duplicate) in [
        (0, 1, false, false),
        (1, 1, false, false),
        (16, 1, false, false),
        (256, 1, false, false),
        (4096, 1, false, false),
        (4096, 16, false, false),
        (256, 1, true, false),
        (256, 1, false, true),
    ] {
        let base = vec![SongLuaOverlayState::default(); overlays];
        let samples: Vec<_> = (0..count)
            .map(|i| {
                let start = if duplicate { 1.0 } else { i as f32 * 0.5 };
                let end = start
                    + if overlap {
                        16.0 - (i % 16) as f32 * 0.5
                    } else {
                        0.25
                    };
                scheduled(i, i % overlays, start, end)
            })
            .collect();
        let run = |old| {
            let mut tracks = Vec::new();
            let mut indices = TrackIndex::new();
            if old {
                baseline::merge_scheduled_overlay_samples(
                    &mut tracks,
                    &mut indices,
                    &base,
                    black_box(samples.clone()),
                );
            } else {
                merge_scheduled_overlay_samples(
                    &mut tracks,
                    &mut indices,
                    &base,
                    black_box(samples.clone()),
                );
            }
            (tracks, indices)
        };
        let old = run(true);
        let new = run(false);
        assert_tracks(&new.0, &old.0);
        assert_eq!(new.1, old.1);
        measure_pair(
            &format!("overlay_{count}x{overlays}_overlap_{overlap}_ties_{duplicate}"),
            if count >= 4096 {
                4
            } else if count <= 1 {
                4096
            } else {
                128
            },
            count.max(1),
            || run(true),
            || run(false),
        );
    }
}
