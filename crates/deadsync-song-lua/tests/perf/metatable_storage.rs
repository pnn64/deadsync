use super::*;

#[path = "metatable_storage_before.rs"]
mod before;

fn no_methods(_: &Lua, _: &Table) -> mlua::Result<()> {
    Ok(())
}

fn install(lua: &Lua, actor: &Table, current: bool) {
    if current {
        install_actor_metatable(lua, actor, no_methods)
    } else {
        before::install_actor_metatable(lua, actor, no_methods)
    }
    .unwrap();
}

#[test]
fn reserved_actor_metatables_keep_lookup_assignment_concat_and_identity() {
    let lua = Lua::new();
    lua.load("Actor={}; ActorFrame=Actor").exec().unwrap();
    let native = lua.create_function(|_, ()| Ok(17)).unwrap();
    let mut metatables = Vec::new();
    for current in [false, true] {
        let actor = lua
            .load("{__songlua_actor_type='ActorFrame',Name='left'}")
            .eval::<Table>()
            .unwrap();
        install(&lua, &actor, current);
        let mt = actor.metatable().unwrap();
        let mut keys: Vec<_> = mt
            .pairs::<String, Value>()
            .map(|entry| entry.unwrap().0)
            .collect();
        keys.sort();
        assert_eq!(keys, ["__concat", "__index", "__newindex", "__tostring"]);
        for key in &keys {
            assert!(
                mt.get::<Function>(key.as_str())
                    .unwrap()
                    .environment()
                    .is_none()
            );
        }
        actor.set("custom", &native).unwrap();
        assert!(actor.raw_get::<Value>("custom").unwrap().is_nil());
        assert_eq!(
            actor
                .raw_get::<Table>(crate::actor_classes::METHODS_KEY)
                .unwrap()
                .raw_get::<Function>("custom")
                .unwrap(),
            native
        );
        actor.set("field", 23).unwrap();
        assert_eq!(actor.get::<i64>("field").unwrap(), 23);
        assert!(actor.get::<Value>("absent").unwrap().is_nil());
        lua.globals().set("actor", &actor).unwrap();
        let text = lua.load("return tostring(actor)").eval::<String>().unwrap();
        assert!(text.starts_with("ActorFrame (") && text.ends_with(')'));
        let joined = lua
            .load("return actor..{Name='right',extra=42}")
            .eval::<Table>()
            .unwrap();
        assert_ne!(joined, actor);
        assert_eq!(joined.metatable().unwrap(), mt);
        assert_eq!(joined.get::<String>("Name").unwrap(), "right");
        assert_eq!(joined.get::<i64>("extra").unwrap(), 42);
        assert_eq!(actor.get::<String>("Name").unwrap(), "left");
        metatables.push(mt);
    }
    assert_ne!(metatables[0], metatables[1]);
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_metatable_storage() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for current in variants {
        let mut samples = Vec::new();
        for sample in 0..10 {
            let lua = Lua::new();
            let actors: Vec<_> = (0..128).map(|_| lua.create_table().unwrap()).collect();
            lua.gc_stop();
            let start = Instant::now();
            let begin = cycles();
            let (_, churn) = allocations::measure(|| {
                for actor in &actors {
                    install(&lua, actor, current);
                    black_box(actor);
                }
            });
            let used = cycles() - begin;
            let nanos = start.elapsed().as_nanos();
            if sample > 0 {
                samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":128,
                "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
            }
        }
        println!(
            "DRAW_CAPTURE_METRIC {}",
            serde_json::json!({"workload":"actor_metatable","variant":if current{"after"}else{"before"},
            "unit":"actors","baseline_commit":"626d9f1f7","samples":samples})
        );
    }
}
