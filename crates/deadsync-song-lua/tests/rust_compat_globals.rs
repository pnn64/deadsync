//! Fixed host bindings from the frozen pre-migration implementation.
use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};

struct Pair {
    rust: RustSongRuntime,
    _dir: tempfile::TempDir,
}
impl Pair {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        let context = SongLuaCompileContext::new(dir.path(), "existing host compatibility surface");
        let mut pair = Self {
            rust: RustSongRuntime::new(&context).unwrap(),
            _dir: dir,
        };
        pair.compare(r#"
            function pack(...) return {n=select('#',...),...} end
            function snapshot(v)
                if type(v)=='table' then
                    local entries={}
                    for k,value in pairs(v) do entries[#entries+1]=snapshot(k)..'='..snapshot(value) end
                    table.sort(entries);return '{'..table.concat(entries,';')..'}'
                elseif type(v)=='number' then return v~=v and 'nan' or string.format('%.17g',v)
                elseif type(v)=='string' then return 's'..#v..':'..v
                elseif type(v)=='boolean' then return tostring(v)
                else return type(v) end
            end
            function observe(fn,...)
                local result=pack(pcall(fn,...))
                if not result[1] then return {ok=false} end
                return result
            end
        "#);
        pair
    }
    fn compare(&mut self, source: &str) {
        let actual = self
            .rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/compat-globals.lua").unwrap())
            .unwrap_or_else(|e| panic!("Rust: {source}\n{e:?}"))
            .to_vec();
        let actual = match actual.as_slice() {
            [] => None,
            [Value::String(id)] => Some(reference_test::snapshot_bytes(
                self.rust.vm_mut().string_bytes(*id).unwrap(),
            )),
            values => panic!("expected no return values or one serialized string: {values:?}"),
        };
        reference_test::assert_debug("song/tests/rust_compat_globals.rs::observations", &actual);
    }
}

const FROZEN_GLOBALS: &[&str] = &[
    "tableshuffle",
    "Basename",
    "findFiles",
    "cleanGSub",
    "force_to_range",
    "wrapped_index",
    "GetVersionParts",
    "GetProductVersion",
    "IsProductVersion",
    "IsMinimumProductVersion",
    "IsITGmania",
    "StepManiaVersionIsSupported",
    "MinimumVersionString",
    "CurrentGameIsSupported",
    "GetThemeVersion",
    "DetermineTimingWindow",
    "GetStepsCredit",
    "GetJudgmentGraphics",
    "GetHoldJudgments",
    "GetHeldMissGraphics",
    "GetFallbackBanner",
    "TotalCourseLength",
    "TotalCourseLengthPlayed",
    "IsGameAndMenuButton",
    "getAuthorTable",
    "courseLengthBySong",
    "lerp_color",
    "CRYPTMAN",
    "NETWORK",
    "IniFile",
    "JsonDecode",
    "JsonEncode",
    "BinaryToHex",
    "GetExJudgmentCounts",
    "GetWorstJudgment",
    "PlayerNumberToString",
    "FormatPercentScore",
    "FindInTable",
    "DeepCopy",
    "SecondsToMSSMsMs",
    "SecondsToMMSSMsMs",
    "FormatNumberAndSuffix",
    "HolidayCheer",
    "DayOfMonth",
    "DayOfYear",
    "Hour",
    "Minute",
];

#[test]
fn frozen_host_globals_keep_their_public_types() {
    let mut pair = Pair::new();
    let names = serde_json::to_string(FROZEN_GLOBALS)
        .unwrap()
        .replace('[', "{")
        .replace(']', "}");
    pair.compare(&format!("local result={{}};for _,name in ipairs{names} do result[name]=type(_G[name]) end;return snapshot(result)"));
}

#[test]
fn version_helpers_keep_string_first_numeric_prefix_and_ignored_argument_rules() {
    let mut pair = Pair::new();
    pair.compare(r#"
        local result={}
        for _,args in ipairs{pack(),pack(nil),pack(true),pack({}),pack(''),pack('1'),pack('1.2'),pack('1.2.0-beta'),
            pack(' 1.2 '),pack('1.2.0.9'),pack('1.2.1'),pack('main'),pack('999999999999999999999999.1'),
            pack('1.2.0',99),pack(1),pack(1,2),pack(1,2,0),pack(1,2,0,0),pack(1.4,2.4),
            pack(true,1,{},2),pack(false,'NaN'),pack(0/0,1,2),pack(math.huge,1,2),pack(string.char(255),1,2)} do
            local row={}
            for _,name in ipairs{'GetVersionParts','GetProductVersion','IsProductVersion','IsMinimumProductVersion'} do
                row[name]=observe(_G[name],unpack(args,1,args.n))
            end
            result[#result+1]=row
        end
        local a,b=GetProductVersion(),GetProductVersion();assert(a~=b);a[1]=99;assert(b[1]==1)
        return snapshot(result)
    "#);
}

#[test]
fn fixed_metadata_helpers_ignore_arguments_and_return_fresh_tables() {
    let mut pair = Pair::new();
    pair.compare(r#"
        local result={}
        for _,name in ipairs{'IsITGmania','StepManiaVersionIsSupported','CurrentGameIsSupported',
            'MinimumVersionString','GetThemeVersion','GetAuthor','SupportsRenderToTexture','IsGameAndMenuButton',
            'TotalCourseLength','TotalCourseLengthPlayed','GetFallbackBanner','GetPlayerOptionsString',
            'GetStepsCredit','courseLengthBySong','GetJudgmentGraphics','GetHoldJudgments','GetHeldMissGraphics',
            'GetCredits','GetExJudgmentCounts','GetAvatarPath','GetPlayerAvatarPath'} do
            local fn=_G[name];result[name]={}
            for i,args in ipairs{pack(),pack(nil),pack(false),pack({},'ignored',function()error('ignored')end)} do
                result[name][i]=observe(fn,unpack(args,1,args.n))
            end
            local a,b=fn(),fn()
            if type(a)=='table' then assert(a~=b);a.changed=true;assert(b.changed==nil) end
        end
        assert(__songlua_side_effect_count==0)
        return snapshot(result)
    "#);
}

#[test]
fn formatting_and_numeric_helpers_match_existing_argument_conversion() {
    let mut pair = Pair::new();
    for args in [
        "pack()",
        "pack(nil)",
        "pack(false)",
        "pack({})",
        "pack(0)",
        "pack(-0)",
        "pack(1.25)",
        "pack(-2.25)",
        "pack(1/3)",
        "pack(' 1.25 ')",
        "pack('nan')",
        "pack('inf')",
        "pack(0/0)",
        "pack(math.huge)",
        "pack('0x10')",
        "pack(string.char(255))",
    ] {
        for name in [
            "PlayerNumberToString",
            "FormatPercentScore",
            "FormatNumberAndSuffix",
            "SecondsToMSSMsMs",
            "SecondsToMMSSMsMs",
            "DetermineTimingWindow",
        ] {
            let source =
                format!("local args={args};return snapshot(observe({name},unpack(args,1,args.n)))");
            let known_overflow =
                name.starts_with("SecondsTo") && matches!(args, "pack('inf')" | "pack(math.huge)");
            if known_overflow {
                // Exercise overflowing formatter inputs in separate VM instances.
                Pair::new().compare(&source);
            } else {
                pair.compare(&source);
            }
        }
    }
    pair.compare(r#"
        local result={}
        for _,args in ipairs{pack(),pack(1,0,4),pack(-5,10,5),pack('0','0.5','1'),pack(false,1,2)} do
            result[#result+1]=observe(force_to_range,unpack(args,1,args.n))
            result[#result+1]=observe(wrapped_index,unpack(args,1,args.n))
        end
        for _,player in ipairs{0,1,2,.5,'P1','p2','PlayerNumber_P1','playernumber_p2','  P1 '} do
            result[#result+1]=PlayerNumberToString(player)
        end
        return snapshot(result)
    "#);
}

#[test]
fn profile_effect_helpers_survive_gc_and_reenter_counter_accessors() {
    let mut pair = Pair::new();
    pair.compare(r#"
        local saved={LoadGuest,LoadProfileCustom,SaveProfileCustom}
        LoadGuest=nil;LoadProfileCustom=nil;SaveProfileCustom=nil;collectgarbage('collect')
        local result={}
        for _,fn in ipairs(saved) do result[#result+1]=pack(fn(nil,false,{},function()error('ignored')end)) end
        result[#result+1]=__songlua_side_effect_count
        local gets,sets=0,0;rawset(_G,'__songlua_side_effect_count',nil)
        setmetatable(_G,{__index=function(t,k)
            if k=='__songlua_side_effect_count' then gets=gets+1;collectgarbage('collect');return '7.9' end
        end,__newindex=function(t,k,v)
            if k=='__songlua_side_effect_count' then sets=sets+1;assert(v==8);return end
            rawset(t,k,v)
        end})
        assert(saved[1]() == true)
        setmetatable(_G,nil);assert(gets==1 and sets==1)
        result[#result+1]={gets,sets};return snapshot(result)
    "#);
}

#[test]
fn installed_native_global_types_remain_available() {
    let mut pair = Pair::new();
    let names = String::from_utf8(reference_test::source_bytes(
        "song/tests/rust_compat_globals.rs::key_inputs",
        "globals",
    ))
    .unwrap();
    pair.compare(&format!("local result={{}};for _,name in ipairs{{{names}}} do result[name]=type(_G[name]) end;return snapshot(result)"));
    for name in [
        "table",
        "math",
        "string",
        "CRYPTMAN",
        "NETWORK",
        "IniFile",
        "lua",
        "PlayerOptions",
        "Difficulty",
        "ModTimerType",
        "CullMode",
        "EditState",
        "ProfileSlot",
        "GameController",
        "PlayerController",
        "HealthState",
        "SortOrder",
        "TapNoteScore",
        "TapNoteType",
        "TapNoteSource",
        "TapNoteSubType",
        "HoldNoteScore",
        "StepsType",
        "StyleType",
    ] {
        let keys = String::from_utf8(reference_test::source_bytes(
            "song/tests/rust_compat_globals.rs::key_inputs",
            name,
        ))
        .unwrap();
        pair.compare(&format!("local result={{}};for _,key in ipairs{{{keys}}} do result[key]=type(_G[{name:?}][key]) end;return snapshot(result)"));
    }
}

#[test]
fn json_and_binary_helpers_keep_existing_depth_sparse_and_invalid_input_rules() {
    let mut pair = Pair::new();
    pair.compare(r#"
        local result={}
        for _,input in ipairs{'null','true','false','0','-0.0','1.0','1.25','9007199254740993',
            '[1,2,3]','[1,null,3]','[null,2,null]','{}','{"a":1,"b":null}','{"a":[1,2]}','invalid','',string.char(255)} do
            local decoded=JsonDecode(input);result[#result+1]={snapshot(decoded),JsonEncode(decoded)}
        end
        for _,args in ipairs{pack(),pack(false),pack(123),pack({}),pack('a'..string.char(0,255,16)),pack(nil)} do
            result[#result+1]={observe(JsonDecode,unpack(args,1,args.n)),observe(BinaryToHex,unpack(args,1,args.n))}
        end
        local cycle={};cycle[1]=cycle;local deep=true;for i=1,18 do deep={deep}end
        for _,args in ipairs{pack(),pack(nil),pack(false),pack(1.0),pack(-0.0),pack(math.huge),pack(0/0),
            pack(function()end),pack('a'..string.char(255)),pack({1,2,3}),pack({alpha=true,beta={1,2}}),pack(cycle),pack(deep)} do
            result[#result+1]=observe(JsonEncode,unpack(args,1,args.n))
        end
        assert(select('#',JsonDecode('null'))==1)
        local a,b=JsonDecode('invalid'),JsonDecode('invalid');assert(a~=b)
        return snapshot(result)
    "#);
}

#[test]
fn offline_services_keep_fixed_results_identity_effects_and_gc() {
    let mut pair = Pair::new();
    pair.compare(r#"
        local result={}
        for _,args in ipairs{pack(),pack(false),pack({},'ignored'),pack(nil)} do
            for _,name in ipairs{'SHA1File','SHA1String','SHA256File','SHA256String','GenerateRandomUUID','SignFileToFile'} do
                result[#result+1]=observe(CRYPTMAN[name],unpack(args,1,args.n))
            end
        end
        assert(NETWORK:IsUrlAllowed('https://example.invalid')==false)
        local response=NETWORK:HttpRequest({},false);local socket=NETWORK:WebSocket()
        assert(response.status==0 and response.code==0 and response.body=='' and response.error=='offline' and type(response.headers)=='table')
        assert(response:IsFinished()==true and response.GetResponse(false)==response)
        local retained=response.GetResponse;response=nil;collectgarbage();local owner=retained();assert(owner==retained())
        owner:Cancel();assert(socket.is_open==false and socket:IsOpen()==false)
        assert(select('#',socket:Send())==0 and select('#',socket:Close())==0)
        for _,path in ipairs{'ThemeInfo.ini','themes/ThemeInfo.ini','themeinfo.ini','ThemeInfo.ini.extra',''} do
            result[#result+1]=IniFile:ReadFile(path);result[#result+1]=IniFile.ReadFile(path)
        end
        assert(IniFile:WriteFile('never-written.ini',{})==true)
        local query={a='x y',b='é',c=true,[2]=3,[false]='!'}
        result[#result+1]=NETWORK:EncodeQueryParameters(query)
        assert(NETWORK.EncodeQueryParameters(query)=='')
        local f=NETWORK.EncodeQueryParameters;NETWORK=nil;collectgarbage();result[#result+1]=f(false,query)
        result[#result+1]=__songlua_side_effect_count
        return snapshot(result)
    "#);
}

#[test]
fn sequence_helpers_preserve_holes_callbacks_and_distinct_identity() {
    let mut pair = Pair::new();
    // The complete runtime includes the late Simply Love formatters.
    pair.compare(r#"
        local result={};local t={};local f=function()end
        local input={1,1,0,-0.0,'a','a',true,true,t,t,f,f,string.char(255),string.char(255)}
        result[#result+1]=deduplicate(input)
        result[#result+1]=map(function(v)collectgarbage();return v*2,'ignored'end,{1,2,3})
        result[#result+1]=map(function(v)if v==2 then return nil else return v end end,{1,2,3})
        result[#result+1]=stringify({1,2.25,true,false,{},'text'},'%.1f')
        local prior=string.format;local calls=0
        string.format=function(form,v)calls=calls+1;collectgarbage();return false,'ignored'end
        local mapped=stringify({1,2,'x'},'%g');string.format=prior;result[#result+1]=mapped;assert(calls==2)
        for _,args in ipairs{pack(),pack(nil),pack(false),pack({}),pack({1,2},'Rows'),pack(12,'Count'),pack(nil,'Nil')}do
            result[#result+1]=TableToString(unpack(args,1,args.n))
        end
        for _,shift in ipairs{0,1,-1,2.5,'2','bad',false}do
            result[#result+1]=table.rotate_left({1,2,3,4},shift);result[#result+1]=table.rotate_right({1,2,3,4},shift)
        end
        assert(FindInTable(t,{t})==1 and FindInTable(f,{f})==nil)
        assert(table.find({f},f)==1)
        math.randomseed(45);result[#result+1]=table.shuffle({1,2,3,4,5});assert(table.shuffle==tableshuffle)
        return snapshot(result)
    "#);
}

#[test]
fn base_table_formatter_keeps_its_pre_theme_compatibility_behavior() {
    let mut pair = Pair::new();
    test_support::install_base_table_helpers(&mut pair.rust).unwrap();
    pair.compare(r#"
        local result={}
        for _,args in ipairs{pack(),pack(nil),pack(false),pack({}),pack({1,2},'Rows'),pack(12,'Count'),pack(nil,'Nil')}do
            result[#result+1]=TableToString(unpack(args,1,args.n))
        end
        local cycle={};cycle.self=cycle
        result[#result+1]=TableToString(cycle)
        result[#result+1]=TableToString('hello','Name','  ')
        local saved=TableToString;TableToString=nil;collectgarbage('collect')
        result[#result+1]=saved(false)
        return snapshot(result)
    "#);
}

#[test]
fn recursive_copy_preserves_existing_alias_metatable_and_binary_key_rules() {
    let mut pair = Pair::new();
    pair.compare(r#"
        local key={tag='key'};local shared={nested={value=7}};local fn=function()end
        local source={[key]=shared,left=shared,right=shared,fn=fn,[string.char(255)]='binary'}
        setmetatable(source,{__index=function()error('copy invoked index')end,__pairs=function()error('copy invoked pairs')end})
        local copy=DeepCopy(source);assert(copy~=source and getmetatable(copy)==nil)
        assert(copy.left~=shared and copy.left~=copy.right and copy.left.nested~=shared.nested and copy.fn==fn)
        local copied_key;for k,v in pairs(copy)do if type(k)=='table' then copied_key=k;assert(v~=shared)end end
        assert(copied_key~=key and copied_key.tag=='key' and copy[string.char(255)]=='binary')
        shared.nested.value=9;assert(copy.left.nested.value==7)
        assert(select('#',DeepCopy())==1 and DeepCopy()==nil)
        local u=newproxy(true);assert(DeepCopy(u)==u)
        return snapshot({copy.left,copy.right,copied_key,copy[string.char(255)]})
    "#);
}

#[test]
fn strings_colors_authors_judgments_and_file_listing_match_existing_helpers() {
    let mut pair = Pair::new();
    for name in ["a.ogg", "B.OGG", "other.wav"] {
        std::fs::write(pair._dir.path().join(name), []).unwrap();
    }
    std::fs::create_dir(pair._dir.path().join("folder.ogg")).unwrap();
    pair.compare(r#"
        local result={}
        for _,args in ipairs{pack(),pack(nil),pack(false),pack('a,b,',','),pack('é🙂',''),pack('',''),pack('aa','a'),pack(12,true),pack(string.char(255),',')}do
            result[#result+1]=observe(string.split,unpack(args,1,args.n))
            result[#result+1]=observe(split,args[2],args[1])
            result[#result+1]=observe(cleanGSub,unpack(args,1,args.n))
            result[#result+1]=observe(Basename,unpack(args,1,args.n))
        end
        result[#result+1]=cleanGSub('a.a.a','.',':')
        for _,args in ipairs{pack(.25,'#ff0000','0,0,1'),pack(1.5,{0,1,0,.5},{1,0,1}),pack('nan',{1,2,3},{4,5,6}),pack(0/0,{1,2,3},{4,5,6}),
            pack(.5,{'1','2','3'},'bad'),pack(.5,{1,2},{}),pack(.5,false,{}),pack(.5,string.char(255),'#fff')} do
            result[#result+1]=observe(lerp_color,unpack(args,1,args.n))
        end
        local methods={GetDescription=function(self)collectgarbage();return 'author'end,GetAuthorCredit=function()return 'author'end,GetChartName=function()return 12 end}
        result[#result+1]=getAuthorTable(methods)
        result[#result+1]=observe(getAuthorTable,setmetatable({},{__index=function(t,k)collectgarbage();return function(self)return true end end}))
        result[#result+1]=observe(getAuthorTable,{GetDescription=false})
        for _,value in ipairs{false,{}, {0,.03,.05,.2}, {{nil,.04,nil,nil,nil,true,.15}}, {{nil,'nan'}}, {.02,nil,.8}}do
            result[#result+1]=GetWorstJudgment(value)
        end
        for _,args in ipairs{pack(),pack('','ogg'),pack('.', '.OGG'),pack('missing','wav'),pack(false,false),pack('','wav')}do
            result[#result+1]=observe(findFiles,unpack(args,1,args.n))
        end
        return snapshot(result)
    "#);
}

#[test]
fn enum_lists_reverse_identity_and_capability_values_match_existing_host() {
    let mut pair = Pair::new();
    pair.compare(r#"
        local result={}
        for _,name in ipairs{'Difficulty','ModTimerType','CullMode','EditState','ProfileSlot',
            'GameController','PlayerController','HealthState','SortOrder','TapNoteScore',
            'TapNoteType','TapNoteSource','TapNoteSubType','HoldNoteScore','StepsType','StyleType'}do
            local enum=_G[name];local reverse=enum.Reverse;local retained=reverse()
            assert(retained==reverse());collectgarbage();assert(retained==reverse())
            result[name]={}
            for i,v in ipairs(enum)do result[name][i]=v end
            result[name].reverse=retained
        end
        result.PlayerOptions=PlayerOptions
        result.ModTimerType.name=ModTimerType.GetName(false,'ignored')
        return snapshot(result)
    "#);
}

#[test]
fn existing_color_helpers_match_palettes_tones_and_reentrant_difficulty_methods() {
    let mut pair = Pair::new();
    pair.compare(r#"
        local result={}
        for _,args in ipairs{pack(),pack(nil),pack(false),pack(1),pack(-1),pack(0/0),
            pack(math.huge),pack('P1'),pack('PlayerNumber_P2'),pack('Hard'),pack('Difficulty_Edit'),
            pack('Stage_1st'),pack('W1'),pack('bad'),pack(string.char(255)),
            pack('#ff000080'),pack('1,.5,.25'),pack({.2,.5,.7}),pack({'0x10','2','3','0.5'}),
            pack({1,2}),pack({1,2,3,false}),pack({1,2,3,4},2),pack({1,2,3,4},{4,3,2,1})}do
            local row={}
            for _,name in ipairs{'PlayerScoreColor','PlayerDarkColor','CustomDifficultyToDarkColor',
                'CustomDifficultyToLightColor','StepsOrTrailToColor','StageToColor','StageToStrokeColor',
                'JudgmentLineToColor','JudgmentLineToStrokeColor','LightenColor','ColorLightTone',
                'ColorMidTone','ColorDarkTone','ColorToHex','BoostColor','BlendColors'}do
                row[name]=observe(_G[name],unpack(args,1,args.n))
            end
            result[#result+1]=row
        end
        for _,index in ipairs{-15,-1,0,1,2,13,999,.5,'1',false,'bad'}do
            for _,decorative in ipairs{true,false,'YES','off',2,0}do
                for _,palette in ipairs{'SL','ITG','itg','DDR','unknown'}do
                    result[#result+1]=GetHexColor(index,decorative,palette)
                end
            end
        end
        local gets,calls=0,0
        local steps=setmetatable({},{__index=function(self,key)
            assert(key=='GetDifficulty');gets=gets+1;collectgarbage()
            return function(owner)assert(owner==self);calls=calls+1;collectgarbage();return 'Difficulty_Hard','ignored'end
        end})
        result[#result+1]=StepsOrTrailToColor(steps);assert(gets==1 and calls==1)
        result[#result+1]=StepsOrTrailToColor({GetDifficulty=function()error('existing swallowed callback error')end})
        result[#result+1]=StepsOrTrailToColor(setmetatable({},{__index=function()error('existing swallowed lookup error')end}))
        local prior=tonumber;tonumber=function()error('host numeric conversion must not invoke replacement')end
        result[#result+1]=BlendColors({'1','0','0'},'0,0,1');tonumber=prior
        local a,b=LightenColor('#010203'),LightenColor('#010203');assert(a~=b and getmetatable(a)==nil)
        return snapshot(result)
    "#);
}

#[test]
fn lua_services_keep_file_read_virtual_write_and_error_argument_contracts() {
    let mut pair = Pair::new();
    std::fs::write(pair._dir.path().join("existing.txt"), b"text\0end").unwrap();
    std::fs::write(pair._dir.path().join("binary.txt"), [255]).unwrap();
    pair.compare(r#"
        local result={}
        for _,args in ipairs{pack(),pack(nil),pack(false),pack(12),pack('existing.txt'),pack('binary.txt'),
            pack('missing.txt'),pack({},'existing.txt'),pack(lua,'existing.txt'),pack(string.char(255))}do
            result[#result+1]=observe(lua.ReadFile,unpack(args,1,args.n))
            result[#result+1]=observe(lua.ReportScriptError,unpack(args,1,args.n))
        end
        local read,write,report=lua.ReadFile,lua.WriteFile,lua.ReportScriptError
        lua=nil;collectgarbage()
        result[#result+1]=read(false,'existing.txt')
        assert(write('never-written.txt','payload')==true)
        assert(select('#',report('retained error'))==0)
        result[#result+1]=__songlua_side_effect_count
        assert(__songlua_reporting_script_error==false)
        return snapshot(result)
    "#);
    assert!(!pair._dir.path().join("never-written.txt").exists());
}

#[test]
#[cfg(debug_assertions)]
fn native_formatter_debug_overflow_is_protected_at_the_song_host_boundary() {
    for name in ["SecondsToMSSMsMs", "SecondsToMMSSMsMs"] {
        let mut pair = Pair::new();
        let source = format!("return {name}(math.huge)");
        let error = pair
            .rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=overflow boundary").unwrap())
            .unwrap_err();
        assert_eq!(error.kind(), deadlib_lua::ErrorKind::Runtime);
        assert!(
            error
                .to_string()
                .contains("attempt to multiply with overflow")
        );
        // The binding catches the shared helper's panic before it reaches the
        // generic host API. The same VM must remain usable after the error.
        pair.rust
            .vm_mut()
            .execute(
                &compile_named(
                    b"assert(SecondsToMSSMsMs(1.25)=='0:01.25')",
                    b"=after error",
                )
                .unwrap(),
            )
            .unwrap();
    }
}

#[test]
fn restored_debug_walks_keep_existing_recursion_and_trace_dispatch() {
    let mut pair = Pair::new();
    pair.compare(r#"
        local log={};Trace=function(text)collectgarbage();log[#log+1]=text end
        rec_print_table({outer={inner=42}},false,2)
        rec_print_table({ignored=true},'prefix',0)
        local owned={__songlua_def_owned=true};owned.cycle=owned
        rec_print_table(owned,'owned ')
        local leaf={GetName=function()return 'leaf'end}
        setmetatable(leaf,{__tostring=function()return 'leaf object'end})
        rec_print_children({GetName=function()return 'parent'end,GetChildren=function()return {only=leaf}end})
        rec_print_children({leaf},'array ')
        return snapshot(log)
    "#);
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
