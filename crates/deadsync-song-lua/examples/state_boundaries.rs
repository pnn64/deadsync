//! Focused differential checks and paired measurements for compiler state access.
use mlua::{Function, Lua, Table, Value, ffi};
use std::hint::black_box;

#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

#[cfg(windows)]
fn pin() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    // SAFETY: this disposable worker changes only its own affinity.
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
        0
    );
}
#[cfg(not(windows))]
fn pin() {}

// Frozen operations from a85295a5e, before the safe library APIs.
fn restore(lua: &Lua, cells: &Table, current: bool) -> mlua::Result<()> {
    for cell in cells.sequence_values::<Table>() {
        let cell = cell?;
        let function = cell.raw_get::<Function>(1)?;
        let index = if current {
            i64::from(cell.raw_get::<u8>(2)?)
        } else {
            cell.raw_get::<i64>(2)?
        };
        let value = cell.raw_get::<Value>(3)?;
        if current {
            function.set_upvalue(index as u8, value)?;
        } else {
            // SAFETY: the protected frame owns all three arguments. The
            // existing closure cell is updated and the frame is then cleared.
            unsafe {
                lua.exec_raw::<()>((function, index, value), |state| {
                    ffi::lua_setupvalue(state, 1, ffi::lua_tointeger(state, 2) as _);
                    ffi::lua_settop(state, 0);
                })?;
            }
        }
    }
    Ok(())
}

#[inline(always)]
fn color_before(table: &Table) -> Option<[f32; 4]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
        table.raw_get::<Option<f32>>(4).ok()?.unwrap_or(1.0),
    ])
}

fn compare(
    name: &str,
    batch: usize,
    work: &mut impl FnMut(bool),
    rows: &mut Vec<serde_json::Value>,
) {
    for sample in 0..10 {
        for current in [sample % 2 != 0, sample % 2 == 0] {
            let measure = metrics::Measurement::start();
            for _ in 0..batch {
                work(black_box(current));
            }
            let mut row = measure.finish();
            if sample == 0 {
                continue;
            }
            row["workload"] = name.into();
            row["current"] = current.into();
            row["sample"] = sample.into();
            row["operations"] = batch.into();
            rows.push(row);
        }
    }
}

fn main() {
    pin();
    let lua = Lua::new();
    let mut rows = Vec::new();
    for count in [1, 8, 32, 60] {
        let names: Vec<_> = (0..count).map(|i| format!("v{i}")).collect();
        let function: Function = lua
            .load(format!(
                "local {} = {}; return function() return {} end",
                names.join(","),
                vec!["{}"; count].join(","),
                names.join(",")
            ))
            .eval()
            .unwrap();
        let cells = lua.create_table().unwrap();
        function
            .for_each_upvalue(|index, _, value| {
                cells.raw_set(
                    index,
                    lua.create_sequence_from([
                        Value::Function(function.clone()),
                        Value::Integer(index.into()),
                        value,
                    ])?,
                )
            })
            .unwrap();
        let expected = function.call::<mlua::MultiValue>(()).unwrap().into_vec();
        for current in [false, true] {
            for index in 1..=count {
                assert!(function.set_upvalue(index as u8, false).unwrap());
            }
            restore(&lua, &cells, current).unwrap();
            lua.gc_collect().unwrap();
            assert_eq!(
                function.call::<mlua::MultiValue>(()).unwrap().into_vec(),
                expected
            );
        }
        compare(
            &format!("restore_{count}_cells"),
            512,
            &mut |current| {
                restore(&lua, &cells, current).unwrap();
            },
            &mut rows,
        );
    }
    for source in [
        "{1,'2.5',3}",
        "{-0.0,2,3,.5}",
        "{1,false,3}",
        "{1,2}",
        "{1,2,3,false}",
        "setmetatable({}, {__index=function() error('raw reads required') end})",
    ] {
        let table: Table = lua.load(source).eval().unwrap();
        assert_eq!(
            color_before(&table).map(|v| v.map(f32::to_bits)),
            deadsync_song_lua::read_color_value(Value::Table(table.clone()))
                .map(|v| v.map(f32::to_bits))
        );
    }
    let table: Table = lua.load("{.25,'0.5',.75,1}").eval().unwrap();
    compare(
        "color_table",
        16384,
        &mut |current| {
            let value = Value::Table(table.clone());
            black_box(if current {
                deadsync_song_lua::read_color_value(value)
            } else {
                let Value::Table(table) = value else {
                    unreachable!()
                };
                color_before(&table)
            });
        },
        &mut rows,
    );
    std::fs::write(std::env::args().nth(1).expect("state_boundaries REPORT"),
        serde_json::to_vec_pretty(&serde_json::json!({"rows":rows,"validation":"restored identities and exact numeric bits"})).unwrap()).unwrap();
}
