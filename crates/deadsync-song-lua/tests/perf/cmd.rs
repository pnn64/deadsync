use super::*;
use std::borrow::Cow;

#[path = "cmd_before.rs"]
mod before;

const FIXTURE: &str = include_str!("../../../../tests/fixtures/song_lua/spooky-door.lua");
const EASES: &str = include_str!("../../../../tests/fixtures/song_lua/runtime-eases.lua");

#[test]
fn scanner_keeps_generated_source_bytes_errors_and_newlines() {
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
        "",
        "local é = 1; return cmd(settext, '曲')",
        "return cmd(settext, 'é\\曲')",
        "\0return cmd(x,1)",
        "-- cmd(ignored)\nlocal text=[==[cmd(ignored)]==]\nreturn text",
        "return cmd --before\n( x,1; --after\n y,2)\n",
    ] {
        compare(source);
    }
    let source = "local text='é曲\\\"';\nreturn cmd(settext, 'é曲'; x,1)\n";
    for (index, _) in source
        .char_indices()
        .chain(std::iter::once((source.len(), '\0')))
    {
        compare(&source[..index]);
    }
    fn compare(source: &str) {
        let expected = before::preprocess_lua_cmd_syntax(source);
        assert_eq!(
            preprocess_lua_cmd_syntax(source),
            expected,
            "source={source:?}"
        );
        let actual = preprocess_lua_cmd_syntax_cow(source);
        if let Ok(Cow::Borrowed(text)) = &actual {
            assert_eq!(*text, source);
        }
        assert_eq!(actual.map(Cow::into_owned), expected, "source={source:?}");
        if let Ok(value) = expected {
            assert_eq!(
                value.bytes().filter(|&b| b == b'\n').count(),
                source.bytes().filter(|&b| b == b'\n').count()
            );
        }
    }
}

#[test]
fn quoted_scans_keep_escape_and_utf8_boundary_behavior() {
    for quote in ['\'', '"'] {
        for length in [0, 1, 15, 16, 31, 32, 33, 63, 64, 65, 127, 128, 4096] {
            for pattern in ["x", "é", "\\x", "\\\"", "\\\\", "x\\é", "x\n", "'\""] {
                for closed in [false, true] {
                    let mut source = format!("prefix{quote}{}", pattern.repeat(length));
                    if closed {
                        source.push(quote);
                        source.push_str(" tail");
                    }
                    assert_eq!(
                        lua_quoted_end(&source, 6),
                        before::lua_quoted_end(&source, 6),
                        "length={length}, pattern={pattern:?}, closed={closed}"
                    );
                }
            }
        }
    }
    let source = "'é\\曲\"text\\'tail'";
    for index in 0..source.len() {
        assert_eq!(
            lua_quoted_end(source, index),
            before::lua_quoted_end(source, index)
        );
    }
}

fn compile(lua: &mlua::Lua, current: bool, source: &str) -> mlua::Result<mlua::Function> {
    let source = source.to_owned();
    let expanded = if current {
        preprocess_lua_cmd_syntax_cow(&source)
    } else {
        before::preprocess_lua_cmd_syntax(&source).map(Cow::Owned)
    }
    .map_err(mlua::Error::external)?;
    lua.load(expanded.as_ref())
        .set_name("@cmd-scan-contract.lua")
        .into_function()
}

#[test]
fn prepared_chunks_keep_bytecode_results_and_stack_diagnostics() {
    for source in [
        "local text='é曲'; return text..' preserved'",
        r#"
            local actor={values={}}
            for _,name in ipairs{'value','sleep'} do
                actor[name]=function(self,v)
                    if type(v)=='function' then v=v() end
                    self.values[#self.values+1]=tostring(v);return self
                end
            end
            local run=cmd(value,1; -- comment
                value,'é曲' sleep, .5;
                value, function() return 7 end)
            run(actor)
            return table.concat(actor.values,'|')
        "#,
        "local value = 3\nerror('runtime failure')",
        "return (",
        "return cmd(value,)",
        "return 'unterminated",
    ] {
        let lua_a = mlua::Lua::new();
        let lua_b = mlua::Lua::new();
        let a = compile(&lua_a, false, source);
        let b = compile(&lua_b, true, source);
        match (a, b) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.dump(false), b.dump(false));
                assert_eq!(
                    a.call::<String>(()).map_err(|e| e.to_string()),
                    b.call::<String>(()).map_err(|e| e.to_string())
                );
            }
            (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
            _ => panic!("compiled result differs for {source:?}"),
        }
    }
}

#[test]
fn borrowed_preprocessing_removes_only_unchanged_output_allocations() {
    use crate::draw_capture::perf::allocations;
    for source in [
        FIXTURE,
        EASES,
        "local value=123; return value",
        "-- comment\nreturn [=[cmd(ignored)]=]",
    ] {
        let (a, old) = allocations::measure(|| {
            std::hint::black_box(before::preprocess_lua_cmd_syntax(source).unwrap())
        });
        let (b, new) = allocations::measure(|| {
            std::hint::black_box(preprocess_lua_cmd_syntax_cow(source).unwrap())
        });
        assert_eq!(a, b);
        assert!(matches!(b, Cow::Borrowed(_)));
        assert_eq!(old.allocs, 1);
        assert_eq!((new.allocs, new.reallocs, new.allocated_bytes), (0, 0, 0));
        let (c, owned) = allocations::measure(|| {
            std::hint::black_box(preprocess_lua_cmd_syntax(source).unwrap())
        });
        assert_eq!(c, a);
        assert_eq!(owned.allocs, 1);
    }
    let source = "return cmd(x,1; y,2)";
    let (a, old) = allocations::measure(|| {
        std::hint::black_box(before::preprocess_lua_cmd_syntax(source).unwrap())
    });
    let (b, new) = allocations::measure(|| {
        std::hint::black_box(preprocess_lua_cmd_syntax_cow(source).unwrap())
    });
    assert_eq!(a, b);
    assert!(matches!(b, Cow::Owned(_)));
    assert_eq!(old.allocs, new.allocs);
    let source = format!("return '{}", "x".repeat(65536));
    let (a, old) = allocations::measure(|| {
        std::hint::black_box(before::preprocess_lua_cmd_syntax(&source).unwrap_err())
    });
    let (b, new) = allocations::measure(|| {
        std::hint::black_box(preprocess_lua_cmd_syntax_cow(&source).unwrap_err())
    });
    assert_eq!(a, b);
    assert_eq!((old.allocs, new.allocs), (2, 1));
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_cmd() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
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
            serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":unit,"input_bytes":input_bytes,"baseline_commit":"77d08fc56"})
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
    let quote = format!(
        "return cmd(value, {:?})",
        "ordinary utf8 text ".repeat(4096)
    );
    let escaped = format!(
        "return cmd(value, {:?})",
        "slash\\word quote\"word ".repeat(2048)
    );
    let commented = format!("return cmd({})", "x, 1; -- comment\n y, 2;\n".repeat(512));
    let no_cmd_quote = format!(
        "local text={:?}; return text",
        "ordinary utf8 text ".repeat(4096)
    );
    for (name, source, count, owned) in [
        ("cmd_fixture", FIXTURE.to_owned(), 512, false),
        ("cmd_eases", EASES.to_owned(), 256, false),
        ("cmd_plain", plain.clone(), 64, false),
        ("cmd_literal", no_cmd_quote, 64, false),
        (
            "cmd_marker_dense",
            "c c c c c c c c c c c c c c c c c c c c c c\n".repeat(1024),
            64,
            false,
        ),
        ("cmd_small", "return cmd(x,1; y,2)".to_owned(), 512, false),
        ("cmd_comments", commented, 64, false),
        ("cmd_long_quote", quote, 64, false),
        ("cmd_escaped", escaped, 64, false),
        (
            "cmd_unterminated",
            format!("return '{}", "x".repeat(65536)),
            64,
            false,
        ),
        ("cmd_owned_fixture", FIXTURE.to_owned(), 512, true),
        ("cmd_owned_plain", plain.clone(), 64, true),
        (
            "cmd_owned_markers",
            "c c c c c c c c c c c c c c c c c c c c c c\n".repeat(1024),
            64,
            true,
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
                        if current && !owned {
                            let _ = black_box(preprocess_lua_cmd_syntax_cow(black_box(&source)));
                        } else if current {
                            let _ = black_box(preprocess_lua_cmd_syntax(black_box(&source)));
                        } else {
                            let _ =
                                black_box(before::preprocess_lua_cmd_syntax(black_box(&source)));
                        }
                    }
                },
            );
        }
    }
    for (name, source, count) in [
        ("quoted_long", format!("\"{}\"", "x".repeat(65536)), 128),
        (
            "quoted_dense_escapes",
            format!("\"{}\"", r"x\\y\z".repeat(4096)),
            128,
        ),
        ("quoted_small", "\"ordinary text\"".to_owned(), 20000),
        (
            "quoted_unterminated",
            format!("\"{}", "x".repeat(65536)),
            128,
        ),
    ] {
        assert_eq!(
            lua_quoted_end(&source, 0),
            before::lua_quoted_end(&source, 0)
        );
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
        ("prepare_fixture", FIXTURE.to_owned(), 32),
        ("prepare_plain", plain, 8),
        ("prepare_cmd", "return cmd(x,1; y,2)".to_owned(), 256),
    ] {
        for current in variants {
            measure(
                name,
                current,
                "prepared_chunks",
                source.len(),
                count,
                || {
                    let lua = mlua::Lua::new();
                    let source = source.clone();
                    compile(&lua, current, &source).unwrap();
                    move || {
                        let expanded = if current {
                            preprocess_lua_cmd_syntax_cow(black_box(&source))
                        } else {
                            before::preprocess_lua_cmd_syntax(black_box(&source)).map(Cow::Owned)
                        }
                        .unwrap();
                        black_box(
                            lua.load(expanded.as_ref())
                                .set_name("@cmd-scan-contract.lua")
                                .into_function()
                                .unwrap(),
                        );
                    }
                },
            );
        }
    }
}
