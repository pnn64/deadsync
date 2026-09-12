use super::*;
use std::cell::RefCell;
use std::hint::black_box;
use std::rc::Rc;

#[path = "sprite_keys_baseline.rs"]
mod baseline;

fn read(actor: &Table, old: bool) -> Result<Vec<crate::SongLuaSpriteState>, String> {
    if old {
        baseline::read_sprite_states(actor)
    } else {
        read_sprite_states(actor)
    }
}
fn bits(result: Result<Vec<crate::SongLuaSpriteState>, String>) -> Result<Vec<(u32, u32)>, String> {
    result.map(|states| {
        states
            .into_iter()
            .map(|s| (s.frame, s.delay.to_bits()))
            .collect()
    })
}
fn actor(lua: &Lua, count: usize) -> Table {
    let actor = lua.create_table().unwrap();
    for index in 0..count {
        actor
            .raw_set(format!("Frame{index:04}"), (index % 8) as u32)
            .unwrap();
        if index % 3 != 0 {
            actor.raw_set(format!("Delay{index:04}"), 0.125).unwrap();
        }
    }
    actor
}

#[test]
fn lua_read_sprite_keys_match_formatting_boundaries_and_have_no_churn() {
    let mut buffer = [0xff; 16];
    for index in (0..20_005).chain([i32::MIN, -10000, -1000, -1, 99999, 1_000_000, i32::MAX]) {
        let len = write_sprite_frame_key(index, &mut buffer);
        assert_eq!(&buffer[..len], format!("Frame{index:04}").as_bytes());
        buffer[..5].copy_from_slice(b"Delay");
        assert_eq!(&buffer[..len], format!("Delay{index:04}").as_bytes());
    }
    crate::perf::assert_no_churn(|| {
        for index in [0, 9, 9999, 10000, i32::MIN, i32::MAX] {
            black_box(write_sprite_frame_key(black_box(index), &mut buffer));
        }
    });
    let lua = Lua::new();
    let actor = actor(&lua, 0);
    read_sprite_states(&actor).unwrap();
    lua.gc_stop();
    crate::perf::assert_no_churn(|| {
        black_box(read_sprite_states(black_box(&actor)).unwrap());
    });
}

#[test]
fn lua_read_sprite_states_preserve_holes_defaults_numeric_values_and_large_indices() {
    let lua = Lua::new();
    for count in [0, 1, 8, 64, 10002] {
        let actor = actor(&lua, count);
        assert_eq!(bits(read(&actor, true)), bits(read(&actor, false)));
        assert_eq!(read(&actor, false).unwrap().len(), count);
        if count > 3 {
            actor.raw_set("Frame0001", Value::Nil).unwrap();
            assert_eq!(bits(read(&actor, true)), bits(read(&actor, false)));
            assert_eq!(read(&actor, false).unwrap().len(), 1);
        }
    }
    for frame in [
        Value::Number(1.5),
        Value::Number(-1.0),
        Value::Number(f64::INFINITY),
        Value::String(lua.create_string("2").unwrap()),
        Value::String(lua.create_string([0xff]).unwrap()),
        Value::Boolean(false),
    ] {
        let actor = actor(&lua, 2);
        actor.raw_set("Frame0000", frame).unwrap();
        assert_eq!(bits(read(&actor, true)), bits(read(&actor, false)));
    }
    for delay in [
        Value::Nil,
        Value::Number(-0.0),
        Value::Number(-1.0),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::String(lua.create_string("0.75").unwrap()),
        Value::String(lua.create_string([0xff]).unwrap()),
        Value::Boolean(false),
    ] {
        let actor = actor(&lua, 2);
        actor.raw_set("Delay0000", delay).unwrap();
        assert_eq!(bits(read(&actor, true)), bits(read(&actor, false)));
    }
}

#[test]
fn lua_read_sprite_states_preserve_metamethod_lookup_and_error_order() {
    for mode in ["normal", "hole", "bad_frame", "bad_delay", "lookup_error"] {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        let trace = Rc::new(RefCell::new(Vec::<String>::new()));
        let log = Rc::clone(&trace);
        let mt = lua.create_table().unwrap();
        mt.set(
            "__index",
            lua.create_function(move |_, (_, key): (Table, String)| {
                log.borrow_mut().push(key.clone());
                if key == "Frame0001" {
                    if mode == "hole" {
                        return Ok(Value::Nil);
                    }
                    if mode == "bad_frame" {
                        return Ok(Value::Boolean(false));
                    }
                    if mode == "lookup_error" {
                        return Err(mlua::Error::runtime("sprite lookup failed"));
                    }
                }
                if key == "Delay0001" && mode == "bad_delay" {
                    return Ok(Value::Boolean(true));
                }
                if let Some(index) = key
                    .strip_prefix("Frame")
                    .and_then(|s| s.parse::<i32>().ok())
                {
                    return Ok(if index < 3 {
                        Value::Integer(index.into())
                    } else {
                        Value::Nil
                    });
                }
                Ok(Value::Nil)
            })
            .unwrap(),
        )
        .unwrap();
        actor.set_metatable(Some(mt)).unwrap();
        let old = bits(read(&actor, true));
        let old_trace = std::mem::take(&mut *trace.borrow_mut());
        let new = bits(read(&actor, false));
        let new_trace = std::mem::take(&mut *trace.borrow_mut());
        assert_eq!(old, new);
        assert_eq!(old_trace, new_trace);
        assert_eq!(new_trace[0], "Frame0000");
        assert_eq!(new_trace[1], "Delay0000");
        if mode == "normal" {
            assert_eq!(new.unwrap().len(), 3);
            assert_eq!(new_trace.last().unwrap(), "Frame0003");
        } else if mode == "hole" {
            assert_eq!(new.unwrap().len(), 1);
            assert_eq!(new_trace.last().unwrap(), "Frame0001");
        } else {
            assert!(new.is_err());
        }
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_read_bench_sprites() {
    for count in [0, 1, 8, 64, 256, 1024] {
        let lua = Lua::new();
        let actor = actor(&lua, count);
        assert_eq!(bits(read(&actor, true)), bits(read(&actor, false)));
        lua.gc_stop();
        for old in order() {
            crate::perf::measure_sampled(
                &format!("sprite_{count}/{}", if old { "old" } else { "new" }),
                64,
                count.max(1),
                || {
                    black_box(read(black_box(&actor), old).unwrap());
                },
            );
        }
    }
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    let mt=lua.load("return {__index=function(t,k) local n=tonumber(k:sub(6)); if n and n<16 then if k:sub(1,5)=='Frame' then return n%4 else return 0.1 end end end}").eval::<Table>().unwrap();
    actor.set_metatable(Some(mt)).unwrap();
    assert_eq!(bits(read(&actor, true)), bits(read(&actor, false)));
    lua.gc_stop();
    for old in order() {
        crate::perf::measure_sampled(
            &format!("sprite_virtual_16/{}", if old { "old" } else { "new" }),
            64,
            16,
            || {
                black_box(read(black_box(&actor), old).unwrap());
            },
        );
    }
}

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    }
}
