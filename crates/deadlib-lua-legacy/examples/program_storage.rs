//! Native execution, unique/shared grouping and finite-byte preparation costs.
use mlua::program::{self, Binding, Bindings, Cell, Datum, Field, FieldValue, Group, Program, Types};
use mlua::{Function, Lua};
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
    // SAFETY: changes only this disposable worker thread's CPU affinity.
    assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) }, 0);
}
#[cfg(not(windows))]
fn pin() {}

fn field(object: u32, name: &str, value: Binding) -> Field {
    Field {
        object,
        name: name.as_bytes().to_vec().into(),
        value: FieldValue::Value(value),
    }
}
fn fixture(lua: &Lua, byte_cells: usize, distinct: bool) -> (Vec<Function>, Bindings) {
    let functions: mlua::Table = lua.load(if byte_cells == 0 {
        "local s={};return {function(e)s.held=not s.held;s.total=s.total+e.delta;return e.button=='Right' and s.held end,function(e)s.total=s.total-e.delta;return s.total<=0 end,function(e)return e.button=='Left' or s.held end}"
    } else {
        "local s={};return {function(e)s.copy=s.mode;if e.button=='Right' then s.mode='armed' elseif e.button=='Left' then s.mode='idle' end;return s.mode=='armed' end,function(e)if s.mode=='armed' then s.mode='cooldown' end;return s.copy==s.mode end,function(e)s.copy=s.mode;return s.mode=='idle' end}"
    }).eval().unwrap();
    let mut bindings = Bindings {
        arguments: vec![Binding::Object(0)],
        upvalues: vec![Binding::Object(1)],
        fields: vec![
            field(
                0,
                "button",
                Binding::Input {
                    slot: 0,
                    types: Types::BYTES,
                },
            ),
            field(
                0,
                "delta",
                Binding::Input {
                    slot: 1,
                    types: Types::NUMBER,
                },
            ),
        ],
        ..Bindings::default()
    };
    if byte_cells == 0 {
        bindings.cells = vec![Cell::Boolean(false), Cell::Number(-0.0)];
        bindings.fields.push(field(1, "held", Binding::Cell(0)));
        bindings.fields.push(field(1, "total", Binding::Cell(1)));
    } else {
        for i in 0..byte_cells {
            let bytes = if distinct {
                format!("initial-state-{i:04}").into_bytes()
            } else {
                b"idle".to_vec()
            };
            bindings.strings.push(bytes.into_boxed_slice());
            bindings.cells.push(Cell::Bytes(i));
        }
        bindings.fields.push(field(1, "mode", Binding::Cell(0)));
        bindings.fields.push(field(1, "copy", Binding::Cell(1)));
        if !distinct {
            bindings.strings.truncate(1);
            bindings.cells.fill(Cell::Bytes(0));
        }
    }
    (
        functions
            .sequence_values::<Function>()
            .collect::<mlua::Result<_>>()
            .unwrap(),
        bindings,
    )
}
fn compile(functions: &[Function], bindings: &Bindings) -> Vec<Program> {
    let compiler = program::Compiler::new(bindings).unwrap();
    functions
        .iter()
        .map(|f| {
            compiler
                .compile(f, &bindings.arguments, &bindings.upvalues)
                .unwrap()
        })
        .collect()
}
fn digest(group: &mut Group, repeats: usize) -> u64 {
    let mut digest = 0u64;
    for i in 0..repeats {
        let inputs = [
            Datum::Bytes([b"Right".as_slice(), b"Other", b"Left"][i % 3]),
            Datum::Number([0.03125, -0.0, 0.0, 0.125][i % 4]),
        ];
        for program in 0..group.len() {
            digest = digest.wrapping_mul(31).wrapping_add(u64::from(
                group
                    .run(program, black_box(&inputs), |_, _| {})
                    .unwrap()
                    .unwrap()
                    .is_truthy(),
            ));
        }
    }
    digest
}
fn main() {
    pin();
    let lua = Lua::new();
    let mut rows = Vec::new();
    let mut outputs = Vec::new();
    for (name, cells, distinct) in [
        ("numeric", 0, false),
        ("bytes3", 3, false),
        ("bytes16", 16, false),
        ("bytes64", 64, false),
        ("distinct64", 64, true),
    ] {
        let (functions, bindings) = fixture(&lua, cells, distinct);
        let programs = compile(&functions, &bindings);
        let machine_payload = programs
            .iter()
            .map(|p| p.machine().retained_bytes())
            .sum::<usize>();
        let mut group = Group::new(programs.clone()).unwrap();
        let result = digest(&mut group, 96);
        let states = (0..bindings.cells.len())
            .map(|i| format!("{:?}", group.cell(i)))
            .collect::<Vec<_>>();
        outputs.push(serde_json::json!({"fixture":name,"digest":result,"states":states,"machine_payload_bytes":machine_payload,"group_payload_bytes":group.retained_bytes()}));
        for sample in 0..10 {
            if cells <= 3 {
                let measure = metrics::Measurement::start();
                let result = digest(&mut group, 65536);
                let mut row = measure.finish();
                black_box(result);
                if sample > 0 {
                    row["fixture"] = name.into();
                    row["operation"] = "execute".into();
                    row["sample"] = sample.into();
                    row["operations"] = (65536 * 3).into();
                    row["digest"] = result.into();
                    rows.push(row);
                }
            }
            let measure = metrics::Measurement::start();
            for _ in 0..128 {
                black_box(Group::new(compile(&functions, &bindings)).unwrap());
            }
            let mut row = measure.finish();
            if sample > 0 {
                row["fixture"] = name.into();
                row["operation"] = "compile_group".into();
                row["sample"] = sample.into();
                row["operations"] = 128.into();
                rows.push(row);
            }
            if cells == 3 {
                for shared in [false, true] {
                    // Compilation and input batch allocation are outside this measurement.
                    // Shared inputs keep external Programs alive; unique inputs have one owner.
                    let batch = (0..256)
                        .map(|_| {
                            if shared {
                                programs.clone()
                            } else {
                                compile(&functions, &bindings)
                            }
                        })
                        .collect::<Vec<_>>();
                    let measure = metrics::Measurement::start();
                    for values in batch {
                        black_box(Group::new(values).unwrap());
                    }
                    let mut row = measure.finish();
                    if sample > 0 {
                        row["fixture"] = name.into();
                        row["operation"] = if shared { "group_shared" } else { "group_unique" }.into();
                        row["sample"] = sample.into();
                        row["operations"] = 256.into();
                        rows.push(row);
                    }
                }
            }
        }
    }
    std::fs::write(
        std::env::args().nth(1).expect("program_storage REPORT"),
        serde_json::to_vec_pretty(&serde_json::json!({"rows":rows,"outputs":outputs})).unwrap(),
    )
    .unwrap();
}
