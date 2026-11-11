use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{self as song, SongLuaCompileContext, SongLuaLiveFrame, SongRuntime};
use std::{path::Path, sync::Arc};
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Bool(bool),
    Number(u64),
    String(Vec<u8>),
}
fn num(n: f64) -> Scalar {
    Scalar::Number(if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    })
}
fn compare(r: &mut SongRuntime, source: &str) -> Vec<Scalar> {
    let vm = r.vm_mut();
    let values = vm
        .execute(&compile_named(source.as_bytes(), b"@/arrow-check.lua").unwrap())
        .unwrap_or_else(|e| panic!("rust {source}: {e:?}"))
        .to_vec();
    let a = values
        .into_iter()
        .map(|v| match v {
            Value::Nil => Scalar::Nil,
            Value::Boolean(b) => Scalar::Bool(b),
            Value::Number(n) => num(n),
            Value::String(id) => Scalar::String(vm.string_bytes(id).unwrap().to_vec()),
            v => panic!("scalar {v:?}"),
        })
        .collect::<Vec<_>>();
    reference_test::assert_source_debug("song/tests/rust_arrows.rs::compare", source, &a);
    a
}
struct Pair {
    r: SongRuntime,
}
impl Pair {
    fn new(context: &SongLuaCompileContext) -> Self {
        let mut s = Self {
            r: SongRuntime::new(context).unwrap(),
        };
        s.run("PS=GAMESTATE:GetPlayerState(PLAYER_1);PO=PS:GetCurrentPlayerOptions();OTHER=GAMESTATE:GetPlayerState(PLAYER_2)");
        s
    }
    fn run(&mut self, s: &str) -> Vec<Scalar> {
        compare(&mut self.r, s)
    }
    fn frame(&mut self, beat: f64, seconds: f64, uptime: f64) {
        self.run(&format!("local r=_G[{key:?}];r[{b:?}]={beat};r[{s:?}]={seconds};r.__songlua_uptime_seconds={uptime};for p=1,2 do if r.__songlua_player_positions then r.__songlua_player_positions[p][{b:?}]={beat} end end",key=song::SONG_LUA_RUNTIME_KEY,b=song::SONG_LUA_RUNTIME_BEAT_KEY,s=song::SONG_LUA_RUNTIME_SECONDS_KEY));
    }
    fn query(&mut self, s: &str) -> f32 {
        let v = self.run(s);
        let Scalar::Number(bits) = v[0] else {
            panic!("number")
        };
        f64::from_bits(bits) as f32
    }
}
fn context() -> SongLuaCompileContext {
    let mut c = SongLuaCompileContext::new(".", "arrow migration");
    c.live = true;
    c.players[1].enabled = true;
    c
}
fn expected(actual: f32, value: &serde_json::Value, id: &serde_json::Value) {
    let ok = match value.as_str() {
        Some("nan") => actual.is_nan(),
        Some("infinity") => actual == f32::INFINITY,
        Some("-infinity") => actual == f32::NEG_INFINITY,
        _ => (actual - value.as_f64().unwrap() as f32).abs() <= 0.0001,
    };
    assert!(ok, "{id}: {actual} vs {value}");
}
fn effects(case: &serde_json::Value) -> String {
    case["effects"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| format!("PO:{k}({v});"))
        .collect()
}
#[test]
fn captured_native_x_cases_match_full_float_results() {
    let json: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/itgmania/arrow-x-native-reference-v1.json"
    ))
    .unwrap();
    let rows = json["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 130);
    for row in rows {
        let mut c = context();
        c.style_name = row["style"].as_str().unwrap().into();
        let mut p = Pair::new(&c);
        p.run(&format!("PS=GAMESTATE:GetPlayerState(PLAYER_{});PO=PS:GetCurrentPlayerOptions();{}PO:Cosecant({});PO:ModTimerSetting({});PO:ModTimerOffset({});PO:ModTimerMult({})",row["player"].as_u64().unwrap()+1,effects(row),row["cosecant"],row["timer"],row["timer_offset"],row["timer_mult"]));
        p.frame(
            row["beat"].as_f64().unwrap(),
            row["seconds"].as_f64().unwrap(),
            row["uptime"].as_f64().unwrap(),
        );
        p.run("ArrowEffects.Update()");
        let value = p.query(&format!(
            "return ArrowEffects.GetXPos(PS,{},{})",
            row["column"], row["y"]
        ));
        expected(value, &row["expected_x"], &row["id"]);
    }
}
#[test]
fn captured_native_zoom_and_column_tiny_cases_match() {
    let json: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/itgmania/arrow-zoom-native-reference-v1.json"
    ))
    .unwrap();
    let rows = json["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 53);
    for row in rows {
        let mut c = context();
        if row["column"].as_u64().unwrap() > 4 {
            c.style_name = "double".into();
        }
        let mut p = Pair::new(&c);
        p.run(&effects(row));
        let v = p.query(&format!(
            "return ArrowEffects.GetZoom(PS,{},{})",
            row["y"], row["column"]
        ));
        expected(v, &row["expected"], &row["id"]);
    }
}
#[test]
fn captured_native_y_cache_wave_reverse_and_selected_timer_cases_match() {
    let json: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/itgmania/arrow-y-native-reference-v1.json"
    ))
    .unwrap();
    let rows = json["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 63);
    for row in rows {
        let mut c = context();
        let ypos = row["kind"] == "ypos";
        if ypos {
            c.style_name = row["style"].as_str().unwrap().into();
        }
        let mut p = Pair::new(&c);
        let query = if ypos {
            p.run(&format!("PS=GAMESTATE:GetPlayerState(PLAYER_{});PO=PS:GetCurrentPlayerOptions();{}PO:Mini({});PO:Reverse({});PO:Centered({});PO:Cosecant({});PO:ModTimerSetting({})",row["player"].as_u64().unwrap_or(0)+1,effects(row),row["mini"],row["reverse"],row["centered"],row["cosecant"],row["timer"]));
            let q = format!(
                "return ArrowEffects.GetYPos(PS,{},{},270)",
                row["column"], row["y"]
            );
            p.query(&q);
            if row.get("cache_timer").is_some() {
                p.run("local o=GAMESTATE:GetPlayerState(PLAYER_1);o:GetCurrentPlayerOptions():ModTimerSetting('ModTimerType_Song');ArrowEffects.GetYPos(o,1,0)");
            }
            p.frame(
                row["beat"].as_f64().unwrap(),
                row["seconds"].as_f64().unwrap(),
                row["uptime"].as_f64().unwrap(),
            );
            p.run("ArrowEffects.Update()");
            q
        } else {
            p.run(&format!(
                "PO:Wave({});PO:WavePeriod({});PO:XMod({})",
                row["wave"], row["period"], row["speed"]
            ));
            format!("return ArrowEffects.GetYOffset(PS,1,{})", row["note"])
        };
        let v = p.query(&query);
        expected(v, &row["expected"], &row["id"]);
    }
}
#[test]
fn alpha_preserves_visibility_boundaries_and_nonfinite_far_edge_results() {
    let mut p = Pair::new(&context());
    p.frame(4., 2., 2.);
    p.run(
        r#"local a=ArrowEffects.GetAlpha
 assert(a(PS,1,720)==1 and a(PS,1,721)==-math.huge)
 assert(math.abs(a(PS,1,600,nil,nil,1000,0.5)-0.8)<0.000001)
 assert(a(PS,1,1000,nil,nil,1000,0.5)==0 and a(PS,1,1250,nil,nil,1000,0.5)==-0.5)
 assert(a(PS,1,100,0.5)==0 and a(PS,1,100,0.499)==1)
 PO:Reverse(1);assert(a(PS,1,721)==-math.huge)
 PO:Stealth(0.6);assert(a(PS,1,100)==0 and a(PS,1,-1)==1)
 PO:StealthPastReceptors(true);assert(a(PS,1,-1)==0 and a(PS,1,100,0)==1)
 PO:Stealth(0);PO:Stealth1(0.6);assert(a(PS,1,100)==0 and a(PS,2,100)==1)
 PO:Stealth1(0);PO:Hidden(1);assert(a(PS,1,100)==0 and a(PS,1,161)==1)
 PO:Hidden(0);PO:Sudden(1);assert(a(PS,1,201)==0 and a(PS,1,159)==1)
 return a(PS,1,721),a(PS,1,1250,nil,nil,1000,0.5)
 "#,
    );
    for script in [
        "PO:Tipsy(0.3);PO:TanTipsy(0.2);PO:AttenuateY(0.4);PO:BeatY(0.3)",
        "PO:StealthType(true);PO:Blink(1);PO:RandomVanish(0.3)",
        "PO:Mini(2);PO:Cosecant(true);PO:Hidden(0.4);PO:Sudden(0.2)",
    ] {
        p.run(script);
        for y in [-400., -1., 0., 160., 220., 900.] {
            p.query(&format!(
                "return ArrowEffects.GetAlpha(PS,2,{y},nil,nil,1000,0.3)"
            ));
        }
    }
}
#[test]
fn rotation_uses_per_player_beat_radians_column_offsets_and_hold_policy() {
    let mut p = Pair::new(&context());
    p.frame(99., 2., 2.);
    p.run(&format!("local r=_G[{:?}];r.__songlua_player_positions[1][{:?}]=math.pi;r.__songlua_player_positions[2][{:?}]=-math.pi",song::SONG_LUA_RUNTIME_KEY,song::SONG_LUA_RUNTIME_BEAT_KEY,song::SONG_LUA_RUNTIME_BEAT_KEY));
    p.run(r#"PO:Confusion(1);PO:ConfusionOffset(math.pi/2);PO:ConfusionOffset2(math.pi/4);PO:Dizzy(1)
 assert(math.abs(ArrowEffects.GetRotationZ(PS,math.pi*1.5,false,2)-45)<0.0001)
 assert(math.abs(ArrowEffects.GetRotationZ(PS,math.pi*1.5,true,2)+45)<0.0001)
 PO:DizzyHolds(true);assert(math.abs(ArrowEffects.GetRotationZ(PS,math.pi*1.5,true,2)-45)<0.0001)
 OTHER:GetCurrentPlayerOptions():Confusion(1);assert(math.abs(ArrowEffects.GetRotationZ(OTHER,0,false,1)-180)<0.0001)
 return ArrowEffects.GetRotationX(PS,0,0,7),ArrowEffects.GetRotationX(PS,0,1),ArrowEffects.GetRotationY(PS),ArrowEffects.GetZPos(PS),ArrowEffects.GetGlow(PS)
 "#);
}
#[test]
fn optional_arguments_and_fallible_methods_keep_conversion_and_selection_order() {
    let mut p = Pair::new(&context());
    for expr in [
        "ArrowEffects.GetXPos(nil)",
        "ArrowEffects.GetZoom(nil)",
        "ArrowEffects.GetYOffset(nil,nil,2)",
        "ArrowEffects.GetYPos(nil,nil,2)",
        "ArrowEffects.GetXPos(PS,'2','64')",
    ] {
        p.run(&format!("return {expr}"));
    }
    for expr in [
        "ArrowEffects.GetXPos(PS,{})",
        "ArrowEffects.GetZoom(PS,0,0)",
        "ArrowEffects.GetYPos(PS,99,0)",
        "ArrowEffects.GetAlpha(PS,1,{},0)",
    ] {
        p.run(&format!(
            "local ok=pcall(function()return {expr}end);assert(not ok);return ok"
        ));
    }
    p.run(r#"LOG={};local o=PO
 PO.GetReversePercentForColumn=function(self,c)LOG[#LOG+1]='reverse'..c;return 0.5 end
 FAKE={GetCurrentPlayerOptions=function()LOG[#LOG+1]='options';collectgarbage('collect');return o end}
 local old=GAMESTATE:GetCurrentStyle();old.__songlua_style_key=nil;old.GetName=function()LOG[#LOG+1]='style';collectgarbage('collect');return 'single' end
 ArrowEffects.GetYPos(FAKE,2,100)
 assert(table.concat(LOG,',')=='options,style,options,reverse1')
 return table.concat(LOG,',')
 "#);
    p.run("local style=GAMESTATE:GetCurrentStyle();style.__songlua_style_key=string.char(255);style.GetName=function()return 'double'end;assert(ArrowEffects.GetXPos(PS,8,0)==224)");
    for timer in ["-0.5", "255.9", "'0x1'"] {
        p.run(&format!("PO.__songlua_player_option_state.modtimersetting={timer};return ArrowEffects.GetAlpha(PS,1,0)"));
    }
}
#[test]
fn arrow_effects_accepts_more_than_eight_arguments() {
    let mut runtime = SongRuntime::new(&context()).unwrap();
    let script = b"local ps=GAMESTATE:GetPlayerState(PLAYER_1); return ArrowEffects.GetZoom(ps,0,1,nil,nil,nil,nil,nil,nil,42)==ArrowEffects.GetZoom(ps,0,1)";
    let result = runtime
        .vm_mut()
        .execute(&compile_named(script, b"=arrow-args").unwrap())
        .unwrap();
    assert_eq!(result.first(), Some(&Value::Boolean(true)));
}
#[test]
fn cache_commit_is_atomic_and_selected_options_remain_rooted_across_collection() {
    let mut p = Pair::new(&context());
    p.frame(0.2, 3., 7.);
    p.run(
        r#"PO:Tipsy(1);PO:Beat(1)
 local fake={__songlua_player_option_state={modtimersetting=2,cosecant=true}}
 local ps={GetCurrentPlayerOptions=function()return fake end}
 ArrowEffects.GetGlow(ps);fake=nil;ps=nil;collectgarbage('collect');ArrowEffects.Update()
 return ArrowEffects.GetYPos(PS,1,0),ArrowEffects.GetXPos(PS,1,10)
 "#,
    );
    p.run("OLD=ArrowEffects.GetXPos(PS,1,10);PO.__songlua_player_option_state.beatoffset='invalid';assert(not pcall(ArrowEffects.Update));PO.__songlua_player_option_state.beatoffset=0;assert(ArrowEffects.GetXPos(PS,1,10)==OLD)");
    p.run("ArrowEffects.Update();collectgarbage('collect');return ArrowEffects.GetXPos(PS,1,10),ArrowEffects.GetYPos(PS,1,0)");
}
#[test]
fn public_arrow_table_collects_while_the_private_updater_and_cache_survive() {
    let mut p = Pair::new(&context());
    p.run("SAVED_UPDATE=ArrowEffects.Update;WEAK=setmetatable({ArrowEffects},{__mode='v'})");
    p.run("ArrowEffects=nil");
    p.run("collectgarbage('collect');collectgarbage('collect');assert(WEAK[1]==nil);SAVED_UPDATE();return type(SAVED_UPDATE)");
    p.r.update_arrow_effects().unwrap();
}
#[test]
fn raw_query_clock_tape_is_independent_per_read_and_timer_domain() {
    let mut c = context();
    c.host_clock_inputs = [11., 12., 13., 14., 15., 16.]
        .into_iter()
        .map(|value| song::SongLuaHostClockInput {
            clock: "ArrowEffects.GetXPos".into(),
            source: "/arrow-check.lua".into(),
            line: 1,
            value,
        })
        .collect();
    let mut p = Pair::new(&c);
    p.frame(1., 3., 7.);
    p.run("PO:Drunk(1);PO:TanDrunk(0.3)");
    p.run("return ArrowEffects.GetXPos(PS,1,0),ArrowEffects.GetXPos(PS,1,0)");
    p.run("PO:ModTimerSetting('ModTimerType_Beat');return ArrowEffects.GetXPos(PS,1,0)");
    p.run("PO:ModTimerSetting('ModTimerType_Default');return ArrowEffects.GetXPos(PS,1,0)");
}
fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}
struct Live {
    r: Option<song::SongLiveScene>,
    _dir: tempfile::TempDir,
}
impl Drop for Live {
    fn drop(&mut self) {
        if let Some(s) = &mut self.r {
            let _ = s.end();
        }
    }
}
impl Live {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.lua");
        std::fs::write(
            &path,
            "return Def.ActorFrame{InitCommand=function(self)ROOT=self end}",
        )
        .unwrap();
        let mut c = SongLuaCompileContext::new(dir.path(), "arrow live phase");
        c.live = true;
        c.players[1].enabled = true;
        c.defer_initial_live_update = true;
        let mut r = song::compile_song_lua_layers(
            &[&path],
            0,
            &c,
            Default::default(),
            slots,
            layer,
            |_, _| None,
        )
        .unwrap();
        let mut s = Self {
            r: r.iter_mut().find_map(|o| o.live_scene.take()),
            _dir: dir,
        };
        s.run("PS=GAMESTATE:GetPlayerState(PLAYER_1);PO=PS:GetCurrentPlayerOptions();SONG=PS:GetPlayerOptions('ModsLevel_Song')");
        s
    }
    fn run(&mut self, source: &str) -> Vec<Scalar> {
        let mut runtime = self.r.as_mut().unwrap().runtime_mut();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            compare(&mut runtime, source)
        }));
        drop(runtime);
        match result {
            Ok(value) => value,
            Err(error) => std::panic::resume_unwind(error),
        }
    }
    fn update(&mut self, time: f64, uptime: f64) {
        let mut frame = SongLuaLiveFrame {
            beat: time * 2.,
            music_seconds: time,
            screen_seconds: time,
            delta_seconds: 0.1,
            uptime_seconds: Some(uptime),
            ..Default::default()
        };
        frame.players[0].life = 1.;
        let r = self.r.as_mut().unwrap().update(frame).unwrap();
        reference_test::assert_debug(
            "song/tests/rust_arrows.rs::Live::update",
            &(
                time.to_bits(),
                uptime.to_bits(),
                &r.errors,
                &r.broadcasts,
                &r.player_writes,
            ),
        );
    }
}
#[test]
fn live_cache_refresh_follows_actor_and_health_callbacks_and_ignores_public_update_override() {
    let mut p = Live::new();
    p.run(r#"for _,o in ipairs{PO,SONG}do o:Tipsy(0.5);o:Beat(1)end
 BASE=ArrowEffects.GetYPos(PS,1,0);ROOT:SetUpdateFunction(function()DURING=ArrowEffects.GetYPos(PS,1,0)end)
 ROOT:addcommand('HealthStateChangedMessage',function()HEALTH=ArrowEffects.GetYPos(PS,1,0)end)
 ArrowEffects.Update=function()error('host phase must bypass public override')end
 "#);
    p.update(0.1, 2.);
    p.run("assert(DURING==BASE and HEALTH==BASE);PREV=ArrowEffects.GetYPos(PS,1,0);assert(PREV~=BASE)");
    p.update(0.2, 2.125);
    p.run("assert(DURING==PREV);assert(ArrowEffects.GetYPos(PS,1,0)~=PREV)");
}
#[test]
fn live_raw_frame_clocks_are_consumed_after_success_and_independent_of_queries() {
    let mut p = Live::new();
    p.run("PO:Tipsy(0.5);SONG:Tipsy(0.5);PO:TanTipsy(0.2);SONG:TanTipsy(0.2)");
    for t in [2., 4.] {
        p.r.as_mut()
            .unwrap()
            .set_arrow_clock_inputs([Some(t), None], [Some(t + 0.1), None]);
        p.update(0.1, 7.);
        p.run("return ArrowEffects.GetYPos(PS,1,0),ArrowEffects.GetXPos(PS,1,0)");
    }
    p.update(0.2, 7.);
    p.run("return ArrowEffects.GetYPos(PS,1,0)");
    p.r.as_mut()
        .unwrap()
        .set_arrow_clock_inputs([Some(9.), None], [Some(9.1), None]);
    p.run("ROOT:SetUpdateFunction(function()PO.__songlua_player_option_state.beatoffset='invalid'end)");
    let mut frame = SongLuaLiveFrame {
        beat: 0.6,
        music_seconds: 0.3,
        screen_seconds: 0.3,
        delta_seconds: 0.1,
        uptime_seconds: Some(7.),
        ..Default::default()
    };
    frame.players[0].life = 1.;
    assert!(p.r.as_mut().unwrap().update(frame).is_err());
    p.run("ROOT:SetUpdateFunction(nil);PO.__songlua_player_option_state.beatoffset=0");
    p.update(0.4, 7.);
    p.run("return ArrowEffects.GetYPos(PS,1,0)");
}
#[test]
fn scene_reload_keeps_arrow_bindings_and_saved_option_handles_in_the_same_vm() {
    let mut p = Live::new();
    p.run("SAVED=ArrowEffects;OLD_OPTIONS=PO;PO:Tipsy(0.5);SONG:Tipsy(0.5)");
    p.update(0.1, 3.);
    let mut r = song::reload_song_lua(p.r.take().unwrap(), slots, layer).unwrap();
    p.r = r.iter_mut().find_map(|o| o.live_scene.take());
    p.run("collectgarbage('collect');assert(SAVED==ArrowEffects);PS=GAMESTATE:GetPlayerState(PLAYER_1);PO=PS:GetCurrentPlayerOptions();SONG=PS:GetPlayerOptions('ModsLevel_Song');return PO==OLD_OPTIONS");
    p.update(0.2, 4.);
    p.run("return SAVED.GetYPos(PS,1,0)");
}

#[test]
fn preparation_and_live_yoffset_keep_pause_speed_ramps_origin_and_music_rate() {
    use deadsync_rules::timing::{
        DelaySegment, SpeedSegment, SpeedUnit, StopSegment, TimingData, TimingSegments,
    };
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-actors/pause-offset.json"
    ))
    .unwrap();
    let mut checked = 0;
    for map in reference["maps"].as_array().unwrap() {
        let name = map["name"].as_str().unwrap();
        let timing = TimingData::from_segments(
            0.125,
            0.,
            &TimingSegments {
                bpms: vec![(0., 120.)],
                stops: if name == "stop" {
                    vec![StopSegment {
                        beat: 4.,
                        duration: 1.,
                    }]
                } else {
                    vec![]
                },
                delays: if name == "delay" {
                    vec![DelaySegment {
                        beat: 4.,
                        duration: 1.,
                    }]
                } else {
                    vec![]
                },
                speeds: vec![
                    SpeedSegment {
                        beat: 0.,
                        ratio: 0.5,
                        delay: 0.,
                        unit: SpeedUnit::Beats,
                    },
                    SpeedSegment {
                        beat: 4.,
                        ratio: 2.,
                        delay: 2.,
                        unit: SpeedUnit::Seconds,
                    },
                ],
                ..Default::default()
            },
            &[],
        );
        for live in [false, true] {
            for rate in [0.5_f32, 0.75, 1., 1.25, 1.5, 2.] {
                let mut c = context();
                c.live = live;
                c.song_music_rate = rate;
                c.song_timing = Some(timing.clone());
                c.player_timing = [Some(timing.clone()), Some(timing.clone())];
                let mut p = Pair::new(&c);
                for sample in map["samples"].as_array().unwrap() {
                    let elapsed = sample["elapsed"].as_f64().unwrap() as f32;
                    let beat = sample["beat"].as_f64().unwrap() as f32;
                    let seconds = if live {
                        f64::from(elapsed + timing.get_time_for_beat_exact(0.))
                    } else {
                        f64::from(elapsed) / f64::from(rate)
                    };
                    p.frame(f64::from(beat), seconds, 0.);
                    for player in ["nil", "PS", "OTHER"] {
                        let v = p.query(&format!(
                            "return ArrowEffects.GetYOffset({player},1,{})",
                            sample["note"]
                        ));
                        assert_eq!(
                            v.to_bits(),
                            (sample["offset"].as_f64().unwrap() as f32).to_bits(),
                            "{name} live={live} rate={rate} {sample}"
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 100);
}

#[test]
fn cmod_and_reentrant_option_getters_keep_the_existing_selection_and_wave_order() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    let timing = TimingData::from_segments(
        0.125,
        0.,
        &TimingSegments {
            bpms: vec![(0., 120.)],
            ..Default::default()
        },
        &[],
    );
    for live in [false, true] {
        let mut c = context();
        c.live = live;
        c.song_timing = Some(timing.clone());
        c.player_timing = [Some(timing.clone()), None];
        c.song_music_rate = 1.5;
        let mut p = Pair::new(&c);
        p.frame(2., 1., 1.);
        for script in [
            "PO:CMod(300);PO:Wave(.4);PO:WavePeriod(.2)",
            "PO:MMod(350)",
            "PO:AMod(250)",
            "PO:XMod(2)",
        ] {
            p.run(script);
            for note in [-3., 0., 2., 4., 12.] {
                p.query(&format!("return ArrowEffects.GetYOffset(PS,1,{note})"));
                p.query(&format!("return ArrowEffects.GetYOffset(OTHER,1,{note})"));
            }
        }
        p.run(r#"LOG={};local original=PO;local count=0
            FAKE={GetPlayerNumber=function()LOG[#LOG+1]='player';return PLAYER_1 end,
            GetCurrentPlayerOptions=function()count=count+1;LOG[#LOG+1]='options'..count;collectgarbage('collect');return original end}
            ArrowEffects.GetYOffset(FAKE,1,8);assert(#LOG>=4);return table.concat(LOG,',')
        "#);
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
