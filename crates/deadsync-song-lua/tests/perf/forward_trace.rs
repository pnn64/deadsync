use super::*;
use crate::actor_classes::forward_values_perf::before;
use crate::actor_classes::{METHODS_KEY, forward};
use std::sync::Mutex;

struct TraceLogger(Mutex<Vec<serde_json::Value>>);
static LOGGER: TraceLogger = TraceLogger(Mutex::new(Vec::new()));
impl log::Log for TraceLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == "deadsync_song_lua::trace"
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            if let Ok(event) = serde_json::from_str(&record.args().to_string()) {
                self.0.lock().unwrap().push(event);
            }
        }
    }
    fn flush(&self) {}
}

#[cfg(not(feature = "test-support"))]
#[test]
#[ignore = "requires a fresh process for the trace logger; run exact --ignored"]
fn forwarding_trace_keeps_events_arities_errors_and_nested_query_suppression() {
    log::set_logger(&LOGGER).expect("run this check alone in a fresh process");
    log::set_max_level(log::LevelFilter::Trace);
    let lua = Lua::new();
    lua.set_app_data(LiveMode(77));
    lua.set_app_data(QueryTrace {
        scene: 77,
        ..QueryTrace::default()
    });
    lua.globals()
        .raw_set(
            crate::SONG_LUA_RUNTIME_KEY,
            lua.load("{__songlua_song_beat=2,__songlua_music_seconds=1}")
                .eval::<Table>()
                .unwrap(),
        )
        .unwrap();
    let receiver = lua
        .load("{__songlua_actor_type='Actor',Name='trace actor'}")
        .eval::<Table>()
        .unwrap();
    let methods = lua.create_table().unwrap();
    receiver.raw_set(METHODS_KEY, &methods).unwrap();
    let old_class = lua.create_table().unwrap();
    let new_class = lua.create_table().unwrap();
    before::forward(&lua, &old_class, "GetName").unwrap();
    forward(&lua, &new_class, "GetName").unwrap();
    let old = old_class.raw_get::<Function>("GetName").unwrap();
    let new = new_class.raw_get::<Function>("GetName").unwrap();
    for (inputs, input_count) in [("actor,17,nil", 3), ("actor,17,nil,'x',true,0,-0.0,nil", 8)] {
        let runner = lua
            .load(format!(
                "return function(call,actor) return call({inputs}) end"
            ))
            .set_name("@forward-trace-oracle.lua")
            .eval::<Function>()
            .unwrap();
        for case in 0..4 {
            let semantic = lua
                .create_function(move |lua, args: MultiValue| {
                    assert_eq!(args.len(), input_count);
                    assert_eq!(lua.app_data_ref::<QueryTrace>().unwrap().depth, 1);
                    trace_stage_query(lua, "nested-must-be-suppressed", 9);
                    Ok(match case {
                        0 => MultiValue::new(),
                        1 => MultiValue::from_vec(vec![Value::Nil]),
                        2 => MultiValue::from_vec(vec![
                            Value::Number(17.0),
                            Value::Nil,
                            Value::String(lua.create_string("ok")?),
                        ]),
                        _ => return Err(mlua::Error::runtime("semantic trace failure")),
                    })
                })
                .unwrap();
            methods.raw_set("GetName", semantic).unwrap();
            let mut previous = None;
            for function in [&old, &new] {
                LOGGER.0.lock().unwrap().clear();
                let result = runner
                    .call::<MultiValue>((function.clone(), receiver.clone()))
                    .map(MultiValue::into_vec)
                    .map_err(|e| e.to_string());
                assert_eq!(lua.app_data_ref::<QueryTrace>().unwrap().depth, 0);
                let events = std::mem::take(&mut *LOGGER.0.lock().unwrap());
                assert_eq!(events.len(), 1);
                assert_eq!(events[0]["operation"], "Actor.GetName");
                if let Some((old_result, old_events)) = &previous {
                    assert_eq!(&result, old_result);
                    assert_eq!(&events, old_events);
                } else {
                    previous = Some((result, events));
                }
            }
        }
    }
}
