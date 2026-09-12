use super::*;
use std::cell::RefCell;
use std::hint::black_box;
use std::rc::Rc;

#[path = "perframe_stream_baseline.rs"]
mod baseline;

#[test]
fn lua_stream_sample_iteration_preserves_float_bits_and_endpoints() {
    let mut spans = vec![
        (-0.0, 0.0),
        (0.0, -0.0),
        (1.0, 1.0),
        (2.0, -2.0),
        (-4.0, 2.0),
        (0.0, 1.0e-8),
        (0.0, 1.0e-4),
        (100.0, 100.01),
        (0.0, 128.0),
        (f32::INFINITY, f32::INFINITY),
        (1.0, f32::NEG_INFINITY),
    ];
    let mut seed = 19u64;
    for _ in 0..2000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let start = (seed >> 32) as i32 as f32 / i32::MAX as f32 * 128.0;
        spans.push((start, start + (seed % 10_000) as f32 * 0.002));
    }
    let bits = |s: SongLuaPerframeSample| {
        (
            s.beat.to_bits(),
            s.eval_beat.to_bits(),
            s.delta_beats.to_bits(),
        )
    };
    for (start, end) in spans {
        let expected: Vec<_> = baseline::perframe_samples(start, end)
            .into_iter()
            .map(bits)
            .collect();
        assert_eq!(
            perframe_samples(start, end)
                .into_iter()
                .map(bits)
                .collect::<Vec<_>>(),
            expected
        );
        let mut actual = perframe_sample_iter(start, end);
        for expected in expected {
            assert_eq!(bits(actual.next().unwrap()), expected);
        }
        assert!(actual.next().is_none());
        assert!(actual.next().is_none());
    }
}

type Trace = Rc<RefCell<Vec<(usize, u32, u32, u32)>>>;

struct Fixture {
    lua: Lua,
    context: SongLuaCompileContext,
    overlays: Vec<SongLuaOverlayCompileActor<()>>,
    tracked: Vec<SongLuaTrackedActor>,
    prefix: Table,
    global: Table,
    messages: Vec<SongLuaMessageEvent>,
    trace: Trace,
}

impl Fixture {
    fn new(
        count: usize,
        dynamic: bool,
        spans: &[(f32, f32)],
        messages: bool,
        failure: Option<f32>,
        tracing: bool,
    ) -> Self {
        let lua = Lua::new();
        let context = SongLuaCompileContext {
            song_display_bpms: [120.0, 180.0],
            song_music_rate: 1.25,
            song_timing_bpms: vec![(0.0, 120.0), (0.5, 180.0), (2.0, 90.0)],
            ..SongLuaCompileContext::new("", "Streaming fixture")
        };
        let runtime = crate::create_song_runtime_table(&lua, &context).unwrap();
        lua.globals()
            .set(crate::SONG_LUA_RUNTIME_KEY, runtime)
            .unwrap();
        let overlays: Vec<_> = (0..count)
            .map(|index| {
                let table = lua.create_table().unwrap();
                reset_actor_capture(&lua, &table).unwrap();
                SongLuaOverlayCompileActor {
                    table,
                    actor: crate::SongLuaOverlayActor {
                        kind: (),
                        name: Some(format!("actor{index}")),
                        parent_index: None,
                        initial_state: SongLuaOverlayState::default(),
                        message_commands: if messages {
                            vec![crate::SongLuaOverlayMessageCommand {
                                message: "Pulse".into(),
                                aux: Some(3.0),
                                blocks: vec![crate::SongLuaOverlayCommandBlock {
                                    start: 0.0,
                                    duration: 0.3,
                                    easing: Some("linear".into()),
                                    opt1: None,
                                    opt2: None,
                                    delta: crate::SongLuaOverlayStateDelta {
                                        x: Some(80.0),
                                        ..Default::default()
                                    },
                                }],
                            }]
                        } else {
                            vec![]
                        },
                    },
                    message_sounds: vec![],
                }
            })
            .collect();
        let tracked: Vec<_> = (0..LUA_PLAYERS)
            .map(|index| {
                let table = lua.create_table().unwrap();
                reset_actor_capture(&lua, &table).unwrap();
                SongLuaTrackedActor {
                    table,
                    actor: Default::default(),
                    target: SongLuaTrackedActorTarget::Player(index),
                }
            })
            .collect();
        let trace: Trace = Rc::default();
        let prefix = lua.create_table().unwrap();
        let global = lua.create_table().unwrap();
        for (index, &(start, end)) in spans.iter().enumerate() {
            let tables: Vec<_> = overlays.iter().map(|a| a.table.clone()).collect();
            let players: Vec<_> = tracked.iter().map(|a| a.table.clone()).collect();
            let trace = Rc::clone(&trace);
            let function = lua
                .create_function(move |lua, (beat, seconds): (f32, f32)| {
                    if tracing {
                        let delta = compile_song_runtime_delta_values(lua)?.0;
                        trace.borrow_mut().push((
                            index,
                            beat.to_bits(),
                            seconds.to_bits(),
                            delta.to_bits(),
                        ));
                    }
                    if failure.is_some_and(|failure| beat >= failure) {
                        return Err(mlua::Error::runtime("sample callback failed"));
                    }
                    if dynamic {
                        for (i, table) in tables.iter().enumerate() {
                            table.raw_set(
                                "__songlua_state_x",
                                beat * 7.0 + i as f32 + index as f32,
                            )?;
                            table.raw_set("__songlua_state_y", (beat * 2.0).sin())?;
                        }
                        for (i, table) in players.iter().enumerate() {
                            table.raw_set("__songlua_state_x", beat * (i + 1) as f32)?;
                        }
                    }
                    Ok(())
                })
                .unwrap();
            let entry = lua.create_table().unwrap();
            entry.raw_set(1, start).unwrap();
            entry.raw_set(2, end).unwrap();
            entry.raw_set(3, function).unwrap();
            let dest = if index % 2 == 0 { &prefix } else { &global };
            dest.raw_set(dest.raw_len() + 1, entry).unwrap();
        }
        let messages = if messages {
            [0.1, 0.4, 0.4, 2.1]
                .into_iter()
                .map(|beat| SongLuaMessageEvent {
                    beat,
                    message: "Pulse".into(),
                    persists: false,
                })
                .collect()
        } else {
            vec![]
        };
        let fixture = Self {
            lua,
            context,
            overlays,
            tracked,
            prefix,
            global,
            messages,
            trace,
        };
        fixture.reset();
        fixture
    }

    fn reset(&self) {
        self.trace.borrow_mut().clear();
        for overlay in &self.overlays {
            set_actor_overlay_getter_state(&self.lua, &overlay.table, overlay.actor.initial_state)
                .unwrap();
        }
        for tracked in &self.tracked {
            tracked.table.raw_set("__songlua_state_x", 0.0).unwrap();
        }
        set_compile_song_runtime_values(&self.lua, 17.0, 8.0).unwrap();
        set_compile_song_runtime_delta_values(&self.lua, 0.2, 0.1).unwrap();
    }

    fn run(
        &mut self,
        old: bool,
    ) -> Result<
        (
            Vec<SongLuaEaseWindow>,
            Vec<SongLuaOverlayEase>,
            SongLuaCompileInfo,
        ),
        String,
    > {
        self.reset();
        if old {
            baseline::compile_perframes(
                &self.lua,
                Some(self.prefix.clone()),
                Some(self.global.clone()),
                &self.context,
                &mut self.overlays,
                &self.tracked,
                &self.messages,
            )
        } else {
            compile_perframes(
                &self.lua,
                Some(self.prefix.clone()),
                Some(self.global.clone()),
                &self.context,
                &mut self.overlays,
                &self.tracked,
                &self.messages,
            )
        }
    }

    fn state(&self) -> String {
        format!(
            "{:?}/{:?}/{:?}/{:?}/{:?}",
            current_overlay_compile_actor_states(&self.overlays).unwrap(),
            current_perframe_player_states(&tracked_player_tables(&self.tracked)).unwrap(),
            compile_song_runtime_values(&self.lua).unwrap(),
            compile_song_runtime_delta_values(&self.lua).unwrap(),
            self.trace.borrow()
        )
    }
}

#[test]
fn lua_stream_compilation_matches_order_gaps_messages_and_callback_errors() {
    for count in [0, 1, 4] {
        for dynamic in [false, true] {
            for messages in [false, true] {
                for failure in [None, Some(0.55)] {
                    let spans = [(0.0, 1.0), (0.25, 0.75), (2.0, 3.0), (3.0, 3.0001)];
                    let mut old = Fixture::new(count, dynamic, &spans, messages, failure, true);
                    let mut new = Fixture::new(count, dynamic, &spans, messages, failure, true);
                    let expected = old.run(true);
                    let actual = new.run(false);
                    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
                    assert_eq!(new.state(), old.state());
                    assert_eq!(actual.is_err(), failure.is_some());
                    if dynamic && failure.is_none() {
                        assert!(!actual.unwrap().0.is_empty());
                    }
                }
            }
        }
    }
    let mut fixture = Fixture::new(0, false, &[], false, None, true);
    assert_eq!(
        format!("{:?}", fixture.run(true)),
        format!("{:?}", fixture.run(false))
    );
}

#[test]
fn lua_stream_snapshot_reuse_and_iterator_have_no_warm_churn() {
    let fixture = Fixture::new(4, false, &[(0.0, 1.0)], false, None, false);
    let players = tracked_player_tables(&fixture.tracked);
    let mut snapshot = PerframeSnapshot::default();
    snapshot.capture(0.0, &players, &fixture.overlays).unwrap();
    fixture.lua.gc_stop();
    crate::perf::assert_no_churn(|| {
        for sample in perframe_sample_iter(0.0, 1.0) {
            snapshot
                .capture(sample.beat, &players, &fixture.overlays)
                .unwrap();
            black_box(&snapshot);
        }
    });
    assert_eq!(snapshot.overlays.len(), 4);
}

#[test]
fn lua_stream_perframes_preserve_side_effect_only_callbacks_and_late_read_errors() {
    for fail in [false, true] {
        let mut results = Vec::new();
        for old in [true, false] {
            let mut fixture = Fixture::new(2, false, &[(0.0, 1.0)], false, None, false);
            let actor = fixture.overlays[1].table.clone();
            let function = fixture.lua.create_function(move |lua, (beat, _): (f32, f32)| {
                crate::runtime::note_song_lua_side_effect(lua)?;
                if fail && beat >= 0.5 {
                    actor.raw_set("__songlua_state_z", Value::Nil)?;
                    let mt = lua.create_table()?;
                    mt.set("__index", lua.load("return function(t,k) if k == '__songlua_state_z' then error('state read failed') end end").eval::<mlua::Function>()?)?;
                    actor.set_metatable(Some(mt))?;
                }
                Ok(())
            }).unwrap();
            fixture
                .prefix
                .raw_get::<Table>(1)
                .unwrap()
                .raw_set(3, function)
                .unwrap();
            let result = fixture.run(old);
            if fail {
                assert!(result.as_ref().unwrap_err().contains("state read failed"));
            } else {
                let (players, overlays, info) = result.as_ref().unwrap();
                assert!(players.is_empty() && overlays.is_empty());
                assert_eq!(info.unsupported_perframes, 0);
            }
            results.push((
                format!("{result:?}"),
                song_lua_side_effect_count(&fixture.lua).unwrap(),
                compile_song_runtime_values(&fixture.lua).unwrap(),
                compile_song_runtime_delta_values(&fixture.lua).unwrap(),
            ));
        }
        assert_eq!(results[0], results[1]);
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_stream_bench_perframes() {
    for (count, dynamic, len) in [
        (0, false, 0.0),
        (1, true, 0.0001),
        (0, false, 1.0),
        (4, false, 1.0),
        (4, true, 1.0),
        (64, false, 1.0),
        (4, false, 32.0),
        (4, true, 32.0),
    ] {
        let spans = if len == 0.0 { vec![] } else { vec![(0.0, len)] };
        let mut fixture = Fixture::new(count, dynamic, &spans, false, None, false);
        let expected = fixture.run(true).unwrap();
        let actual = fixture.run(false).unwrap();
        assert_eq!(format!("{expected:?}"), format!("{actual:?}"));
        fixture.lua.gc_stop();
        let units = baseline::perframe_samples(0.0, len).len();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "perframe_{count}_dynamic_{dynamic}_len_{len}/{}",
                    if old { "old" } else { "new" }
                ),
                8,
                units,
                || {
                    let output = fixture.run(old).unwrap();
                    black_box((&fixture.overlays, &fixture.tracked));
                    black_box(output);
                },
            );
        }
    }
}
