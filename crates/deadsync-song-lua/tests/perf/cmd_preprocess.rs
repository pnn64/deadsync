use super::*;
use std::borrow::Cow;
use std::hint::black_box;
use std::path::{Path, PathBuf};

#[path = "cmd_preprocess_baseline.rs"]
mod baseline;

fn preprocess(source: &str, old: bool) -> Result<String, String> {
    if old {
        baseline::preprocess_lua_cmd_syntax(source)
    } else {
        preprocess_lua_cmd_syntax(source)
    }
}

fn long_string(level: usize, payload: &str) -> String {
    let equals = "=".repeat(level);
    format!("[{equals}[{payload}]{equals}]")
}

#[test]
fn cmd_preprocess_borrowed_bodies_preserve_text_comment_newlines_and_literal_markers() {
    let inputs = [
        "",
        "zoom, 2; x, 1",
        "settext, '--literal'; y, 4",
        "settext, [=[--literal; )]=]; x, 1",
        "-- first\r\n x, 1; -- last",
        "x, --[==[\n\r\n\u{e9} ignored\n]==] 1; y,2",
        "--[not a long string\n x,1",
        "--[=[unclosed\n x,1",
        "settext, '\u{e9}\0'; -- \u{65e5}\u{672c}\n zoom, 2",
    ];
    for body in inputs {
        let actual = lua_cmd_without_comments(body).unwrap();
        assert_eq!(
            actual.as_ref(),
            baseline::lua_cmd_without_comments(body).unwrap()
        );
        if !body.contains("--")
            || body.starts_with("settext, '--literal'")
            || body.starts_with("settext, [=[--literal")
        {
            assert!(matches!(actual, Cow::Borrowed(_)));
            assert_eq!(actual.as_ptr(), body.as_ptr());
        } else {
            assert!(matches!(actual, Cow::Owned(_)));
        }
    }
    for body in [
        "x,1 -- comment\n settext, 'unclosed",
        "-- comment\n settext, [=[unclosed",
    ] {
        assert_eq!(
            lua_cmd_without_comments(body).unwrap_err(),
            baseline::lua_cmd_without_comments(body).unwrap_err()
        );
    }
}

#[test]
fn cmd_preprocess_inline_commands_preserve_nested_splits_empty_entries_and_spill_boundaries() {
    for count in [0, 1, 7, 8, 9, 15, 16, 17, 64, 257] {
        let mut body = (0..count)
            .map(|i| format!("x, f({i}, {{[1]=g(2,3),label=';'}}, [=[; )]=])"))
            .collect::<Vec<_>>()
            .join(";");
        for suffix in ["", ";", ";;"] {
            body.push_str(suffix);
            let expected = baseline::lua_cmd_commands(&body).unwrap();
            let actual = lua_cmd_commands(&body).unwrap();
            assert_eq!(actual.as_slice(), expected);
            assert_eq!(actual.spilled(), actual.len() > 8);
        }
    }
    for body in [
        "; ;",
        "x, {1;2}; y, array[1]; -- ; ignored\n z, 3",
        "x, '\\'; y, 2",
        "x, 'unterminated",
        "x, [==[unterminated",
        "x, ] ; y, 1",
    ] {
        assert_eq!(
            lua_cmd_commands(body).map(|v| v.into_vec()),
            baseline::lua_cmd_commands(body)
        );
    }
}

#[test]
fn cmd_preprocess_long_delimiters_match_parent_for_overlaps_near_matches_and_random_payloads() {
    for level in [0, 1, 2, 3, 8, 64, 1024] {
        for payload in [
            "",
            "hello\n\u{e9}\u{65e5}\0",
            "]]",
            "]=]]=]",
            "]===]===]",
            "cmd(x); ) --",
        ] {
            let input = long_string(level, payload);
            let open = lua_long_bracket_end(&input, 0).unwrap();
            assert_eq!(
                lua_long_string_end(&input, 0, open),
                baseline::lua_long_string_end(&input, 0, open)
            );
            for cut in [1, 2, level + 2] {
                let input = &input[..input.len() - cut];
                assert_eq!(
                    lua_long_string_end(input, 0, open),
                    baseline::lua_long_string_end(input, 0, open)
                );
            }
        }
        let near = (format!("]{}]", "=".repeat(level + 1))).repeat(256);
        let input = long_string(level, &near);
        let open = lua_long_bracket_end(&input, 0).unwrap();
        assert_eq!(
            lua_long_string_end(&input, 0, open),
            baseline::lua_long_string_end(&input, 0, open)
        );
    }
    let alphabet = ["a", "=", "]", "[", "\u{e9}", "\n", "'", "--", "\0"];
    let mut seed = 0x98765432_u32;
    for case in 0..2048 {
        let mut payload = String::new();
        for _ in 0..64 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            payload.push_str(alphabet[seed as usize % alphabet.len()]);
        }
        let input = long_string(case % 17, &payload);
        let open = lua_long_bracket_end(&input, 0).unwrap();
        assert_eq!(
            lua_long_string_end(&input, 0, open),
            baseline::lua_long_string_end(&input, 0, open)
        );
    }
    for level in [0, 1, 2, 14, 15, 64] {
        for count in [7, 8, 9, 8192] {
            let input = long_string(level, &format!("]{}]", "=".repeat(level + 1)).repeat(count));
            let open = lua_long_bracket_end(&input, 0).unwrap();
            assert_eq!(
                lua_long_string_end(&input, 0, open),
                baseline::lua_long_string_end(&input, 0, open)
            );
            let missing = &input[..input.len() - level - 2];
            assert_eq!(
                lua_long_string_end(missing, 0, open),
                baseline::lua_long_string_end(missing, 0, open)
            );
        }
    }
}

#[test]
fn cmd_preprocess_complete_outputs_preserve_identifier_rules_expansions_and_error_priority() {
    let commands = [
        "cmd()",
        "cmd(; ;)",
        "cmd(x, 1; y, 2)",
        "cmd \t\r\n (x, f(1,2))",
        "cmdx(x,1)",
        "xcmd(x,1)",
        "_cmd(x,1)",
        "cmd",
        "cmd  ",
        "obj.cmd(x,1)",
        "cmd(1bad, 1)",
        "cmd(x 1)",
        "cmd(x, 1",
        "cmd(settext, 'bad)",
        "cmd(settext, [==[bad)",
        "cmd(1bad, 1; settext, 'bad)",
        "cmd(x, 1; --[=[missing\n y, 2)",
        "cmd(settext, '\u{e9} -- ; )')",
        "cmd(settext, [=[\u{65e5}\u{672c}; ) --]=]; x, 2)",
        "cmd(x, --\r\n 1; --[==[ ) ; ignored\n]==]\n y, 2)",
    ];
    for command in commands {
        for prefix in [
            "",
            "local first=cmd(zoom, 2); ",
            "-- cmd(invalid)\n",
            "local s='cmd(invalid)'; ",
            "\u{e9} ",
        ] {
            let input = format!("{prefix}return {command} -- cmd(noop)");
            assert_eq!(
                preprocess(&input, false),
                preprocess(&input, true),
                "{input:?}"
            );
        }
    }
    for input in [
        "return 'unclosed",
        "return [=[unclosed",
        "--[=[unclosed\nreturn cmd(x,1)",
        "local text=[==[cmd(noop)]==]; return cmd(x,1)",
    ] {
        assert_eq!(preprocess(input, false), preprocess(input, true));
    }
}

#[test]
fn cmd_preprocess_generated_sources_match_parent_before_and_after_syntax_errors() {
    let arguments = [
        "1",
        "f(1,2)",
        "{1,2,label=';'}",
        "array[1]",
        "'--text; )'",
        "[==[\u{e9}; ) --]==]",
    ];
    let separators = [";", "; -- ignored ) ;\n", "; --[=[ignored ) ;\n]=]\n"];
    let mut seed = 0x12345678_u32;
    for case in 0..1024 {
        let mut source = String::from(
            "local literal='cmd(noop)'; -- cmd(noop)\nlocal first=cmd(x,1); return cmd(",
        );
        for i in 0..case % 24 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            source.push_str(if i % 3 == 0 { "x, " } else { "settext, " });
            source.push_str(arguments[seed as usize % arguments.len()]);
            source.push_str(separators[seed as usize % separators.len()]);
        }
        source.push(')');
        assert_eq!(
            preprocess(&source, false),
            preprocess(&source, true),
            "case {case}"
        );
        for broken in [
            "; return cmd(x 1)",
            "; return cmd(settext, 'bad)",
            "; return cmd(x, 1",
        ] {
            assert_eq!(
                preprocess(&(source.clone() + broken), false),
                preprocess(&(source.clone() + broken), true)
            );
        }
    }
}

#[test]
fn cmd_preprocess_lua_execution_preserves_argument_effects_method_order_and_partial_errors() {
    for body in [
        "x, touch(1); y, touch(2); settext, '--text; )'",
        "x, -- ignored\n touch(1); settext, [==[\u{e9}; ) --]==]; y, touch(2)",
        "x, touch(1); missing, touch(3); y, touch(2)",
    ] {
        let mut results = Vec::new();
        for old in [true, false] {
            let lua = mlua::Lua::new();
            lua.load(
                r#"log={}; function touch(x) log[#log+1]='arg'..x;return x end
                actor={x=function(self,x)log[#log+1]='x'..x;self.a=x end,
                y=function(self,x)log[#log+1]='y'..x;self.b=x end,
                settext=function(self,x)log[#log+1]=x;self.label=x end}"#,
            )
            .exec()
            .unwrap();
            let function = lua
                .load(preprocess(&format!("return cmd({body})"), old).unwrap())
                .set_name("cmd-behavior")
                .eval::<mlua::Function>()
                .unwrap();
            let actor: mlua::Table = lua.globals().get("actor").unwrap();
            let error = function
                .call::<mlua::Value>(actor.clone())
                .err()
                .map(|e| e.to_string());
            results.push((
                error,
                lua.load("return table.concat(log,'|')")
                    .eval::<String>()
                    .unwrap(),
                actor.get::<Option<i32>>("a").unwrap(),
                actor.get::<Option<i32>>("b").unwrap(),
                actor.get::<Option<String>>("label").unwrap(),
            ));
        }
        assert_eq!(results[0], results[1]);
    }
}

fn corpus() -> Vec<(PathBuf, String)> {
    fn visit(path: &Path, paths: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, paths);
            } else if path.extension().is_some_and(|s| s == "lua") {
                paths.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/noteskins");
    let mut paths = Vec::new();
    visit(&root, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect()
}

#[test]
fn cmd_preprocess_repository_noteskin_corpus_matches_parent_including_existing_errors() {
    let inputs = corpus();
    assert!(inputs.len() >= 100);
    for (path, input) in &inputs {
        assert_eq!(
            preprocess(input, false),
            preprocess(input, true),
            "{}",
            path.display()
        );
    }
}

#[test]
fn cmd_preprocess_prepared_helpers_have_zero_churn_and_whole_sources_use_less_storage() {
    for body in ["", "x,1;y,2", "settext, '--literal'; x, 1"] {
        crate::perf::assert_no_churn(|| {
            black_box(lua_cmd_without_comments(body).unwrap());
        });
        if body.is_empty() {
            crate::perf::assert_no_churn(|| {
                black_box(baseline::lua_cmd_without_comments(body).unwrap());
            });
            continue;
        }
        crate::perf::assert_reduced_churn(
            || {
                black_box(baseline::lua_cmd_without_comments(body).unwrap());
            },
            || {
                black_box(lua_cmd_without_comments(body).unwrap());
            },
        );
    }
    for count in [0, 1, 7, 8] {
        let body = std::iter::repeat_n("x,1", count)
            .collect::<Vec<_>>()
            .join(";");
        crate::perf::assert_no_churn(|| {
            black_box(lua_cmd_commands(&body).unwrap());
        });
        crate::perf::assert_reduced_churn(
            || {
                black_box(baseline::lua_cmd_commands(&body).unwrap());
            },
            || {
                black_box(lua_cmd_commands(&body).unwrap());
            },
        );
    }
    for level in [0, 1, 64] {
        let input = long_string(level, "payload");
        let open = lua_long_bracket_end(&input, 0).unwrap();
        crate::perf::assert_no_churn(|| {
            black_box(lua_long_string_end(&input, 0, open).unwrap());
        });
        crate::perf::assert_reduced_churn(
            || {
                black_box(baseline::lua_long_string_end(&input, 0, open).unwrap());
            },
            || {
                black_box(lua_long_string_end(&input, 0, open).unwrap());
            },
        );
    }
    let dense = long_string(2, &"]=]".repeat(8192));
    crate::perf::assert_no_churn(|| {
        black_box(lua_long_string_end(&dense, 0, 4).unwrap());
    });
    for source in [
        "return cmd(x,1;y,2)",
        "return cmd(settext,[==[hello]==];x,1)",
        "local a=cmd();return cmd(x,1;y,2)",
    ] {
        crate::perf::assert_reduced_churn(
            || {
                black_box(preprocess(source, true).unwrap());
            },
            || {
                black_box(preprocess(source, false).unwrap());
            },
        );
    }
}

fn source(count: usize, width: usize, style: &str) -> String {
    let command = match style {
        "comments" => "x, --[=[ignored ; )\n]=] 1",
        "long" => "settext, [==[\u{e9}; ) -- long literal]==]",
        _ => "x, f(1,2)",
    };
    let body = std::iter::repeat_n(command, width)
        .collect::<Vec<_>>()
        .join(";");
    (0..count)
        .map(|i| format!("local actor_{i}=cmd({body});\n"))
        .collect()
}

#[test]
#[ignore = "manual old/new CPU, throughput and allocator benchmark"]
fn cmd_preprocess_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (name, body) in [
        ("empty", "".into()),
        ("plain", "x,1;y,2".into()),
        ("quoted_marker", "settext, '--text'; x, 1".into()),
        ("commented", "x, --ignored\n 1; y, 2".into()),
        ("large_plain", "x, f(1,2);".repeat(128)),
    ] {
        for old in order {
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("strip/{name}/{mode}"),
                4096,
                body.len().max(1),
                || {
                    if old {
                        black_box(Cow::Owned(
                            baseline::lua_cmd_without_comments(black_box(&body)).unwrap(),
                        ))
                    } else {
                        black_box(lua_cmd_without_comments(black_box(&body)).unwrap())
                    }
                },
            );
        }
    }
    for width in [0, 1, 7, 8, 9, 16, 64] {
        let body = std::iter::repeat_n("x, f(1,2)", width)
            .collect::<Vec<_>>()
            .join(";");
        for old in order {
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("split/{width}/{mode}"),
                4096,
                width.max(1),
                || {
                    if old {
                        black_box(baseline::lua_cmd_commands(black_box(&body)).unwrap());
                    } else {
                        black_box(lua_cmd_commands(black_box(&body)).unwrap());
                    }
                },
            );
        }
    }
    for (name, level, payload) in [
        ("empty", 0, String::new()),
        ("short", 2, "hello".into()),
        ("wide", 64, "hello".into()),
        ("long", 2, "abc\u{e9}".repeat(8192)),
        ("near", 64, format!("]{}]", "=".repeat(65)).repeat(256)),
        ("dense", 2, "]=]".repeat(8192)),
    ] {
        let input = long_string(level, &payload);
        let open = lua_long_bracket_end(&input, 0).unwrap();
        for old in order {
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("delimiter/{name}/{mode}"),
                if input.len() < 1024 { 4096 } else { 128 },
                input.len(),
                || {
                    if old {
                        black_box(
                            baseline::lua_long_string_end(black_box(&input), 0, open).unwrap(),
                        );
                    } else {
                        black_box(lua_long_string_end(black_box(&input), 0, open).unwrap());
                    }
                },
            );
        }
    }
    for (count, width, style) in [
        (0, 0, "plain"),
        (1, 1, "plain"),
        (1, 7, "plain"),
        (1, 8, "plain"),
        (1, 9, "plain"),
        (64, 4, "plain"),
        (64, 4, "comments"),
        (64, 4, "long"),
        (1, 64, "plain"),
    ] {
        let source = source(count, width, style);
        for old in order {
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("preprocess/{count}/{width}/{style}/{mode}"),
                if count >= 64 { 128 } else { 4096 },
                source.len().max(1),
                || {
                    black_box(preprocess(black_box(&source), old).unwrap());
                },
            );
        }
    }
    let corpus = corpus();
    let bytes = corpus.iter().map(|(_, s)| s.len()).sum::<usize>();
    for old in order {
        let mode = if old { "old" } else { "new" };
        crate::perf::measure_sampled(&format!("corpus/noteskins/{mode}"), 64, bytes, || {
            for (_, input) in &corpus {
                let _ = black_box(preprocess(black_box(input), old));
            }
        });
    }
}
