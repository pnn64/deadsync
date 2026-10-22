use super::sound_path_perf::{directory, prepare, snapshot};
use super::*;

#[path = "comparison_sound_before.rs"]
mod before;

fn record(current: bool, lua: &Lua, dir: &Path, args: &MultiValue) -> mlua::Result<()> {
    if current {
        super::record_song_lua_sound_path(lua, dir, args, true)
    } else {
        before::record_song_lua_sound_path(lua, dir, args, true)
    }
}

#[test]
fn inline_sound_scans_preserve_long_values_errors_coercion_and_stop_order() {
    let dir = directory(false);
    for mode in 0..10 {
        for prefix in [false, true] {
            let mut outcomes = Vec::new();
            for current in [false, true] {
                let lua = Lua::new();
                prepare(&lua, &dir, 0, true);
                let paths: Table = lua.globals().raw_get(SONG_LUA_SOUND_PATHS_KEY).unwrap();
                let key = dir.join("sound0.wav").to_string_lossy().into_owned();
                let value = match mode {
                    0 => Value::String(lua.create_string(&key).unwrap()),
                    1 => Value::String(lua.create_string("not-matched").unwrap()),
                    2 => Value::String(lua.create_string(b"bad\xff").unwrap()),
                    3 => Value::Integer(17),
                    4 => Value::Boolean(false),
                    5 => Value::Table(lua.create_table().unwrap()),
                    6..=9 => Value::String(
                        lua.create_string("x".repeat([255, 256, 257, 4096][mode - 6]))
                            .unwrap(),
                    ),
                    _ => unreachable!(),
                };
                if prefix {
                    paths.raw_set(1, key.as_str()).unwrap();
                }
                paths.raw_set(if prefix { 2 } else { 1 }, value).unwrap();
                let args = MultiValue::from_vec(vec![Value::String(
                    lua.create_string("sound0.wav").unwrap(),
                )]);
                let result = record(current, &lua, &dir, &args).map_err(|e| e.to_string());
                outcomes.push((
                    result,
                    snapshot(lua.globals().raw_get(SONG_LUA_SOUND_PATHS_KEY).unwrap()),
                    snapshot(lua.globals().raw_get(SONG_LUA_SOUND_CALLS_KEY).unwrap()),
                    format!("{:?}", crate::lua_util::take_runtime_sounds(&lua)),
                ));
            }
            assert_eq!(outcomes[0], outcomes[1]);
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_comparison_sound() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let dir = directory(false);
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, n, long) in [
        ("scan_empty", 0, false),
        ("scan_one", 1, false),
        ("scan_128", 128, false),
        ("scan_long_128", 128, true),
    ] {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                prepare(&lua, &dir, n, false);
                if long {
                    let paths: Table = lua.globals().raw_get(SONG_LUA_SOUND_PATHS_KEY).unwrap();
                    for i in 1..=n {
                        paths.raw_set(i, "x".repeat(4096)).unwrap();
                    }
                }
                let args = MultiValue::from_vec(vec![Value::String(
                    lua.create_string(if n == 128 && !long {
                        "sound127.wav"
                    } else {
                        "sound0.wav"
                    })
                    .unwrap(),
                )]);
                lua.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..64 {
                        black_box(record(current, &lua, &dir, &args).unwrap());
                    }
                });
                let used = cycles() - begin;
                let ns = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":ns,"thread_cycles":used,"units":64,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},"unit":"sound records","baseline_commit":"142996521","samples":samples})
            );
        }
    }
}
