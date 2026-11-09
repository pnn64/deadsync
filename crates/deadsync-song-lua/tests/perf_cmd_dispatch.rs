//! Original streamed command/scanner fixtures over the Rust compiler and VM.
use deadlib_lua::{Value, Vm, compile_named};
use deadsync_song_lua::preprocess_lua_cmd_syntax;
use deadsync_song_lua::test_support::{
    cmd_append_newlines as append_lua_newlines, cmd_commands as lua_cmd_commands,
    cmd_newline_count as lua_newline_count, cmd_preprocess_cow as preprocess_lua_cmd_syntax_cow,
    cmd_quoted_end as lua_quoted_end,
};
#[expect(
    dead_code,
    reason = "this fixture uses scoped measurement from the shared allocation API"
)]
#[path = "../../../tests/support/perf.rs"]
mod allocations;
#[path = "support/bench_clock.rs"]
mod bench_clock;
fn dump(vm: &mut Vm, f: Value) -> Vec<u8> {
    vm.with_native_stack(|stack| {
        let base = stack.top();
        stack.push(f)?;
        let mut bytes = Vec::new();
        assert_eq!(
            stack.dump(|_, block| {
                bytes.extend_from_slice(block);
                Ok(0)
            })?,
            0
        );
        stack.set_top(base)?;
        Ok(bytes)
    })
    .unwrap()
}
fn call_text(vm: &mut Vm, f: Value) -> Result<String, String> {
    let values = vm.call(f, &[]).map_err(|e| e.to_string())?;
    let [Value::String(id)] = values else {
        panic!("source result string")
    };
    let id = *id;
    Ok(std::str::from_utf8(vm.string_bytes(id).unwrap())
        .unwrap()
        .to_owned())
}

#[path = "perf/cmd_dispatch_before.rs"]
mod before;

const FIXTURE: &str = include_str!("../../../tests/fixtures/song_lua/spooky-door.lua");
const EASES: &str = include_str!("../../../tests/fixtures/song_lua/runtime-eases.lua");
const COMMAND_BODIES: [&str; 14] = [
    "",
    " ",
    ";",
    ";;",
    "x",
    "x,1",
    " y,2;",
    "x,1 y,2",
    "value,1+2*3",
    "value,'é'",
    "callback,function() return 3 end",
    "x,",
    "x,(",
    ")",
];

#[test]
fn streamed_command_slices_keep_boundaries_and_first_errors() {
    for a in COMMAND_BODIES {
        for b in COMMAND_BODIES {
            for c in COMMAND_BODIES {
                let body = format!("{a}{b}{c}");
                let expected = before::lua_cmd_commands(&body).map(|values| values.into_vec());
                assert_eq!(
                    lua_cmd_commands(&body).collect::<Result<Vec<_>, _>>(),
                    expected,
                    "body={body:?}"
                );
            }
        }
    }
    for body in [
        "\n x,1; \n; y,2 \n",
        "\n",
        "x,1; bad,; later,2",
        "x, function() return {1,2} end; y, ({3})[1]",
        "x, [=[é曲]=];",
        "x,'unterminated",
    ] {
        assert_eq!(
            lua_cmd_commands(body).collect::<Result<Vec<_>, _>>(),
            before::lua_cmd_commands(body).map(|v| v.into_vec())
        );
    }
    let mut failed = lua_cmd_commands("x,");
    assert!(failed.next().unwrap().is_err());
    assert!(failed.next().is_none());
}

#[test]
fn source_byte_dispatch_keeps_generated_outputs_and_errors() {
    let tokens = [
        "a",
        "c",
        "cmd(x,1)",
        "-- line\n",
        "'quoted'",
        "[=[long]=]",
        ";",
        " ",
        "-",
        "[",
        "cmd(",
        "\"escaped\\\"text\"",
        "xcmd",
        "cmd --skip\n(",
    ];
    for a in tokens {
        for b in tokens {
            for c in tokens {
                let source = format!("{a}{b}{c}");
                compare(&source);
            }
        }
    }
    for source in [
        FIXTURE,
        EASES,
        "local é=1; return cmd(value, '曲')",
        "\0cmd(x,1)",
        "--[=[cmd(ignored)]=]\nreturn cmd(x, 1 + 2 * 3 and 4 or 7)",
        "local c='cmd(fake)'; return c",
        "return cmd -- skip\n(value,1;value,2)",
        "return [=[unterminated",
        "return 'unterminated",
    ] {
        compare(source);
    }
    fn compare(source: &str) {
        assert_eq!(
            preprocess_lua_cmd_syntax(source),
            before::preprocess_lua_cmd_syntax(source),
            "source={source:?}"
        );
        let a = preprocess_lua_cmd_syntax_cow(source);
        let b = before::preprocess_lua_cmd_syntax_cow(source);
        assert_eq!(a, b);
        if let (Ok(a), Ok(b)) = (a, b) {
            assert_eq!(
                matches!(a, std::borrow::Cow::Borrowed(_)),
                matches!(b, std::borrow::Cow::Borrowed(_))
            );
        }
    }
}

#[test]
fn byte_newline_counts_keep_source_lines_and_compiled_behavior() {
    for size in [0, 1, 7, 8, 15, 16, 17, 31, 32, 33, 64, 4096] {
        for pattern in ["x", "\n", "\r\n", "é曲", "\u{2003}\n\t", "x\n\r"] {
            let source = pattern.repeat(size);
            assert_eq!(lua_newline_count(&source), before::newline_count(&source));
            let mut a = String::new();
            let mut b = String::new();
            a.extend(source.chars().filter(|&ch| ch == '\n'));
            append_lua_newlines(&mut b, &source);
            assert_eq!(a, b);
        }
    }
    for quote in ['\'', '"'] {
        for size in [0, 1, 3, 4, 7, 8, 15, 16, 31, 32, 33, 63, 64, 65, 4096] {
            for pattern in ["x", "é", "\\x", "\\\"", "\\\\", "x\\曲", "'\""] {
                for closed in [false, true] {
                    let mut text = format!("prefix{quote}{}", pattern.repeat(size));
                    if closed {
                        text.push(quote);
                        text.push_str(" tail");
                    }
                    assert_eq!(lua_quoted_end(&text, 6), before::lua_quoted_end(&text, 6));
                }
            }
        }
    }
    let text = "'é\\曲\"text\\'tail'";
    for index in 0..text.len() {
        assert_eq!(
            lua_quoted_end(text, index),
            before::lua_quoted_end(text, index)
        );
    }
    for source in [
        "local actor={values={}}; actor.value=function(self,v) self.values[#self.values+1]=tostring(v);return self end; local f=cmd(value,1+2*3; value,false or 7; value, 3>=2 and 9 or 4);f(actor);return table.concat(actor.values,'|')",
        "return 'é曲'",
        "local value=1\nerror('failure')",
        "return cmd(value,1 and)",
        "return (",
    ] {
        let outcomes = [false, true].map(|current| {
            let mut lua = Vm::new();
            let result = {
                let owned = source.to_owned();
                let expanded = if current {
                    preprocess_lua_cmd_syntax_cow(&owned)
                } else {
                    before::preprocess_lua_cmd_syntax_cow(&owned)
                };
                expanded.map_err(|e| e.to_string()).and_then(|expanded| {
                    compile_named(expanded.as_bytes(), b"@cmd-dispatch-contract.lua")
                        .map(|program| Value::Function(lua.load_program(&program)))
                        .map_err(|e| e.to_string())
                })
            };
            match result {
                Ok(f) => Ok((dump(&mut lua, f), call_text(&mut lua, f))),
                Err(e) => Err(e.to_string()),
            }
        });
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

#[test]
fn dispatch_changes_preserve_allocation_contracts() {
    for source in [
        FIXTURE,
        EASES,
        "return cmd(x,1; y,2)",
        "return cmd(value,1+2*3 and 4 or 7)",
        "return 'unterminated",
    ] {
        let (a, old) = allocations::measure(|| {
            std::hint::black_box(before::preprocess_lua_cmd_syntax_cow(source))
        });
        let (b, new) =
            allocations::measure(|| std::hint::black_box(preprocess_lua_cmd_syntax_cow(source)));
        assert_eq!(a, b);
        assert_eq!(
            (old.allocs, old.reallocs, old.allocated_bytes),
            (new.allocs, new.reallocs, new.allocated_bytes)
        );
    }
    let body = "value,1; ".repeat(512);
    let (_, old) = allocations::measure(|| {
        for command in before::lua_cmd_commands(&body).unwrap() {
            std::hint::black_box(command);
        }
    });
    let (_, new) = allocations::measure(|| {
        for command in lua_cmd_commands(&body) {
            std::hint::black_box(command.unwrap());
        }
    });
    assert_eq!(old.allocs, 1);
    assert!(old.reallocs > 0);
    assert_eq!(new, allocations::Churn::default());
    let source = format!("return cmd({body})");
    let (a, old) = allocations::measure(|| {
        std::hint::black_box(before::preprocess_lua_cmd_syntax_cow(&source))
    });
    let (b, new) =
        allocations::measure(|| std::hint::black_box(preprocess_lua_cmd_syntax_cow(&source)));
    assert_eq!(a, b);
    assert_eq!(old.allocs, new.allocs + 1);
    assert!(old.reallocs > new.reallocs);
    assert!(old.allocated_bytes > new.allocated_bytes);
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_cmd_dispatch() {
    use bench_clock::{cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    fn measure<W: FnMut()>(
        name: &str,
        current: bool,
        unit: &str,
        input_bytes: usize,
        count: usize,
        mut setup: impl FnMut() -> W,
    ) {
        let mut samples = Vec::new();
        for sample in 0..10 {
            let mut work = setup();
            let start = Instant::now();
            let start_cycles = cycles();
            let (_, churn) = allocations::measure(|| {
                for _ in 0..count {
                    work();
                }
            });
            let used = cycles() - start_cycles;
            let nanos = start.elapsed().as_nanos();
            if sample > 0 {
                samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":count,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
            }
        }
        println!(
            "DRAW_CAPTURE_METRIC {}",
            serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":unit,"input_bytes":input_bytes,"baseline_commit":"7ae304506"})
        );
    }
    let _affinity = pin_cpu2();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let variants = if reverse {
        [true, false]
    } else {
        [false, true]
    };
    let plain = format!(
        "local value,total=0,0\n{}return total",
        "value=value+1; total=total+value\n".repeat(2048)
    );
    for (name, source, count, owned) in [
        ("dispatch_fixture", FIXTURE.to_owned(), 512, false),
        ("dispatch_eases", EASES.to_owned(), 256, false),
        (
            "dispatch_dense",
            "c c c c c c c c c c c c c c c c c c c c c c\n".repeat(1024),
            64,
            false,
        ),
        ("dispatch_plain", plain.clone(), 64, false),
        (
            "dispatch_small",
            "return cmd(x,1; y,2)".to_owned(),
            512,
            false,
        ),
        (
            "dispatch_operators",
            format!(
                "return cmd({})",
                "value, 1 + 2 * 3 and 4 or 7; ".repeat(512)
            ),
            64,
            false,
        ),
        (
            "dispatch_comments",
            format!("return cmd({})", "x, 1; -- comment\n y, 2;\n".repeat(512)),
            64,
            false,
        ),
        ("dispatch_owned_fixture", FIXTURE.to_owned(), 512, true),
        (
            "dispatch_newlines",
            format!(
                "return cmd({}x,{}1;{}y,{}2{})",
                "\n".repeat(2048),
                " \n".repeat(1024),
                "\n\t".repeat(1024),
                " \n".repeat(1024),
                "\n".repeat(2048)
            ),
            64,
            false,
        ),
    ] {
        assert_eq!(
            preprocess_lua_cmd_syntax(&source),
            before::preprocess_lua_cmd_syntax(&source)
        );
        for current in variants {
            measure(
                name,
                current,
                "preprocessed_sources",
                source.len(),
                count,
                || {
                    let source = source.clone();
                    move || {
                        if owned {
                            let _ = black_box(if current {
                                preprocess_lua_cmd_syntax(black_box(&source))
                            } else {
                                before::preprocess_lua_cmd_syntax(black_box(&source))
                            });
                        } else {
                            let _ = black_box(if current {
                                preprocess_lua_cmd_syntax_cow(black_box(&source))
                            } else {
                                before::preprocess_lua_cmd_syntax_cow(black_box(&source))
                            });
                        }
                    }
                },
            );
        }
    }
    for (name, source, count) in [
        ("quote_control_short", "'ordinary text'".to_owned(), 20000),
        (
            "quote_control_long",
            format!("'{}'", "x".repeat(65536)),
            128,
        ),
    ] {
        for current in variants {
            measure(name, current, "quoted_scans", source.len(), count, || {
                let source = source.clone();
                move || {
                    let _ = black_box(if current {
                        lua_quoted_end(black_box(&source), 0)
                    } else {
                        before::lua_quoted_end(black_box(&source), 0)
                    });
                }
            });
        }
    }
    for (name, source, count) in [
        ("newline_short", " \r\n\t \n".to_owned(), 20000),
        ("newline_ascii", " \r\n\t \n".repeat(4096), 128),
        ("newline_unicode", "\u{2003}é曲\n\r\n".repeat(4096), 128),
        ("newline_absent", "é曲 \t".repeat(4096), 128),
    ] {
        assert_eq!(lua_newline_count(&source), before::newline_count(&source));
        for current in variants {
            measure(name, current, "newline_scans", source.len(), count, || {
                let source = source.clone();
                move || {
                    black_box(if current {
                        lua_newline_count(black_box(&source))
                    } else {
                        before::newline_count(black_box(&source))
                    });
                }
            });
        }
    }
    for (name, commands, count) in [("commands_inline", 4, 2000), ("commands_spilled", 512, 64)] {
        let body = "value,1; ".repeat(commands);
        for current in variants {
            measure(name, current, "command_lists", body.len(), count, || {
                let body = body.clone();
                move || {
                    if current {
                        for command in lua_cmd_commands(black_box(&body)) {
                            black_box(command.unwrap());
                        }
                    } else {
                        for command in before::lua_cmd_commands(black_box(&body)).unwrap() {
                            black_box(command);
                        }
                    }
                }
            });
        }
    }
    for (name, source, count) in [
        ("dispatch_prepare_fixture", FIXTURE.to_owned(), 32),
        ("dispatch_prepare_plain", plain, 8),
        (
            "dispatch_prepare_cmd",
            "return cmd(x,1; y,2)".to_owned(),
            256,
        ),
    ] {
        for current in variants {
            measure(
                name,
                current,
                "prepared_chunks",
                source.len(),
                count,
                || {
                    let mut lua = Vm::new();
                    let source = source.clone();
                    let expanded = if current {
                        preprocess_lua_cmd_syntax_cow(&source)
                    } else {
                        before::preprocess_lua_cmd_syntax_cow(&source)
                    }
                    .unwrap();
                    lua.load_program(
                        &compile_named(expanded.as_bytes(), b"@cmd-dispatch-contract.lua").unwrap(),
                    );
                    move || {
                        let expanded = if current {
                            preprocess_lua_cmd_syntax_cow(black_box(&source))
                        } else {
                            before::preprocess_lua_cmd_syntax_cow(black_box(&source))
                        }
                        .unwrap();
                        black_box(
                            lua.load_program(
                                &compile_named(expanded.as_bytes(), b"@cmd-dispatch-contract.lua")
                                    .unwrap(),
                            ),
                        );
                    }
                },
            );
        }
    }
}
