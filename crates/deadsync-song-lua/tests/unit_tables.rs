//! Original table and timing units on the Rust interpreter and native host.
use deadlib_lua::{Error, HostCall, Value};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};
use std::path::Path;
#[path = "support/baked_program.rs"]
mod api;

fn runtime() -> RustSongRuntime {
    RustSongRuntime::new(&SongLuaCompileContext::new(".", "table unit fixture")).unwrap()
}
fn eval(host: &mut RustSongRuntime, source: &str) -> Value {
    api::eval(host.vm_mut(), source)[0]
}
fn field(host: &mut RustSongRuntime, owner: Value, name: &str) -> Value {
    api::get(host.vm_mut(), owner, name.as_bytes())
}
fn index(host: &mut RustSongRuntime, owner: Value, index: usize) -> Value {
    let Value::Table(owner) = owner else {
        panic!("fixture table expected")
    };
    host.vm_mut()
        .raw_table_get(owner, Value::Number(index as f64))
        .unwrap()
}
fn number(value: Value) -> f32 {
    api::as_number(value) as f32
}
fn text(host: &mut RustSongRuntime, value: Value) -> String {
    String::from_utf8(api::bytes(host.vm_mut(), value)).unwrap()
}
fn length(host: &mut RustSongRuntime, value: Value) -> usize {
    let Value::Table(value) = value else {
        panic!("fixture table expected")
    };
    host.vm_mut().raw_table_len(value).unwrap()
}
fn invoke(host: &mut RustSongRuntime, owner: Value, name: &str, args: &[Value]) -> Value {
    let function = field(host, owner, name);
    api::call_one(host.vm_mut(), function, args)
}
fn timing_runtime(bpms: [f32; 2], data: &deadsync_rules::timing::TimingData) -> RustSongRuntime {
    let mut context = SongLuaCompileContext::new(".", "table timing unit fixture");
    context.song_display_bpms = bpms;
    context.song_timing = Some(data.clone());
    RustSongRuntime::new(&context).unwrap()
}
fn fixture_table(
    host: &mut RustSongRuntime,
    callback: fn(&mut HostCall<'_>) -> Result<(), Error>,
    args: &[Value],
) -> Value {
    let vm = host.vm_mut();
    let callback = vm.new_host_function(callback, &[]).unwrap();
    api::call_one(vm, callback, args)
}

#[test]
fn split_table_handles_empty_separator() {
    let mut host = runtime();
    let table = eval(&mut host, "return string.split('abc', '')");
    let value = index(&mut host, table, 1);
    assert_eq!(text(&mut host, value), "a");
    let value = index(&mut host, table, 2);
    assert_eq!(text(&mut host, value), "b");
    let value = index(&mut host, table, 3);
    assert_eq!(text(&mut host, value), "c");
}

#[test]
fn range_table_counts_down_when_needed() {
    let mut host = runtime();
    let table = eval(&mut host, "return range(3,1)");
    assert!(
        matches!(table, Value::Table(_)),
        "range should return a table"
    );
    assert_eq!(number(index(&mut host, table, 1)), 3.);
    assert_eq!(number(index(&mut host, table, 2)), 2.);
    assert_eq!(number(index(&mut host, table, 3)), 1.);
}

#[test]
fn rotate_lua_table_wraps_indices() {
    let mut host = runtime();
    let rotated = eval(&mut host, "return table.rotate_right({'a','b','c'},1)");
    let value = index(&mut host, rotated, 1);
    assert_eq!(text(&mut host, value), "c");
    let value = index(&mut host, rotated, 2);
    assert_eq!(text(&mut host, value), "a");
    let value = index(&mut host, rotated, 3);
    assert_eq!(text(&mut host, value), "b");
}

#[test]
fn lua_table_to_string_reports_sequence_length() {
    let mut host = runtime();
    test_support::install_base_table_helpers(&mut host).unwrap();
    let value = eval(&mut host, "return TableToString({'a','b'},'Rows')");
    assert_eq!(text(&mut host, value), "Rows = {...2 item(s)}");
}

#[test]
fn gameplay_layout_places_reverse_slots() {
    let mut context = SongLuaCompileContext::new(".", "layout unit");
    context.screen_height = 480.;
    let mut host = RustSongRuntime::new(&context).unwrap();
    let layout = eval(&mut host, "return GetGameplayLayout(nil,true)");
    let combo = field(&mut host, layout, "Combo");
    let error_bar = field(&mut host, layout, "ErrorBar");
    assert_eq!(number(field(&mut host, combo, "y")), 210.0);
    assert_eq!(number(field(&mut host, error_bar, "y")), 270.0);
    assert_eq!(number(field(&mut host, error_bar, "maxHeight")), 30.0);
}

#[test]
fn author_table_deduplicates_step_methods() {
    let mut host = runtime();
    let authors = eval(
        &mut host,
        "local steps={};for _,method in ipairs{'GetDescription','GetAuthorCredit','GetChartName'} do steps[method]=function() return 'author' end end;return getAuthorTable(steps)",
    );
    assert_eq!(length(&mut host, authors), 1);
    let value = index(&mut host, authors, 1);
    assert_eq!(text(&mut host, value), "author");
}

#[test]
fn ex_judgment_counts_defaults_to_zero() {
    let mut host = runtime();
    let counts = eval(&mut host, "return GetExJudgmentCounts()");
    assert_eq!(number(field(&mut host, counts, "W1")), 0.);
    assert_eq!(number(field(&mut host, counts, "Miss")), 0.);
    assert_eq!(number(field(&mut host, counts, "totalRolls")), 0.);
}

#[test]
fn radar_values_default_to_zero() {
    let mut host = runtime();
    let radar = eval(
        &mut host,
        "return GAMESTATE:GetCurrentSteps(PLAYER_1):GetRadarValues()",
    );
    let value = invoke(&mut host, radar, "GetValue", &[]);
    assert_eq!(number(value), 0.0);
}

#[test]
fn song_group_table_has_zero_sync_offset() {
    let mut host = runtime();
    let group = eval(&mut host, "return SONGMAN:GetGroup()");
    let value = invoke(&mut host, group, "GetSyncOffset", &[]);
    assert_eq!(number(value), 0.0);
}

#[test]
fn single_value_array_wraps_table() {
    fn make(call: &mut HostCall<'_>) -> Result<(), Error> {
        let input = call.arguments()[0];
        let value = test_support::fixture_single_value_array(call, input)?;
        call.push(value)
    }
    let mut host = runtime();
    let value = eval(&mut host, "return {name='song'}");
    let array = fixture_table(&mut host, make, &[value]);
    assert_eq!(length(&mut host, array), 1);
    let first = index(&mut host, array, 1);
    let name = field(&mut host, first, "name");
    assert_eq!(text(&mut host, name), "song");
}

#[test]
fn string_method_returns_fixed_value() {
    fn make(call: &mut HostCall<'_>) -> Result<(), Error> {
        let table = call.try_new_table()?;
        call.with_roots(&[Value::Table(table)], |call| {
            test_support::fixture_string_method(call, table, "GetName", "single")?;
            call.push(Value::Table(table))
        })
    }
    let mut host = runtime();
    let table = fixture_table(&mut host, make, &[]);
    let value = invoke(&mut host, table, "GetName", &[]);
    assert_eq!(text(&mut host, value), "single");
}

#[test]
fn path_methods_report_path_presence() {
    fn make(call: &mut HostCall<'_>) -> Result<(), Error> {
        let table = call.try_new_table()?;
        call.with_roots(&[Value::Table(table)], |call| {
            test_support::fixture_path_methods(
                call,
                table,
                "GetPath",
                "HasPath",
                Some(Path::new("songs\\pack\\song.ogg")),
            )?;
            call.push(Value::Table(table))
        })
    }
    let mut host = runtime();
    let table = fixture_table(&mut host, make, &[]);
    let value = invoke(&mut host, table, "GetPath", &[]);
    assert_eq!(text(&mut host, value), "songs/pack/song.ogg");
    let value = invoke(&mut host, table, "HasPath", &[]);
    assert!(api::as_bool(value));
}

#[test]
fn native_float_timing_queries_preserve_separate_offset_inputs() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    let mut timing = TimingData::from_segments(
        0.01,
        0.0,
        &TimingSegments {
            bpms: vec![(0.0, 130.0)],
            ..Default::default()
        },
        &[],
    );
    timing.shift_group_offset_seconds(-0.009);
    timing.set_global_offset_seconds(-0.008);
    let mut host = timing_runtime([130.0, 130.0], &timing);
    let table = eval(
        &mut host,
        "return GAMESTATE:GetCurrentSong():GetTimingData()",
    );
    for (method, input, expected) in [
        // C++ float order: (-song_offset - group_offset) - global_offset.
        ("GetElapsedTimeFromBeat", 0.0_f32, 0.026999998837709427_f32),
        (
            "GetBeatFromElapsedTime",
            0.027732426300644875_f32,
            0.0015869262861087918_f32,
        ),
    ] {
        let value = number(invoke(
            &mut host,
            table,
            method,
            &[table, Value::Number(input as f64)],
        ));
        assert_eq!(value.to_bits(), expected.to_bits(), "native {method}");
    }
}

#[test]
fn timing_getters_match_native() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/itgmania-song-lua-micro");
    let options = deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
    let song = deadsync_simfile::song::parse_song_meta_file(
        &root.join("timing-data.ssc"),
        &options,
        0.0,
        |_| 0.0,
    )
    .expect("parse native timing control");
    let payload = deadsync_simfile::cache::load_gameplay_charts_with_options(
        &song,
        &[0, 1],
        &deadsync_simfile::cache::GameplayChartLoadOptions {
            cache_dir: &root,
            parse_options: &options,
            allow_cache_read: false,
            allow_cache_write: false,
            verify_cache_freshness: true,
            global_offset_seconds: 0.0,
        },
        |_| 0.0,
    )
    .expect("load both native timing owners");
    let native: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("timing-data-native.json")).expect("native timing values"),
    )
    .expect("native timing JSON");

    for (name, data) in [
        ("Song", song.song_timing.as_ref().expect("song timing")),
        ("Chart1", &payload.charts[0].timing),
        ("Chart2", &payload.charts[1].timing),
    ] {
        let mut host = timing_runtime([60.0, 60.0], data);
        let timing = eval(
            &mut host,
            "return GAMESTATE:GetCurrentSong():GetTimingData()",
        );
        for (method, arg, key) in [
            ("GetElapsedTimeFromBeat", 0.0, "Actor.x"),
            ("GetElapsedTimeFromBeat", 6.0, "Actor.y"),
            ("GetBeatFromElapsedTime", 3.0, "Actor.z"),
            ("GetBPMAtBeat", 6.0, "Actor.aux"),
            ("GetBPMAtBeat", 3.995, "Actor.rotationx"),
        ] {
            let value = number(invoke(
                &mut host,
                timing,
                method,
                &[timing, Value::Number(arg)],
            ));
            let expected = native[name][key].as_f64().expect("native number") as f32;
            assert!(
                (value - expected).abs() < 0.000_01,
                "{name}.{method}: {value} vs {expected}"
            );
        }
        let get = field(&mut host, timing, "GetBPMsAndTimes");
        let rows = api::call_one(host.vm_mut(), get, &[timing, Value::Number(0.)]);
        assert_eq!(
            length(&mut host, rows) as f64,
            native[name]["Actor.zoomy"].as_f64().unwrap()
        );
        assert_eq!(
            {
                let row = index(&mut host, rows, 1);
                number(index(&mut host, row, 2))
            },
            native[name]["Actor.zoomx"].as_f64().unwrap() as f32
        );
        let strings = api::call_one(host.vm_mut(), get, &[timing]);
        assert!(matches!(index(&mut host, strings, 1), Value::String(_)));
        let Value::Table(rows_id) = rows else {
            unreachable!()
        };
        host.vm_mut()
            .raw_table_set(rows_id, Value::Number(1.), Value::Nil)
            .unwrap();
        assert!({
            let fresh = api::call_one(host.vm_mut(), get, &[timing, Value::Boolean(true)]);
            matches!(index(&mut host, fresh, 1), Value::Table(_))
        });
    }
}

#[test]
fn timing_table_exposes_bpm_policy() {
    let data = deadsync_rules::timing::TimingData::from_segments(
        0.0,
        0.0,
        &deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 120.0), (4.0, 180.0)],
            ..Default::default()
        },
        &[],
    );
    let mut host = timing_runtime([120.0, 180.0], &data);
    let timing = eval(
        &mut host,
        "return GAMESTATE:GetCurrentSong():GetTimingData()",
    );
    let get_bpms = field(&mut host, timing, "GetBPMs");
    let get_timed_bpms = field(&mut host, timing, "GetBPMsAndTimes");
    let has_bpm_changes = field(&mut host, timing, "HasBPMChanges");
    let bpms = api::call_one(host.vm_mut(), get_bpms, &[]);
    let timed_bpms = api::call_one(
        host.vm_mut(),
        get_timed_bpms,
        &[timing, Value::Boolean(true)],
    );
    let timed_bpm_strings = api::call_one(
        host.vm_mut(),
        get_timed_bpms,
        &[timing, Value::Boolean(false)],
    );

    assert_eq!(number(index(&mut host, bpms, 1)), 120.0);
    assert_eq!(number(index(&mut host, bpms, 2)), 180.0);
    let second = index(&mut host, timed_bpms, 2);
    assert_eq!(number(index(&mut host, second, 1)), 4.0);
    assert_eq!(number(index(&mut host, second, 2)), 180.0);
    assert_eq!(
        {
            let value = index(&mut host, timed_bpm_strings, 1);
            text(&mut host, value)
        },
        "0.000000=120.000000"
    );
    assert!(api::call_bool(host.vm_mut(), has_bpm_changes, &[]));
}

#[test]
fn style_table_exposes_column_info() {
    let mut context = SongLuaCompileContext::new(".", "style table unit");
    context.style_name = "double".into();
    let mut host = RustSongRuntime::new(&context).unwrap();
    let style = eval(&mut host, "return GAMESTATE:GetCurrentStyle()");
    let info = invoke(
        &mut host,
        style,
        "GetColumnInfo",
        &[style, Value::Number(1.)],
    );
    let value = invoke(&mut host, style, "ColumnsPerPlayer", &[]);
    assert_eq!(number(value), 8.);
    let value = field(&mut host, info, "Name");
    assert_eq!(text(&mut host, value), "Left");
    assert_eq!(number(field(&mut host, info, "Track")), 0.);
}

#[test]
fn display_bpms_for_args_uses_table_and_rate() {
    let mut host = runtime();
    let steps = eval(
        &mut host,
        "local bpms={100,200};return {GetDisplayBpms=function() return bpms end}",
    );
    let function = host.vm_mut().get_global(b"GetDisplayBPMs").unwrap();
    let bpms = api::call_one(
        host.vm_mut(),
        function,
        &[Value::Nil, steps, Value::Number(1.5)],
    );
    assert_eq!(
        [
            number(index(&mut host, bpms, 1)),
            number(index(&mut host, bpms, 2))
        ],
        [150.0, 300.0]
    );
    // The public helper publishes the rate through its scaled BPM values.
    assert_eq!(number(index(&mut host, bpms, 1)) / 100.0, 1.5);
}
