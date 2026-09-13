use super::*;
use std::hint::black_box;

#[path = "table_calls_baseline.rs"]
mod baseline;

#[test]
fn lua_stream_pass_table_calls_preserve_self_arity_mutation_and_first_return() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let function: Function = lua
        .load(
            r#"
        return function(...)
            assert(select('#', ...) == 1)
            local self = ...
            self.calls = self.calls + 1
            return self, 'discarded', 12
        end
    "#,
        )
        .eval()
        .unwrap();
    for old in [true, false] {
        table.raw_set("calls", 0).unwrap();
        let result = if old {
            baseline::call_table_function(&table, &function)
        } else {
            call_table_function(&table, &function)
        }
        .unwrap();
        assert_eq!(table.raw_get::<i64>("calls").unwrap(), 1);
        let Value::Table(result) = result else {
            panic!("expected receiver")
        };
        assert_eq!(result.to_pointer(), table.to_pointer());
    }
    for source in [
        "return function(...) assert(select('#', ...) == 1) end",
        "return function(self) return nil, 99 end",
    ] {
        let function: Function = lua.load(source).eval().unwrap();
        assert!(matches!(
            baseline::call_table_function(&table, &function).unwrap(),
            Value::Nil
        ));
        assert!(matches!(
            call_table_function(&table, &function).unwrap(),
            Value::Nil
        ));
    }
}

#[test]
fn lua_stream_pass_table_calls_preserve_string_coercions_and_callback_errors() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    for value in [
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(-17),
        Value::Number(0.125),
        Value::String(lua.create_string("long\0é".repeat(128)).unwrap()),
        Value::String(lua.create_string([255, 0]).unwrap()),
        Value::Table(lua.create_table().unwrap()),
    ] {
        let function = lua
            .create_function(move |_, receiver: Table| {
                receiver.set("called", true)?;
                Ok(value.clone())
            })
            .unwrap();
        table.raw_set("GetName", function).unwrap();
        let old = baseline::call_string_method(&table, "GetName").map_err(|err| err.to_string());
        let new = call_string_method(&table, "GetName").map_err(|err| err.to_string());
        assert_eq!(new, old);
        assert!(table.raw_get::<bool>("called").unwrap());
    }
    let function: Function = lua
        .load("return function(self) self.called = 91; error('method failure') end")
        .eval()
        .unwrap();
    table.raw_set("GetName", function.clone()).unwrap();
    let old = baseline::call_table_function(&table, &function)
        .unwrap_err()
        .to_string();
    let new = call_table_function(&table, &function)
        .unwrap_err()
        .to_string();
    assert_eq!(new, old);
    assert_eq!(table.raw_get::<i64>("called").unwrap(), 91);
    assert_eq!(
        call_string_method(&table, "GetName")
            .unwrap_err()
            .to_string(),
        baseline::call_string_method(&table, "GetName")
            .unwrap_err()
            .to_string()
    );
    table.raw_set("GetName", 3).unwrap();
    assert_eq!(
        call_string_method(&table, "GetName")
            .unwrap_err()
            .to_string(),
        baseline::call_string_method(&table, "GetName")
            .unwrap_err()
            .to_string()
    );
    table.raw_set("GetName", Value::Nil).unwrap();
    assert_eq!(call_string_method(&table, "GetName").unwrap(), None);
}

fn author_fixture(lua: &Lua) -> Table {
    lua.load(r#"
        local receiver = {trace = ''}
        return setmetatable(receiver, {__index = function(self, key)
            self.trace = self.trace .. key .. ';'
            if key == 'GetDescription' then
                return function(...) assert(select('#', ...) == 1); assert((...) == receiver)
                    receiver.GetAuthorCredit = function(self) self.trace = self.trace .. 'credit;'; return 'credit' end
                    return 'description'
                end
            elseif key == 'GetChartName' then return function(self) return 'description' end end
        end})
    "#).eval().unwrap()
}

#[test]
fn lua_stream_pass_table_calls_preserve_author_lookup_order_and_deduplication() {
    let lua = Lua::new();
    let mut outputs = Vec::new();
    for old in [true, false] {
        let receiver = author_fixture(&lua);
        let steps = Value::Table(receiver.clone());
        let out = if old {
            baseline::create_author_table(&lua, Some(&steps))
        } else {
            create_author_table(&lua, Some(&steps))
        }
        .unwrap();
        outputs.push((
            out.sequence_values::<String>()
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap(),
            receiver.raw_get::<String>("trace").unwrap(),
        ));
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0].0, ["description", "credit"]);
    assert_eq!(outputs[0].1, "GetDescription;credit;GetChartName;");
}

#[test]
fn lua_stream_pass_table_calls_preserve_bpm_fallback_order_and_mutations() {
    let lua = Lua::new();
    for valid in [false, true] {
        let mut outputs = Vec::new();
        for old in [true, false] {
            let receiver: Table = lua
                .load(
                    r#"
                local receiver = {trace = '', valid = false}
                receiver.GetDisplayBpms = function(...)
                    assert(select('#', ...) == 1); assert((...) == receiver)
                    receiver.trace = receiver.trace .. 'display;'
                    receiver.GetTimingData = function(self)
                        receiver.trace = receiver.trace .. 'timing;'
                        local timing = {}
                        timing.GetActualBPM = function(...)
                            assert(select('#', ...) == 1); assert((...) == timing)
                            receiver.trace = receiver.trace .. 'actual;'
                            return {125, 250}, 999
                        end
                        return timing
                    end
                    if receiver.valid then return {90, 180} end
                    return {-1, 0}
                end
                return receiver
            "#,
                )
                .eval()
                .unwrap();
            receiver.set("valid", valid).unwrap();
            let out = if old {
                baseline::display_bpms_from_table(&receiver)
            } else {
                display_bpms_from_table(&receiver)
            }
            .unwrap();
            outputs.push((out, receiver.raw_get::<String>("trace").unwrap()));
        }
        assert_eq!(outputs[0], outputs[1]);
        assert_eq!(
            outputs[0].0,
            if valid { [90.0, 180.0] } else { [125.0, 250.0] }
        );
        assert_eq!(
            outputs[0].1,
            if valid {
                "display;"
            } else {
                "display;timing;actual;"
            }
        );
    }
}

#[test]
fn lua_stream_pass_table_calls_scalar_callback_has_zero_heap_churn() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let function: Function = lua
        .load("return function(self) return 17 end")
        .eval()
        .unwrap();
    for _ in 0..8 {
        call_table_function(&table, &function).unwrap();
    }
    lua.gc_stop();
    crate::perf::assert_no_churn(|| {
        black_box(call_table_function(black_box(&table), black_box(&function)).unwrap());
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_stream_pass_bench_table_calls() {
    for kind in [
        "scalar",
        "string_short",
        "string_long",
        "string_missing",
        "bpm_direct",
        "bpm_fallback",
        "bpm_missing",
        "authors",
    ] {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        let text = lua
            .create_string(if kind == "string_long" {
                "long\0é".repeat(128)
            } else {
                "author".to_owned()
            })
            .unwrap();
        table.raw_set("text", text).unwrap();
        let function: Function = lua
            .load("return function(self) return 17 end")
            .eval()
            .unwrap();
        let string: Function = lua
            .load("return function(self) return self.text end")
            .eval()
            .unwrap();
        if kind != "string_missing" {
            table.raw_set("GetName", string.clone()).unwrap();
        }
        for key in ["GetDescription", "GetAuthorCredit", "GetChartName"] {
            table.raw_set(key, string.clone()).unwrap();
        }
        let bpms = lua.create_sequence_from([125.0, 250.0]).unwrap();
        table.raw_set("bpms", bpms).unwrap();
        let bpm: Function = lua
            .load("return function(self) return self.bpms end")
            .eval()
            .unwrap();
        if kind == "bpm_direct" {
            table.raw_set("GetDisplayBpms", bpm.clone()).unwrap();
        }
        if kind == "bpm_fallback" {
            let invalid: Function = lua
                .load("return function(self) return false end")
                .eval()
                .unwrap();
            table.raw_set("GetDisplayBpms", invalid).unwrap();
            let timing: Function = lua
                .load("return function(self) return self end")
                .eval()
                .unwrap();
            table.raw_set("GetTimingData", timing).unwrap();
            table.raw_set("GetActualBPM", bpm).unwrap();
        }
        let steps = Value::Table(table.clone());
        let work = |old: bool| {
            if kind == "scalar" {
                black_box(
                    if old {
                        baseline::call_table_function(black_box(&table), black_box(&function))
                    } else {
                        call_table_function(black_box(&table), black_box(&function))
                    }
                    .unwrap(),
                );
            } else if kind.starts_with("string_") {
                black_box(
                    if old {
                        baseline::call_string_method(black_box(&table), "GetName")
                    } else {
                        call_string_method(black_box(&table), "GetName")
                    }
                    .unwrap(),
                );
            } else if kind.starts_with("bpm_") {
                black_box(
                    if old {
                        baseline::display_bpms_from_table(black_box(&table))
                    } else {
                        display_bpms_from_table(black_box(&table))
                    }
                    .unwrap(),
                );
            } else {
                black_box(
                    if old {
                        baseline::create_author_table(black_box(&lua), Some(black_box(&steps)))
                    } else {
                        create_author_table(black_box(&lua), Some(black_box(&steps)))
                    }
                    .unwrap(),
                );
            }
        };
        work(true);
        work(false);
        lua.gc_collect().unwrap();
        lua.gc_stop();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!("table_calls_{kind}/{}", if old { "old" } else { "new" }),
                512,
                1,
                || work(black_box(old)),
            );
        }
    }
}
