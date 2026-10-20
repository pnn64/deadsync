use super::*;
use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as UpdateValue};
use std::path::PathBuf;

#[allow(dead_code, unused_imports)]
#[path = "compile_finalize_before.rs"]
mod before;

type Compiled = DefaultCompiledSongLua<(), ()>;
type Actor = SongLuaOverlayActor<SongLuaOverlayKind<(), (), TextAttribute>>;

fn actor(parent_index: Option<usize>, wrapper: bool) -> Actor {
    Actor {
        kind: if wrapper {
            SongLuaOverlayKind::WrapperState
        } else {
            SongLuaOverlayKind::ActorFrame
        },
        name: None,
        parent_index,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    }
}

fn fixture(count: usize, wrappers: bool, payload: usize) -> Compiled {
    let mut out = Compiled::default();
    for index in 0..count {
        let mut value = actor(wrappers.then_some(count + index), false);
        value.name = Some(format!("actor-{index}"));
        value.initial_state.x = index as f32;
        out.overlays.push(value);
    }
    if wrappers {
        out.overlays.extend((0..count).map(|_| actor(None, true)));
    }
    for index in 0..payload {
        out.messages.push(SongLuaMessageEvent {
            beat: (payload - index) as f32,
            message: format!("payload-message-{index}"),
            persists: index % 2 == 0,
        });
        out.sound_paths
            .push(PathBuf::from(format!("sound/path-{index}.ogg")));
    }
    out
}

fn split(
    current: bool,
    input: Compiled,
    layers: Vec<usize>,
    paths: &[PathBuf],
    primary: usize,
) -> Result<Vec<Compiled>, String> {
    if current {
        split_compiled_song_lua(input, layers, paths, primary)
    } else {
        before::split_compiled_song_lua(input, layers, paths, primary)
    }
}

#[test]
fn wrapper_order_preserves_sibling_stability_and_exact_graph_errors() {
    for count in 0..=4u32 {
        for encoding in 0..(count + 1).pow(count) {
            let mut code = encoding;
            let parents: Vec<_> = (0..count)
                .map(|_| {
                    let value = code % (count + 1);
                    code /= count + 1;
                    (value != count).then_some(value as usize)
                })
                .collect();
            for mask in 0..(1u32 << count) {
                let values: Vec<_> = parents
                    .iter()
                    .enumerate()
                    .map(|(index, parent)| actor(*parent, mask & (1 << index) != 0))
                    .collect();
                assert_eq!(
                    wrapper_overlay_order(&values),
                    before::wrapper_overlay_order(&values),
                    "parents={parents:?}, mask={mask}"
                );
            }
        }
    }
    for count in [1, 2, 8, 65, 1024] {
        let input = fixture(count, true, 0);
        assert_eq!(
            wrapper_overlay_order(&input.overlays),
            before::wrapper_overlay_order(&input.overlays)
        );
        let mut invalid = input.overlays;
        invalid[0].parent_index = Some(invalid.len());
        assert_eq!(
            wrapper_overlay_order(&invalid),
            before::wrapper_overlay_order(&invalid)
        );
    }
}

fn decorated_fixture(count: usize, wrappers: bool) -> Compiled {
    let mut input = fixture(count, wrappers, 5);
    input.loaded_style = Some("single".into());
    input.loaded_steps = Some([Some(0), None]);
    input.startup_reloads = 2;
    input.screen_overlay_index = Some(0);
    input.overlays[count - 1].kind = SongLuaOverlayKind::ActorProxy {
        target: crate::SongLuaProxyTarget::Actor { overlay_index: 0 },
    };
    input
        .overlay_updates
        .push(crate::SongLuaOverlayUpdateTrack {
            overlay_index: 0,
            target: Target::X,
            samples: vec![crate::SongLuaOverlayUpdateSample {
                time: 1.0,
                value: UpdateValue::F32(12.0),
            }],
        });
    input
        .stateful_message_captures
        .push(crate::SongLuaStatefulMessageCapture {
            message: "Changed".into(),
            overlay_targets: vec![(0, vec![Target::X])],
            writes: vec![crate::SongLuaStatefulMessageWrite {
                overlay_index: 0,
                beat: 1.0,
                target: Target::X,
                value: UpdateValue::F32(4.0),
                delay_seconds: 0.0,
                duration_seconds: 1.0,
                easing: None,
                opt1: None,
            }],
        });
    for immediate in [false, false, true, true] {
        input.draw_frames.push(crate::SongLuaDrawFrame {
            second: 1.0,
            immediate,
            owners: vec![0],
            owner_ends: vec![2],
            ops: vec![
                crate::SongLuaDrawOp::Begin {
                    capture: 0,
                    preserve: false,
                },
                crate::SongLuaDrawOp::Finish { capture: 0 },
            ],
        });
    }
    input
}

#[test]
fn split_preserves_layer_ownership_indices_draws_and_shared_payload_values() {
    for count in [2, 7, 65] {
        for wrappers in [false, true] {
            for layer_count in [1, 2, 3] {
                let paths: Vec<_> = (0..layer_count)
                    .map(|layer| PathBuf::from(format!("layer-{layer}.lua")))
                    .collect();
                let len = count * if wrappers { 2 } else { 1 };
                let layers: Vec<_> = (0..len)
                    .map(|index| (index % count) % layer_count)
                    .collect();
                for primary in 0..layer_count {
                    let prior = split(
                        false,
                        decorated_fixture(count, wrappers),
                        layers.clone(),
                        &paths,
                        primary,
                    )
                    .unwrap();
                    let current = split(
                        true,
                        decorated_fixture(count, wrappers),
                        layers.clone(),
                        &paths,
                        primary,
                    )
                    .unwrap();
                    assert_eq!(format!("{current:?}"), format!("{prior:?}"));
                    assert_eq!(current[0].draw_frames.len(), 3);
                }
            }
        }
    }
    let paths = [PathBuf::from("one.lua")];
    assert_eq!(
        split(true, fixture(1, false, 0), vec![], &paths, 0).unwrap_err(),
        split(false, fixture(1, false, 0), vec![], &paths, 0).unwrap_err()
    );
    assert_eq!(
        split(true, fixture(1, false, 0), vec![2], &paths, 0).unwrap_err(),
        split(false, fixture(1, false, 0), vec![2], &paths, 0).unwrap_err()
    );
}

#[test]
fn moved_payloads_keep_layers_independent_and_preserve_original_allocations() {
    let paths = [PathBuf::from("one.lua"), PathBuf::from("two.lua")];
    let mut input = fixture(0, false, 4);
    input.loaded_style = Some("single".into());
    let messages = input.messages.as_ptr();
    let sounds = input.sound_paths.as_ptr();
    let style = input.loaded_style.as_ref().unwrap().as_ptr();
    let mut outputs = split(true, input, vec![], &paths, 0).unwrap();
    assert_eq!(outputs[1].messages.as_ptr(), messages);
    assert_eq!(outputs[1].sound_paths.as_ptr(), sounds);
    assert_eq!(outputs[1].loaded_style.as_ref().unwrap().as_ptr(), style);
    let expected = format!("{:?}", outputs[1]);
    outputs[0].messages[0].message.push_str(" changed");
    outputs[0].sound_paths[0].push("changed");
    outputs[0]
        .loaded_style
        .as_mut()
        .unwrap()
        .push_str(" changed");
    assert_eq!(format!("{:?}", outputs[1]), expected);
}

fn fixture_paths() -> Vec<(Vec<PathBuf>, bool)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![
        (
            vec![root.join("../../tests/fixtures/song_lua/basic.lua")],
            false,
        ),
        (vec![root.join("tests/perf/compile_finalize.lua")], true),
        (vec![root.join("tests/perf/live_actors.lua")], true),
        (
            vec![
                root.join("tests/perf/compile_finalize.lua"),
                root.join("tests/perf/compile_finalize.lua"),
            ],
            true,
        ),
    ]
}

fn try_compile_fixture(
    current: bool,
    paths: &[PathBuf],
    live: bool,
) -> Result<Vec<Compiled>, String> {
    let refs: Vec<_> = paths.iter().map(PathBuf::as_path).collect();
    let directory = paths
        .first()
        .and_then(|path| path.parent())
        .unwrap_or(Path::new(env!("CARGO_MANIFEST_DIR")));
    let mut context = SongLuaCompileContext::new(directory, "finalization fixture");
    context.live = live;
    context.music_length_seconds = 1.0;
    if current {
        compile_song_lua_layers_with_default_host::<(), (), _>(
            &refs,
            0,
            &context,
            SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::from([])),
            |_| None,
            |_, _| None,
        )
    } else {
        before::compile_song_lua_layers_with_default_host::<(), (), _>(
            &refs,
            0,
            &context,
            SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::from([])),
            |_| None,
            |_, _| None,
        )
    }
}

fn compile_fixture(current: bool, paths: &[PathBuf], live: bool) -> Vec<Compiled> {
    try_compile_fixture(current, paths, live).unwrap()
}

#[test]
fn live_identity_output_preserves_empty_sessions_and_invalid_layer_errors() {
    let prior = compile_fixture(false, &[], true);
    let current = compile_fixture(true, &[], true);
    assert_eq!(format!("{current:?}"), format!("{prior:?}"));
    assert_eq!(current.len(), 1);
    assert_eq!(
        current[0].live_indices,
        (0..current[0].overlays.len()).collect::<Vec<_>>()
    );

    let paths =
        [Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/perf/compile_invalid_layer.lua")];
    let prior = try_compile_fixture(false, &paths, true).unwrap_err();
    let current = try_compile_fixture(true, &paths, true).unwrap_err();
    assert_eq!(current, prior);
    assert_eq!(current, "song lua overlay has invalid layer index 1");
}

#[test]
fn late_targets_preserve_sprite_fallback_proxy_rebinding_and_getter_errors() {
    fn resolve(
        current: bool,
        proxies: bool,
        fail: bool,
    ) -> (Result<String, String>, [bool; 2], Vec<String>) {
        let lua = Lua::new();
        lua.globals().set("fail", fail).unwrap();
        let tables = lua
            .load(
                r#"
            log = {}
            local function probe(name, values)
                return setmetatable({}, {__index = function(_, key)
                    log[#log + 1] = name .. ':' .. key
                    return values[key]
                end})
            end
            local target = probe('target', {__songlua_visible = false})
            local sprite = probe('sprite', {
                __songlua_actor_type = 'Sprite', __songlua_aft_capture_name = 'capture',
            })
            local proxy = probe('actor', {
                __songlua_proxy_target_kind = fail and {} or 'actor',
                __songlua_proxy_target_actor = target,
            })
            local player = probe('player', {
                __songlua_proxy_target_kind = 'player', __songlua_proxy_player_index = 0,
                __songlua_proxy_target_actor = target,
            })
            return {target, sprite, proxy, player, target}
        "#,
            )
            .eval::<Table>()
            .unwrap();
        let mut overlays: Vec<crate::SongLuaOverlayCompileActor<_>> = tables
            .sequence_values::<Table>()
            .enumerate()
            .map(|(index, table)| {
                let mut value = actor(None, false);
                value.kind = match index {
                    1 => SongLuaOverlayKind::Actor,
                    2 | 3 => SongLuaOverlayKind::ActorProxy {
                        // Both proxies begin with a player target. Lua may rebind either one to an actor.
                        target: crate::SongLuaProxyTarget::Player { player_index: 1 },
                    },
                    _ => SongLuaOverlayKind::ActorFrame,
                };
                crate::SongLuaOverlayCompileActor {
                    actor: value,
                    table: table.unwrap(),
                    message_sounds: Vec::new(),
                }
            })
            .collect();
        if !proxies {
            overlays.truncate(2);
        }
        let mut hidden = [false; 2];
        let result = if current {
            resolve_late_actor_targets(&mut overlays, &mut hidden)
        } else {
            before::resolve_late_actor_targets(&mut overlays, &mut hidden)
        };
        if !fail {
            assert!(
                matches!(&overlays[1].actor.kind, SongLuaOverlayKind::AftSprite { capture_name } if capture_name == "capture")
            );
            if proxies {
                assert!(matches!(
                    overlays[2].actor.kind,
                    SongLuaOverlayKind::ActorProxy {
                        target: crate::SongLuaProxyTarget::Actor { overlay_index: 4 }
                    }
                ));
                assert_eq!(hidden, [true, false]);
            }
        }
        let result = result.map(|_| {
            format!(
                "{:?}",
                overlays
                    .iter()
                    .map(|overlay| &overlay.actor)
                    .collect::<Vec<_>>()
            )
        });
        let log = lua
            .globals()
            .get::<Table>("log")
            .unwrap()
            .sequence_values::<String>()
            .collect::<mlua::Result<Vec<_>>>()
            .unwrap();
        (result, hidden, log)
    }
    for proxies in [false, true] {
        for fail in [false, true] {
            let prior = resolve(false, proxies, fail);
            let current = resolve(true, proxies, fail);
            assert_eq!(current, prior, "proxies={proxies}, fail={fail}");
            assert_eq!(current.0.is_err(), proxies && fail);
        }
    }
}

#[test]
fn complete_fixture_compiles_preserve_outputs_and_initial_live_updates() {
    for (paths, live) in fixture_paths() {
        let mut prior = compile_fixture(false, &paths, live);
        let mut current = compile_fixture(true, &paths, live);
        assert_eq!(format!("{current:?}"), format!("{prior:?}"), "{paths:?}");
        if !live {
            continue;
        }
        let prior = prior
            .iter_mut()
            .find_map(|item| item.live_scene.as_mut())
            .unwrap();
        let current = current
            .iter_mut()
            .find_map(|item| item.live_scene.as_mut())
            .unwrap();
        for index in 1..=8 {
            let seconds = index as f64 / 60.0;
            let frame = crate::SongLuaLiveFrame {
                beat: seconds * 2.0,
                music_seconds: seconds,
                screen_seconds: seconds,
                delta_seconds: 1.0 / 60.0,
                ..Default::default()
            };
            let old_output = prior.advance(frame.clone()).unwrap();
            let new_output = current.advance(frame).unwrap();
            assert!(old_output.errors.is_empty(), "{:?}", old_output.errors);
            assert!(new_output.errors.is_empty(), "{:?}", new_output.errors);
            assert_eq!(format!("{new_output:?}"), format!("{old_output:?}"));
            assert_eq!(current.states(), prior.states());
        }
    }
}

#[test]
fn finalization_allocations_remove_wrapper_lists_payload_copies_and_growth() {
    use crate::draw_capture::perf::allocations;
    use std::hint::black_box;
    let values = fixture(1024, true, 0);
    let (_, prior) = allocations::measure(|| {
        black_box(before::wrapper_overlay_order(&values.overlays).unwrap())
    });
    let (_, current) =
        allocations::measure(|| black_box(wrapper_overlay_order(&values.overlays).unwrap()));
    assert!(prior.allocs >= 1024);
    assert!(current.allocs <= 8);
    assert!(current.allocated_bytes < prior.allocated_bytes);
    let paths = [PathBuf::from("one.lua")];
    for (count, payload) in [(0, 128), (128, 0)] {
        let old = fixture(count, false, payload);
        let new = fixture(count, false, payload);
        let old_layers = vec![0; count];
        let new_layers = old_layers.clone();
        let (_, prior) =
            allocations::measure(|| black_box(split(false, old, old_layers, &paths, 0).unwrap()));
        let (_, current) =
            allocations::measure(|| black_box(split(true, new, new_layers, &paths, 0).unwrap()));
        if payload > 0 {
            assert_eq!(prior.allocs, current.allocs + payload * 2 + 2);
        } else {
            assert!(current.reallocs < prior.reallocs);
        }
        assert!(current.allocated_bytes < prior.allocated_bytes);
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_compile_finalize() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    fn measure<W: FnMut()>(
        name: &str,
        current: bool,
        unit: &str,
        count: usize,
        mut setup: impl FnMut() -> W,
    ) {
        let mut samples = Vec::new();
        for sample in 0..10 {
            let mut work = setup();
            let start = Instant::now();
            let begin = cycles();
            let (_, churn) = allocations::measure(|| {
                for _ in 0..count {
                    work();
                }
            });
            let used = cycles() - begin;
            let nanos = start.elapsed().as_nanos();
            if sample > 0 {
                samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":count,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
            }
        }
        println!(
            "DRAW_CAPTURE_METRIC {}",
            serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},"samples":samples,"unit":unit,"baseline_commit":"bacdc3e37"})
        );
    }
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, count, wrappers) in [
        ("order_empty", 0, false),
        ("order_identity128", 128, false),
        ("order_forward128", 128, true),
        ("order_forward1024", 1024, true),
    ] {
        for current in variants {
            measure(name, current, "wrapper_orders", 512, || {
                let input = fixture(count, wrappers, 0);
                let order = if current {
                    wrapper_overlay_order
                } else {
                    before::wrapper_overlay_order
                };
                black_box(order(&input.overlays).unwrap());
                move || {
                    black_box(order(black_box(&input.overlays)).unwrap());
                }
            });
        }
    }
    for (name, count, wrappers, payload, layers) in [
        ("split_empty", 0, false, 0, 1),
        ("split_payload_one", 0, false, 128, 1),
        ("split_payload_three", 0, false, 128, 3),
        ("split_flat128", 128, false, 0, 1),
        ("split_wrappers128", 128, true, 0, 1),
        ("split_wrappers_three", 128, true, 32, 3),
    ] {
        for current in variants {
            measure(name, current, "compiled_splits", 64, || {
                let paths: Vec<_> = (0..layers)
                    .map(|index| PathBuf::from(format!("layer-{index}.lua")))
                    .collect();
                let build = || {
                    let input = fixture(count, wrappers, payload);
                    let len = input.overlays.len();
                    (
                        input,
                        (0..len)
                            .map(|index| (index % count.max(1)) % layers)
                            .collect::<Vec<_>>(),
                    )
                };
                let (warm, ownership) = build();
                black_box(split(current, warm, ownership, &paths, 0).unwrap());
                let mut inputs: Vec<_> = (0..64).map(|_| build()).collect();
                move || {
                    let (input, ownership) = inputs.pop().unwrap();
                    black_box(split(current, input, ownership, &paths, 0).unwrap());
                }
            });
        }
    }
    for (index, (paths, live)) in fixture_paths().into_iter().enumerate() {
        let name = [
            "compile_basic_offline",
            "compile_wrappers_live",
            "compile_actors_live",
            "compile_two_layers_live",
        ][index];
        let expected = format!("{:?}", compile_fixture(false, &paths, live));
        for current in variants {
            drop(black_box(compile_fixture(current, &paths, live)));
            let mut samples = Vec::new();
            for sample in 0..10 {
                let start = Instant::now();
                let begin = cycles();
                let (output, churn) =
                    allocations::measure(|| compile_fixture(current, &paths, live));
                let used = cycles() - begin;
                let nanos = start.elapsed().as_nanos();
                assert_eq!(format!("{output:?}"), expected, "fixture={name}");
                drop(output);
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":1,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},"samples":samples,"unit":"fixture_compiles","baseline_commit":"bacdc3e37","output_validation":"Every sample compared exactly against the frozen compiler's complete Debug DTO snapshot; output formatting and destruction excluded from timing."})
            );
        }
    }
}
