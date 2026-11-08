use super::*;

#[path = "startup_capture_before.rs"]
mod before;

#[test]
fn flat_snapshots_keep_exact_key_value_order_identity_and_binary_strings() {
    let lua = Lua::new();
    let table = lua
        .load(
            r#"
        local key = {}
        local callback = function() return key end
        return {[false] = 'false', [17] = key, [key] = callback,
            [callback] = key, [string.char(255)] = true, 'one', 'two'}
    "#,
        )
        .eval::<Table>()
        .unwrap();
    for source in [lua.create_table().unwrap(), table] {
        let old = before::snapshot_function_action_table(&lua, source.clone()).unwrap();
        let new = snapshot_function_action_table(&lua, source.clone()).unwrap();
        assert_eq!(old.raw_get::<Table>(1).unwrap(), source);
        assert_eq!(new.raw_get::<Table>(1).unwrap(), source);
        let old: Vec<_> = old
            .raw_get::<Table>(2)
            .unwrap()
            .sequence_values::<Table>()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.raw_get::<Value>(1).unwrap(),
                    entry.raw_get::<Value>(2).unwrap(),
                )
            })
            .collect();
        let entries = new.raw_get::<Table>(2).unwrap();
        let new: Vec<_> = (2..=entries.raw_len())
            .step_by(2)
            .map(|index| {
                (
                    entries.raw_get::<Value>(index - 1).unwrap(),
                    entries.raw_get::<Value>(index).unwrap(),
                )
            })
            .collect();
        assert_eq!(new, old);
    }
}

#[test]
fn snapshots_restore_cycles_aliases_replaced_cells_and_custom_environments() {
    let lua = Lua::new();
    let functions = lua
        .load(
            r#"
        local graph = {}
        for i = 1, 256 do graph[i] = {value = i} end
        graph.self = graph
        graph.alias = graph[17]
        local original, shared = graph, graph[17]
        local number, missing = 23, nil
        local private = {number = 19}
        local custom = assert(loadstring('special = {number = 99}; extra = 101'))
        setfenv(custom, {special = private})
        local function mutate()
            for i = 1, 256 do graph[i].value = -i end
            shared.extra = true
            graph.self, graph.alias = nil, {}
            graph = {replacement = true}
            number, missing = 41, 'changed'
            SNAPSHOT_EXTRA = true
            custom()
        end
        local function verify()
            assert(graph == original and graph.self == graph and graph.alias == shared)
            assert(graph[17] == shared and shared.extra == nil)
            assert(number == 23 and missing == nil and SNAPSHOT_EXTRA == nil)
            assert(getfenv(custom).special == private and getfenv(custom).extra == nil)
            for i = 1, 256 do assert(graph[i].value == i) end
            return 'restored'
        end
        return {mutate, verify, custom}
    "#,
        )
        .eval::<Table>()
        .unwrap();
    let mutate = functions.raw_get::<Function>(1).unwrap();
    let verify = functions.raw_get::<Function>(2).unwrap();
    let custom = functions.raw_get::<Function>(3).unwrap();
    for current in [false, true, true, false] {
        let roots = lua
            .create_sequence_from([mutate.clone(), verify.clone(), custom.clone()])
            .unwrap();
        let snapshot = if current {
            snapshot_function_action_tables(&lua, &roots)
        } else {
            before::snapshot_function_action_tables(&lua, &roots)
        }
        .unwrap();
        mutate.call::<()>(()).unwrap();
        if current {
            restore_function_action_tables(&lua, snapshot)
        } else {
            before::restore_function_action_tables(&lua, snapshot)
        }
        .unwrap();
        assert_eq!(verify.call::<String>(()).unwrap(), "restored");
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_snapshot_storage() {
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
            let table = lua.create_table().unwrap();
            for index in 1..=256 {
                table.raw_set(format!("field{index}"), index).unwrap();
            }
            lua.gc_stop();
            let start = Instant::now();
            let begin = cycles();
            let (_, churn) = allocations::measure(|| {
                for _ in 0..64 {
                    let snapshot = if current {
                        snapshot_function_action_table(&lua, table.clone())
                    } else {
                        before::snapshot_function_action_table(&lua, table.clone())
                    }
                    .unwrap();
                    black_box(snapshot);
                }
            });
            let used = cycles() - begin;
            let nanos = start.elapsed().as_nanos();
            if sample > 0 {
                samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":64,
                    "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
            }
        }
        println!(
            "DRAW_CAPTURE_METRIC {}",
            serde_json::json!({"workload":"snapshot_fields256",
            "variant":if current {"after"} else {"before"},"unit":"tables","baseline_commit":"31a7355f5","samples":samples})
        );
    }
}
