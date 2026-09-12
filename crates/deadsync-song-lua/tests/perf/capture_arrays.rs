use super::*;
use std::hint::black_box;

#[path = "capture_arrays_baseline.rs"]
mod baseline;

const CASES: [(&str, &str); 10] = [
    ("color", "__songlua_state_diffuse"),
    ("vec2", "__songlua_state_texcoord_offset"),
    ("vec3", "__songlua_state_effect_magnitude"),
    ("vec4", "__songlua_state_glow"),
    ("vec5", "__songlua_state_effect_timing"),
    ("size", "__songlua_state_size"),
    ("stretch", "__songlua_state_stretch_rect"),
    ("immediate3", "__songlua_state_effect_magnitude"),
    ("immediate4", "__songlua_state_effect_color1"),
    ("immediate5", "__songlua_state_effect_timing"),
];

fn write(lua: &Lua, actor: &Table, kind: &str, v: [f32; 5], old: bool) -> mlua::Result<()> {
    match (old, kind) {
        (true, "color") => baseline::capture_block_set_color(lua, actor, [v[0], v[1], v[2], v[3]]),
        (false, "color") => capture_block_set_color(lua, actor, [v[0], v[1], v[2], v[3]]),
        (true, "vec2") => {
            baseline::capture_block_set_vec2(lua, actor, "texcoord_offset", [v[0], v[1]])
        }
        (false, "vec2") => capture_block_set_vec2(lua, actor, "texcoord_offset", [v[0], v[1]]),
        (true, "vec3") => {
            baseline::capture_block_set_vec3(lua, actor, "effect_magnitude", [v[0], v[1], v[2]])
        }
        (false, "vec3") => {
            capture_block_set_vec3(lua, actor, "effect_magnitude", [v[0], v[1], v[2]])
        }
        (true, "vec4") => {
            baseline::capture_block_set_vec4(lua, actor, "glow", [v[0], v[1], v[2], v[3]])
        }
        (false, "vec4") => capture_block_set_vec4(lua, actor, "glow", [v[0], v[1], v[2], v[3]]),
        (true, "vec5") => baseline::capture_block_set_vec5(lua, actor, "effect_timing", v),
        (false, "vec5") => capture_block_set_vec5(lua, actor, "effect_timing", v),
        (true, "size") => baseline::capture_block_set_size(lua, actor, [v[0], v[1]]),
        (false, "size") => capture_block_set_size(lua, actor, [v[0], v[1]]),
        (true, "stretch") => {
            baseline::capture_block_set_stretch(lua, actor, [v[0], v[1], v[2], v[3]])
        }
        (false, "stretch") => capture_block_set_stretch(lua, actor, [v[0], v[1], v[2], v[3]]),
        (true, "immediate3") => {
            baseline::capture_immediate_vec3(lua, actor, "effect_magnitude", [v[0], v[1], v[2]])
        }
        (false, "immediate3") => {
            capture_immediate_vec3(lua, actor, "effect_magnitude", [v[0], v[1], v[2]])
        }
        (true, "immediate4") => {
            baseline::capture_immediate_vec4(lua, actor, "effect_color1", [v[0], v[1], v[2], v[3]])
        }
        (false, "immediate4") => {
            capture_immediate_vec4(lua, actor, "effect_color1", [v[0], v[1], v[2], v[3]])
        }
        (true, "immediate5") => baseline::capture_immediate_vec5(lua, actor, "effect_timing", v),
        (false, "immediate5") => capture_immediate_vec5(lua, actor, "effect_timing", v),
        _ => unreachable!(),
    }
}

fn colors(lua: &Lua, nested: bool, old: bool, values: [f32; 4]) -> Table {
    match (old, nested) {
        (true, false) => baseline::make_color_table(lua, values),
        (false, false) => make_color_table(lua, values),
        (true, true) => baseline::make_vertex_color_table(lua, [values; 4]),
        (false, true) => make_vertex_color_table(lua, [values; 4]),
    }
    .unwrap()
}

fn fingerprint(value: Value) -> String {
    match value {
        Value::Table(table) => {
            let mut entries: Vec<_> = table
                .pairs::<Value, Value>()
                .map(|pair| {
                    let (key, value) = pair.unwrap();
                    (fingerprint(key), fingerprint(value))
                })
                .collect();
            entries.sort();
            format!("{entries:?}")
        }
        Value::Number(number) => format!("n:{:x}", number.to_bits()),
        value => format!("{value:?}"),
    }
}

fn fixture(active: bool) -> (Lua, Table) {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    reset_actor_capture(&lua, &actor).unwrap();
    if active {
        let runtime =
            crate::create_song_runtime_table(&lua, &SongLuaCompileContext::new("", "")).unwrap();
        lua.globals()
            .set(crate::SONG_LUA_RUNTIME_KEY, runtime)
            .unwrap();
        set_compile_song_runtime_values(&lua, 7.5, 3.75).unwrap();
        begin_overlay_update_capture(&lua, HashMap::from([(actor.to_pointer() as usize, 0)]));
    }
    (lua, actor)
}

#[test]
fn lua_transfer_color_tables_preserve_components_and_independent_identity() {
    let lua = Lua::new();
    for nested in [false, true] {
        for v in [
            [0.0, 0.5, 1.0, 2.0],
            [-0.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY],
        ] {
            let old = colors(&lua, nested, true, v);
            let new = colors(&lua, nested, false, v);
            assert_eq!(
                fingerprint(Value::Table(old)),
                fingerprint(Value::Table(new.clone()))
            );
            assert_eq!(new.raw_len(), 4);
            assert!(new.metatable().is_none());
            let again = colors(&lua, nested, false, v);
            assert_ne!(new.to_pointer(), again.to_pointer());
            if nested {
                for index in 1..=4 {
                    let row: Table = new.raw_get(index).unwrap();
                    assert_eq!(row.raw_len(), 4);
                    assert_ne!(
                        row.to_pointer(),
                        again.raw_get::<Table>(index).unwrap().to_pointer()
                    );
                }
            }
        }
    }
}

#[test]
fn lua_transfer_capture_arrays_preserve_blocks_state_and_old_aliases() {
    for (kind, state_key) in CASES {
        let mut outputs = Vec::new();
        for old in [true, false] {
            let (lua, actor) = fixture(false);
            write(&lua, &actor, kind, [0.25, 0.5, 0.75, 1.0, 2.0], old).unwrap();
            let alias: Table = actor.raw_get(state_key).unwrap();
            let before = fingerprint(Value::Table(alias.clone()));
            write(&lua, &actor, kind, [-0.0, 2.0, 3.0, 4.0, 5.0], old).unwrap();
            let current: Table = actor.raw_get(state_key).unwrap();
            assert_ne!(alias.to_pointer(), current.to_pointer());
            assert_eq!(before, fingerprint(Value::Table(alias)));
            flush_actor_capture(&actor).unwrap();
            outputs.push((
                fingerprint(Value::Table(actor.clone())),
                read_actor_capture_blocks(&actor).unwrap(),
            ));
        }
        assert_eq!(outputs[0], outputs[1], "{kind}");
    }
}

#[test]
fn lua_transfer_capture_arrays_preserve_active_updates_and_nonfinite_values() {
    for (kind, _) in CASES {
        for v in [
            [0.25, 0.5, 0.75, 1.0, 2.0],
            [-0.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0],
        ] {
            let mut outputs = Vec::new();
            for old in [true, false] {
                let (lua, actor) = fixture(true);
                write(&lua, &actor, kind, v, old).unwrap();
                let mut updates = Vec::new();
                drain_overlay_update_capture(&lua, |index, values, scheduled, final_values| {
                    let scheduled: Vec<_> = scheduled
                        .iter()
                        .map(|update| {
                            (
                                update.delay_seconds.to_bits(),
                                update.duration_seconds.to_bits(),
                                &update.easing,
                                update.opt1.map(f32::to_bits),
                                update.target,
                                &update.value,
                            )
                        })
                        .collect();
                    updates.push(format!(
                        "{index}: {values:?} {scheduled:?} {final_values:?}"
                    ));
                    Ok(())
                })
                .unwrap();
                assert!(!updates.is_empty());
                outputs.push((fingerprint(Value::Table(actor)), updates));
            }
            assert_eq!(outputs[0], outputs[1], "{kind}");
        }
    }
}

#[test]
fn lua_transfer_capture_arrays_preserve_partial_errors_and_write_order() {
    for (kind, _) in CASES {
        let mut outputs = Vec::new();
        for old in [true, false] {
            let (lua, actor) = fixture(false);
            lua.globals()
                .set("writes", lua.create_table().unwrap())
                .unwrap();
            let mt = lua.load("return {__newindex=function(t,k,v) writes[#writes+1]=k; if k:sub(1,16)=='__songlua_state_' then error('state write failed') end rawset(t,k,v) end}").eval::<Table>().unwrap();
            actor.set_metatable(Some(mt)).unwrap();
            let error = write(&lua, &actor, kind, [0.25, 0.5, 0.75, 1.0, 2.0], old)
                .unwrap_err()
                .to_string();
            outputs.push((
                error,
                fingerprint(Value::Table(actor)),
                fingerprint(lua.globals().get("writes").unwrap()),
            ));
        }
        assert_eq!(outputs[0], outputs[1], "{kind}");
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_transfer_bench_arrays() {
    for nested in [false, true] {
        let lua = Lua::new();
        let v = [0.25, 0.5, 0.75, 1.0];
        assert_eq!(
            fingerprint(Value::Table(colors(&lua, nested, true, v))),
            fingerprint(Value::Table(colors(&lua, nested, false, v)))
        );
        lua.gc_stop();
        for old in order() {
            crate::perf::measure_sampled(
                &format!("color_nested_{nested}/{}", if old { "old" } else { "new" }),
                64,
                16,
                || {
                    for _ in 0..16 {
                        black_box(colors(black_box(&lua), nested, old, black_box(v)));
                    }
                },
            );
        }
    }
    for active in [false, true] {
        for (kind, _) in CASES {
            let (lua, actor) = fixture(active);
            let v = [0.25, 0.5, 0.75, 1.0, 2.0];
            write(&lua, &actor, kind, v, true).unwrap();
            let before = fingerprint(Value::Table(actor.clone()));
            write(&lua, &actor, kind, v, false).unwrap();
            assert_eq!(before, fingerprint(Value::Table(actor.clone())));
            lua.gc_stop();
            for old in order() {
                crate::perf::measure_sampled(
                    &format!(
                        "array_{kind}_active_{active}/{}",
                        if old { "old" } else { "new" }
                    ),
                    64,
                    16,
                    || {
                        for _ in 0..16 {
                            write(black_box(&lua), black_box(&actor), kind, black_box(v), old)
                                .unwrap();
                        }
                        black_box(&actor);
                    },
                );
            }
        }
    }
}

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    }
}
