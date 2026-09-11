// Reference routines frozen from 1d95c241f / 0.5.1136.
// The scoped-command reference uses the shared method/delimiter parser to isolate argument ownership.
use super::*;
use std::hint::black_box;

fn chain_fixture(count: usize) -> String {
    "self:x(12):y(-4):zoom(0.5):diffuse(Tint):queuecommand('Ready')\n".repeat(count)
}

fn command_fixture() -> (CommandContext, HashMap<String, String>) {
    let mut context = CommandContext::default();
    context
        .colors
        .insert("tint".into(), "0.25,0.5,0.75,1".into());
    let scope = HashMap::from([("tint".into(), "1,0.5,0.25,1".into())]);
    (context, scope)
}

fn corpus() -> Vec<(String, String)> {
    fn visit(path: &Path, output: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, output);
            } else if is_lua_path(&path) {
                output.push((
                    path.to_string_lossy().into_owned(),
                    fs::read_to_string(path).unwrap(),
                ));
            }
        }
    }
    let mut output = Vec::new();
    visit(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/noteskins"),
        &mut output,
    );
    output.sort_by(|a, b| a.0.cmp(&b.0));
    output
}

#[test]
fn matching_preserves_offsets_unicode_and_malformed_delimiters() {
    for content in [
        "",
        "()",
        "prefix(x(y)) trailing",
        "é🙂{東京{a}z}",
        "(()",
        ")((",
        "'quoted(')",
        "«a«b»c»",
        "{{}}",
        "same",
    ] {
        for (open, close) in [('(', ')'), ('{', '}'), ('«', '»'), ('a', 'a')] {
            for offset in (0..=content.len() + 2).chain([usize::MAX]) {
                let expected = legacy_find_matching(content, offset, open, close);
                assert_eq!(
                    find_matching(content, offset, open, close),
                    expected,
                    "{content:?}, {offset}"
                );
                assert_eq!(
                    crate::lua::itg_find_matching(content, offset, open, close),
                    expected
                );
            }
        }
    }
}

#[test]
fn comment_removal_matches_legacy_quotes_long_comments_and_utf8() {
    let pieces = [
        "",
        "x=1",
        "é🙂東京",
        "\r\n",
        "-- hi\n",
        "--[not long\n",
        "--[[abc\ndef]]",
        "--[=[é\n東京]=]",
        "--[==[open",
        "'-- literal'",
        "\"escaped \\\" -- literal\"",
        "--[[]]",
        "-",
        "--",
    ];
    for a in pieces {
        for b in pieces {
            for c in pieces {
                let input = format!("{a}{b}{c}");
                assert_eq!(
                    &*strip_lua_comments(&input),
                    legacy_strip_lua_comments(&input),
                    "{input:?}"
                );
            }
        }
    }
}

#[test]
fn scoped_commands_match_legacy_aliases_colors_quotes_and_invalid_calls() {
    let (context, scope) = command_fixture();
    for body in [
        "",
        "nothing to emit",
        "self:",
        "self:x(",
        "self:x() : y( 1 , , 2 )",
        "self:diffuse(TINT):diffuse(color('#123456')):diffuse('#ff000080')",
        "self:diffuse('1, 0.5, 0, 1'):queuecommand('Ready'):zoom((3/2))",
        "self:x({1,2,[3]=4}):y(math.max(1, 2)) self:z(-1)",
        "self:queuecommand('東京'):x(12); self:x(0.25):unknown(variable)",
    ] {
        assert_eq!(
            parse_self_chain_commands_scoped(body, &context, &scope),
            legacy_parse_self_chain_commands_scoped(body, &context, &scope),
            "{body}"
        );
    }
    for count in [1, 16, 128] {
        let body = chain_fixture(count);
        assert_eq!(
            parse_self_chain_commands_scoped(&body, &context, &scope),
            legacy_parse_self_chain_commands_scoped(&body, &context, &scope)
        );
    }
}

#[test]
fn bundled_noteskin_comment_text_matches_legacy() {
    let files = corpus();
    assert!(files.len() >= 100);
    for (path, content) in files {
        assert_eq!(
            &*strip_lua_comments(&content),
            legacy_strip_lua_comments(&content),
            "{path}"
        );
    }
}

#[test]
fn unchanged_scripts_and_borrowed_arguments_have_no_heap_churn() {
    let (context, scope) = command_fixture();
    let input = "local texture='--literal'; self:x(12):queuecommand('東京')";
    crate::perf::assert_no_churn(|| {
        let stripped = strip_lua_comments(black_box(input));
        assert!(matches!(stripped, Cow::Borrowed(_)));
        for argument in [" TINT ", "12", "'Ready'", "missing_variable"] {
            assert!(matches!(
                context.resolve_command_arg(argument, &scope),
                Cow::Borrowed(_)
            ));
        }
        assert_eq!(find_matching("é(x)", 2, '(', ')'), Some(4));
    });
}

#[test]
fn scoped_command_output_uses_one_allocation_without_growth() {
    let (context, scope) = command_fixture();
    let input = chain_fixture(128);
    crate::perf::assert_churn_budget(1, input.len(), || {
        black_box(parse_self_chain_commands_scoped(&input, &context, &scope));
    });
    crate::perf::assert_no_churn(|| {
        assert!(parse_self_chain_commands_scoped("no commands", &context, &scope).is_none());
    });
}

#[test]
fn commented_scripts_keep_one_bounded_output_allocation() {
    let input = "local a=1 -- line\nlocal b='-- literal' --[=[long\n東京]=]\n".repeat(64);
    crate::perf::assert_churn_budget(1, input.len(), || {
        black_box(strip_lua_comments(&input));
    });
}

#[test]
#[ignore = "manual old/new parser benchmark; run in release"]
fn noteskin_parsing_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for count in [16, 256, 2048] {
        let input = "self:x(1):y(2):zoom(0.5)\n".repeat(count);
        let positions: Vec<_> = input.match_indices('(').map(|(i, _)| i).collect();
        for old in order {
            let suffix = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("matching_{count}_{suffix}"),
                if count > 256 { 2 } else { 16 },
                positions.len(),
                || {
                    let mut sum = 0usize;
                    for &position in black_box(&positions) {
                        let close = if old {
                            legacy_find_matching(black_box(&input), position, '(', ')')
                        } else {
                            find_matching(black_box(&input), position, '(', ')')
                        };
                        sum = sum.wrapping_add(close.unwrap_or(0));
                    }
                    sum
                },
            );
        }
    }
    let (context, scope) = command_fixture();
    for count in [1, 16, 128] {
        let body = chain_fixture(count);
        for old in order {
            let suffix = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("commands_{count}_{suffix}"),
                64,
                count * 5,
                || {
                    if old {
                        legacy_parse_self_chain_commands_scoped(black_box(&body), &context, &scope)
                    } else {
                        parse_self_chain_commands_scoped(black_box(&body), &context, &scope)
                    }
                },
            );
        }
    }
    for (label, input) in [
        ("plain", "local x=1; self:x(2):y(3)\n".repeat(256)),
        ("quoted", "local x='-- literal'; self:x(2)\n".repeat(256)),
        (
            "comments",
            "-- comment\nself:x(2) --[[long\n comment]]\n".repeat(256),
        ),
    ] {
        for old in order {
            let suffix = if old { "old" } else { "new" };
            // Drop both returned variants within the measured operation.
            crate::perf::measure_sampled(
                &format!("comments_{label}_{suffix}"),
                128,
                input.len(),
                || {
                    if old {
                        black_box(legacy_strip_lua_comments(black_box(&input)));
                    } else {
                        black_box(strip_lua_comments(black_box(&input)));
                    }
                },
            );
        }
    }
}

#[test]
#[ignore = "manual full-parser corpus comparison; run baseline and new release executables"]
fn noteskin_corpus_bench() {
    use std::hash::{Hash, Hasher};
    fn hash_value(value: &serde_json::Value, hash: &mut impl Hasher) {
        match value {
            serde_json::Value::Object(map) => {
                let mut entries: Vec<_> = map.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                for (key, value) in entries {
                    key.hash(hash);
                    hash_value(value, hash);
                }
            }
            serde_json::Value::Array(array) => {
                for value in array {
                    hash_value(value, hash);
                }
            }
            _ => value.to_string().hash(hash),
        }
    }
    let files = corpus();
    let metrics = noteskin_itg::IniData::default();
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let mut sprites = 0;
    for (path, content) in &files {
        let declaration = parse_actor_decl(content, &metrics);
        sprites += declaration.sprites.len();
        hash_value(
            &serde_json::to_value(declaration).unwrap_or_else(|e| panic!("{path}: {e}")),
            &mut hash,
        );
    }
    eprintln!(
        "CORPUS files={} sprites={} digest={:016x}",
        files.len(),
        sprites,
        hash.finish()
    );
    crate::perf::measure_sampled("corpus", 4, files.len(), || {
        for (_, content) in &files {
            black_box(parse_actor_decl(black_box(content), &metrics));
        }
    });
    let actors = (0..128).map(|i| format!("Def.Sprite{{ Texture='note.png', InitCommand=function(self) self:x({i}):y(0):zoom(0.5) end }};\n")).collect::<String>();
    crate::perf::measure_sampled("actor_bundle", 16, 128, || {
        parse_actor_decl(black_box(&actors), &metrics)
    });
}

fn legacy_strip_lua_comments(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut out = Vec::with_capacity(content.len());
    let mut idx = 0usize;
    let mut quote = 0u8;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if quote != 0 {
            out.push(byte);
            if byte == b'\\' && idx + 1 < bytes.len() {
                idx += 1;
                out.push(bytes[idx]);
            } else if byte == quote {
                quote = 0;
            }
            idx += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = byte;
            out.push(byte);
            idx += 1;
            continue;
        }
        if byte != b'-' || bytes.get(idx + 1) != Some(&b'-') {
            out.push(byte);
            idx += 1;
            continue;
        }

        let mut body = idx + 2;
        let mut equals = 0usize;
        if bytes.get(body) == Some(&b'[') {
            body += 1;
            while bytes.get(body) == Some(&b'=') {
                equals += 1;
                body += 1;
            }
        }
        let long_comment = bytes.get(body) == Some(&b'[');
        if long_comment {
            idx = body + 1;
            while idx < bytes.len() {
                let equals_end = idx + 1 + equals;
                let closing = bytes[idx] == b']'
                    && equals_end < bytes.len()
                    && bytes[idx + 1..equals_end].iter().all(|byte| *byte == b'=')
                    && bytes[equals_end] == b']';
                if closing {
                    idx += equals + 2;
                    break;
                }
                out.push(if bytes[idx] == b'\n' { b'\n' } else { b' ' });
                idx += 1;
            }
        } else {
            idx += 2;
            while idx < bytes.len() && bytes[idx] != b'\n' {
                out.push(b' ');
                idx += 1;
            }
        }
    }
    String::from_utf8(out).expect("comment removal preserves valid UTF-8")
}

fn legacy_find_matching(content: &str, open_idx: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in content
        .char_indices()
        .skip_while(|(idx, _)| *idx < open_idx)
    {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn legacy_parse_self_chain_commands_scoped(
    body: &str,
    context: &CommandContext,
    scope: &HashMap<String, String>,
) -> Option<String> {
    let mut out = String::new();
    let mut cursor = 0usize;
    while let Some(rel) = body[cursor..].find("self:") {
        let mut name_start = cursor + rel + 5;
        loop {
            let Some((name, args, next)) = parse_lua_method_call(body, name_start) else {
                cursor = name_start;
                break;
            };
            if !out.is_empty() {
                out.push(';');
            }
            out.push_str(name);
            for arg in args {
                out.push(',');
                out.push_str(&legacy_resolve_command_arg(context, arg, scope));
            }
            cursor = next;

            let chain = skip_ws(body, next);
            if body.as_bytes().get(chain).is_some_and(|b| *b == b':') {
                name_start = chain + 1;
                continue;
            }
            break;
        }
    }
    (!out.is_empty()).then_some(out)
}

fn legacy_resolve_command_arg(
    context: &CommandContext,
    raw: &str,
    scope: &HashMap<String, String>,
) -> String {
    let key = raw.trim().trim_matches('"').trim_matches('\'');
    get_ascii_lowercase_from_two(scope, &context.colors, key)
        .cloned()
        .or_else(|| parse_lua_color_expr(raw))
        .unwrap_or_else(|| raw.trim().to_string())
}
