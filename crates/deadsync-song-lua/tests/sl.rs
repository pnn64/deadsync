use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, {SongRuntime, test_support},
};
use std::path::Path;

fn context() -> SongLuaCompileContext {
    SongLuaCompileContext::new(Path::new("."), "SL state migration")
}
fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/sl.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}
fn number(n: f64) -> Scalar {
    Scalar::Number(if n.is_nan() {
        f64::NAN.to_bits()
    } else {
        n.to_bits()
    })
}
struct Pair {
    rust: SongRuntime,
}
impl Pair {
    fn new(context: &SongLuaCompileContext) -> Self {
        Self {
            rust: SongRuntime::new(context).unwrap(),
        }
    }
    fn compare(&mut self, source: &str) {
        let actual: Vec<_> = execute(&mut self.rust, source)
            .into_iter()
            .map(|v| match v {
                Value::Nil => Scalar::Nil,
                Value::Boolean(b) => Scalar::Boolean(b),
                Value::Number(n) => number(n),
                Value::String(id) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(id).unwrap().to_vec())
                }
                other => panic!("scalar expected: {other:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_sl.rs::Pair::compare",
            source,
            &actual,
        );
    }
}

const SNAPSHOT: &str = r#"
function snapshot(v)
    local kind=type(v)
    if kind=='table' then
        local entries={}
        for k,value in pairs(v) do entries[#entries+1]=snapshot(k)..'='..snapshot(value) end
        table.sort(entries);return '{'..table.concat(entries,';')..'}'
    elseif kind=='string' then return 's'..#v..':'..v
    elseif kind=='number' then return v~=v and 'nan' or string.format('%.17g',v)
    elseif kind=='boolean' then return tostring(v)
    else return kind end
end
"#;

#[test]
fn complete_sl_state_preserves_context_values_and_independent_player_tables() {
    use song::SongLuaSpeedMod::*;
    for (i, filter) in [0, 50, 75, 95, 13, 100, 255].into_iter().enumerate() {
        let mut context = context();
        context.song_music_rate = [1.2345679, 0., -1., f32::NAN, f32::INFINITY, 2., 0.75][i];
        context.players[0].speedmod = [X(1.25), C(400.), M(650.), A(321.5)][i % 4];
        context.players[0].background_filter_percent = filter;
        context.players[0].step_statistics_enabled = i % 2 == 0;
        context.players[0].timing_windows = [false, true, false, true, true];
        context.players[0].noteskin_name = "custom skin".into();
        context.players[0].life_meter_type = "Battery".into();
        context.players[0].hide_lifebar = true;
        context.players[1].column_cues = true;
        context.players[1].nps_graph_at_top = true;
        context.players[1].hide_danger = true;
        let mut p = Pair::new(&context);
        p.compare(SNAPSHOT);
        p.compare(
            "return snapshot(SL),SL.SRPG9.GetLogo(),select('#',SL.SRPG9.MaybeRandomizeColor())",
        );
        p.compare(r#"
            assert(SL.P1.Stages~=SL.P2.Stages and SL.Global.Stages~=SL.P1.Stages)
            assert(SL.P1.Stages.Stats[1].column_judgments[1]~=SL.P1.Stages.Stats[1].column_judgments[2])
            SL.P1.Stages.Stats[1].column_judgments[1].Early.W1=17
            SL.P1.ActiveModifiers.TimingWindows[1]=true
            SL.JudgmentColors.Casual[1][1]=.123
            return snapshot(SL.P2),snapshot(SL.Global.Stages),snapshot(SL.JudgmentColors.ITG)
        "#);
    }
}

#[test]
fn mode_tables_match_the_frozen_simply_love_fixture_and_theme_queries() {
    let fixture =
        include_str!("../../../tests/fixtures/itgmania-song-lua-micro/simply-love-mode-tables.lua");
    for rescore in [false, true] {
        let context = context();
        let mut p = Pair::new(&context);
        p.compare(&format!("ThemePrefs.Set('RescoreEarlyHits',{rescore})"));

        test_support::rebuild_sl(&mut p.rust, &context).unwrap();
        p.compare(SNAPSHOT);
        p.compare(&format!("expected=(function()\n{fixture}\nend)();assert(snapshot(SL.Preferences)==snapshot(expected.Preferences));assert(snapshot(SL.Metrics)==snapshot(expected.Metrics));return snapshot(SL.Preferences),snapshot(SL.Metrics)"));
        p.compare("return THEME:GetMetric('ScreenGameplay','MinSecondsToStep'),THEME:GetMetric('ScoreKeeperNormal','PercentScoreWeightW1')");
        p.compare("ThemePrefs.Set('RescoreEarlyHits',not ThemePrefs.Get('RescoreEarlyHits'));return SL.Preferences.ITG.MinTNSToScoreNotes,SL.Preferences['FA+'].MinTNSToScoreNotes");
    }
}

#[test]
fn sl_initialization_preserves_getter_order_collection_and_failure_atomicity() {
    let context = context();
    let mut p = Pair::new(&context);
    p.compare(
        r#"
        reads={};prior=SL
        ThemePrefs=setmetatable({}, {__index=function(t,k)
            assert(k=='Get');reads[#reads+1]='lookup';collectgarbage('collect')
            return function(name)reads[#reads+1]=name;collectgarbage('collect');return 0 end
        end})
    "#,
    );

    test_support::rebuild_sl(&mut p.rust, &context).unwrap();
    p.compare("return table.concat(reads,'|'),SL~=prior,SL.Preferences.ITG.MinTNSToScoreNotes,SL.Preferences['FA+'].MinTNSToScoreNotes,#SL.P1.Stages.Stats[1].column_judgments");
    for value in [
        "false",
        "{Get={}}",
        "{Get=function()collectgarbage('collect');error('failed')end}",
    ] {
        p.compare(&format!("prior=SL;ThemePrefs={value}"));

        assert!(test_support::rebuild_sl(&mut p.rust, &context).is_err());
        p.compare("return SL==prior,SL.Global.GameMode");
    }
}

#[test]
fn preference_defaults_storage_case_and_return_counts_match_production() {
    let mut p = Pair::new(&context());
    for name in song::THEME_PREF_ROW_NAMES.iter().copied().chain([
        "ThemeFont",
        "SongSelectBG",
        "ResultsBG",
        "unknown",
        "USEIMAGECACHE",
    ]) {
        p.compare(&format!(
            "return ThemePrefs.Get({name:?}),ThemePrefs:Get({:?})",
            name.to_ascii_uppercase()
        ));
    }
    p.compare(r#"
        local value={nested={7}}
        assert(select('#',ThemePrefs.Set('VisualStyle',value))==0)
        assert(ThemePrefs:Get('VisualStyle')==value)
        local saved=ThemePrefs.Get
        assert(select('#',ThemePrefs.Save())==0)
        assert(select('#',ThemePrefs.Set('unknown',false))==0)
        ThemePrefs.Set('VisualStyle',nil)
        return saved('VisualStyle'),saved('visualstyle'),saved('unknown'),__songlua_side_effect_count
    "#);
    for key in ["nil", "12", "false", "{}", "string.char(255)"] {
        p.compare(&format!("return ThemePrefs.Get({key}),select('#',ThemePrefs.Set({key},13)),__songlua_side_effect_count"));
    }
    p.compare("__songlua_side_effect_count={};local ok=pcall(ThemePrefs.Set,'x',5);__songlua_side_effect_count=0;return ok,ThemePrefs.Get('x')");
}

#[test]
fn custom_preference_definitions_are_fresh_and_init_all_preserves_stored_values() {
    let mut p = Pair::new(&context());
    p.compare(SNAPSHOT);
    p.compare(r#"
        local a,b=SL_CustomPrefs.Get(),SL_CustomPrefs.Get()
        assert(a~=b and a.VisualStyle~=b.VisualStyle and a.VisualStyle.Choices~=b.VisualStyle.Choices)
        local original=snapshot(a)
        a.VisualStyle.Default='custom';a.VisualStyle.Choices[1]='changed'
        ThemePrefs.InitAll(a)
        ThemePrefs:InitAll(b)
        SL_CustomPrefs.Validate();SL_CustomPrefs.Init()
        return original,snapshot(b),ThemePrefs.Get('VisualStyle'),__songlua_side_effect_count
    "#);
    p.compare("ThemePrefs.Set('VisualStyle',nil);return ThemePrefs.Get('VisualStyle')");
}

#[test]
fn init_all_handles_reentrant_defaults_typed_pairs_and_partial_errors() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        ThemePrefs.InitAll({x=setmetatable({}, {__index=function(t,k)
            assert(k=='Default');ThemePrefs.Set('y',4);collectgarbage('collect');return {nested=9}
        end})})
        assert(ThemePrefs.Get('x').nested==9)
        ThemePrefs.InitAll({x=setmetatable({}, {__index=function()error('must not read stored default')end})})
        ThemePrefs.InitAll({z=setmetatable({}, {__index=function()ThemePrefs.Set('before_error',8);error('ignored default')end})})
        ThemePrefs.InitAll({[17]={Default=false}})
        return ThemePrefs.Get('y'),ThemePrefs.Get('z'),ThemePrefs.Get('17'),ThemePrefs.Get('before_error'),__songlua_side_effect_count
    "#);
    for args in [
        "nil",
        "false",
        "{x=false}",
        "{[true]={Default=9}}",
        "{[string.char(255)]={Default=4}}",
        "ThemePrefs",
        "{q={Default=0}}",
    ] {
        p.compare(&format!("local ok=pcall(ThemePrefs.InitAll,{args});return ok,ThemePrefs.Get('q'),__songlua_side_effect_count"));
    }
    p.compare("return select('#',ThemePrefs:InitAll({dot={Default=3}})),ThemePrefs.Get('dot')");
}

#[test]
fn chart_info_preserves_stream_identity_defaults_and_player_argument_rules() {
    let mut p = Pair::new(&context());
    p.compare(SNAPSHOT);
    p.compare(
        r#"
        local streams=SL.P1.Streams
        streams.NotesPerMeasure=false;streams.PeakNPS=false;streams.Hash=12
        assert(ParseChartInfo(nil,'P1')==streams)
        return snapshot(streams),__songlua_side_effect_count
    "#,
    );
    for player in [
        "'P1'",
        "'p2'",
        "'PlayerNumber_P2'",
        "0",
        "1",
        "1.5",
        "'1'",
        "false",
        "{}",
    ] {
        p.compare(&format!(r#"
            seen={{}};local function stat(self,pn)seen[#seen+1]=pn;return {{pn=pn}} end
            local steps={{GetNotesPerMeasure=stat,GetNpsPerMeasure=stat,GetPeakNps=function(self,pn)seen[#seen+1]=pn;return 17.25 end}}
            local result=ParseChartInfo(steps,{player})
            return table.concat(seen,','),snapshot(result),result==SL.P2.Streams,__songlua_side_effect_count
        "#));
    }
    for malformed in ["SL=false", "SL={P1=false}", "SL={P1={Streams=false}}"] {
        p.compare(&format!("{malformed};local ok=pcall(ParseChartInfo,nil,'P1');return ok,__songlua_side_effect_count"));
    }
    p.compare("SL=nil;local s=ParseChartInfo();return s==SL.P1.Streams,snapshot(s)");
}

#[test]
fn chart_callbacks_reenter_collect_replace_globals_and_keep_partial_mutations() {
    let mut p = Pair::new(&context());
    p.compare(SNAPSHOT);
    p.compare(r#"
        local streams=SL.P2.Streams;seen={}
        local steps=setmetatable({}, {__index=function(self,k)
            seen[#seen+1]='get:'..k;collectgarbage('collect')
            return function(self,pn)
                seen[#seen+1]=k..':'..pn
                if k=='GetNotesPerMeasure' then SL=nil;ParseChartInfo(nil,'P1');collectgarbage('collect');return {8} end
                if k=='GetNpsPerMeasure' then return {16} end
                return 32
            end
        end})
        local result=ParseChartInfo(steps,'P2')
        return result==streams,SL.P2==nil,snapshot(result),table.concat(seen,'|'),__songlua_side_effect_count
    "#);
    p.compare(
        r#"
        local ok=pcall(ParseChartInfo,{GetNotesPerMeasure=function()return {19}end,
            GetNpsPerMeasure=function()collectgarbage('collect');error('failure')end},'P1')
        return ok,SL.P1.Streams.NotesPerMeasure[1],SL.P1.Streams.PeakNPS,__songlua_side_effect_count
    "#,
    );
    p.compare("local ok=pcall(ParseChartInfo,{GetNotesPerMeasure=setmetatable({}, {__call=function()return {}end})},'P1');return ok,__songlua_side_effect_count");
}

#[test]
fn column_cues_follow_current_state_and_mutate_the_returned_columns() {
    let mut p = Pair::new(&context());
    p.compare(SNAPSHOT);
    p.compare(r#"
        local cues={{columns={{noteType=4,x=1},{noteType=1},{noteType=nil}}}}
        local steps={GetColumnCues=function(self,minimum)assert(minimum==2.25);collectgarbage('collect');return cues end}
        SL.Global.ColumnCueMinTime=2.25
        assert(select('#',ParseColumnCues(steps,'P2'))==0)
        return SL.P2.Streams.ColumnCues==cues,snapshot(cues),__songlua_side_effect_count
    "#);
    p.compare("local ok=pcall(ParseColumnCues,{GetColumnCues=function()return {{columns=false}}end},'P1');return ok");
}

#[test]
fn retained_queries_release_discarded_managers_and_states_without_crossing_runtimes() {
    let mut first = SongRuntime::new(&context()).unwrap();
    let mut second = SongRuntime::new(&context()).unwrap();
    execute(
        &mut first,
        r#"
        local weak=setmetatable({}, {__mode='v'})
        weak.prefs=ThemePrefs;weak.sl=SL
        local value={x=11};weak.value=value
        ThemePrefs.Set('value',value);query=ThemePrefs.Get;parser=ParseChartInfo
        ThemePrefs=nil;SL=nil;value=nil;collectgarbage('collect');collectgarbage('collect')
        assert(weak.prefs==nil and weak.sl==nil and query('value').x==11)
        assert(parser(nil,'P2')==SL.P2.Streams)
        query=nil;collectgarbage('collect');collectgarbage('collect');assert(weak.value==nil)
    "#,
    );
    execute(
        &mut second,
        "assert(ThemePrefs.Get('value')==false and SL.Global.GameMode=='ITG' and __songlua_side_effect_count==0)",
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
