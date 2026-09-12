use super::*;
use std::hint::black_box;

#[path = "capture_dispatch_baseline.rs"]
mod baseline;

struct Fixture {
    lua: Lua,
    actors: Vec<Table>,
}

impl Fixture {
    fn new(count: usize, message: Option<&str>) -> Self {
        let lua = Lua::new();
        let runtime =
            crate::create_song_runtime_table(&lua, &SongLuaCompileContext::new("", "")).unwrap();
        lua.globals()
            .set(crate::SONG_LUA_RUNTIME_KEY, runtime)
            .unwrap();
        set_compile_song_runtime_values(&lua, 7.5, 3.75).unwrap();
        let actors: Vec<_> = (0..count + 1)
            .map(|_| lua.create_table().unwrap())
            .collect();
        begin_overlay_update_capture(
            &lua,
            actors[..count]
                .iter()
                .enumerate()
                .map(|(i, a)| (a.to_pointer() as usize, i))
                .collect(),
        );
        let fixture = Self { lua, actors };
        fixture.message(message);
        fixture
    }

    fn message(&self, message: Option<&str>) {
        let mut capture = self
            .lua
            .app_data_mut::<SongLuaOverlayUpdateCapture>()
            .unwrap();
        capture.active_broadcast = message.map(str::to_owned);
        capture.active_broadcast_command = message.map(|name| {
            self.lua
                .create_string(format!("{name}MessageCommand"))
                .unwrap()
        });
    }

    fn clear(&self) {
        drain_overlay_update_capture(&self.lua, |_, _, _, _| Ok(())).unwrap();
        let mut capture = self
            .lua
            .app_data_mut::<SongLuaOverlayUpdateCapture>()
            .unwrap();
        for writes in capture.stateful_writes.values_mut() {
            writes.clear();
        }
        capture.runtime_broadcasts.clear();
    }

    fn record(
        &self,
        old: bool,
        actor: usize,
        scheduled: bool,
        easing: Option<&str>,
        value: SongLuaOverlayUpdateValue,
    ) -> bool {
        let mut capture = self
            .lua
            .app_data_mut::<SongLuaOverlayUpdateCapture>()
            .unwrap();
        let actor = &self.actors[actor];
        let target = if matches!(value, SongLuaOverlayUpdateValue::VertexColors(_)) {
            SongLuaOverlayUpdateTarget::VertexColors
        } else {
            SongLuaOverlayUpdateTarget::X
        };
        match (old, scheduled) {
            (true, true) => baseline::record_scheduled(
                &mut capture,
                actor,
                -0.0,
                0.25,
                1.5,
                easing.map(str::to_owned),
                Some(-0.0),
                target,
                value,
            ),
            (false, true) => capture.record_scheduled(
                actor,
                -0.0,
                0.25,
                1.5,
                easing.map(str::to_owned),
                Some(-0.0),
                target,
                value,
            ),
            (true, false) => baseline::record(&mut capture, actor, -0.0, target, value),
            (false, false) => capture.record(actor, -0.0, target, value),
        }
    }

    fn record_batch(
        &self,
        old: bool,
        count: usize,
        actor: usize,
        scheduled: bool,
        easing: Option<&str>,
        value: &SongLuaOverlayUpdateValue,
    ) {
        self.clear();
        for _ in 0..count {
            black_box(self.record(old, actor, scheduled, easing, value.clone()));
        }
        black_box(
            &self
                .lua
                .app_data_ref::<SongLuaOverlayUpdateCapture>()
                .unwrap()
                .scheduled,
        );
    }

    fn update(
        &self,
        old: bool,
        actor: usize,
        immediate: bool,
        key: &str,
        value: SongLuaOverlayUpdateValue,
    ) -> bool {
        match (old, immediate) {
            (true, true) => baseline::record_overlay_update_capture_immediate(
                &self.lua,
                &self.actors[actor],
                key,
                value,
            ),
            (true, false) => {
                baseline::record_overlay_update_capture(&self.lua, &self.actors[actor], key, value)
            }
            (false, true) => {
                record_overlay_update_capture_immediate(&self.lua, &self.actors[actor], key, value)
            }
            (false, false) => {
                record_overlay_update_capture(&self.lua, &self.actors[actor], key, value)
            }
        }
    }

    fn update_batch(&self, old: bool, count: usize, immediate: bool) {
        self.clear();
        for i in 0..count {
            black_box(self.update(
                old,
                0,
                immediate,
                "x",
                SongLuaOverlayUpdateValue::F32(i as f32),
            ));
        }
    }
}

fn assert_capture(new: &Fixture, old: &Fixture) {
    let a = new
        .lua
        .app_data_ref::<SongLuaOverlayUpdateCapture>()
        .unwrap();
    let b = old
        .lua
        .app_data_ref::<SongLuaOverlayUpdateCapture>()
        .unwrap();
    assert_eq!(a.touched, b.touched);
    assert_eq!(a.touched_flags, b.touched_flags);
    assert_eq!(a.values, b.values);
    assert_eq!(a.final_values, b.final_values);
    assert_eq!(a.stateful_messages, b.stateful_messages);
    assert_eq!(a.stateful_writes, b.stateful_writes);
    assert_eq!(a.runtime_broadcasts, b.runtime_broadcasts);
    for (a, b) in a.scheduled.iter().zip(&b.scheduled) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.delay_seconds.to_bits(), b.delay_seconds.to_bits());
            assert_eq!(a.duration_seconds.to_bits(), b.duration_seconds.to_bits());
            assert_eq!(a.easing, b.easing);
            assert_eq!(a.opt1.map(f32::to_bits), b.opt1.map(f32::to_bits));
            assert_eq!(a.target, b.target);
            assert_eq!(a.value, b.value);
        }
    }
}

#[test]
fn record_dispatch_preserves_active_inactive_unknown_and_owned_writes() {
    let old = Fixture::new(2, None);
    let new = Fixture::new(2, None);
    let colors = Arc::new([[0.0, -0.0, 0.5, 1.0]; 4]);
    for message in [None, Some("A"), Some("B"), Some("A"), Some(""), None] {
        old.message(message);
        new.message(message);
        for actor in [0, 2, 1, 0] {
            for scheduled in [false, true] {
                for easing in [None, Some("decelerate"), Some("")] {
                    for value in [
                        SongLuaOverlayUpdateValue::F32(-0.0),
                        SongLuaOverlayUpdateValue::VertexColors(colors.clone()),
                    ] {
                        assert_eq!(
                            old.record(true, actor, scheduled, easing, value.clone()),
                            new.record(false, actor, scheduled, easing, value)
                        );
                    }
                }
            }
        }
        assert_capture(&new, &old);
    }
    let capture = new
        .lua
        .app_data_ref::<SongLuaOverlayUpdateCapture>()
        .unwrap();
    let SongLuaOverlayUpdateValue::VertexColors(actual) = &capture.scheduled[0][1].value else {
        panic!("lost colors")
    };
    assert!(Arc::ptr_eq(actual, &colors));
    assert_eq!(
        capture.scheduled[0][0].opt1.unwrap().to_bits(),
        (-0.0_f32).to_bits()
    );
    drop(capture);
    old.clear();
    new.clear();
    assert_capture(&new, &old);
}

#[test]
fn inactive_capture_dispatch_only_allocates_owned_easing_once() {
    let fixture = Fixture::new(1, None);
    let value = SongLuaOverlayUpdateValue::VertexColors(Arc::new([[0.5; 4]; 4]));
    fixture.record_batch(false, 128, 0, true, Some("decelerate"), &value);
    crate::perf::assert_churn_budget(128, 128 * "decelerate".len(), || {
        fixture.record_batch(false, 128, 0, true, Some("decelerate"), &value);
    });
    fixture.clear();
    fixture.record_batch(false, 128, 0, false, None, &value);
    crate::perf::assert_no_churn(|| fixture.record_batch(false, 128, 0, false, None, &value));
    fixture.clear();
    fixture.record_batch(false, 128, 0, true, None, &value);
    crate::perf::assert_no_churn(|| fixture.record_batch(false, 128, 0, true, None, &value));
}

#[test]
fn capture_dispatch_key_tracks_live_handlers_and_preserves_suppression() {
    let old = Fixture::new(1, None);
    let new = Fixture::new(1, None);
    for message in [None, Some("A"), Some("B"), Some("日本語"), Some("")] {
        old.message(message);
        new.message(message);
        for mode in 0..4 {
            for fixture in [&old, &new] {
                let key = format!("{}MessageCommand", message.unwrap_or("A"));
                let value = match mode {
                    0 | 3 => Value::Nil,
                    1 => Value::Function(fixture.lua.create_function(|_, ()| Ok(())).unwrap()),
                    _ => Value::Integer(7), // Conversion failure is treated as no handler.
                };
                fixture.actors[0].set(key, value).unwrap();
                fixture.actors[0]
                    .set("__songlua_capture_duration", 0.5)
                    .unwrap();
            }
            for immediate in [true, false] {
                old.clear();
                new.clear();
                for actor in [0, 1] {
                    for key in ["x", "unsupported"] {
                        assert_eq!(
                            old.update(
                                true,
                                actor,
                                immediate,
                                key,
                                SongLuaOverlayUpdateValue::F32(3.0)
                            ),
                            new.update(
                                false,
                                actor,
                                immediate,
                                key,
                                SongLuaOverlayUpdateValue::F32(3.0)
                            )
                        );
                    }
                }
                assert_capture(&new, &old);
                if mode == 1 && message.is_some() {
                    let capture = new
                        .lua
                        .app_data_ref::<SongLuaOverlayUpdateCapture>()
                        .unwrap();
                    assert!(capture.values[0].is_empty());
                    assert!(capture.scheduled[0].is_empty());
                    assert_eq!(capture.touched.is_empty(), immediate);
                }
            }
        }
    }
    // Missing runtime tables must still use the same zero-beat fallback.
    for fixture in [&old, &new] {
        fixture.clear();
        fixture.message(Some("Fallback"));
        fixture
            .lua
            .globals()
            .raw_remove(crate::SONG_LUA_RUNTIME_KEY)
            .unwrap();
    }
    assert!(old.update(true, 0, true, "x", SongLuaOverlayUpdateValue::F32(9.0)));
    assert!(new.update(false, 0, true, "x", SongLuaOverlayUpdateValue::F32(9.0)));
    assert_capture(&new, &old);
    assert_eq!(
        new.lua
            .app_data_ref::<SongLuaOverlayUpdateCapture>()
            .unwrap()
            .stateful_writes["Fallback"][0]
            .beat,
        0.0
    );
}

#[test]
fn capture_dispatch_cached_key_keeps_metatable_lookups() {
    let name = "Long".repeat(64);
    let fixture = Fixture::new(1, Some(&name));
    fixture.actors[0]
        .set(
            format!("{name}MessageCommand"),
            fixture.lua.create_function(|_, ()| Ok(())).unwrap(),
        )
        .unwrap();
    fixture.update_batch(false, 128, true);
    crate::perf::assert_no_churn(|| fixture.update_batch(false, 128, true));

    for old in [true, false] {
        let fixture = Fixture::new(1, Some("Inherited"));
        let meta = fixture.lua.create_table().unwrap();
        let lookup = fixture.lua.load("return function(_, key) assert(key == 'InheritedMessageCommand'); return function() end end").eval::<Function>().unwrap();
        meta.set("__index", lookup).unwrap();
        fixture.actors[0].set_metatable(Some(meta)).unwrap();
        assert!(fixture.update(old, 0, true, "x", SongLuaOverlayUpdateValue::F32(1.0)));
        assert!(
            fixture
                .lua
                .app_data_ref::<SongLuaOverlayUpdateCapture>()
                .unwrap()
                .values[0]
                .is_empty()
        );
    }
}

fn broadcast(lua: &Lua, old: bool, name: &str) -> mlua::Result<()> {
    if old {
        baseline::broadcast_song_lua_message(lua, name, None)
    } else {
        broadcast_song_lua_message(lua, name, None)
    }
}

#[test]
fn capture_dispatch_nested_broadcast_restores_names_on_success_and_error() {
    for old in [true, false] {
        for fail in [false, true] {
            let fixture = Fixture::new(2, None);
            let registry = song_lua_actor_registry(&fixture.lua).unwrap();
            registry.raw_set(1, fixture.actors[0].clone()).unwrap();
            fixture
                .lua
                .globals()
                .raw_set(ACTIVE_BROADCAST_KEY, "before")
                .unwrap();
            let receiver = fixture.actors[1].clone();
            let write = fixture
                .lua
                .create_function(move |lua, number: f32| {
                    if !old {
                        let capture = lua.app_data_ref::<SongLuaOverlayUpdateCapture>().unwrap();
                        let (message, command) = if number == 2.0 {
                            ("Inner", "InnerMessageCommand")
                        } else {
                            ("Outer", "OuterMessageCommand")
                        };
                        assert_eq!(capture.active_broadcast.as_deref(), Some(message));
                        assert_eq!(
                            capture
                                .active_broadcast_command
                                .as_ref()
                                .unwrap()
                                .as_bytes()
                                .as_ref(),
                            command.as_bytes()
                        );
                    }
                    let value = SongLuaOverlayUpdateValue::F32(number);
                    let recorded = if old {
                        baseline::record_overlay_update_capture_immediate(
                            lua, &receiver, "x", value,
                        )
                    } else {
                        record_overlay_update_capture_immediate(lua, &receiver, "x", value)
                    };
                    assert!(recorded);
                    Ok(())
                })
                .unwrap();
            fixture.lua.globals().set("write", write).unwrap();
            fixture
                .lua
                .globals()
                .set(
                    "nested",
                    fixture
                        .lua
                        .create_function(move |lua, ()| broadcast(lua, old, "Inner"))
                        .unwrap(),
                )
                .unwrap();
            fixture.actors[0]
                .set(
                    "OuterMessageCommand",
                    fixture
                        .lua
                        .load("return function() write(1); pcall(nested); write(3) end")
                        .eval::<Function>()
                        .unwrap(),
                )
                .unwrap();
            let inner = if fail {
                "return function() write(2); error('inner failed') end"
            } else {
                "return function() write(2) end"
            };
            fixture.actors[0]
                .set(
                    "InnerMessageCommand",
                    fixture.lua.load(inner).eval::<Function>().unwrap(),
                )
                .unwrap();
            broadcast(&fixture.lua, old, "Outer").unwrap();
            let capture = fixture
                .lua
                .app_data_ref::<SongLuaOverlayUpdateCapture>()
                .unwrap();
            assert!(capture.active_broadcast.is_none());
            assert!(capture.active_broadcast_command.is_none());
            assert_eq!(
                capture.stateful_writes["Outer"]
                    .iter()
                    .map(|w| w.value.clone())
                    .collect::<Vec<_>>(),
                vec![
                    SongLuaOverlayUpdateValue::F32(1.0),
                    SongLuaOverlayUpdateValue::F32(3.0)
                ]
            );
            assert_eq!(capture.stateful_writes["Inner"][0].beat, 7.5);
            assert_eq!(
                capture.stateful_writes["Inner"][0].value,
                SongLuaOverlayUpdateValue::F32(2.0)
            );
            assert_eq!(
                capture
                    .runtime_broadcasts
                    .iter()
                    .map(|(_, m, _)| m.as_str())
                    .collect::<Vec<_>>(),
                ["Outer", "Inner"]
            );
            drop(capture);
            assert_eq!(
                fixture
                    .lua
                    .globals()
                    .get::<String>(ACTIVE_BROADCAST_KEY)
                    .unwrap(),
                "before"
            );
            if fail {
                assert!(
                    broadcast(&fixture.lua, old, "Inner")
                        .unwrap_err()
                        .to_string()
                        .contains("inner failed")
                );
                let capture = fixture
                    .lua
                    .app_data_ref::<SongLuaOverlayUpdateCapture>()
                    .unwrap();
                assert!(capture.active_broadcast.is_none());
                assert!(capture.active_broadcast_command.is_none());
            }
            let before = runtime_broadcast_captures(&fixture.lua).len();
            broadcast(&fixture.lua, old, " \t").unwrap();
            assert_eq!(runtime_broadcast_captures(&fixture.lua).len(), before);
        }
    }
}

fn overlays(fixture: &Fixture) -> Vec<SongLuaOverlayCompileActor<()>> {
    fixture.actors[..fixture.actors.len() - 1]
        .iter()
        .map(|table| SongLuaOverlayCompileActor {
            table: table.clone(),
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

fn reset(lua: &Lua, overlays: &[SongLuaOverlayCompileActor<()>], old: bool) -> Result<(), String> {
    if old {
        baseline::reset_overlay_compile_actor_capture_tables(lua, overlays)
    } else {
        reset_overlay_compile_actor_capture_tables(lua, overlays)
    }
}

#[test]
fn capture_dispatch_reset_preserves_fields_and_stops_at_first_error() {
    for old in [true, false] {
        for failure in [None, Some(0), Some(1), Some(2)] {
            let fixture = Fixture::new(3, None);
            let overlays = overlays(&fixture);
            for (index, actor) in fixture.actors[..3].iter().enumerate() {
                actor.set("keep", index).unwrap();
                actor.set("__songlua_capture_cursor", 17.0).unwrap();
                actor.set("__songlua_capture_easing", "linear").unwrap();
            }
            if let Some(index) = failure {
                let meta = fixture.lua.create_table().unwrap();
                meta.set(
                    "__newindex",
                    fixture
                        .lua
                        .load("return function() error('reset failed') end")
                        .eval::<Function>()
                        .unwrap(),
                )
                .unwrap();
                fixture.actors[index].set_metatable(Some(meta)).unwrap();
            }
            let result = reset(&fixture.lua, &overlays, old);
            assert_eq!(result.is_err(), failure.is_some());
            if let Err(error) = result {
                assert!(error.contains("reset failed"));
            }
            for (index, actor) in fixture.actors[..3].iter().enumerate() {
                assert_eq!(actor.get::<usize>("keep").unwrap(), index);
                let reached = failure.is_none_or(|failed| index <= failed);
                assert_eq!(
                    actor.get::<f32>("__songlua_capture_cursor").unwrap(),
                    if reached { 0.0 } else { 17.0 }
                );
                if failure.is_none_or(|failed| index < failed) {
                    assert!(matches!(
                        actor.get::<Value>("__songlua_capture_easing").unwrap(),
                        Value::Nil
                    ));
                    assert_eq!(
                        actor
                            .get::<Table>("__songlua_capture_blocks")
                            .unwrap()
                            .raw_len(),
                        0
                    );
                    assert_eq!(actor.get::<f32>("__songlua_capture_duration").unwrap(), 0.0);
                    assert_eq!(
                        actor
                            .get::<f32>("__songlua_capture_tween_time_left")
                            .unwrap(),
                        0.0
                    );
                }
            }
            reset(&fixture.lua, &[], old).unwrap();
        }
    }
}

#[test]
fn capture_dispatch_reset_only_allocates_required_lua_tables() {
    let fixture = Fixture::new(128, None);
    let overlays = overlays(&fixture);
    reset(&fixture.lua, &overlays, false).unwrap();
    // mlua routes the 56-byte empty Lua tables through Rust's allocator.
    fixture.lua.gc_stop();
    crate::perf::assert_churn_budget(128, 128 * 56, || {
        reset(&fixture.lua, &overlays, false).unwrap()
    });
}

fn pair(name: &str, units: usize, iterations: usize, mut old: impl FnMut(), mut new: impl FnMut()) {
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
fn capture_dispatch_bench() {
    let colors = SongLuaOverlayUpdateValue::VertexColors(Arc::new([[0.5; 4]; 4]));
    let scalar = SongLuaOverlayUpdateValue::F32(0.5);
    let long_easing = "e".repeat(256);
    for (label, count, active, actor, scheduled, easing, value) in [
        ("record_scalar_1", 1, false, 0, false, None, &scalar),
        ("record_scalar_128", 128, false, 0, false, None, &scalar),
        ("record_colors_128", 128, false, 0, false, None, &colors),
        ("scheduled_plain_128", 128, false, 0, true, None, &scalar),
        (
            "scheduled_easing_1",
            1,
            false,
            0,
            true,
            Some("decelerate"),
            &scalar,
        ),
        (
            "scheduled_easing_128",
            128,
            false,
            0,
            true,
            Some("decelerate"),
            &scalar,
        ),
        (
            "scheduled_long_colors_128",
            128,
            false,
            0,
            true,
            Some(long_easing.as_str()),
            &colors,
        ),
        (
            "scheduled_active_128",
            128,
            true,
            0,
            true,
            Some("decelerate"),
            &colors,
        ),
        (
            "scheduled_unknown_128",
            128,
            true,
            1,
            true,
            Some("decelerate"),
            &colors,
        ),
    ] {
        let old = Fixture::new(1, active.then_some("Active"));
        let new = Fixture::new(1, active.then_some("Active"));
        old.record_batch(true, count, actor, scheduled, easing, value);
        new.record_batch(false, count, actor, scheduled, easing, value);
        assert_capture(&new, &old);
        pair(
            label,
            count,
            512,
            || old.record_batch(true, count, actor, scheduled, easing, value),
            || new.record_batch(false, count, actor, scheduled, easing, value),
        );
    }
    for (len, handler, immediate) in [
        (8, true, true),
        (8, true, false),
        (256, true, true),
        (8, false, true),
        (8, false, false),
    ] {
        let message = "m".repeat(len);
        let old = Fixture::new(1, Some(&message));
        let new = Fixture::new(1, Some(&message));
        if handler {
            for fixture in [&old, &new] {
                fixture.actors[0]
                    .set(
                        format!("{message}MessageCommand"),
                        fixture.lua.create_function(|_, ()| Ok(())).unwrap(),
                    )
                    .unwrap();
            }
        }
        old.update_batch(true, 128, immediate);
        new.update_batch(false, 128, immediate);
        assert_capture(&new, &old);
        pair(
            &format!("lookup_128_name_{len}_handler_{handler}_immediate_{immediate}"),
            128,
            256,
            || old.update_batch(true, 128, immediate),
            || new.update_batch(false, 128, immediate),
        );
    }
    // Includes command cache creation/restoration and real Lua callback dispatch.
    for count in [0, 1, 16, 128] {
        let old = Fixture::new(1, None);
        let new = Fixture::new(1, None);
        for (is_old, fixture) in [(true, &old), (false, &new)] {
            song_lua_actor_registry(&fixture.lua)
                .unwrap()
                .raw_set(1, fixture.actors[0].clone())
                .unwrap();
            let command = fixture
                .lua
                .create_function(move |lua, actor: Table| {
                    for i in 0..count {
                        let value = SongLuaOverlayUpdateValue::F32(i as f32);
                        let result = if is_old {
                            baseline::record_overlay_update_capture_immediate(
                                lua, &actor, "x", value,
                            )
                        } else {
                            record_overlay_update_capture_immediate(lua, &actor, "x", value)
                        };
                        black_box(result);
                    }
                    Ok(())
                })
                .unwrap();
            fixture.actors[0]
                .set("PulseMessageCommand", command)
                .unwrap();
        }
        let run = |fixture: &Fixture, is_old| {
            fixture.clear();
            broadcast(&fixture.lua, is_old, "Pulse").unwrap();
        };
        run(&old, true);
        run(&new, false);
        assert_capture(&new, &old);
        pair(
            &format!("broadcast_{count}_writes"),
            count.max(1),
            256,
            || run(&old, true),
            || run(&new, false),
        );
    }
    for count in [0, 1, 16, 128] {
        let old = Fixture::new(count, None);
        let new = Fixture::new(count, None);
        let old_overlays = overlays(&old);
        let new_overlays = overlays(&new);
        pair(
            &format!("reset_{count}_actors"),
            count.max(1),
            256,
            || reset(&old.lua, &old_overlays, true).unwrap(),
            || reset(&new.lua, &new_overlays, false).unwrap(),
        );
    }
}
