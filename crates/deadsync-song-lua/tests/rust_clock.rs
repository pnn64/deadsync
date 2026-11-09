use deadlib_lua::{Value, compile};
use deadsync_rules::timing::{DelaySegment, StopSegment, TimingData, TimingSegments, WarpSegment};
use deadsync_song_lua::{LUA_PLAYERS, SongLuaCompileContext, rust_engine::RustSongRuntime};

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}

struct Pair {
    rust: RustSongRuntime,
}

impl Pair {
    fn new(context: &SongLuaCompileContext) -> Self {
        let mut rust = RustSongRuntime::new(context).unwrap();
        let position = rust.song_position().unwrap();
        rust.vm_mut().set_global(b"position", position).unwrap();
        for index in 0..LUA_PLAYERS {
            if let Some(position) = rust.player_position(index).unwrap() {
                let name = format!("player{}", index + 1);
                rust.vm_mut().set_global(name.as_bytes(), position).unwrap();
            }
        }
        Self { rust }
    }

    fn compare(&mut self, script: &str) -> Vec<Scalar> {
        let program = compile(script.as_bytes()).unwrap();
        let values = self.rust.vm_mut().execute(&program).unwrap().to_vec();
        let actual: Vec<_> = values
            .into_iter()
            .map(|value| match value {
                Value::Nil => Scalar::Nil,
                Value::Boolean(value) => Scalar::Boolean(value),
                Value::Number(value) => Scalar::Number(value.to_bits()),
                Value::String(id) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(id).unwrap().to_vec())
                }
                other => panic!("expected a scalar, got {other:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_clock.rs::Pair::compare",
            script,
            &actual,
        );
        actual
    }

    fn snapshot(&mut self) {
        self.compare("return position:GetSongBeat(), position:GetSongBeatVisible(), position:GetMusicSeconds(), position:GetMusicSecondsVisible(), position:GetCurBPS(), position:GetFreeze(), position:GetDelay()");
        let actual = self.rust.values().unwrap();
        reference_test::assert_debug(
            "song/tests/rust_clock.rs::Pair::snapshot::values",
            &(actual.0.to_bits(), actual.1.to_bits()),
        );
        let actual = self.rust.delta_values().unwrap();
        reference_test::assert_debug(
            "song/tests/rust_clock.rs::Pair::snapshot::delta_values",
            &(actual.0.to_bits(), actual.1.to_bits()),
        );
    }

    fn beat(&mut self, beat: f32) {
        self.rust.set_beat(beat).unwrap();
        self.snapshot();
    }
}

fn context() -> SongLuaCompileContext {
    SongLuaCompileContext::new(".", "Rust clock regression")
}

#[test]
fn replay_music_time_preserves_native_float_rounding_rate_and_origin() {
    for (origin, rate) in [(0.0, 1.0), (1.25, 2.0), (-0.5, 0.5), (0.0, 1.7)] {
        let mut context = context();
        context.song_music_rate = rate;
        context.song_timing = Some(TimingData::from_segments(
            origin,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..Default::default()
            },
            &[],
        ));
        let mut pair = Pair::new(&context);
        for seconds in [
            0.0,
            -0.0,
            1e-9,
            -1e-9,
            1.0 / 60.0,
            0.123456789123,
            2.0,
            1e15,
        ] {
            // Keep the input as a Lua double until the actual native-float boundary.
            let script = format!(
                "__songlua_compile_song_runtime.__songlua_music_seconds = {seconds:?}; return position:GetMusicSeconds()"
            );
            let actual = pair.compare(&script);
            let expected = (seconds * f64::from(rate)) as f32 + origin;
            assert_eq!(actual, vec![Scalar::Number(f64::from(expected).to_bits())]);
        }
        pair.compare("local r=__songlua_compile_song_runtime; r.__songlua_music_seconds='0.125'; r.__songlua_music_rate='2.5'; tonumber=function() error('replacement must not run') end; return position:GetMusicSeconds()");
    }
}

#[test]
fn bpm_segments_deltas_and_backward_seeks_match_legacy() {
    let mut context = context();
    context.song_music_rate = 1.5;
    context.song_timing_bpms = vec![(0.0, 60.0), (4.0, 120.0), (10.0, 90.0)];
    let mut pair = Pair::new(&context);
    pair.snapshot();
    for beat in [-2.0, 0.0, 3.999, 4.0, 9.0, 10.0, 20.0, -1.0, 2.0] {
        pair.beat(beat);
    }
    for (beat, seconds) in [(0.5, 1.0 / 60.0), (-0.0, 1e-9), (-2.0, -3.5)] {
        pair.rust.set_values(beat, seconds).unwrap();

        pair.rust.set_delta_values(beat, seconds).unwrap();
        pair.snapshot();
    }

    pair.rust.reset().unwrap();
    pair.snapshot();
}

#[test]
fn native_timing_stops_delays_warps_and_seeks_match_legacy() {
    let mut context = context();
    context.song_music_rate = 1.25;
    context.song_timing = Some(TimingData::from_segments(
        0.75,
        0.125,
        &TimingSegments {
            bpms: vec![(0.0, 120.0), (8.0, 90.0)],
            stops: vec![StopSegment {
                beat: 4.0,
                duration: 0.75,
            }],
            delays: vec![DelaySegment {
                beat: 6.0,
                duration: 0.25,
            }],
            warps: vec![WarpSegment {
                beat: 10.0,
                length: 2.0,
            }],
            ..Default::default()
        },
        &[],
    ));
    let mut pair = Pair::new(&context);
    for beat in [
        -4.0, 0.0, 3.9999, 4.0, 4.0001, 5.9999, 6.0, 6.0001, 8.0, 10.0, 11.0, 12.0, 20.0, 4.0, 0.0,
    ] {
        pair.beat(beat);
    }
}

#[test]
fn live_players_keep_separate_positions_and_reset_without_losing_identity() {
    let mut context = context();
    context.live = true;
    context.song_music_rate = 2.0;
    context.song_timing = Some(TimingData::from_segments(
        1.25,
        0.0,
        &TimingSegments {
            bpms: vec![(0.0, 90.0)],
            ..Default::default()
        },
        &[],
    ));
    let mut pair = Pair::new(&context);
    pair.snapshot();
    let values = pair.compare("local r=__songlua_compile_song_runtime; r.__songlua_music_seconds=3.25; r.__songlua_song_beat=7; local p=r.__songlua_player_positions; p[1].__songlua_song_beat=5; p[1].__songlua_music_seconds=2; p[2].__songlua_song_beat=9; p[2].__songlua_freeze=true; saved=player1.GetMusicSeconds; collectgarbage('collect'); return position:GetMusicSeconds(), player1:GetMusicSeconds(), player2:GetSongBeat(), player2:GetFreeze()");
    assert_eq!(
        values,
        vec![
            Scalar::Number(3.25f64.to_bits()),
            Scalar::Number(2.0f64.to_bits()),
            Scalar::Number(9.0f64.to_bits()),
            Scalar::Boolean(true)
        ]
    );

    pair.rust.set_live(false);
    pair.compare("return saved(), position:GetMusicSeconds()");

    pair.rust.set_live(true);

    pair.rust.reset().unwrap();
    pair.snapshot();
    pair.compare("return saved==player1.GetMusicSeconds, saved(), player1:GetCurBPS(), player2:GetSongBeat(), player2:GetFreeze()");
    let mut separate = Pair::new(&context);
    separate.compare("return position:GetSongBeat(), position:GetCurBPS(), player1:GetSongBeat()");
}

#[test]
fn getters_preserve_metamethod_coercion_and_callback_captures_under_gc() {
    let mut pair = Pair::new(&context());
    pair.compare(
        r#"
        local r=__songlua_compile_song_runtime
        r.__songlua_song_beat=nil
        marker={}
        setmetatable(r,{__index=function(_,key)
            collectgarbage('collect')
            if fail then error(marker) end
            if key=='__songlua_song_beat' then return '1.23456789123' end
        end})
        fail=false
        local value=position:GetSongBeat()
        __songlua_compile_song_runtime={__songlua_song_beat=123,__songlua_music_seconds=456}
        collectgarbage('collect')
        return value,position:GetSongBeat()
    "#,
    );
    let b = pair.rust.values().unwrap();
    assert_eq!(b, (123.0, 456.0));
    pair.compare("fail=true; return pcall(function() position:GetSongBeat() end)==false");
}

#[test]
fn lua_error_identity_is_preserved_by_host_getters() {
    // Direct Lua lookup and the Rust host getter must preserve the same error object.
    let script = r#"
        local r=__songlua_compile_song_runtime
        local marker={}
        r.__songlua_song_beat=nil
        setmetatable(r,{__index=function()
            collectgarbage('collect')
            error(marker)
        end})
        local direct_ok,direct_error=pcall(function() return r.__songlua_song_beat end)
        local host_ok,host_error=pcall(position.GetSongBeat)
        return direct_ok,direct_error==marker,host_ok,host_error==marker,type(host_error)
    "#;
    let mut pair = Pair::new(&context());
    let actual = pair
        .rust
        .vm_mut()
        .execute(&compile(script.as_bytes()).unwrap())
        .unwrap()
        .to_vec();
    assert_eq!(
        &actual[..4],
        &[
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Boolean(false),
            Value::Boolean(true)
        ]
    );
    let Value::String(kind) = actual[4] else {
        panic!("error type")
    };
    assert_eq!(pair.rust.vm_mut().string_bytes(kind).unwrap(), b"table");
}

#[test]
fn temporary_player_tables_remain_rooted_during_reentrant_reset_setters() {
    let mut context = context();
    context.live = true;
    let mut pair = Pair::new(&context);
    pair.compare(
        r#"
        local r=__songlua_compile_song_runtime
        writes={}
        local p=setmetatable({}, {__newindex=function(self,key,value)
            r.__songlua_player_positions=nil
            collectgarbage('collect')
            writes[#writes+1]=key
            rawset(self,key,value)
        end})
        r.__songlua_player_positions={p}
        return true
    "#,
    );

    pair.rust.reset().unwrap();
    assert_eq!(
        pair.compare("return #writes, table.concat(writes,','), position:GetSongBeat()")[0],
        Scalar::Number(5.0f64.to_bits())
    );
}

#[test]
fn generated_bpm_tables_and_invalid_numeric_fields_follow_legacy_behavior() {
    let mut pair = Pair::new(&context());
    pair.compare(
        r#"
        local r=__songlua_compile_song_runtime
        r.__songlua_timing_bpms=nil
        setmetatable(r,{__index=function(_,key)
            collectgarbage('collect')
            if key=='__songlua_timing_bpms' then return {{'0','120','0'},{'8','60','4'}} end
        end})
        return true
    "#,
    );
    for beat in [0.0, 4.0, 8.0, 16.0] {
        pair.beat(beat);
    }
    pair.compare("__songlua_compile_song_runtime.__songlua_song_beat={}; return pcall(position.GetSongBeat)==false");
    assert!(pair.rust.values().is_err());
    pair.compare("__songlua_compile_song_runtime=nil; return position:GetCurBPS()");
    assert!(pair.rust.set_beat(2.0).is_err());
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
