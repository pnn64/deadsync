use super::*;

#[allow(dead_code, unused_imports)]
#[path = "query_encode_before.rs"]
mod before;

#[test]
fn components_keep_percent_spelling_utf8_and_all_ascii_bytes() {
    assert_eq!(url_encode_component("a b+c/%\0"), "a%20b%2Bc%2F%25%00");
    assert_eq!(url_encode_component("é曲"), "%C3%A9%E6%9B%B2");
    assert_eq!(url_encode_component("AZaz09-._~"), "AZaz09-._~");
    for byte in 0..=127 {
        let input = char::from(byte).to_string();
        assert_eq!(
            url_encode_component(&input),
            before::url_encode_component(&input)
        );
    }
    for input in [
        String::new(),
        "😀é曲\0% /".repeat(1024),
        "aZ0-._~".repeat(1024),
    ] {
        assert_eq!(
            url_encode_component(&input),
            before::url_encode_component(&input)
        );
    }
    for scalar in (0..=0x10ffff).step_by(127).filter_map(char::from_u32) {
        let input = scalar.to_string();
        assert_eq!(
            url_encode_component(&input),
            before::url_encode_component(&input)
        );
    }
}

#[test]
fn queries_keep_sorting_lossy_strings_coercions_and_raw_iteration() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.raw_set("b", "two words").unwrap();
    table.raw_set("a", 1).unwrap();
    assert_eq!(
        encode_query_params(table.clone()).unwrap(),
        "a=1&b=two%20words"
    );
    table.raw_set("1", "string").unwrap();
    table.raw_set(1, "number").unwrap();
    table.raw_set(true, false).unwrap();
    table.raw_set("true", true).unwrap();
    table.raw_set("", "").unwrap();
    table.raw_set("a=", "&").unwrap();
    table
        .raw_set(
            lua.create_string([0xff, b'a']).unwrap(),
            lua.create_string([0xfe, 0]).unwrap(),
        )
        .unwrap();
    let unsupported = lua
        .load("return setmetatable({}, {__tostring=function() error('must not stringify') end})")
        .eval::<Table>()
        .unwrap();
    table.raw_set(unsupported.clone(), "skipped").unwrap();
    table.raw_set("unsupported", unsupported).unwrap();
    let meta = lua.load("return {__pairs=function() error('must iterate raw entries') end, __index=function() error('must not index') end}").eval::<Table>().unwrap();
    table.set_metatable(Some(meta)).unwrap();
    let entries = table.clone().pairs::<Value, Value>().count();
    lua.gc_collect().unwrap();
    let output = encode_query_params(table.clone()).unwrap();
    assert_eq!(output, before::encode_query_params(table.clone()).unwrap());
    assert!(output.contains("%EF%BF%BDa=%EF%BF%BD%00"));
    assert!(output.contains("unsupported="));
    let parts: Vec<_> = output.split('&').collect();
    assert!(parts.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(entries, table.pairs::<Value, Value>().count());

    for value in [
        Value::Nil,
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Integer(i64::MIN),
        Value::Integer(i64::MAX),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::Number(f64::NEG_INFINITY),
        Value::Number(1.23456789012345e-200),
        Value::String(lua.create_string([0xff, 0, b'a']).unwrap()),
        Value::Table(lua.create_table().unwrap()),
    ] {
        let expected = before::query_value_text(value.clone());
        assert_eq!(query_value_text(value.clone()), expected);
        with_query_value_text(value.clone(), |text| assert_eq!(text, expected.as_deref()));
        let query = lua.create_table().unwrap();
        query.raw_set("value", value).unwrap();
        assert_eq!(
            encode_query_params(query.clone()).unwrap(),
            before::encode_query_params(query).unwrap()
        );
    }
    assert_eq!(
        encode_query_params(lua.create_table().unwrap()).unwrap(),
        ""
    );
}

#[test]
fn encode_callback_keeps_receiver_dispatch_and_input_tables() {
    for current in [false, true] {
        let lua = Lua::new();
        let network = if current {
            create_network_table(&lua)
        } else {
            before::create_network_table(&lua)
        }
        .unwrap();
        lua.globals().set("NETWORK", network).unwrap();
        lua.load(
            r#"
            local query = {b='two words', a=1}
            assert(NETWORK:EncodeQueryParameters(query) == 'a=1&b=two%20words')
            assert(NETWORK.EncodeQueryParameters(NETWORK, query) == 'a=1&b=two%20words')
            assert(NETWORK.EncodeQueryParameters(query) == '')
            assert(NETWORK:EncodeQueryParameters() == '')
            assert(NETWORK.EncodeQueryParameters() == '')
            assert(NETWORK:EncodeQueryParameters(false) == '')
            assert(NETWORK:EncodeQueryParameters('a=1') == '')
            assert(query.a == 1 and query.b == 'two words')
            collectgarbage('collect')
            assert(NETWORK:EncodeQueryParameters(query) == 'a=1&b=two%20words')
        "#,
        )
        .exec()
        .unwrap();
    }
}

#[test]
fn query_allocations_remove_growth_components_and_owned_scalar_text() {
    use crate::draw_capture::perf::allocations;
    use std::hint::black_box;
    for input in [
        "a b+c/%".to_owned(),
        " /%".repeat(1024),
        "abcd".repeat(2048),
    ] {
        let (output, churn) = allocations::measure(|| url_encode_component(black_box(&input)));
        assert_eq!(output, before::url_encode_component(&input));
        assert_eq!(churn.allocs, 1);
        assert_eq!(churn.reallocs, 0);
        assert_eq!(churn.allocated_bytes, output.len());
    }
    let lua = Lua::new();
    for value in [
        Value::String(lua.create_string("value").unwrap()),
        Value::Integer(i64::MIN),
        Value::Boolean(true),
    ] {
        black_box(before::query_value_text(value.clone()));
        with_query_value_text(value.clone(), |text| {
            black_box(text);
        });
        let (_, prior) = allocations::measure(|| {
            black_box(before::query_value_text(value.clone()));
        });
        let (_, current) = allocations::measure(|| {
            with_query_value_text(value.clone(), |text| {
                black_box(text);
            })
        });
        assert_eq!(prior.allocs, 1);
        assert_eq!(current, allocations::Churn::default());
    }
    for strings in [false, true] {
        let query = query_fixture(&lua, 1, strings);
        black_box(before::encode_query_params(query.clone()).unwrap());
        black_box(encode_query_params(query.clone()).unwrap());
        let (old, prior) =
            allocations::measure(|| before::encode_query_params(query.clone()).unwrap());
        let (new, current) = allocations::measure(|| encode_query_params(query.clone()).unwrap());
        assert_eq!(new, old);
        assert_eq!(prior.allocs, current.allocs + 4);
        assert_eq!(current.reallocs, 0);
        assert!(current.allocated_bytes < prior.allocated_bytes);
    }
}

fn query_fixture(lua: &Lua, count: usize, strings: bool) -> Table {
    let query = lua.create_table().unwrap();
    for index in 0..count {
        if strings {
            query
                .raw_set(format!("key {index}"), format!("value/{index}"))
                .unwrap();
        } else {
            query.raw_set(index, index + 1).unwrap();
        }
    }
    query
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_query_encode() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    fn measure<W: FnMut()>(
        name: &str,
        current: bool,
        unit: &str,
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
            serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},"samples":samples,"unit":unit,"baseline_commit":"11cd0e266"})
        );
    }
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, input, count) in [
        ("url_empty", String::new(), 20000),
        ("url_unreserved_short", "abc-._~".to_owned(), 10000),
        ("url_escaped_short", "a b+c/%".to_owned(), 10000),
        ("url_unicode", "é曲".to_owned(), 10000),
        ("url_nul", "a\0b".to_owned(), 10000),
        ("url_unreserved_long", "abcdefgh".repeat(1024), 512),
        ("url_escaped_long", " /%".repeat(1024), 512),
        ("url_mixed_long", "abcé曲 /%".repeat(512), 512),
    ] {
        for current in variants {
            measure(name, current, "url_components", count, || {
                let input = input.clone();
                let encode = if current {
                    url_encode_component
                } else {
                    before::url_encode_component
                };
                black_box(encode(&input));
                move || {
                    black_box(encode(black_box(&input)));
                }
            });
        }
    }
    for (name, kind) in [
        ("text_string", 0),
        ("text_integer", 1),
        ("text_boolean", 2),
        ("text_number_control", 3),
        ("text_lossy", 4),
    ] {
        for current in variants {
            measure(name, current, "query_value_reads", 10000, || {
                let lua = Lua::new();
                let value = match kind {
                    0 => Value::String(lua.create_string("two words").unwrap()),
                    1 => Value::Integer(i64::MIN),
                    2 => Value::Boolean(true),
                    3 => Value::Number(12.345),
                    _ => Value::String(lua.create_string([0xff; 32]).unwrap()),
                };
                with_query_value_text(value.clone(), |text| {
                    black_box(text);
                });
                black_box(before::query_value_text(value.clone()));
                let fixture = (lua, value);
                move || {
                    let (_, value) = black_box(&fixture);
                    if current {
                        with_query_value_text(value.clone(), |text| {
                            black_box(text);
                        });
                    } else {
                        let text = before::query_value_text(value.clone());
                        black_box(text.as_deref());
                    }
                }
            });
        }
    }
    for (name, count, strings, lossy, api) in [
        ("query_empty", 0, true, false, false),
        ("query_one", 1, true, false, false),
        ("query_strings64", 64, true, false, false),
        ("query_integers64", 64, false, false, false),
        ("query_lossy8", 8, true, true, false),
        ("api_empty", 0, true, false, true),
        ("api_one", 1, true, false, true),
        ("api_strings64", 64, true, false, true),
        ("api_lossy8", 8, true, true, true),
    ] {
        for current in variants {
            measure(
                name,
                current,
                if api { "lua_query_calls" } else { "queries" },
                if count > 8 { 128 } else { 1000 },
                || {
                    let lua = Lua::new();
                    let query = query_fixture(&lua, count, strings);
                    if lossy {
                        for index in 0..count {
                            query
                                .raw_set(
                                    format!("key {index}"),
                                    lua.create_string([0xff, 0, b'a']).unwrap(),
                                )
                                .unwrap();
                        }
                    }
                    let network = if current {
                        create_network_table(&lua)
                    } else {
                        before::create_network_table(&lua)
                    }
                    .unwrap();
                    let callback: mlua::Function = network.get("EncodeQueryParameters").unwrap();
                    let encode = if current {
                        encode_query_params
                    } else {
                        before::encode_query_params
                    };
                    for _ in 0..4 {
                        if api {
                            callback
                                .call::<mlua::String>((network.clone(), query.clone()))
                                .unwrap();
                        } else {
                            black_box(encode(query.clone()).unwrap());
                        }
                    }
                    let fixture = (lua, network, query, callback);
                    move || {
                        let (_, network, query, callback) = black_box(&fixture);
                        if api {
                            black_box(
                                callback
                                    .call::<mlua::String>((network.clone(), query.clone()))
                                    .unwrap(),
                            );
                        } else {
                            black_box(encode(black_box(query.clone())).unwrap());
                        }
                    }
                },
            );
        }
    }
}
