use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};
use std::path::Path;

fn context() -> SongLuaCompileContext {
    SongLuaCompileContext::new(Path::new("."), "Song option migration")
}
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/song-options.lua").unwrap())
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
    rust: RustSongRuntime,
}
impl Pair {
    fn new(context: &SongLuaCompileContext) -> Self {
        let mut rust = RustSongRuntime::new(context).unwrap();
        test_support::expose_catalog(&mut rust).unwrap();
        let mut pair = Self { rust };
        pair.compare(SNAPSHOT);
        pair.compare(OBSERVE);
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
                other => panic!("scalar expected: {other:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_song_options.rs::Pair::compare",
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

const OBSERVE: &str = r#"
SO=catalog.song_option_operations
function outcome(f,...)
    local function collect(ok,...)
        local result={ok=ok,n=ok and select('#',...) or 0}
        if ok then for i=1,select('#',...) do
            local v=select(i,...)
            if type(v)=='table' then
                local index=0
                for level=0,3 do if v==SO:Select(level) then index=level+1 end end
                result[i]='owner:'..index
            else result[i]=v end
        end end
        return result
    end
    return collect(pcall(f,...))
end
function all_options()
    local out={}
    for i=0,3 do out[i+1]=SO:Select(i) end
    return snapshot(out)
end
"#;

#[test]
fn four_levels_preserve_initial_values_methods_and_current_timing_identity() {
    for rate in [1.0, 1.25, 0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mut context = context();
        context.song_music_rate = rate;
        let mut p = Pair::new(&context);
        p.compare(r#"
            assert(SO:Select(3)==catalog.current_song_options)
            for i=0,3 do
                assert(SO:Select(i)==catalog.song_options[i+1])
                for j=i+1,3 do assert(SO:Select(i)~=SO:Select(j)) end
            end
            local result={}
            for _,name in ipairs{'MusicRate','Haste','AssistClap','AssistMetronome','StaticBackground','RandomBGOnly','SaveScore','SaveReplay','AutosyncSetting'} do
                result[name]=outcome(SO:Select(3)[name],SO:Select(3))
            end
            return all_options(),snapshot(result),SO:Text('Current')
        "#);
    }
}

#[test]
fn float_setters_preserve_previous_results_chaining_and_partial_failure() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local owner=SO:Select('Current');local results={}
        for _,name in ipairs{'MusicRate','Haste'} do
            local f=owner[name]
            local function pack(...) return {n=select('#',...),...} end
            for _,args in ipairs{pack(.5),pack(.75,.25),pack(.25,.5,true),pack(.5,-1),pack(0),pack(-1),pack(3),pack(3.1),pack('1.2'),pack('0x2'),pack(false),pack('bad'),pack(math.huge),pack(0/0),pack('nan'),pack('inf'),pack(nil,'nan'),pack(nil,'inf'),pack(nil,false),pack(nil,true),pack(nil,.3,false),pack(.25,.3,false,true)} do
                results[#results+1]={name,outcome(f,owner,unpack(args,1,args.n)),owner.__songlua_music_rate,owner.__songlua_haste,owner.__songlua_music_rate_speed,owner.__songlua_haste_speed}
            end
            results[#results+1]=outcome(f,true)
            results[#results+1]=outcome(f,nil,true)
            results[#results+1]=outcome(f,.5,.25)
            results[#results+1]=outcome(f,{impostor=true},.75,.5)
        end
        return snapshot(results),all_options()
    "#);
    p.compare(
        r#"
        local owner=SO:Select(3)
        owner.__songlua_music_rate=1;owner.__songlua_music_rate_speed=1
        local ok,err=pcall(owner.MusicRate,owner,2,-1)
        assert(not ok and string.find(tostring(err),'negative option speed',1,true))
        assert(owner:MusicRate()==2)
        return owner:MusicRate()
    "#,
    );
}

#[test]
fn boolean_setters_use_truthiness_for_previous_state_and_boolean_chain_arguments() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local owner=SO:Select(3);local results={}
        for _,name in ipairs{'AssistClap','AssistMetronome','StaticBackground','RandomBGOnly','SaveScore','SaveReplay'} do
            local f=owner[name];local key='__songlua_'..string.lower(name)
            for _,value in ipairs{true,false,0,'',{},function()end} do
                owner[key]=value
                results[#results+1]={name,outcome(f,owner),outcome(f,owner,0),outcome(f,owner,false,false),outcome(f,true,true)}
            end
            owner[key]=nil
            results[#results+1]=outcome(f,owner)
        end
        return snapshot(results),all_options()
    "#);
}

#[test]
fn enum_arguments_are_strict_but_stored_integer_fields_use_native_coercion() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local out={};local owner=SO:Select(3)
        for _,value in ipairs{0,1,3,-1,4,.5,0/0,math.huge,'Off','song','MACHINE','Tempo','AutosyncType_Off','autosynctype_song','1',false,{}} do
            out[#out+1]=outcome(owner.AutosyncSetting,owner,value)
        end
        for _,value in ipairs{-.5,-1,1.9,99,'1.9','-.5','0x2','1e19',1e19,math.huge,0/0,'bad',false,{}} do
            owner.__songlua_autosync=value
            out[#out+1]={outcome(owner.AutosyncSetting,owner),outcome(SO.Text,SO,3)}
        end
        owner.__songlua_autosync=99
        assert(not pcall(owner.AutosyncSetting,owner,1) and owner.__songlua_autosync==1)
        owner.__songlua_autosync=99
        assert(owner:AutosyncSetting(2,true)==owner and owner.__songlua_autosync==2)
        for _,value in ipairs{0,1,2,3,-1,4,.5,'Preferred','STAGE','Song','Current','ModsLevel_Song','modslevel_song','2',false,{}} do
            out[#out+1]={outcome(SO.Select,SO,value),outcome(SO.Text,SO,value)}
        end
        return snapshot(out),all_options()
    "#);
}

#[test]
fn assignment_parsing_and_level_propagation_match_without_replacing_tables() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local out={};local original={};local getters={}
        for i=0,3 do original[i+1]=SO:Select(i);getters[i+1]=original[i+1].MusicRate;original[i+1].custom='keep' end
        local cases={'','1.25xMusic, Clap, 25% Haste, AutosyncMachine, EffectPitch, SaveReplay',
            'no clap,no haste,no autosync,no effect,no savescore',
            '2xmusic,0xmusic,4xmusic,01.50xmusic',
            '.5xmusic,1.xmusic,+2xmusic,-1xmusic,1e2xmusic,0x2xmusic,1.2.3xmusic',
            '50% clap, 30% haste,*4 haste,garbage haste,No Haste',
            'autosync,autosynctempo,effectspeed,metronome,staticbg,randombg',
            'clap\t,no\thaste, no  metronome,ignored, no effectpitch',
            '99999999999999999999999999999999999999999999999999xmusic'}
        for level=0,3 do for _,text in ipairs(cases) do
            for i=0,3 do SO:Assign(i,'2xmusic,clap,haste') end
            assert(select('#',SO:Assign(level,text))==0)
            for i=0,3 do assert(SO:Select(i)==original[i+1] and SO:Select(i).MusicRate==getters[i+1] and SO:Select(i).custom=='keep') end
            out[#out+1]=all_options()
            for i=0,3 do out[#out+1]=SO:Text(i) end
        end end
        local before=all_options()
        for _,bad in ipairs{false,{},1} do assert(not pcall(SO.Assign,SO,2,bad)) end
        assert(before==all_options())
        return snapshot(out)
    "#);
}

#[test]
fn incremental_stage_changes_restart_and_attack_clear_preserve_shared_values() {
    let mut p = Pair::new(&context());
    p.compare(
        r#"
        SO:Assign(0,'1.2xmusic,clap');local before=SO:Text(0)
        SO:Apply(1,'haste,metronome');assert(SO:Text(0)==before)
        SO:Apply(2,'no clap,autosynctempo');SO:Apply(3,'no haste')
        local out={all_options()}
        local value={shared=true}
        SO:Select(1).__songlua_savescore=value
        SO:ClearAttacks();assert(SO:Select(2).__songlua_savescore==value)
        out[#out+1]=all_options()
        SO:Select(0).__songlua_assistclap=value
        SO:Restart()
        for i=1,3 do assert(SO:Select(i).__songlua_assistclap==value) end
        out[#out+1]=all_options()
        return snapshot(out)
    "#,
    );
}

#[test]
fn current_options_approach_song_targets_in_float_order_and_drive_timing() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    let mut context = context();
    context.song_timing = Some(TimingData::from_segments(
        0.,
        0.125,
        &TimingSegments {
            bpms: vec![(0., 120.)],
            ..Default::default()
        },
        &[],
    ));
    let mut p = Pair::new(&context);
    p.compare(r#"
        local song=SO:Select(2);local current=SO:Select(3);local timing=catalog.song:GetTimingData()
        song:MusicRate(1.75,.3);song:Haste(.8,.7);song:AssistClap(true)
        local out={}
        for _,delta in ipairs{0,.00001,.0166667,.1,.125,.75,1,10} do
            SO:Advance(delta)
            out[#out+1]={current:MusicRate(),current:Haste(),current:AssistClap(),timing:GetElapsedTimeFromBeat(4)}
        end
        assert(current:MusicRate()==1.75 and current:Haste()==song:Haste())
        song:MusicRate(.75,.2);song:Haste(-.5,.5)
        for _,delta in ipairs{.125,.3,.75,10} do SO:Advance(delta);out[#out+1]={current:MusicRate(),current:Haste()} end
        return snapshot(out),all_options()
    "#);
}

#[test]
fn update_failures_preserve_song_phase_order_and_recover() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local song=SO:Select(2);local current=SO:Select(3);local out={}
        song:MusicRate(2,.5);song:Haste(.5,1);song:AssistClap(true)
        song.__songlua_haste_speed=-1
        local ok,err=pcall(SO.Advance,SO,.25)
        assert(not ok and string.find(tostring(err),'negative native option approach',1,true))
        assert(current:MusicRate()==1.125 and current:Haste()==0 and current:AssistClap()==false)
        out[#out+1]=all_options()
        song.__songlua_haste_speed=1
        for _,delta in ipairs{-1,math.huge,0/0,'bad'} do
            local before=all_options();assert(not pcall(SO.Advance,SO,delta));assert(before==all_options())
        end
        SO:Advance(.25);out[#out+1]=all_options()
        song.__songlua_music_rate_speed=0/0
        assert(not pcall(SO.Advance,SO,0))
        song.__songlua_music_rate_speed=math.huge
        SO:Advance(.1);out[#out+1]=all_options()
        return snapshot(out)
    "#);
}

#[test]
fn raw_state_reads_ignore_metatables_and_saved_primitives_survive_global_replacement() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local owner=SO:Select(3)
        setmetatable(owner,{__index=function()error('index')end,__newindex=function()error('newindex')end})
        owner.__songlua_music_rate='0x2';owner.__songlua_music_rate_speed=' 1.5 '
        tonumber=function()error('replaced tonumber')end
        local out={outcome(owner.MusicRate,owner),outcome(SO.Text,SO,3)}
        rawset(owner,'__songlua_music_rate',nil)
        assert(not pcall(owner.MusicRate,owner,1))
        SO:Assign(3,'1.5xmusic');out[#out+1]=outcome(owner.MusicRate,owner)
        catalog.song_options={};catalog.current_song_options={}
        SO:Assign(3,'2xmusic');assert(SO:Select(3)==owner)
        return snapshot(out),all_options()
    "#);
}

#[test]
fn saved_getters_retain_only_their_option_owner_and_runtime_instances_are_isolated() {
    let mut runtime = RustSongRuntime::new(&context()).unwrap();
    test_support::expose_catalog(&mut runtime).unwrap();
    test_support::release_catalog_root(&mut runtime).unwrap();
    test_support::retire_game_state_globals(&mut runtime).unwrap();
    test_support::retire_statistics_globals(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        weak=setmetatable({catalog.song_options[1],catalog.song_options[2],catalog.song_options[3],catalog.song_options[4]},{__mode='v'})
        saved=catalog.song_options[4].MusicRate
        catalog=nil;SONGMAN=nil
    "#,
    );
    execute(
        &mut runtime,
        r#"
        collectgarbage('collect');collectgarbage('collect')
        assert(weak[1]==nil and weak[2]==nil and weak[3]==nil and weak[4]~=nil)
        assert(saved(2)==1);assert(saved()==2);saved=nil
    "#,
    );
    execute(
        &mut runtime,
        "collectgarbage('collect');collectgarbage('collect');assert(weak[4]==nil)",
    );
    let mut second = Pair::new(&context());
    second.compare("return SO:Select(3):MusicRate()");
}

#[test]
fn frozen_song_option_string_control_runs_unchanged_with_real_actor_and_message_bindings() {
    let fixture = include_str!(
        "../../../tests/fixtures/itgmania-song-lua-micro/song-options-string/control.lua"
    );
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-song-lua-micro/song-options-string/native.json"
    ))
    .unwrap();
    const OBSERVE_MESSAGES: &str = r#"
        messages={};PLAYER_1='PlayerNumber_P1'
        local broadcast=MESSAGEMAN.Broadcast
        MESSAGEMAN.Broadcast=function(self,name,params)
            messages[#messages+1]=name
            return broadcast(self,name,params)
        end
    "#;
    let context = context();
    let mut runtime = RustSongRuntime::new(&context).unwrap();
    test_support::expose_catalog(&mut runtime).unwrap();
    // Use the installed GameState, PlayerOptions and NOTESKIN bindings.
    execute(&mut runtime, OBSERVE_MESSAGES);
    let definition = execute(&mut runtime, fixture)[0];
    runtime.instantiate_actor(definition).unwrap();
    let expected: Vec<String> = native["message_dispatches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_owned())
        .collect();
    let results = execute(&mut runtime, "return unpack(messages)");
    let actual: Vec<String> = results
        .into_iter()
        .map(|v| {
            let Value::String(id) = v else { panic!() };
            String::from_utf8(runtime.vm_mut().string_bytes(id).unwrap().to_vec()).unwrap()
        })
        .collect();
    assert_eq!(actual, expected);
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
