use super::*;
use std::path::PathBuf;

#[path = "sound_path_before.rs"]
mod before;

fn directory(lossy: bool) -> PathBuf {
    let mut path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/foreign-asset-sounds");
    #[cfg(windows)]
    if lossy {
        use std::os::windows::ffi::OsStringExt;
        path.push(std::ffi::OsString::from_wide(&[
            b'a' as u16,
            0xd800,
            b'b' as u16,
        ]));
    }
    #[cfg(not(windows))]
    if lossy {
        path.push("unicode");
    }
    std::fs::create_dir_all(&path).unwrap();
    for i in 0..128 {
        let file = path.join(format!("sound{i}.wav"));
        if !file.exists() {
            std::fs::write(file, b"RIFF").unwrap();
        }
    }
    let file = path.join("image.png");
    if !file.exists() {
        std::fs::write(file, b"image").unwrap();
    }
    path
}

fn run(current: bool, lua: &Lua, dir: &Path, args: &MultiValue, capture: bool) -> mlua::Result<()> {
    if current {
        super::record_song_lua_sound_path(lua, dir, args, capture)
    } else {
        before::record_song_lua_sound_path(lua, dir, args, capture)
    }
}

fn snapshot(value: Value) -> String {
    match value {
        Value::String(value) => format!("{:?}", value.as_bytes().as_ref()),
        Value::Table(table) => {
            let mut entries = Vec::new();
            table
                .for_each::<i64, Value>(|key, value| {
                    entries.push((key, snapshot(value)));
                    Ok(())
                })
                .unwrap();
            entries.sort();
            format!("{entries:?}")
        }
        other => format!("{other:?}"),
    }
}

fn prepare(lua: &Lua, dir: &Path, entries: usize, calls: bool) {
    install_sound_globals(lua, dir).unwrap();
    let runtime = lua.create_table().unwrap();
    runtime
        .raw_set(crate::SONG_LUA_RUNTIME_BEAT_KEY, 3.25)
        .unwrap();
    runtime
        .raw_set(crate::SONG_LUA_RUNTIME_SECONDS_KEY, 1.5)
        .unwrap();
    lua.globals()
        .raw_set(crate::SONG_LUA_RUNTIME_KEY, runtime)
        .unwrap();
    crate::lua_util::begin_overlay_update_capture_from_indices(lua, std::iter::empty());
    let paths: Table = lua.globals().raw_get(SONG_LUA_SOUND_PATHS_KEY).unwrap();
    for i in 0..entries {
        paths
            .raw_set(
                i + 1,
                dir.join(format!("sound{i}.wav")).to_string_lossy().as_ref(),
            )
            .unwrap();
    }
    if calls {
        lua.globals()
            .raw_set(SONG_LUA_SOUND_CALLS_KEY, lua.create_table().unwrap())
            .unwrap();
    }
}

#[test]
fn sound_recording_keeps_dedup_calls_partial_errors_and_lossy_paths() {
    for lossy in [false, true] {
        let dir = directory(lossy);
        for capture in [false, true] {
            for calls in [false, true] {
                for mode in 0..6 {
                    for input in ["sound0.wav", "sound1.wav", "missing.wav", "image.png"] {
                        let mut outcomes = Vec::new();
                        for current in [false, true] {
                            let lua = Lua::new();
                            prepare(&lua, &dir, 1, calls);
                            let globals = lua.globals();
                            match mode {
                                1 => globals
                                    .raw_set(SONG_LUA_SOUND_PATHS_KEY, Value::Nil)
                                    .unwrap(),
                                2 => globals.raw_set(SONG_LUA_SOUND_PATHS_KEY, false).unwrap(),
                                3 => globals
                                    .raw_get::<Table>(SONG_LUA_SOUND_PATHS_KEY)
                                    .unwrap()
                                    .raw_set(1, lua.create_string(b"bad\xff").unwrap())
                                    .unwrap(),
                                4 => globals
                                    .raw_get::<Table>(SONG_LUA_SOUND_PATHS_KEY)
                                    .unwrap()
                                    .raw_set(1, 17)
                                    .unwrap(),
                                5 => globals.raw_set(SONG_LUA_SOUND_CALLS_KEY, false).unwrap(),
                                _ => (),
                            }
                            let args = MultiValue::from_vec(vec![
                                Value::Table(lua.create_table().unwrap()),
                                Value::String(lua.create_string(input).unwrap()),
                                Value::Nil,
                            ]);
                            let result =
                                run(current, &lua, &dir, &args, capture).map_err(|e| e.to_string());
                            let events = crate::lua_util::take_runtime_sounds(&lua);
                            assert!(events.iter().all(|event| event.beat == 3.25));
                            outcomes.push((
                                result,
                                snapshot(globals.raw_get(SONG_LUA_SOUND_PATHS_KEY).unwrap()),
                                snapshot(globals.raw_get(SONG_LUA_SOUND_CALLS_KEY).unwrap()),
                                format!("{events:?}"),
                            ));
                        }
                        assert_eq!(outcomes[0], outcomes[1]);
                    }
                }
            }
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_sound_path() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    let plain = directory(false);
    let lossy = directory(true);
    for (name, entries, capture, calls, unique, input, non_utf8) in [
        ("sound_path_new64", 0, true, true, true, "sound0.wav", false),
        (
            "sound_path_duplicate",
            1,
            true,
            true,
            false,
            "sound0.wav",
            false,
        ),
        (
            "sound_path_no_calls",
            1,
            true,
            false,
            false,
            "sound0.wav",
            false,
        ),
        (
            "sound_path_no_capture",
            1,
            false,
            true,
            false,
            "sound0.wav",
            false,
        ),
        (
            "sound_path_search128",
            128,
            true,
            true,
            false,
            "sound127.wav",
            false,
        ),
        (
            "sound_path_missing",
            0,
            true,
            true,
            false,
            "missing.wav",
            false,
        ),
        (
            "sound_path_not_audio",
            0,
            true,
            true,
            false,
            "image.png",
            false,
        ),
        ("sound_path_lossy", 1, true, true, false, "sound0.wav", true),
    ] {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let dir = if non_utf8 { &lossy } else { &plain };
                prepare(&lua, dir, entries, calls);
                let args: Vec<_> = (0..64)
                    .map(|i| {
                        MultiValue::from_vec(vec![Value::String(
                            lua.create_string(if unique {
                                format!("sound{i}.wav")
                            } else {
                                input.to_owned()
                            })
                            .unwrap(),
                        )])
                    })
                    .collect();
                lua.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for args in &args {
                        black_box(run(current, &lua, dir, args, capture).unwrap());
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
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},
                "unit":"sound calls","baseline_commit":"494323364","samples":samples})
            );
        }
    }
}
