use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};
use std::path::Path;

fn context() -> SongLuaCompileContext {
    SongLuaCompileContext::new(Path::new("."), "Player option migration")
}
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/player-options.lua").unwrap())
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
                other => panic!("scalar expected: {other:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_player_options.rs::Pair::compare",
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

const SETUP: &str = r#"
function pack(...) return {n=select('#',...),...} end
function outcome(owner,fn,...)
    local r=pack(pcall(fn,...))
    if not r[1] then return {ok=false} end
    for i=2,r.n do if r[i]==owner then r[i]='owner' end end
    return r
end
options=catalog.player_options[1][3]
NOTESKIN={DoesNoteSkinExist=function(self,name) return name=='cel' or name=='metal' or name=='my skin' end,
GetNoteSkinNames=function()return {'cel','metal','my skin'}end}
"#;

#[test]
fn eight_initial_objects_match_profiles_and_keep_level_identity() {
    for speed in [
        song::SongLuaSpeedMod::X(1.0),
        song::SongLuaSpeedMod::X(1.75),
        song::SongLuaSpeedMod::C(450.0),
        song::SongLuaSpeedMod::M(600.0),
        song::SongLuaSpeedMod::A(300.0),
    ] {
        let mut c = context();
        c.players[0].speedmod = speed;
        c.players[0].mini = 0.25;
        c.players[0].perspective.tilt = -0.3;
        c.players[0].perspective.skew = 0.3;
        c.players[0].noteskin_name = "metal".into();
        c.players[1].autoplay = 1.0;
        let mut p = Pair::new(&c);
        p.compare(r#"
            local out={}
            for pn=1,2 do for level=1,4 do
                local o=catalog.player_options[pn][level]
                for other=level+1,4 do assert(o~=catalog.player_options[pn][other]) end
                assert(o~=catalog.player_options[3-pn][level])
                out[#out+1]={snapshot(o),pack(o:XMod()),pack(o:CMod()),pack(o:MMod()),pack(o:AMod()),pack(o:CAMod()),pack(o:NoteSkin())}
            end end
            return snapshot(out)
        "#);
    }
}

#[test]
fn complete_declared_surface_and_column_methods_match_native_dispatch() {
    let mut names = song::SONG_LUA_PLAYER_OPTION_CAPABILITIES
        .iter()
        .map(|s| (*s).to_owned())
        .collect::<Vec<_>>();
    for prefix in song::SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES {
        for col in 1..=16 {
            names.push(format!("{prefix}{col}"));
        }
    }
    let names = names
        .iter()
        .map(|s| format!("{s:?}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut p = Pair::new(&context());
    p.compare(&format!(
        r#"
        local out={{}}
        for _,name in ipairs{{{names}}} do
            local fn=options[name];assert(type(fn)=='function' and options[name]==fn)
            local row={{name, outcome(options,fn,options)}}
            for _,args in ipairs{{pack(options,0.25),pack(options,nil,2),pack(options,true,false),
                pack(options,'0.5',3,true),pack(options,0.75,-1),pack(options,false)}} do
                row[#row+1]=outcome(options,fn,unpack(args,1,args.n))
            end
            row[#row+1]=snapshot(options);out[#out+1]=row
        end
        assert(options.NotAnOption==nil and options.MoveX17==nil and options.movex1==nil)
        return snapshot(out)
    "#
    ));
}

#[test]
fn scalar_aliases_enum_conversion_and_fixed_tuple_methods_preserve_partial_writes() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local out={}
        for _,name in ipairs{'Mini','Incoming','Space','Hallway','Distant','Overhead','Tilt','Skew',
            'Mirror','FailSetting','ModTimerSetting','LifeSetting','BatteryLives'} do
            local fn=options[name];local rows={name}
            for _,args in ipairs{pack(),pack(nil),pack(true),pack(false),pack(0.5,-1),pack('nan','nan'),
                pack('inf'),pack(math.huge),pack(0/0),pack('song'),pack('FailType_Off'),pack('off'),
                pack('1'),pack({},0.2,1),pack(options,nil,'0x10'),pack(options,0.75,{},true),
                pack(options,0.5,-1,true),pack(options,nil,0,true)} do
                rows[#rows+1]={outcome(options,fn,unpack(args,1,args.n)),snapshot(options)}
            end
            out[#out+1]=rows
        end
        return snapshot(out)
    "#);
}

#[test]
fn speed_fields_modes_and_approaches_keep_native_write_and_return_order() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local out={}
        for _,name in ipairs{'XMod','CMod','MMod','AMod','CAMod','TimeSpacing','ScrollSpeed','ScrollBPM','MaxScrollBPM'} do
            local fn=options[name]
            for _,args in ipairs{pack(options),pack(options,2),pack(options,nil,3),pack(options,300,-1),
                pack(options,0),pack(options,-2),pack(options,'nan'),pack(options,'inf'),
                pack(options,math.huge),pack(options,0/0),pack(options,'450','0.25',true),
                pack(options,nil),pack(500,2,true),pack(nil,1,true),pack(true)} do
                out[#out+1]={name,outcome(options,fn,unpack(args,1,args.n)),snapshot(options),
                    pack(options:XMod()),pack(options:CMod()),pack(options:MMod()),pack(options:AMod()),pack(options:CAMod())}
            end
        end
        return snapshot(out)
    "#);
}

#[test]
fn string_parser_preserves_grammar_field_sharing_skin_precedence_and_resets() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local out={};local trace={}
        NOTESKIN.DoesNoteSkinExist=function(self,name)trace[#trace+1]=name;return name=='metal' or name=='my skin'end
        for _,s in ipairs{'50% Reverse, *3 25% Mini, C450','1.5x','m700','*2 CA250','A300',
            '25% Incoming, 75% Space','No Mirror, 50% Left, 51% Right','ModTimerSong, FailAtEnd',
            'NoAttacks, RandomAttacks','*2 30+0% Drunk','-nan Tiny, 1e999 Dark',
            '25% 50% Reverse, *2 *3 Mini','25* Reverse','unknown', 'my skin','metal',
            'No NoteSkin','no noholds','ModTimerDefault','ClearAll','*3 20% MoveX16',''} do
            local ok=outcome(options,options.FromString,options,s)
            out[#out+1]={s,ok,snapshot(options),__songlua_side_effect_count,table.concat(trace,'|')};trace={}
        end
        for _,s in ipairs{'ClearAll','clearall','CLEARALL'} do
            NOTESKIN.DoesNoteSkinExist=function()return false end
            NOTESKIN.GetNoteSkinNames=function()return {'first',42}end
            out[#out+1]={outcome(options,options.FromString,options,s),snapshot(options)}
        end
        return snapshot(out)
    "#);
}

#[test]
fn lazy_methods_observe_replaced_state_and_speeds_through_reentrant_metamethods() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local out={};local saved=options.Drunk;local trace={}
        local replacement={drunk=9};local approaches={drunk=5}
        local old=setmetatable({}, {__index=function(t,k)
            trace[#trace+1]='read:'..k;options.__songlua_player_option_state=replacement
            collectgarbage('collect');return 0.25
        end,__newindex=function(t,k,v)trace[#trace+1]='write:'..k;rawset(t,k,v)end})
        options.__songlua_player_option_state=old;options.__songlua_player_option_speeds=approaches
        out[1]=outcome(options,saved,options,0.5,2)
        out[2]={snapshot(old),snapshot(replacement),snapshot(approaches),table.concat(trace,'|')}
        old=nil;collectgarbage('collect');out[3]=pack(saved())
        options.Drunk=nil;local recreated=options.Drunk
        assert(recreated~=saved and recreated==options.Drunk)
        out[4]=pack(recreated())
        for _,field in ipairs{'__songlua_player_option_state','__songlua_player_option_speeds'} do
            for _,value in ipairs{false,9,'bad',{drunk='bad'},{drunk='0x10'}} do
                options[field]=value;out[#out+1]={field,outcome(options,saved,options,0.75,-1),snapshot(options)}
            end
            options[field]=nil
        end
        return snapshot(out)
    "#);
}

#[test]
fn derived_reverse_attack_and_window_queries_follow_current_state_and_style() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local out={};options:Reverse(0.2);options:Split(0.5);options:Alternate(0.25);options:Cross(0.4);options:Reverse4(0.8)
        for _,style in ipairs{'single','double','pump-double','pump-single','bogus'} do
            GAMESTATE={__songlua_current_style={{__songlua_style_key=style}}}
            for col=-1,11 do out[#out+1]={style,col,pack(options:GetReversePercentForColumn(col))} end
        end
        GAMESTATE={GetCurrentStyle=function()collectgarbage('collect');return {GetName=function()return 'double'end}end}
        out[#out+1]=pack(options:GetReversePercentForColumn(7))
        GAMESTATE={GetCurrentStyle=function()error('style')end}
        out[#out+1]=pack(options:GetReversePercentForColumn(4))
        options.__songlua_player_option_state=setmetatable({}, {__index=function(t,k)
            options.__songlua_player_option_state={randattack=1};collectgarbage('collect');return 0
        end})
        out[#out+1]=options:GetStepAttacks()
        for _,args in ipairs{pack(options),pack(options,0),pack(options,'w3'),pack(options,'TimingWindow_Mine'),
            pack(options,'0'),pack(options,2.5),pack(options,{},true),pack(4,false),pack(3,true)} do
            out[#out+1]={outcome(options,options.DisableTimingWindow,unpack(args,1,args.n)),
                options:GetDisabledTimingWindows(),__songlua_side_effect_count}
        end
        local first=options:GetDisabledTimingWindows();first[1]='changed';out[#out+1]=options:GetDisabledTimingWindows()
        out[#out+1]=outcome(options,options.ResetDisabledTimingWindows,options,false)
        return snapshot(out)
    "#);
}

#[test]
fn noteskin_callbacks_keep_receiver_rules_gc_ownership_and_partial_error_writes() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local out={};local saved=options.NoteSkin;local alternate={__songlua_noteskin_name='metal'}
        NOTESKIN.DoesNoteSkinExist=function(self,name)collectgarbage('collect');return name=='cel' or name=='42'end
        for _,args in ipairs{pack(),pack('cel'),pack(options),pack(options,'cel'),pack(alternate,42),
            pack(options,'missing',true),pack(options,{},true),pack(options,string.char(255))} do
            out[#out+1]={outcome(options,saved,unpack(args,1,args.n)),snapshot(options),snapshot(alternate),__songlua_side_effect_count}
        end
        __songlua_side_effect_count='bad'
        out[#out+1]={outcome(options,saved,options,'cel'),snapshot(options)}
        __songlua_side_effect_count=0
        NOTESKIN.DoesNoteSkinExist=setmetatable({}, {__call=function()error('must not call')end})
        out[#out+1]={outcome(options,saved,options,'cel'),snapshot(options)}
        return snapshot(out)
    "#);
}

#[test]
fn retained_scalar_method_owns_only_its_level_and_discarded_cycles_collect() {
    let mut runtime = RustSongRuntime::new(&context()).unwrap();
    test_support::expose_catalog(&mut runtime).unwrap();
    test_support::release_catalog_root(&mut runtime).unwrap();
    test_support::retire_game_state_globals(&mut runtime).unwrap();
    test_support::retire_statistics_globals(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        weak=setmetatable({},{__mode='v'})
        for p=1,2 do for l=1,4 do weak[(p-1)*4+l]=catalog.player_options[p][l] end end
        saved=catalog.player_options[1][3].Drunk
        local owner=catalog.player_options[1][3];local old=owner.__songlua_player_option_state
        weak.old=old;owner.__songlua_player_option_state={drunk=0.75};owner=nil;old=nil
        catalog=nil;collectgarbage('collect');collectgarbage('collect')
        for i=1,8 do assert((weak[i]~=nil)==(i==3)) end
        assert(weak.old==nil and saved()==0.75)
        saved=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[3]==nil)
    "#,
    );
    let mut other = RustSongRuntime::new(&context()).unwrap();
    test_support::expose_catalog(&mut other).unwrap();
    execute(
        &mut other,
        "assert(catalog.player_options[1][3]:Drunk()==0)",
    );
}

#[test]
fn deleting_eager_methods_uses_the_native_generic_fallback_and_original_owner() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local out={};local original=options
        for _,name in ipairs{'FromString','NoteSkin','XMod','AMod','GetReversePercentForColumn','GetDisabledTimingWindows'} do
            local previous=options[name];options[name]=nil
            local fn=options[name];assert(fn~=previous and options[name]==fn)
            out[#out+1]={name,outcome(options,fn,options),outcome(options,fn,{},0.5,2),snapshot(options)}
        end
        local original_index=getmetatable(original).__index
        local alternate={}
        local fn=original_index(alternate,'MoveX01')
        assert(fn==original.MoveX01 and alternate.MoveX01==nil)
        out[#out+1]={outcome(original,fn,alternate,0.25,3),snapshot(original)}
        original.__songlua_player_option_state=setmetatable({}, {__index=function(t,k)
            collectgarbage('collect');return '0x10'
        end})
        tonumber=function()error('changed primitive')end
        out[#out+1]=pack(fn())
        return snapshot(out)
    "#);
}

#[test]
fn explicit_native_diagnostics_preserve_messages_and_partial_writes() {
    let mut p = Pair::new(&context());
    p.compare(
        r#"
        local result={}
        for _,case in ipairs{
            {'Mini',pack(0.25,-1),'Arg must be greater than or equal to zero.'},
            {'Incoming',pack(0.5,-1),'Arg must be greater than or equal to zero.'},
            {'ScrollBPM',pack(400,-1),'Arg must be greater than or equal to zero.'},
            {'TimeSpacing',pack(1,-1),'Arg must be greater than or equal to zero.'},
            {'XMod',pack(2,-1),'negative option speed'},
            {'CMod',pack(0),'speed mod must be finite and greater than zero'},
            {'ModTimerSetting',pack('bogus'),'Invalid ModTimerType'},
            {'FailSetting',pack('bogus'),'Invalid FailType'},
            {'DisableTimingWindow',pack('bogus'),'Expected TimingWindow'}
        } do
            local ok,err=pcall(options[case[1]],options,unpack(case[2],1,case[2].n))
            assert(not ok and string.find(tostring(err),case[3],1,true))
            result[#result+1]={case[1],snapshot(options)}
        end
        return snapshot(result)
    "#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
