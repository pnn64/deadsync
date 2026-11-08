//! Paired production input dispatch and exact fallback event construction.
use mlua::{Function, Lua, Table, Value};
use std::hint::black_box;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

#[path = "../src/input.rs"]
mod after;
#[path = "../tests/perf/input_dispatch_baseline.rs"]
mod before;
#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;
#[path = "../tests/perf/input_dispatch_tree.rs"]
mod tree;

#[cfg(windows)]
fn pin() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    // SAFETY: only this disposable benchmark worker's affinity changes.
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
        0
    );
}
#[cfg(not(windows))]
fn pin() {}

macro_rules! event {
    ($module:ident, $index:expr) => {{
        let index = $index;
        let kind = match index % 3 {
            0 => $module::SongLuaInputType::FirstPress,
            1 => $module::SongLuaInputType::Repeat,
            _ => $module::SongLuaInputType::Release,
        };
        $module::SongLuaInputEvent {
            player: if matches!(index % 4, 0 | 3) {
                None
            } else {
                Some(index % 2)
            },
            controller: if matches!(index % 4, 1 | 3) {
                None
            } else {
                Some(index % 2)
            },
            button: if index % 2 == 0 { "Left" } else { "Right" },
            game_button: if index % 2 == 0 { "Left" } else { "Right" },
            kind,
            device: "InputDevice_Key",
            device_button: "DeviceButton_right",
            level: if index >= 8 {
                f32::from_bits(0x7fc0_1234)
            } else {
                [0.0, 0.25, 0.5, 1.0][index % 4]
            },
            z: index as i32 - 4,
            down: kind!($module, kind),
            ago: if index >= 12 {
                -0.0
            } else {
                index as f64 / 32.0
            },
            is_joystick: index % 3 == 1,
            is_mouse: index % 3 == 2,
        }
    }};
}
macro_rules! kind {
    ($module:ident, $kind:expr) => {
        $kind != $module::SongLuaInputType::Release
    };
}

fn compare(
    name: &str,
    batch: usize,
    mut work: impl FnMut(bool) -> u64,
    rows: &mut Vec<serde_json::Value>,
) {
    let mut digests = [0_u64; 2];
    for sample in 0..10 {
        for current in [sample % 2 != 0, sample % 2 == 0] {
            let measure = metrics::Measurement::start();
            let mut digest = 0_u64;
            for _ in 0..batch {
                digest = digest
                    .wrapping_mul(31)
                    .wrapping_add(black_box(work(current)));
            }
            let mut row = measure.finish();
            digests[usize::from(current)] = digests[usize::from(current)].wrapping_add(digest);
            if sample == 0 {
                continue;
            }
            row["workload"] = name.into();
            row["current"] = current.into();
            row["operations"] = batch.into();
            row["sample"] = sample.into();
            rows.push(row);
        }
    }
    assert_eq!(digests[0], digests[1], "{name}: output digest changed");
}

fn setup(current: bool, count: usize, mixed: bool) -> Lua {
    setup_variant(if current { 2 } else { 0 }, count, mixed, false)
}

fn setup_variant(variant: u8, count: usize, mixed: bool, all_lua: bool) -> Lua {
    let lua = Lua::new();
    let screen = lua.create_table().unwrap();
    match variant {
        0 => before::install(&lua, &screen).unwrap(),
        1 => tree::install(&lua, &screen).unwrap(),
        _ => after::install(&lua, &screen).unwrap(),
    }
    let add = screen.get::<Function>("AddInputCallback").unwrap();
    for index in 0..count {
        let source = if all_lua {
            "return function(e) return DISPATCH_FALSE end".to_owned()
        } else if count == 1 {
            "return function(e) return e.GameButton=='Right' and e.DeviceInput.down end".to_owned()
        } else {
            format!("return function(e) return e.GameButton=='Unused{index}' end")
        };
        let f: Function = lua
            .load(source)
            .set_name("@input-dispatch-fixture.lua")
            .eval()
            .unwrap();
        add.call::<()>((screen.clone(), f)).unwrap();
    }
    if mixed {
        let f: Function = lua
            .load("return function(e) e.button='changed';return false end")
            .eval()
            .unwrap();
        add.call::<()>((screen, f)).unwrap();
    }
    lua
}

fn snapshot(table: &Table) -> serde_json::Value {
    let mut rows = Vec::new();
    for row in table.pairs::<mlua::String, Value>() {
        let (key, value) = row.unwrap();
        let value = match value {
            Value::Nil => serde_json::json!(["nil"]),
            Value::Boolean(v) => serde_json::json!(["boolean", v]),
            Value::Integer(v) => serde_json::json!(["number", (v as f64).to_bits()]),
            Value::Number(v) => serde_json::json!(["number", v.to_bits()]),
            Value::String(v) => serde_json::json!(["bytes", v.as_bytes().as_ref()]),
            Value::Table(v) => snapshot(&v),
            other => panic!("unexpected event value: {other:?}"),
        };
        rows.push(serde_json::json!([key.as_bytes().as_ref(), value]));
    }
    rows.into()
}

fn main() {
    pin();
    let mut rows = Vec::new();
    let lua = Lua::new();
    let next: Function = lua
        .load("return function(t,k) local ok,key=pcall(next,t,k);return ok,key end")
        .eval()
        .unwrap();
    let mut snapshots = Vec::new();
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Relaxed);
        }
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let (mut artifact, lua_bytes) = {
        let lua = Lua::new();
        lua.set_app_data(Dropped(dropped.clone()));
        let f: Function = lua
            .load("return function(e) return e.GameButton=='Right' and e.DeviceInput.down end")
            .eval()
            .unwrap();
        let artifact = after::SongLuaCompiledInput::compile(&f).unwrap();
        lua.gc_collect().unwrap();
        (artifact, lua.used_memory())
    };
    assert!(dropped.load(Relaxed));
    for index in 0..24 {
        let e = event!(after, index);
        assert_eq!(artifact.evaluate(&e), e.game_button == "Right" && e.down);
    }
    let native_artifact = serde_json::json!({"lua_allocator_bytes_lower_bound":lua_bytes,"native_owned_payload":artifact.retained_bytes(),"loading_vm_destroyed":dropped.load(Relaxed),"scope":"standalone input predicate; production scene registry separately retains its VM and original functions"});
    for index in 0..16 {
        let old = before::create_event(&lua, &event!(before, index)).unwrap();
        let new = after::create_event(&lua, &event!(after, index)).unwrap();
        let expected = snapshot(&old);
        assert_eq!(snapshot(&new), expected);
        // Nil-key tombstones are observable through next even when pairs skips
        // their values. Compare these as well as all ordinary raw traversal.
        for key in ["controller", "PlayerNumber", "MultiPlayer"] {
            let a = next.call::<(bool, Value)>((old.clone(), key)).unwrap();
            let b = next.call::<(bool, Value)>((new.clone(), key)).unwrap();
            assert_eq!(a, b);
        }
        snapshots.push(expected);
    }
    compare(
        "event_fields",
        512,
        |current| {
            let mut sum = 0;
            for index in 0..8 {
                let event = if current {
                    after::create_event(&lua, &event!(after, index))
                } else {
                    before::create_event(&lua, &event!(before, index))
                }
                .unwrap();
                black_box(&event);
                sum += event.raw_len() as u64;
            }
            sum
        },
        &mut rows,
    );
    for (count, mixed) in [(0, false), (1, false), (8, false), (64, false), (8, true)] {
        let old = setup(false, count, mixed);
        let new = setup(true, count, mixed);
        for index in 0..24 {
            let a = before::dispatch(&old, &event!(before, index)).unwrap();
            let b = after::dispatch(&new, &event!(after, index)).unwrap();
            assert_eq!(a, b);
        }
        compare(
            &format!(
                "dispatch_{count}_{}",
                if mixed { "mixed" } else { "closed" }
            ),
            64,
            |current| {
                let mut digest = 0_u64;
                for index in 0..8 {
                    let (handled, errors) = if current {
                        after::dispatch(&new, &event!(after, index))
                    } else {
                        before::dispatch(&old, &event!(before, index))
                    }
                    .unwrap();
                    assert!(errors.is_empty());
                    digest = digest.rotate_left(1) ^ u64::from(handled);
                }
                digest
            },
            &mut rows,
        );
        // Whole preparation returns VMs with original callback identity roots;
        // native code is additional retained payload in this mixed scene owner.
        for sample in 0..10 {
            for current in [sample % 2 != 0, sample % 2 == 0] {
                let mut prepared = Vec::with_capacity(16);
                let measure = metrics::Measurement::start();
                for _ in 0..16 {
                    prepared.push(setup(current, count, mixed));
                }
                let mut row = measure.finish();
                for artifact in &prepared {
                    black_box(artifact);
                }
                drop(prepared);
                if sample == 0 {
                    continue;
                }
                row["workload"] =
                    format!("prepare_{count}_{}", if mixed { "mixed" } else { "closed" }).into();
                row["current"] = current.into();
                row["operations"] = 16.into();
                row["sample"] = sample.into();
                rows.push(row);
            }
        }
    }
    // Both sides already have native prefixes and batched fields. These rows
    // isolate only the ordered registry/cursor change with all callbacks in Lua.
    for count in [8, 64, 512] {
        let old = setup_variant(1, count, false, true);
        let new = setup_variant(2, count, false, true);
        compare(
            &format!("registry_{count}_lua"),
            if count == 512 { 16 } else { 64 },
            |current| {
                let mut digest = 0;
                for index in 0..8 {
                    let (handled, errors) = if current {
                        after::dispatch(&new, &event!(after, index))
                    } else {
                        tree::dispatch(&old, &event!(tree, index))
                    }
                    .unwrap();
                    assert!(!handled && errors.is_empty());
                    digest += 1;
                }
                digest
            },
            &mut rows,
        );
    }
    std::fs::write(std::env::args().nth(1).expect("input_dispatch REPORT"),serde_json::to_vec_pretty(&serde_json::json!({"rows":rows,"event_snapshots":snapshots,"native_artifact":native_artifact,"validation":"all raw field/child traversal and nil-key next results match; native and fallback dispatch results match for alternate/repeated inputs; scene VM and original function roots remain"})).unwrap()).unwrap();
}
