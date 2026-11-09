use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};
use std::path::Path;

fn context() -> SongLuaCompileContext {
    SongLuaCompileContext::new(Path::new("."), "GameState migration")
}
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/game-state.lua").unwrap())
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
            "song/tests/rust_game_state.rs::Pair::compare",
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
fn full_class_surface_defaults_and_selector_argument_rules_match_production() {
    for enabled in [[true, true], [true, false], [false, true], [false, false]] {
        let mut c = context();
        for (i, value) in enabled.into_iter().enumerate() {
            c.players[i].enabled = value;
            c.players[i].autoplay = 1.0;
        }
        let mut p = Pair::new(&c);
        p.compare(r#"
            local names={};for k in pairs(GameState)do names[#names+1]=k end;table.sort(names)
            assert(getmetatable(GAMESTATE)=='(hidden)' and rawget(GAMESTATE,'GetCurrentSong')==nil)
            assert(GAMESTATE.__songlua_class_methods==GameState)
            local out={names}
            for _,name in ipairs{'GetMasterPlayerNumber','GetNumSidesJoined','GetNumPlayersEnabled','IsCourseMode','IsEventMode','GetCoinMode','GetPremium','GetCoins','GetCoinsNeededToJoin','GetNumStagesLeft','GetCurrentStageIndex','GetCourseSongIndex','GetPlayMode','GetSortOrder','GetPlayerFailType'}do out[#out+1]={name,GAMESTATE[name](GAMESTATE)} end
            assert(GAMESTATE:GetCurrentSong()==catalog.song and GAMESTATE:GetCurrentCourse()==catalog.course)
            assert(GAMESTATE:GetCurrentGame()==GAMESTATE:GetCurrentGame())
            assert(GAMESTATE:GetCurrentGame():GetName()=='dance')
            assert(GAMESTATE:GetEnabledPlayers()==GAMESTATE:GetHumanPlayers())
            for _,v in ipairs{0,1,-1,2,0.5,'P1','p2','playernumber_p1','PLAYERNUMBER_P2','0',false,{}}do
                out[#out+1]={GAMESTATE:IsPlayerEnabled(v),GAMESTATE:IsHumanPlayer(v),GAMESTATE:IsSideJoined(v),IsHumanPlayer(v),GAMESTATE:GetPlayerState(v)~=nil,GAMESTATE:GetCurrentTrail(v)~=nil,(pcall(GAMESTATE.GetCurrentSteps,GAMESTATE,v))}
            end
            assert(GAMESTATE.GetPlayerState(0)==nil and GameState.GetPlayerState({},0)==states[1])
            assert(GAMESTATE:GetPlayerState(0)==states[1] and GAMESTATE:GetPlayerState(1)==states[2])
            assert(GetSongAndSteps('invalid')==catalog.song)
            return snapshot(out),IsAutoplay(0),IsAutoplay(1)
        "#);
    }
}

#[test]
fn class_overrides_saved_methods_and_private_selection_slots_keep_native_identity() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local original=GAMESTATE;local getter=GameState.GetCurrentSong;local setter=GameState.SetCurrentSong
        local fake={tag='fake'};local out={}
        GameState.GetCurrentSong=function(self)return fake end
        assert(original:GetCurrentSong()==fake and getter()==catalog.song)
        original.GetCurrentSong=function()return 'own' end
        assert(original:GetCurrentSong()=='own');original.GetCurrentSong=nil
        GameState.Custom=function(self)return self end;assert(original:Custom()==original)
        assert(select('#',setter(original,fake))==0 and getter()==fake)
        assert(GetSongAndSteps()==fake)
        for _,v in ipairs{false,0,'bad'}do setter(original,v);assert(getter()==fake)end
        local trail={tag='trail'};original:SetCurrentTrail('p2',trail)
        assert(original:GetCurrentTrail(1)==trail)
        original:SetCurrentTrail('p2',nil);assert(original:GetCurrentTrail(1)==trail)
        local count=__songlua_side_effect_count;original:SetCurrentTrail('bad',trail)
        assert(__songlua_side_effect_count==count+1)
        GameState.GetPlayerState=function()return {GetPlayerController=function()return 'PlayerController_Autoplay'end}end
        assert(IsAutoplay('bad'))
        GAMESTATE={};collectgarbage('collect')
        assert(getter()==fake and original:Custom()==original)
        return __songlua_side_effect_count,snapshot(original.__songlua_current_style),snapshot(trail)
    "#);
}

#[test]
fn environment_values_keep_receiver_conversion_and_metamethod_reentry() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local env=GAMESTATE:Env();assert(env==GAMESTATE:Env())
        setenv('key',{v=1});assert(getenv('key')==env.key)
        setenv(42,'numeric');assert(getenv('42')=='numeric')
        setenv('key',nil);assert(getenv('key')==nil)
        local calls={};setmetatable(env,{__index=function(self,key)
            calls[#calls+1]='get:'..key;collectgarbage('collect');return 'value'
        end,__newindex=function(self,key,value)
            calls[#calls+1]='set:'..key;collectgarbage('collect');rawset(self,key,value)
        end})
        setenv('reentrant','stored');assert(getenv('missing')=='value')
        GameState.Env=function()return {}end;assert(getenv('reentrant')=='stored')
        local out={};for _,bad in ipairs{false,{},function()end}do out[#out+1]=pcall(getenv,bad);out[#out+1]=pcall(setenv,bad,1)end
        return snapshot(calls),snapshot(out),__songlua_side_effect_count
    "#);
}

#[test]
fn styles_helpers_clock_and_raw_event_preference_match_native() {
    for live in [false, true] {
        let mut c = context();
        c.live = live;
        c.song_music_rate = 1.5;
        c.music_length_seconds = 123.5;
        c.screen_height = 720.0;
        c.players[0].screen_x = 100.;
        c.players[1].screen_x = 500.;
        let mut p = Pair::new(&c);
        p.compare(r#"
            local out={};local r=__songlua_compile_song_runtime
            r.__songlua_song_beat=1.123456789;r.__songlua_song_bps=2.234567891;r.__songlua_music_seconds=3.345678912
            out[#out+1]={GAMESTATE:GetSongBeat(),GAMESTATE:GetSongBPS(),GAMESTATE:GetCurMusicSeconds(),currentTimeSongOrCourse(),totalLengthSongOrCourse()}
            assert(GAMESTATE:GetSongPosition()~=states[1]:GetSongPosition())
            for _,style in ipairs{'single','double','solo','versus','unknown'}do
                local old=GAMESTATE:GetCurrentStyle();assert(select('#',GAMESTATE:SetCurrentStyle(style))==0)
                local current=GAMESTATE:GetCurrentStyle();assert(current~=old)
                out[#out+1]={current:GetName(),current:GetStepsType(),current:ColumnsPerPlayer(),GetNotefieldWidth(),current:GetWidth()}
            end
            GameState.GetCurrentStyle=function()error('must use private style')end
            out[#out+1]=GetNotefieldWidth()
            PREFSMAN.GetPreference=function()error('must use raw preference')end
            PREFSMAN.__songlua_preference_values.EventMode='truthy';out[#out+1]=GAMESTATE:IsEventMode()
            out[#out+1]={GetNotefieldX('p1'),GetNotefieldX(1),GetNotefieldX('bad')}
            out[#out+1]=GetGameplayLayout('p1',false);out[#out+1]=GetGameplayLayout('p2',true)
            out[#out+1]={IsW0Judgment(),IsW010Judgment(),GetDefaultFailType(),GetComboThreshold()}
            __songlua_compile_song_runtime={};collectgarbage('collect')
            return snapshot(out),GAMESTATE:GetSongBeat(),currentTimeSongOrCourse(),__songlua_side_effect_count
        "#);
    }
}

#[test]
fn display_bpm_helpers_preserve_dynamic_getter_order_and_fallbacks() {
    let mut c = context();
    c.song_display_bpms = [100., 200.];
    c.song_music_rate = 1.25;
    let mut p = Pair::new(&c);
    p.compare(r#"
        local log={};local function mark(s)log[#log+1]=s;collectgarbage('collect')end
        local timing={GetActualBPM=function()mark('actual');return {'120','240'}end}
        local value={GetDisplayBpms=function()mark('display');return {-1,0}end,GetTimingData=function()mark('timing');return timing end}
        local out={GetDisplayBPMs(),StringifyDisplayBPMs(),GetDisplayBPMs(nil,value,1.5),StringifyDisplayBPMs(nil,value,1.5)}
        value.GetDisplayBpms=function()mark('positive');return {123.4,234.5}end
        out[#out+1]=GetDisplayBPMs(nil,value,'2');out[#out+1]=StringifyDisplayBPMs(nil,value,'2')
        value.GetDisplayBpms=false;out[#out+1]=pcall(GetDisplayBPMs,nil,value)
        value.GetDisplayBpms=nil;value.GetTimingData=function()return nil end
        out[#out+1]=GetDisplayBPMs(nil,value)
        return snapshot(out),snapshot(log)
    "#);
}

#[test]
fn game_commands_apply_clauses_in_order_with_legacy_one_based_numeric_players() {
    let mut p = Pair::new(&context());
    for script in [
        "assert(GAMESTATE:ApplyGameCommand('mod,25% mini;stagemod,75% reverse;mod,50% mini')==GAMESTATE);return all_options(),__songlua_side_effect_count",
        "GAMESTATE:ApplyGameCommand('stagemod,2x, 1.5xMusic',1);return all_options(),__songlua_side_effect_count",
        "GAMESTATE:ApplyGameCommand('mod,3x',2.75);return all_options(),__songlua_side_effect_count",
        "GAMESTATE:ApplyGameCommand('mod,metal;noop,\"stagemod,reverse;mod,4x\";mod,25% drunk','playernumber_p2');return all_options(),__songlua_side_effect_count",
        "assert(GAMESTATE:ApplyStageModifiers('p1','C450, 2xMusic')==GAMESTATE);return all_options(),__songlua_side_effect_count",
        "assert(GAMESTATE:SetSongOptions('Stage','1.25xMusic, haste')==GAMESTATE);return GAMESTATE:GetSongOptionsString(),GAMESTATE:GetSongOptions('Stage'),all_options(),__songlua_side_effect_count",
    ] {
        p.compare(script);
    }
    p.compare(r#"
        local out={}
        for _,p in ipairs{0,3,-1,0.5,'0','bad',false,{}}do out[#out+1]=pcall(GAMESTATE.ApplyGameCommand,GAMESTATE,'mod,2x',p)end
        for _,text in ipairs{false,1,{}}do out[#out+1]=pcall(GAMESTATE.ApplyGameCommand,GAMESTATE,text);out[#out+1]=pcall(GAMESTATE.ApplyStageModifiers,GAMESTATE,0,text)end
        for _,name in ipairs{'InsertCoin','SaveProfiles','AddStageToPlayer','ResetPlayerOptions','SetPreferredDifficulty','UnjoinPlayer','JoinPlayer'}do out[#out+1]=pack(GAMESTATE[name](GAMESTATE,false,{}))end
        return snapshot(out),all_options(),__songlua_side_effect_count
    "#);
}

#[test]
fn modifier_queries_compare_current_values_without_mutating_live_options() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        local o=GAMESTATE:GetPlayerState(0):GetCurrentPlayerOptions();local out={}
        local function query(text)out[#out+1]={text,GAMESTATE:PlayerIsUsingModifier(0,text)}end
        for _,text in ipairs{'1x','2x','no reverse','reverse','unknown-modifier','cel','metal','1xMusic','2xMusic','50% dark'}do query(text)end
        o:Reverse(0.5);query('*999 50% reverse');query('reverse, no reverse, 50% reverse')
        o:XMod(2);query('2x');o:CMod(500);query('C500');query('2x');o:XMod(2);query('2x');query('C500')
        GAMESTATE:GetSongOptionsObject(3):MusicRate(1.5);query('1.5xmusic');query('1xmusic')
        local before=all_options()
        for _,text in ipairs{'NoAttacks','modtimergame','default','overhead','metal','unknown','2x, no reverse'}do query(text)end
        assert(all_options()==before)
        return snapshot(out),all_options(),__songlua_side_effect_count
    "#);
}

fn selection_context() -> SongLuaCompileContext {
    let mut c = context();
    c.players[1].enabled = true;
    c.song_steps = [
        (
            song::SongLuaDifficulty::Easy,
            "same.ogg",
            "StepsType_Dance_Single",
        ),
        (
            song::SongLuaDifficulty::Hard,
            "same.ogg",
            "StepsType_Dance_Single",
        ),
        (
            song::SongLuaDifficulty::Challenge,
            "other.ogg",
            "StepsType_Dance_Single",
        ),
        (
            song::SongLuaDifficulty::Medium,
            "same.ogg",
            "StepsType_Dance_Routine",
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (difficulty, music, kind))| song::SongLuaStep {
        steps_type: kind.into(),
        difficulty,
        description: format!("chart-{i}"),
        chart_name: String::new(),
        credit: String::new(),
        meter: 8,
        music_file: music.into(),
        possible_grade_points: 0,
    })
    .collect();
    c.player_step_indices = [Some(0), Some(1)];
    c
}

#[test]
fn selection_uses_authored_metadata_and_broadcasts_even_unchanged_pointers() {
    let mut p = Pair::new(&selection_context());
    p.compare(r#"
        __songlua_broadcast_messages={};local all=catalog.song:GetAllSteps()
        all[1].GetDifficulty=function()return 'Difficulty_Edit'end
        all[2].GetMusicPath=function()return 'different'end
        assert(GAMESTATE:GetEasiestStepsDifficulty()=='Difficulty_Easy')
        assert(GAMESTATE:SetCurrentSteps(0,all[1])==GAMESTATE)
        assert(GAMESTATE:GetCurrentSteps(1)==all[2]) -- authored music is identical
        GAMESTATE:SetCurrentSteps(0,all[3]);assert(GAMESTATE:GetCurrentSteps(1)==all[3])
        GAMESTATE:SetCurrentSteps(0,all[3])
        local out={pcall(GAMESTATE.SetCurrentSteps,GAMESTATE,0,all[4]),pcall(GAMESTATE.SetCurrentSteps,GAMESTATE,0,{}),(pcall(GAMESTATE.SetCurrentSteps,GAMESTATE,'bad',all[1]))}
        GAMESTATE:SetCurrentSteps(0,nil);out[#out+1]=GAMESTATE:GetEasiestStepsDifficulty()
        GAMESTATE:SetCurrentSteps(1,nil);out[#out+1]=GAMESTATE:GetEasiestStepsDifficulty() or 'nil'
        return snapshot(out),snapshot(__songlua_broadcast_messages),__songlua_side_effect_count
    "#);
}

#[test]
fn frozen_current_option_assertions_run_through_real_gamestate_and_playerstate() {
    let mut p = Pair::new(&context());
    p.compare(r#"
        function _ITG_OPTIONS_AT(player,level,method,...)
            if player==-1 then
                if method=='SetSongOptions'then return GAMESTATE:SetSongOptions(level,...)end
                local options=GAMESTATE:GetSongOptionsObject(level);return options[method](options,...)
            end
            local state=GAMESTATE:GetPlayerState(player)
            if method=='SetPlayerOptions'then return state:SetPlayerOptions(level,...)end
            local options=state:GetPlayerOptions(level);return options[method](options,...)
        end
        function _ITG_OPTIONS_LEVEL(value)
            local chosen=GAMESTATE:GetSongOptionsObject(value)
            for _,name in ipairs{'Preferred','Stage','Song','Current'}do if chosen==GAMESTATE:GetSongOptionsObject(name)then return 'ModsLevel_'..name end end
        end
        function _ITG_USING_MODIFIER(player,text)return GAMESTATE:PlayerIsUsingModifier(player,text)end
        function _ITG_OPTIONS_ADVANCE(seconds,scaled) groups:Advance(seconds) end
        function _ITG_OPTIONS_RATE_TWEENS(value)
            local old=PREFSMAN:GetPreference('RateModsAffectFGChanges')
            if type(value)=='boolean'then PREFSMAN:SetPreference('RateModsAffectFGChanges',value)end
            return old
        end
    "#);
    for source in [
        include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/current-options/window-control.json"
        ),
        include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/current-options/native-control.json"
        ),
        include_str!(
            "../../../tests/fixtures/itgmania-song-lua-micro/current-options/speed-query-control.json"
        ),
    ] {
        let data: serde_json::Value = serde_json::from_str(source).unwrap();
        p.compare(data["lua_assertions"].as_str().unwrap());
    }
}

#[test]
fn synchronous_step_messages_reenter_before_forcing_other_players_and_restore_error_guards() {
    let c = selection_context();
    let source = r#"
        seen={};phase='';depth=0;all=GAMESTATE:GetCurrentSong():GetAllSteps()
        local function observe(name)
            local a=GAMESTATE:GetCurrentSteps(0);local b=GAMESTATE:GetCurrentSteps(1)
            seen[#seen+1]={name,a and a:GetDescription() or 'nil',b and b:GetDescription() or 'nil',__songlua_active_broadcast}
        end
        return Def.Actor{
            CurrentStepsP1ChangedMessageCommand=function()
                observe('P1')
                if phase=='reenter' and depth==0 then depth=1;GAMESTATE:SetCurrentSteps(0,all[2]);depth=0 end
                if phase=='error'then error('selection listener failure')end
            end,
            CurrentStepsP2ChangedMessageCommand=function()observe('P2')end,
            ScriptErrorMessageCommand=function(self,params)
                seen[#seen+1]={'ScriptError',params.message}
                if phase=='warning'then GAMESTATE:GetEasiestStepsDifficulty()end
                if phase=='warning-error'then error('warning listener failure')end
            end
        }
    "#;
    let mut rust = RustSongRuntime::new(&c).unwrap();
    test_support::expose_catalog(&mut rust).unwrap();
    execute(&mut rust, SNAPSHOT);
    execute(&mut rust, SETUP);
    let definition = execute(&mut rust, source)[0];
    let scene = rust.instantiate_actor(definition).unwrap();
    let root = rust.actor_scene_root(scene).unwrap();
    rust.vm_mut().set_global(b"root", root).unwrap();
    let mut p = Pair { rust };
    p.compare(r#"
        __songlua_broadcast_messages={};__songlua_active_broadcast='outer';phase='reenter'
        GAMESTATE:SetCurrentSteps(0,all[3]);assert(GAMESTATE:GetCurrentSteps(0)==all[2] and GAMESTATE:GetCurrentSteps(1)==all[2])
        assert(__songlua_active_broadcast=='outer')
        return snapshot(seen),snapshot(__songlua_broadcast_messages),__songlua_side_effect_count
    "#);
    p.compare(
        r#"
        phase='error';local before=__songlua_side_effect_count
        assert(not pcall(GAMESTATE.SetCurrentSteps,GAMESTATE,0,all[3]))
        assert(GAMESTATE:GetCurrentSteps(0)==all[3] and GAMESTATE:GetCurrentSteps(1)==all[2])
        assert(__songlua_side_effect_count==before and __songlua_active_broadcast=='outer')
        phase='';__songlua_suppress_broadcast_dispatch=true
        GAMESTATE:SetCurrentSteps(0,all[3]);assert(GAMESTATE:GetCurrentSteps(1)==all[3])
        __songlua_suppress_broadcast_dispatch=false
        return snapshot(seen),snapshot(__songlua_broadcast_messages),__songlua_side_effect_count
    "#,
    );
    p.compare(
        r#"
        phase='warning';GAMESTATE:SetCurrentSteps(0,nil);GAMESTATE:SetCurrentSteps(1,nil)
        assert(GAMESTATE:GetEasiestStepsDifficulty()==nil)
        assert(__songlua_reporting_script_error==false)
        phase='warning-error';assert(not pcall(GAMESTATE.GetEasiestStepsDifficulty,GAMESTATE))
        assert(__songlua_reporting_script_error==false and __songlua_active_broadcast=='outer')
        return snapshot(seen),snapshot(__songlua_broadcast_messages),__songlua_side_effect_count
    "#,
    );
}

#[test]
fn collecting_style_callbacks_and_public_catalog_replacement_keep_authored_step_registry() {
    let mut p = Pair::new(&selection_context());
    p.compare(
        r#"
        local all=GAMESTATE:GetCurrentSong():GetAllSteps();local original=GAMESTATE
        local setter=GameState.SetCurrentSteps;local getter=GameState.GetCurrentSteps
        GAMESTATE:GetCurrentStyle().GetStepsType=function(self)
            catalog.song.__songlua_steps={};catalog.steps={};catalog.player_states={}
            GAMESTATE={};GameState={};collectgarbage('collect')
            return 'StepsType_Dance_Single'
        end
        assert(setter({},0,all[3])==original)
        assert(getter({},0)==all[3] and getter({},1)==all[3])
        local song,steps=GetSongAndSteps(1);assert(steps==all[3])
        return __songlua_side_effect_count,steps:GetDescription(),song:GetMainTitle()
    "#,
    );
}

#[test]
fn invalid_calls_preserve_explicit_native_diagnostics_and_earlier_state() {
    let mut p = Pair::new(&selection_context());
    p.compare(r#"
        local function fails(message,fn,...)
            local ok,err=pcall(fn,...);assert(not ok)
            assert(string.find(tostring(err),message,1,true),tostring(err))
            return message
        end
        local all=GAMESTATE:GetCurrentSong():GetAllSteps();local out={}
        out[#out+1]=fails('Expected PlayerNumber',GAMESTATE.GetCurrentSteps,GAMESTATE,'bad')
        out[#out+1]=fails('Expected PlayerNumber',GAMESTATE.SetCurrentSteps,GAMESTATE,'bad',all[1])
        out[#out+1]=fails('Expected Steps',GAMESTATE.SetCurrentSteps,GAMESTATE,0,{})
        out[#out+1]=fails('No compatible style for steps/trail.',GAMESTATE.SetCurrentSteps,GAMESTATE,0,all[4])
        out[#out+1]=fails('Expected game command string',GAMESTATE.ApplyGameCommand,GAMESTATE,false)
        out[#out+1]=fails('Expected PlayerNumber',GAMESTATE.ApplyGameCommand,GAMESTATE,'mod,2x',0)
        out[#out+1]=fails('Expected modifier string',GAMESTATE.ApplyStageModifiers,GAMESTATE,0,{})
        out[#out+1]=fails('Expected ModsLevel',GAMESTATE.SetSongOptions,GAMESTATE,'bad','2xMusic')
        out[#out+1]=fails('Expected song options string',GAMESTATE.SetSongOptions,GAMESTATE,0,false)
        assert(GAMESTATE:GetCurrentSteps(0)==all[1] and GAMESTATE:GetCurrentSteps(1)==all[2])
        return snapshot(out),all_options(),__songlua_side_effect_count
    "#);
}

#[test]
fn public_game_state_owns_levels_until_retirement_and_saved_queries_release_the_instance() {
    let mut runtime = RustSongRuntime::new(&context()).unwrap();
    test_support::expose_catalog(&mut runtime).unwrap();
    test_support::release_catalog_root(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        weak=setmetatable({GAMESTATE,GameState,catalog.song,catalog.player_states[1]},{__mode='v'})
        saved=GAMESTATE.GetCurrentSong;catalog=nil;SONGMAN=nil
        collectgarbage('collect');assert(weak[1] and weak[2] and weak[3] and weak[4])
    "#,
    );
    test_support::retire_game_state_globals(&mut runtime).unwrap();
    test_support::retire_statistics_globals(&mut runtime).unwrap();
    execute(
        &mut runtime,
        r#"
        collectgarbage('collect');collectgarbage('collect')
        assert(weak[1]==nil and weak[2]==nil and weak[4]==nil)
        assert(saved()==weak[3]);saved=nil
        collectgarbage('collect');collectgarbage('collect');assert(weak[3]==nil)
    "#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
