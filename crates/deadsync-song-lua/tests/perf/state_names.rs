use super::*;
use std::hint::black_box;

#[path = "state_names_baseline.rs"]
mod baseline;

const FIELDS: [(&str, &str); 5] = [
    ("__songlua_state_text_align", "HorizAlign_Right"),
    ("__songlua_state_effect_clock", "BGM"),
    ("__songlua_state_text_glow_mode", "TextGlowMode_Both"),
    ("__songlua_state_blend", "BlendMode_Add"),
    ("__songlua_state_effect_mode", "diffuseblink"),
];

fn read(actor: &Table, old: bool) -> Result<SongLuaOverlayState, String> {
    if old {
        baseline::actor_overlay_initial_state(actor)
    } else {
        actor_overlay_initial_state(actor)
    }
}

#[test]
fn capture_sampling_state_names_preserve_aliases_coercions_errors_and_spill_boundaries() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    for (field, alias) in FIELDS {
        let mut values = vec![
            Value::Nil,
            Value::Boolean(false),
            Value::Integer(42),
            Value::Number(-0.25),
            Value::Table(actor.clone()),
        ];
        for text in [
            alias.to_owned(),
            alias.to_ascii_uppercase(),
            format!(" '\"{alias}\"' "),
            "".into(),
            "\u{2003}\u{a0}".into(),
            "beat\0\u{e9}".into(),
            "x".repeat(31),
            "x".repeat(32),
            "x".repeat(33),
            "beat".repeat(1024),
        ] {
            values.push(Value::String(lua.create_string(text).unwrap()));
        }
        values.push(Value::String(lua.create_string([255, 0]).unwrap()));
        for value in values {
            actor.raw_set(field, value).unwrap();
            assert_eq!(read(&actor, false), read(&actor, true), "field={field}");
        }
        actor.raw_remove(field).unwrap();
    }
    for (field, alias) in FIELDS {
        actor.raw_set(field, alias).unwrap();
    }
    let state = read(&actor, false).unwrap();
    assert_eq!(state.text_align, deadlib_present::actors::TextAlign::Right);
    assert_eq!(state.effect_clock, EffectClock::Beat);
    assert_eq!(state.blend, SongLuaOverlayBlendMode::Add);
    assert_eq!(
        state.effect_mode,
        deadlib_present::anim::EffectMode::DiffuseBlink
    );
}

#[test]
fn capture_sampling_state_names_preserve_dynamic_lookup_order_gc_and_partial_errors() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    let trace = Rc::new(std::cell::RefCell::new(Vec::new()));
    let fail = Rc::new(std::cell::Cell::new(false));
    let meta = lua.create_table().unwrap();
    meta.raw_set(
        "__index",
        lua.create_function({
            let trace = Rc::clone(&trace);
            let fail = Rc::clone(&fail);
            move |lua, (actor, key): (Table, String)| {
                trace.borrow_mut().push(key.clone());
                if key == "__songlua_state_x" {
                    actor.raw_set("__songlua_state_y", 71.0)?;
                    return Ok(Value::Number(4.0));
                }
                if fail.get() && key == "__songlua_state_text_glow_mode" {
                    return Err(mlua::Error::RuntimeError("state sentinel".into()));
                }
                lua.gc_collect()?;
                Ok(match FIELDS.iter().find(|(field, _)| *field == key) {
                    Some((_, alias)) => Value::String(lua.create_string(alias)?),
                    None => Value::Nil,
                })
            }
        })
        .unwrap(),
    )
    .unwrap();
    actor.set_metatable(Some(meta)).unwrap();
    for failure in [false, true] {
        fail.set(failure);
        let mut results = Vec::new();
        for old in [true, false] {
            actor.raw_remove("__songlua_state_y").unwrap();
            trace.borrow_mut().clear();
            let result = read(&actor, old);
            assert_eq!(actor.raw_get::<f64>("__songlua_state_y").unwrap(), 71.0);
            if !failure {
                let state = result.as_ref().unwrap();
                assert_eq!((state.x, state.y), (4.0, 71.0));
            }
            results.push((result, trace.borrow().clone()));
        }
        assert_eq!(results[0], results[1]);
        if failure {
            assert!(
                results[1]
                    .0
                    .as_ref()
                    .unwrap_err()
                    .contains("state sentinel")
            );
        }
    }
}

struct Fixture {
    _lua: Lua,
    actor: Table,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        if mode != "empty" {
            for (field, alias) in FIELDS {
                actor
                    .raw_set(
                        field,
                        if mode == "long" {
                            "beat".repeat(128)
                        } else {
                            alias.into()
                        },
                    )
                    .unwrap();
            }
        }
        actor.raw_set("__songlua_state_x", 19.0).unwrap();
        Self { _lua: lua, actor }
    }
    fn batch(&self, old: bool) {
        for _ in 0..64 {
            black_box(read(&self.actor, old).unwrap());
        }
    }
}

#[test]
fn capture_sampling_state_names_eliminate_five_warm_temporary_strings() {
    let fixture = Fixture::new("normal");
    fixture.batch(true);
    fixture.batch(false);
    crate::perf::assert_no_churn(|| fixture.batch(false));
    crate::perf::assert_reduced_churn(|| fixture.batch(true), || fixture.batch(false));
    assert_eq!(read(&fixture.actor, false), read(&fixture.actor, true));
    let empty = Fixture::new("empty");
    empty.batch(false);
    crate::perf::assert_no_churn(|| empty.batch(false));
}

#[test]
#[ignore = "manual paired release benchmark; run serially"]
fn capture_sampling_state_names_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for mode in ["empty", "normal", "long"] {
        for old in order {
            let fixture = Fixture::new(mode);
            fixture.batch(old);
            crate::perf::measure_sampled(
                &format!("state_names_{mode}/{}", if old { "old" } else { "new" }),
                128,
                64,
                || fixture.batch(old),
            );
        }
    }
}
