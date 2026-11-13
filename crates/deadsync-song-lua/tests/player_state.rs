use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, {SongRuntime, test_support},
};
use std::path::Path;

fn context() -> SongLuaCompileContext {
    SongLuaCompileContext::new(Path::new("."), "Player state migration")
}
fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/player-state.lua").unwrap())
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
        let mut rust = SongRuntime::new(context).unwrap();
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
            "song/tests/rust_player_state.rs::Pair::compare",
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
states=catalog.player_states
groups=catalog.option_operations
players=catalog.player_options
songs=catalog.song_options
function all_options() return snapshot({players,songs}) end
options=players[1][3]
NOTESKIN={DoesNoteSkinExist=function(self,name) return name=='cel' or name=='metal' or name=='my skin' end,
GetNoteSkinNames=function()return {'cel','metal','my skin'}end}
"#;

#[test]
fn player_state_surface_identity_and_level_selection_match_native() {
    for live in [false, true] {
        let mut c = context();
        c.live = live;
        c.players[0].enabled = true;
        c.players[1].enabled = false;
        let mut p = Pair::new(&c);
        p.compare(
            r#"
            local out={}
            for pn=1,2 do
                local s=states[pn]
                out[#out+1]={s:GetPlayerNumber(),s:GetPlayerController(),s:GetHealthState()}
                assert(s:GetCurrentPlayerOptions()==players[pn][4])
                assert(s.GetCurrentPlayerOptions(false,123)==players[pn][4])
                assert(s:GetSongPosition()==s.GetSongPosition(nil))
                assert(s:GetSongPosition()~=states[3-pn]:GetSongPosition())
                for i,name in ipairs{'Preferred','Stage','Song','Current'} do
                    assert(s:GetPlayerOptions(i-1)==players[pn][i])
                    assert(s:GetPlayerOptions('ModsLevel_'..name)==players[pn][i])
                    assert(s.GetPlayerOptions(string.lower(name))==players[pn][i])
                    out[#out+1]={s:GetPlayerOptionsString(name),s:GetPlayerOptionsArray(name)}
                end
                for _,value in ipairs{false,{},-1,4,0.5,'0','ModsLevel_song','unknown'} do
                    out[#out+1]=outcome(s,s.GetPlayerOptions,s,value)
                    out[#out+1]=outcome(s,s.GetPlayerOptionsString,s,value)
                end
                assert(not pcall(s.GetPlayerOptions,s))
            end
            return snapshot(out)
        "#,
        );
    }
}

#[test]
fn strings_arrays_preserve_native_order_rounding_aliases_and_skin_capitalization() {
    let mut p = Pair::new(&context());
    let mut names = song::SONG_LUA_PLAYER_OPTION_CAPABILITIES
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    for prefix in song::SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES {
        for col in 1..=16 {
            names.push(format!("{prefix}{col}"));
        }
    }
    // Populate all known fields, including column modifiers, with varied raw values.
    let names = names
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(",");
    p.compare(&format!(r#"
        local o=players[1][4];local state=o.__songlua_player_option_state
        for i,name in ipairs{{{names}}} do state[string.lower(name)]=({{0.125,0.375,1,-0.125,true,false}})[i%6+1] end
        state.randattack=0.375;state.noattack=1
        -- Column values use strict numeric extraction, unlike ordinary display fields.
        for k,v in pairs(state) do if type(v)=='boolean' then state[k]=v and 1 or 0 end end
        return states[1]:GetPlayerOptionsString(3),snapshot(states[1]:GetPlayerOptionsArray(3))
    "#));
    p.compare(
        r#"
        local out={};local o=players[1][4];local s=o.__songlua_player_option_state
        for _,value in ipairs{0,0.5,1,2,3,4,-0.5,'2.9',-2,math.huge,false,{}} do
            s.failsetting=value;s.modtimersetting=value
            out[#out+1]=outcome(states[1],states[1].GetPlayerOptionsString,states[1],3)
        end
        s.failsetting=nil;s.modtimersetting=nil
        for _,mode in ipairs{'xmod','cmod','mmod','amod','camod','unknown'} do
            o.__songlua_speedmod_active=mode;o['__songlua_speedmod_'..mode]=123.125
            out[#out+1]=states[1]:GetPlayerOptionsString(3)
        end
        for _,tilt in ipairs{0,0.125,-0.375,1} do for _,skew in ipairs{0,tilt,-tilt,0.6} do
            s.tilt=tilt;s.skew=skew;out[#out+1]=states[1]:GetPlayerOptionsString(3)
        end end
        for _,skin in ipairs{'','cel','metal','école','øskin','ÿskin','ßskin','αskin'} do
            o.__songlua_noteskin_name=skin;out[#out+1]=states[1]:GetPlayerOptionsString(3)
        end
        local a=states[1]:GetPlayerOptionsArray(3);a[1]='changed'
        assert(states[1]:GetPlayerOptionsArray(3)[1]=='NoHideLights')
        return snapshot(out)
    "#,
    );
}

#[test]
fn set_player_options_preserves_owners_and_propagates_from_the_requested_level() {
    for level in 0..4 {
        let mut p = Pair::new(&context());
        p.compare(r#"
            saved={}
            for i,o in ipairs(players[1]) do
                o:FromString('50% mini, *2 25% reverse, metal')
                o:DisableTimingWindow('W3');o.custom='keep';o.Mini=function() return 'override' end
                saved[i]={o,o.__songlua_player_option_state,o.__songlua_player_option_speeds,o.__songlua_disabled_timing_windows,o.Mini}
            end
        "#);
        p.compare(&format!(r#"
            local result=pack(states[1]:SetPlayerOptions({level},'2x, *3 75% drunk, FailOff, ModTimerBeat'))
            local identities={{}}
            for i,o in ipairs(players[1]) do
                local old=saved[i]
                identities[i]={{o==old[1],o.__songlua_player_option_state==old[2],o.__songlua_player_option_speeds==old[3],o.__songlua_disabled_timing_windows==old[4],o.Mini==old[5],o.custom,o:NoteSkin()}}
            end
            return all_options(),snapshot(identities),snapshot(result),__songlua_side_effect_count
        "#));
        p.compare(
            r#"
            local out={}
            for _,value in ipairs{false,12,{},'','default','metal'} do
                out[#out+1]=pack(states[1]:SetPlayerOptions(0,value))
                out[#out+1]=all_options()
            end
            states[1].SetPlayerOptions(0,'2x') -- no table receiver, no change
            return snapshot(out),all_options(),__songlua_side_effect_count
        "#,
        );
    }
}

#[test]
fn set_player_options_writes_current_first_and_keeps_partial_failure_state() {
    let mut p = Pair::new(&context());
    p.compare(
        r#"
        players[1][1].__songlua_player_option_state=false
        local ok=pcall(states[1].SetPlayerOptions,states[1],0,'50% mini')
        assert(not ok and players[1][4]:Mini()==0.5)
        return all_options(),__songlua_side_effect_count
    "#,
    );
    let mut p = Pair::new(&context());
    p.compare(
        r#"
        local count=0
        NOTESKIN.DoesNoteSkinExist=function(self,name)
            count=count+1;collectgarbage('collect');return name=='metal'
        end
        NOTESKIN.GetNoteSkinNames=function() collectgarbage('collect');return {'metal'} end
        local out=pack(states[1]:SetPlayerOptions(1,'metal, 50% mini'))
        return snapshot(out),all_options(),count,__songlua_side_effect_count
    "#,
    );
}

#[test]
fn restart_apply_and_attack_rebuild_keep_level_identities_and_master_song_ownership() {
    let mut p = Pair::new(&context());
    for script in [
        "groups:Apply(0,0,'1.5x, 25% mini, 1.5xMusic, *2 30% haste, metal');return all_options()",
        "groups:Apply(1,1,'50% reverse, 2xMusic');return all_options()",
        "groups:Restart();return all_options()",
        "groups:RebuildAttacks(0,false,{'*2 75% mini, 3xMusic','25% reverse'});return all_options()",
        "groups:RebuildAttacks(1,true,{'*2 50% mini, 3xMusic','2.5xMusic, 50% haste'});return all_options()",
        "groups:ClearAttacks();return all_options()",
        "groups:RebuildAttacks(0,true,{});return all_options()",
        "groups:Restart();return all_options()",
    ] {
        p.compare(script);
    }
    p.compare(r#"
        for pn=1,2 do for level=1,4 do assert(states[pn]:GetPlayerOptions(level-1)==players[pn][level]) end end
        return __songlua_side_effect_count
    "#);
}

#[test]
fn group_updates_preserve_f32_rates_independent_deltas_and_cmod_speed() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        PREFSMAN.GetPreference=function(self,key)assert(key=='RateModsAffectFGChanges');return true end
        groups:Apply(0,2,'*0.7 3xMusic, *0.3 80% haste, *1.3 90% drunk, *1.7 3x, metal')
        groups:Apply(1,2,'*1.1 C450, *0.6 80% reverse')
        -- Apply touches Current too. Restore values so subsequent updates approach.
        for pn=1,2 do players[pn][4]:FromString('default') end
        songs[4]:MusicRate(1);songs[4]:Haste(0)
        return all_options()
    "#);
    for script in [
        "groups:Advance(0.1234564);return all_options()",
        "groups:Advance(0.12345649);return all_options()",
        "groups:Advance(0.12345651);return all_options()",
        "groups:Advance(0.2,{0.07,0.13,0.19});return all_options()",
        "PREFSMAN.GetPreference=function()return false end;groups:Advance(0.3);return all_options()",
        "groups:Advance(20);return all_options()",
    ] {
        p.compare(script);
    }
}

#[test]
fn updates_copy_discrete_values_and_windows_but_approach_numeric_fields_in_sorted_order() {
    let mut p = Pair::new(&context());
    p.compare(
        r#"
        local target=players[1][3];local current=players[1][4]
        target:FromString('*2 50% drunk, FailOff, ModTimerSong, metal')
        target:DisableTimingWindow('W3')
        target.__songlua_player_option_state.custom='string'
        target.__songlua_player_option_state.bool=true
        current.__songlua_player_option_state.currentonly=0.8
        current:FailSetting('FailType_Immediate')
        current.__songlua_player_option_speeds={drunk=77}
        groups:Advance(0.1)
        assert(current.__songlua_disabled_timing_windows~=target.__songlua_disabled_timing_windows)
        return all_options()
    "#,
    );
    p.compare(
        r#"
        players[1][3].__songlua_player_option_state.aaa=1
        players[1][3].__songlua_player_option_state.zzz=1
        players[1][4].__songlua_player_option_state.zzz={}
        local ok=pcall(groups.Advance,groups,0.2)
        assert(not ok)
        assert(players[1][4].__songlua_player_option_state.aaa>0)
        return all_options()
    "#,
    );
}

#[test]
fn clock_validation_and_callback_failures_preserve_native_commit_boundaries() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local out={}
        for _,seconds in ipairs{-1,0/0,math.huge,1e14} do out[#out+1]=pcall(groups.Advance,groups,seconds) end
        groups:Advance(1)
        out[#out+1]=pcall(groups.Advance,groups,0.5)
        out[#out+1]=pcall(groups.Advance,groups,2,{0.1,-1,0.1})
        out[#out+1]=pcall(groups.Advance,groups,1.5)
        groups:Restart();out[#out+1]=pcall(groups.Advance,groups,0)
        groups:Apply(0,2,'3xMusic, 50% mini');songs[4]:MusicRate(1);players[1][4]:Mini(0)
        PREFSMAN.GetPreference=function()error('preference failure')end
        out[#out+1]=pcall(groups.Advance,groups,1)
        out[#out+1]=songs[4]:MusicRate();out[#out+1]=players[1][4]:Mini()
        PREFSMAN.GetPreference=function()return true end
        out[#out+1]=pcall(groups.Advance,groups,0.5)
        groups:Advance(1);out[#out+1]=players[1][4]:Mini()
        return snapshot(out),all_options()
    "#);
}

#[test]
fn advancement_deduplicates_coerced_keys_before_ordered_partial_writes() {
    let mut runtime = SongRuntime::new(&context()).unwrap();
    test_support::expose_catalog(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        current=catalog.player_options[1][4]
        target=catalog.player_options[1][3]
        current.__songlua_player_option_state={[1]=0,['1']=0,aaa=0,zzz={}}
        target.__songlua_player_option_state={[1]=99,['1']=1,aaa=1,zzz=1}
    "#,
    );
    assert!(runtime.advance_options(1.0, Some([0.25; 3])).is_err());
    execute(
        &mut runtime,
        r#"
        local values=current.__songlua_player_option_state
        assert(values[1]==0)
        assert(values['1']==0.25 and values.aaa==0.25)
        assert(type(values.zzz)=='table')
    "#,
    );
}

#[test]
fn advancement_preserves_long_and_unicode_option_keys() {
    let mut runtime = SongRuntime::new(&context()).unwrap();
    test_support::expose_catalog(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        local long=string.rep('long_modifier_',6)
        local current=catalog.player_options[1][4]
        local target=catalog.player_options[1][3]
        current.__songlua_player_option_state={[long]=0,['été']=0}
        target.__songlua_player_option_state={[long]=1,['été']=1}
        "#,
    );
    runtime.advance_options(0.25, Some([0.25; 3])).unwrap();
    execute(
        &mut runtime,
        r#"
        local long=string.rep('long_modifier_',6)
        local current=catalog.player_options[1][4].__songlua_player_option_state
        assert(current[long]==0.25 and current['été']==0.25)
        "#,
    );
    execute(
        &mut runtime,
        "catalog.player_options[1][3].__songlua_player_option_state={[string.char(255)]=1}",
    );
    assert!(runtime.advance_options(0.5, Some([0.25; 3])).is_err());
}
#[test]
fn preference_reentry_gc_and_public_alias_replacement_preserve_native_owners() {
    let mut p = Pair::new(&context());
    p.compare(
        r#"
        groups:Apply(0,2,'3xMusic, *0.4 50% mini')
        groups:Apply(1,2,'*0.3 75% mini')
        songs[4]:MusicRate(1);players[1][4]:Mini(0);players[2][4]:Mini(0)
        local calls=0;local trace={}
        PREFSMAN.GetPreference=function(self,key)
            calls=calls+1
            trace[#trace+1]={songs[4]:MusicRate(),players[1][4]:Mini(),players[2][4]:Mini()}
            if calls==1 then
                catalog.player_options={};catalog.song_options={};catalog.player_states={}
                groups:Advance(1.2,{0.1,0.2,0.3})
                collectgarbage('collect')
            end
            return true
        end
        groups:Advance(1,{0.1,0.4,0.5})
        assert(states[1]:GetCurrentPlayerOptions()==players[1][4])
        return snapshot(trace),all_options(),calls
    "#,
    );
}

#[test]
fn controller_and_health_queries_read_published_phase_results() {
    use deadsync_song_lua::PlayerStateStatus;
    let mut c = context();
    c.players[0].enabled = true;
    c.players[1].enabled = false;
    let mut p = Pair::new(&c);
    let query = "return states[1]:GetPlayerController(),states[2]:GetPlayerController(),states[1]:GetHealthState(),states[2]:GetHealthState(),__songlua_side_effect_count";
    for (autoplay, health) in [
        (None, [None, None]),
        (
            Some([true, false]),
            [Some("HealthState_Danger"), Some("HealthState_Hot")],
        ),
        (
            Some([false, true]),
            [Some("HealthState_Dead"), Some("HealthState_Alive")],
        ),
        (None, [None, Some("HealthState_Danger")]),
    ] {
        p.rust
            .set_player_state_status(std::array::from_fn(|index| PlayerStateStatus {
                autoplay: autoplay.map(|v| v[index]),
                health: health[index],
            }));
        p.compare(query);
        p.compare("collectgarbage('collect'); return");
        p.compare(query);
    }
}

#[test]
fn player_positions_retain_original_backing_and_queries_do_not_advance_options() {
    for live in [false, true] {
        let mut c = context();
        c.live = live;
        let mut p = Pair::new(&c);
        p.compare(r#"
            r=__songlua_compile_song_runtime
            r.__songlua_song_beat=8;r.__songlua_music_seconds=3;r.__songlua_music_rate=1.5
            if r.__songlua_player_positions then
                r.__songlua_player_positions[1].__songlua_song_beat=9
                r.__songlua_player_positions[2].__songlua_song_beat=10
            end
            groups:Apply(0,2,'50% mini');players[1][4]:Mini(0)
            local out={}
            for pn=1,2 do
                local pos=states[pn]:GetSongPosition()
                out[pn]={pos:GetSongBeat(),pos:GetMusicSeconds(),pos:GetCurBPS(),pos:GetFreeze(),pos:GetDelay()}
            end
            __songlua_compile_song_runtime={}
            collectgarbage('collect')
            return snapshot(out),states[1]:GetSongPosition():GetSongBeat(),players[1][4]:Mini()
        "#);
    }
}

#[test]
fn host_phase_entry_points_match_the_native_operations() {
    let mut p = Pair::new(&context());

    p.rust
        .apply_modifiers(0, 0, "*2 50% mini, 1.5xMusic, metal")
        .unwrap();
    p.compare("return all_options()");

    p.rust.restart_options().unwrap();

    p.rust
        .rebuild_attack_options(0, true, &["*3 90% mini, 2xMusic".into()])
        .unwrap();

    p.rust.advance_options(0.25, Some([0.2, 0.1, 0.3])).unwrap();
    p.compare("return all_options()");

    p.rust.clear_attack_options().unwrap();
    p.rust.advance_options(1.0, None).unwrap();
    p.compare("return all_options()");
    assert!(p.rust.advance_options(0.5, None).is_err());
    assert!(p.rust.apply_modifiers(2, 0, "2x").is_err());
    assert!(p.rust.apply_modifiers(0, 4, "2x").is_err());
}

#[test]
fn saved_state_queries_survive_gc_and_release_unneeded_owners() {
    let mut runtime = SongRuntime::new(&context()).unwrap();
    test_support::expose_catalog(&mut runtime).unwrap();
    test_support::release_catalog_root(&mut runtime).unwrap();
    test_support::retire_game_state_globals(&mut runtime).unwrap();
    test_support::retire_statistics_globals(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        local s=catalog.player_states[1]
        savedCurrent=s.GetCurrentPlayerOptions;savedPosition=s.GetSongPosition
        savedController=s.GetPlayerController;savedNumber=s.GetPlayerNumber
        weak=setmetatable({s,s:GetPlayerOptions(0),s:GetCurrentPlayerOptions()},{__mode='v'})
        catalog=nil
    "#,
    );
    execute(
        &mut runtime,
        r#"
        collectgarbage('collect');collectgarbage('collect')
        assert(weak[1]==nil and weak[2]==nil)
        assert(savedCurrent()==weak[3]);assert(savedPosition():GetSongBeat()==0)
        assert(savedController()=='PlayerController_Human');assert(savedNumber()=='PlayerNumber_P1')
        savedCurrent=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[3]==nil)
    "#,
    );
}

#[test]
fn raw_option_state_and_captured_primitives_ignore_public_replacements() {
    let mut p = Pair::new(&context());
    p.compare(
        r#"
        local originalNext=next;local originalNumber=tonumber
        local target=players[1][3];local current=players[1][4]
        target.__songlua_player_option_state=setmetatable({drunk=0.5,failsetting='0x2'},
            {__index=function()error('state fallback must not run')end})
        target.__songlua_player_option_speeds=setmetatable({drunk='0x2'},
            {__index=function()error('speed fallback must not run')end})
        current.__songlua_player_option_state={drunk='0'}
        next=function()error('replaced next')end;tonumber=function()error('replaced tonumber')end
        groups:Advance(0.1)
        local text=states[1]:GetPlayerOptionsString(3)
        states[1]:SetPlayerOptions(1,'75% mini')
        next=originalNext;tonumber=originalNumber
        return text,all_options()
    "#,
    );
    p.compare(r#"
        local out={};local target=players[1][3];local current=players[1][4]
        target.__songlua_player_option_state={drunk=1};target.__songlua_player_option_speeds={drunk=1}
        for i,value in ipairs{false,{},'bad',math.huge,0/0} do
            current.__songlua_player_option_state={drunk=value}
            out[#out+1]=pcall(groups.Advance,groups,i)
            out[#out+1]=snapshot(current.__songlua_player_option_state)
        end
        return snapshot(out)
    "#);
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
