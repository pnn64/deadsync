use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};
use std::path::Path;

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
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/screen-ui.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
struct Pair {
    rust: RustSongRuntime,
}
impl Pair {
    fn new() -> Self {
        let context = SongLuaCompileContext::new(Path::new("."), "screen UI migration");
        let rust = RustSongRuntime::new(&context).unwrap();
        let mut pair = Self { rust };
        pair.compare(SETUP);
        pair
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
                v => panic!("scalar expected {v:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_screen_ui.rs::Pair::compare",
            source,
            &actual,
        );
    }
}

const SETUP: &str = r#"
__songlua_side_effect_count=0
function snapshot(v)
    if type(v)~='table' then return type(v)=='function' and '<function>' or tostring(v) end
    local keys={};for k in pairs(v)do keys[#keys+1]=k end
    table.sort(keys,function(a,b)return tostring(a)<tostring(b)end)
    local out={};for _,k in ipairs(keys)do out[#out+1]=tostring(k)..'='..snapshot(v[k])end
    return '{'..table.concat(out,',')..'}'
end
"#;

const CUSTOM: &str = "SpeedModType SpeedMod Mini Spacing NoteSkin JudgmentGraphic HoldJudgment HeldGraphic HeldMissGraphic ComboFont NoteSkinVariant BackgroundFilter NoteFieldOffsetX NoteFieldOffsetY VisualDelay MusicRate StepChart ScreenAfterPlayerOptions ScreenAfterPlayerOptions2 ScreenAfterPlayerOptions3 ScreenAfterPlayerOptions4 Hide GameplayExtras GameplayExtrasB GameplayExtrasC ResultsExtras LifeMeterType DataVisualizations TargetScore TargetScoreNumber ActionOnMissedTarget TiltMultiplier ErrorBar ErrorBarTrim ErrorBarOptions ErrorBarCap MeasureCounter MeasureCounterOptions MeasureCounterLookAhead MeasureLines TimingWindowOptions TimingWindows FAPlus MinIndicator MiniIndicator MiniIndicatorColor StepStatsInfo JudgmentFlash ScoreBoxOptions StepStatsExtra FunOptions LifeBarOptions ComboColors ComboMode TimerMode JudgmentAnimation RailBalance ExtraAesthetics";

#[test]
fn all_option_factories_match_native_metadata_and_create_independent_rows() {
    let mut p = Pair::new();
    p.compare("return snapshot(ThemePrefsRows),snapshot(OperatorMenuOptionRows),snapshot(getmetatable(OperatorMenuOptionRows))");
    for name in CUSTOM.split_whitespace() {
        for name in [
            name.to_owned(),
            name.to_ascii_lowercase(),
            name.to_ascii_uppercase(),
        ] {
            p.compare(&format!(r#"
                local row=CustomOptionRow('{name}');assert(type(row)=='table')
                assert(row~=CustomOptionRow('{name}') and row.Choices~=CustomOptionRow('{name}').Choices)
                assert(row:EnabledForPlayers()~=row:EnabledForPlayers())
                return snapshot(row),snapshot(row:EnabledForPlayers())
            "#));
        }
    }
    for name in song::THEME_PREF_ROW_NAMES
        .iter()
        .copied()
        .chain(["Unknown", ""])
    {
        p.compare(&format!("return snapshot(ThemePrefsRows:GetRow('{name}')),snapshot(ThemePrefsRows.GetRow('{name}'))"));
    }
    for name in [
        "ConfAspectRatio",
        "ConfDisplayResolution",
        "ConfDisplayMode",
        "ConfRefreshRate",
        "ConfFullscreenType",
    ] {
        p.compare(&format!(
            "assert({name}()~={name}());return snapshot({name}())"
        ));
    }
    for name in [
        "Theme",
        "EditorNoteskin",
        "DefaultFailType",
        "LongAndMarathonTime",
        "MusicWheelSpeed",
        "VideoRenderer",
        "GlobalOffsetSeconds",
        "VisualDelaySeconds",
        "MemoryCards",
        "CustomSongsMaxSeconds",
        "CustomSongsMaxMegabytes",
        "CustomSongsLoadTimeout",
        "Unknown",
        "theme",
        "longandmarathontime",
    ] {
        for value in [
            "nil",
            "'Marathon'",
            "'LONG'",
            "false",
            "17",
            "{}",
            "string.char(255)",
        ] {
            p.compare(&format!("return snapshot(OperatorMenuOptionRows:{name}({value})),snapshot(OperatorMenuOptionRows.{name}({value}))"));
        }
    }
    p.compare(r#"
        assert(OperatorMenuOptionRows.Unknown~=OperatorMenuOptionRows.Unknown)
        assert(rawget(OperatorMenuOptionRows,'Unknown')==nil)
        return snapshot(OperatorMenuOptionRows[17]()),snapshot(OperatorMenuOptionRows[false]()),
            snapshot(OperatorMenuOptionRows[string.char(255)]()),snapshot(ThemePrefsRows:GetRow(false))
    "#);
    for value in [
        "nil",
        "false",
        "true",
        "0",
        "{}",
        "function()end",
        "string.char(255)",
        "'unknown'",
        "' Mini '",
    ] {
        p.compare(&format!(
            "return CustomOptionRow({value}),ScreenString({value})"
        ));
    }
}

#[test]
fn load_selections_uses_captured_count_raw_lists_and_native_argument_forms() {
    let mut p = Pair::new();
    for list in [
        "{}",
        "{false,false}",
        "{false,true}",
        "{false,0}",
        "{[8]=true}",
        "{false,'yes'}",
    ] {
        p.compare(&format!(r#"
            local row=CustomOptionRow('Mini');local list={list}
            setmetatable(list,{{__index=function()error('raw read')end,__newindex=function()error('raw write')end}})
            row.Choices={{}};assert(row:LoadSelections(list)==list)
            local direct={list};assert(row.LoadSelections(direct,1)==direct)
            return snapshot(list),snapshot(direct),snapshot(row.LoadSelections()),snapshot(row.LoadSelections(false,1))
        "#));
    }
    p.compare(
        r#"
        local row=CustomOptionRow('Mini');assert(row.LoadSelections(row)==row and row[1]==true)
        local list={false,false,true};local fresh=row.LoadSelections('ignored',list,1)
        assert(fresh~=list and fresh[1]==true and list[1]==false and list[3]==true)
        local other={};assert(row.LoadSelections({},other)==other)
        return snapshot(list),snapshot(other),snapshot(fresh)
    "#,
    );
}

#[test]
fn save_selections_publishes_preferences_and_player_modifiers_with_existing_policies() {
    let mut p = Pair::new();
    for name in CUSTOM.split_whitespace() {
        p.compare(&format!(r#"
            local row=CustomOptionRow('{name}');local a={{false,true,false,true}};__songlua_side_effect_count=0
            assert(select('#',row:SaveSelections(a,'P2'))==0)
            assert(select('#',row.SaveSelections({{true}},'P1'))==0)
            return snapshot(SL.Global.ActiveModifiers),snapshot(SL.P1.ActiveModifiers),snapshot(SL.P2.ActiveModifiers),__songlua_side_effect_count
        "#));
    }
    for factory in [
        "ConfAspectRatio()",
        "ConfDisplayResolution()",
        "ConfDisplayMode()",
        "ConfRefreshRate()",
        "ConfFullscreenType()",
        "OperatorMenuOptionRows:MusicWheelSpeed()",
        "OperatorMenuOptionRows:LongAndMarathonTime('Marathon')",
        "OperatorMenuOptionRows:MemoryCards()",
        "OperatorMenuOptionRows:Unknown()",
    ] {
        p.compare(&format!(r#"
            local row={factory};log={{}};__songlua_side_effect_count=0
            PREFSMAN={{SetPreference=function(self,k,v)log[#log+1]={{k,v}};collectgarbage('collect');return {{}} end}}
            row:SaveSelections({{false,true}});row.SaveSelections({{false,false,false}})
            return snapshot(log),__songlua_side_effect_count
        "#));
    }
    p.compare(
        r#"
        local row=ThemePrefsRows:GetRow('CasualMaxMeter');row:SaveSelections({false,false,true})
        assert(ThemePrefs:Get('CasualMaxMeter')==7)
        return ThemePrefs:Get('CasualMaxMeter'),__songlua_side_effect_count
    "#,
    );
}

#[test]
fn save_uses_current_fields_and_callbacks_with_rooted_values_and_partial_errors() {
    let mut p = Pair::new();
    p.compare(r#"
        row=ConfRefreshRate();local owner=row;local log={};local value={marker=19}
        row.Values={value};weak=setmetatable({value},{__mode='v'});value=nil
        PREFSMAN=setmetatable({}, {__index=function(self,key)
            row.Values={};collectgarbage('collect');return function(self,k,v)
                assert(v==weak[1]);log[#log+1]={k,v.marker};collectgarbage('collect')
            end
        end})
        row.SaveSelections({},{});assert(weak[1]~=nil)
        collectgarbage('collect');assert(weak[1]==nil)
        row.Values=nil;row.Choices=nil
        setmetatable(row,{__index=function(self,key)
            collectgarbage('collect');log[#log+1]=key
            if key=='Values'then return false elseif key=='Choices'then return {'fallback'}end
        end})
        PREFSMAN={SetPreference=function(self,k,v)log[#log+1]=v end};row:SaveSelections({})
        __songlua_side_effect_count=false;assert(not pcall(row.SaveSelections,row,{}))
        __songlua_side_effect_count=0;PREFSMAN={SetPreference=setmetatable({},{__call=function()error('must be function')end})}
        assert(not pcall(row.SaveSelections,row,{}));assert(__songlua_side_effect_count==1)
        return snapshot(log),__songlua_side_effect_count
    "#);
    p.compare(r#"
        local row=CustomOptionRow('Hide');local log={}
        row.Choices={'Targets','Combo',false,17,'Judgments'}
        local original=SL.P2.ActiveModifiers;SL.P2.ActiveModifiers=setmetatable({}, {__newindex=function(t,k,v)
            collectgarbage('collect');log[#log+1]={k,v}
            if k=='HideCombo'then error('stop after first')end;rawset(t,k,v)
        end})
        __songlua_side_effect_count=0;assert(not pcall(row.SaveSelections,row,{true,false,true},'P2'))
        local partial=SL.P2.ActiveModifiers.HideTargets
        SL.P2.ActiveModifiers=original;row:SaveSelections({true,false,true},'P2')
        return snapshot(log),partial,SL.P2.ActiveModifiers.HideTargets,SL.P2.ActiveModifiers.HideCombo,__songlua_side_effect_count
    "#);
    p.compare(r#"
        local row=CustomOptionRow('Mini');row.Values={10,20,30,40};row.Values[4]=nil;row.Values[3]=nil
        local a={false,false,false,true};row:SaveSelections(a)
        local absent=SL.P1.ActiveModifiers.Mini==nil
        row.Values=false;row.Choices={};row:SaveSelections({});assert(SL.P1.ActiveModifiers.Mini==nil)
        row.SelectType={};assert(not pcall(row.SaveSelections,row,{}))
        return absent,__songlua_side_effect_count
    "#);
}

#[test]
fn theme_row_initialization_forwards_exact_arguments_to_current_global_method() {
    let mut p = Pair::new();
    p.compare(r#"
        ThemePrefsRows.InitAll();assert(ThemePrefs:Get('CasualMaxMeter')==12)
        local custom={X={Default=81}};ThemePrefsRows:InitAll(custom);assert(ThemePrefs:Get('X')==81)
        local log={};ThemePrefs={InitAll=function(...)assert(select('#',...)==1);collectgarbage('collect');log[#log+1]=snapshot(...)end}
        SL_CustomPrefs={Get=function()error('must use internal definitions')end}
        ThemePrefsRows.InitAll(custom);ThemePrefsRows:InitAll(custom);ThemePrefsRows.InitAll(nil)
        ThemePrefsRows.InitAll(false);ThemePrefsRows.InitAll('x');ThemePrefsRows:InitAll()
        return snapshot(log)
    "#);
    p.compare(
        "ThemePrefs={InitAll={}};local ok=pcall(ThemePrefsRows.InitAll);assert(not ok);return ok",
    );
}

#[test]
fn screen_routing_background_fit_and_centering_use_existing_dynamic_policies() {
    let mut p = Pair::new();
    p.compare(r#"
        local out={};for k,v in pairs(Branch)do out[k]=v('ignored')end
        return snapshot(out),ScreenSystemLayerHelpers:GetCreditsMessage(),SelectMusicOrCourse(),
            ScreenString('Music_Rate'),ScreenString({},'Cancel'),ScreenString(''),snapshot(bg_fit_functions)
    "#);
    for mode in [
        "BackgroundFitMode_CoverDistort",
        "BackgroundFitMode_CoverPreserve",
        "BackgroundFitMode_FitInside",
        "BackgroundFitMode_FitInsideAvoidLetter",
        "BackgroundFitMode_FitInsideAvoidPillar",
    ] {
        p.compare(&format!(r#"
            local log={{}};local actor={{}}
            for _,name in ipairs({{'GetWidth','GetHeight','zoomx','zoomy','zoom'}})do
                actor[name]=function(self,value)log[#log+1]={{name,value}};collectgarbage('collect');return name=='GetWidth' and 80 or 40 end
            end
            bg_fit_functions.{mode}(actor,320,90);return snapshot(log)
        "#));
    }
    for style in [
        "StyleType_OnePlayerTwoSides",
        "StyleType_TwoPlayersSharedSides",
        "StyleType_OnePlayerOneSide",
        "StyleType_TwoPlayersTwoSides",
        "unknown",
    ] {
        for preference in ["true", "false", "nil", "0"] {
            p.compare(&format!(r#"
                local log={{}};GAMESTATE={{GetCurrentStyle=function()log[#log+1]='style';return {{GetStyleType=function()return '{style}'end}}end}}
                PREFSMAN={{GetPreference=function(self,key)log[#log+1]=key;collectgarbage('collect');return {preference} end}}
                return Center1Player(),snapshot(log)
            "#));
        }
    }
}

#[test]
fn simply_love_formatting_and_messages_preserve_dynamic_lookup_order_and_reentry() {
    let mut p = Pair::new();
    p.compare(r#"
        local cycle={};cycle.self=cycle
        return TableToString(cycle),TableToString({},'Empty','  '),TableToString(false),TableToString(12,'N'),TableToString('hello')
    "#);
    p.compare(r#"
        local log={};TableToString=function(v)log[#log+1]='format';collectgarbage('collect');return 'formatted' end
        MESSAGEMAN={Broadcast=function(self,message,args)
            log[#log+1]={message,args};collectgarbage('collect');Trace=function(v)log[#log+1]='new trace:'..v end
        end}
        Trace=function()error('stale trace')end
        local value={};assert(select('#',SM(value,4,true))==0)
        assert(select('#',SM(17,0,false))==0)
        local before=#log;MESSAGEMAN.Broadcast=function()error('broadcast failed')end
        assert(not pcall(SM,'stop'));assert(#log==before)
        return snapshot(log)
    "#);
}

#[test]
fn saved_row_callbacks_retain_only_required_objects_then_collect() {
    let mut runtime = RustSongRuntime::new(&SongLuaCompileContext::new(
        Path::new("."),
        "option row ownership",
    ))
    .unwrap();
    execute(
        &mut runtime,
        r#"
        local a=CustomOptionRow('Mini');local b=ConfAspectRatio();local c=CustomOptionRow('Hide')
        weak=setmetatable({a,b,c,ThemePrefsRows,OperatorMenuOptionRows},{__mode='v'})
        save,load,enabled=a.SaveSelections,b.LoadSelections,c.EnabledForPlayers
        factory=OperatorMenuOptionRows.Unknown;init=ThemePrefsRows.InitAll
        ThemePrefsRows=nil;OperatorMenuOptionRows=nil;a=nil;b=nil;c=nil
        collectgarbage('collect');collectgarbage('collect')
        assert(weak[1] and weak[2]==nil and weak[3]==nil and weak[4]==nil and weak[5]==nil)
        local row=weak[1];row.Values={19};row=nil;save({true},'P2')
        assert(SL.P2.ActiveModifiers.Mini==19 and #enabled()==2 and load({})[1]==true)
        assert(factory().Name=='Unknown');init()
        save=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil)
    "#,
    );
    test_support::release_catalog_root(&mut runtime).unwrap();
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
