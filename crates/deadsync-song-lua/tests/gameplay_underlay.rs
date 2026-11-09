use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime, test_support},
};
use std::path::Path;

struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
}
impl Pair {
    fn new(live: bool, enabled: [bool; 2], setup: &str) -> Self {
        Self::style(live, enabled, setup, "single")
    }
    fn style(live: bool, enabled: [bool; 2], setup: &str, style: &str) -> Self {
        let mut context = SongLuaCompileContext::new(Path::new("."), "underlay migration");
        context.live = live;
        context.style_name = style.to_owned();
        context.screen_width = 854.;
        context.screen_height = 480.;
        for (player, enabled) in context.players.iter_mut().zip(enabled) {
            player.enabled = enabled;
        }

        let mut rust = SongRuntime::new(&context).unwrap();
        let scene = test_support::gameplay_screen(&mut rust, &context).unwrap();
        assert_eq!(rust.begin_actor_preparation().unwrap(), !live);

        assert!(test_support::gameplay_combo(&mut rust, scene).unwrap());
        let mut p = Self { rust, scene };
        p.script(
            r#"
            TOP=SCREENMAN:GetTopScreen();root=TOP;observations={}
            function log(...)
                local t={};for i=1,select('#',...) do local v=select(i,...)
                    t[i]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
                end;observations[#observations+1]=table.concat(t,',')
            end
            function inspect(a)
                log(a.__songlua_actor_type,a:GetName(),a:GetVisible(),a:GetX(),a:GetY(),
                    a:GetWidth(),a:GetHeight(),a:GetZoomX(),a:GetZoomY(),a:GetDiffuseAlpha(),
                    a:GetTweenTimeLeft(),a:GetRotationZ(),a.Text)
                for _,child in ipairs(a) do assert(child:GetParent()==a);inspect(child) end
            end
        "#,
        );
        p.script(setup);
        p
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/gameplay-underlay.lua").unwrap())
            .unwrap_or_else(|e| panic!("Rust: {source}\n{e:?}"));
        let Value::String(actual) = self
            .rust
            .vm_mut()
            .execute(
                &compile_named(b"return table.concat(observations,'|')", b"=observations").unwrap(),
            )
            .unwrap()[0]
        else {
            panic!()
        };
        reference_test::assert_debug(
            "song/tests/rust_gameplay_underlay.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }
    fn load(&mut self) {
        let result = test_support::gameplay_underlay(&mut self.rust, self.scene);
        let diagnostics = self.rust.take_actor_diagnostics();
        assert!(
            result.is_ok(),
            "underlay: {result:?}\nactor diagnostics: {diagnostics:#?}"
        );
        assert!(result.unwrap());
        assert!(
            diagnostics.is_empty(),
            "unexpected actor diagnostics: {diagnostics:#?}"
        );
    }
}

#[test]
fn complete_underlay_keeps_native_tree_order_options_and_initial_song_message() {
    for live in [false, true] {
        for enabled in [[true, false], [false, true], [true, true]] {
            let mut p = Pair::new(live, enabled, "");
            p.load();
            p.script(r#"
                local u=TOP:GetChild('Underlay');inspect(u)
                local title=u:GetChild('SongMeter'):GetChild('SongTitle')
                assert(title:GetText()==GAMESTATE:GetCurrentSong():GetDisplayFullTitle())
                for _,pn in ipairs(GAMESTATE:GetHumanPlayers()) do
                    local n=ToEnumShortString(pn)
                    assert(u:GetChild('NoteFieldContainer'..n))
                    local state=GAMESTATE:GetPlayerState(pn)
                    assert(TOP:GetChild('PlayerOptions'..n):GetText()==state:GetPlayerOptionsString('ModsLevel_Current'))
                end
                log(__songlua_side_effect_count)
            "#);
        }
    }
}

#[test]
fn step_statistics_loads_its_real_shared_environment_and_nested_resources() {
    for live in [false, true] {
        let mut p = Pair::new(
            live,
            [true, false],
            "SL.P1.ActiveModifiers.DataVisualizations='Step Statistics'",
        );
        p.load();
        p.script(
            r#"
            local pane=TOP:GetChild('Underlay'):GetChild('StepStatsPaneP1')
            assert(pane and pane:GetChild('BannerAndData'))
            inspect(pane)
            assert(type(__songlua_step_statistics_environment.LoadActor)=='function')
            assert(rawget(__songlua_step_statistics_environment,'Def')==nil)
            assert(type(Scrolling_NPS_Histogram)=='function')
        "#,
        );
    }
}

#[test]
fn note_field_error_bar_variants_use_relative_environments_and_actual_layouts() {
    for live in [false, true] {
        for mode in ["Colorful", "Monochrome", "Text"] {
            let mut p = Pair::new(
                live,
                [true, false],
                &format!(
                    "SL.P1.ActiveModifiers.ErrorBar='{mode}';SL.P1.ActiveModifiers.NoteFieldOffsetX=12;SL.P1.ActiveModifiers.NoteFieldOffsetY=-7;SL.P1.ActiveModifiers.MeasureLines='Eighth'"
                ),
            );
            p.load();
            p.script(
                r#"
                local u=TOP:GetChild('Underlay');inspect(u:GetChild('NoteFieldContainerP1'))
                local player=TOP:GetChild('PlayerP1');log(player:GetX(),player:GetY())
                local field=player:GetChild('NoteField')
                assert(rawget(field,'__songlua_beat_bars')==true)
                local alpha=rawget(field,'__songlua_beat_bars_alpha')
                assert(alpha[1]==0.75 and alpha[2]==0.5 and alpha[3]==0.25 and alpha[4]==0)
                assert(not pcall(field.GetBeatBars,field))
            "#,
            );
        }
    }
}

#[test]
fn retained_underlay_messages_keep_danger_state_and_title_after_collection() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, true], "");
        p.load();
        p.script(r#"
            collectgarbage('collect');collectgarbage('collect')
            local u=TOP:GetChild('Underlay')
            local d=u:GetChild('DangerP1');assert(d)
            for _,health in ipairs{'HealthState_Danger','HealthState_Alive','HealthState_Dead'} do
                MESSAGEMAN:Broadcast('HealthStateChanged',{PlayerNumber=PLAYER_1,HealthState=health});inspect(d)
            end
            MESSAGEMAN:Broadcast('CurrentSongChanged')
            inspect(u:GetChild('SongMeter'))
        "#);
    }
}

#[test]
fn underlay_startup_queues_wait_until_the_original_discovery_boundary() {
    let mut p = Pair::new(
        false,
        [true, false],
        r#"
        local ctor=Def.ActorFrame
        Def.ActorFrame=function(t)
            if t.Name=='NoteFieldContainerP1' then
                local init=t.InitCommand;local on=t.OnCommand
                t.InitCommand=function(self)log('Init');self:queuecommand('Probe');if init then init(self)end end
                t.BeginCommand=function(self)log('Begin')end
                t.OnCommand=function(self)log('On');on(self)end
                t.ProbeCommand=function(self)log('Probe')end
            end
            return ctor(t)
        end
    "#,
    );
    p.load();
    p.script("assert(table.concat(observations,',')=='Init,Begin,On')");
}

#[test]
fn screen_release_relinquishes_loaded_underlay_callback_owners() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, true], "");
        p.load();
        p.script(r#"
            local u=TOP:GetChild('Underlay')
            local a=u:GetChild('NoteFieldContainerP1');local b=u:GetChild('SongMeter');local c=u:GetChild('DangerP2')
            assert(a and b and c);WEAK=setmetatable({a,b,c},{__mode='v'})
            root=nil;TOP=nil;__songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil
            GetPlayerAF=nil;MusicRateDisplay=nil;dispP1=nil;dispP2=nil
        "#);
        assert!(p.rust.release_actor_scene(p.scene).unwrap());
        p.rust
            .vm_mut()
            .execute(
                &compile_named(
                    br#"
            collectgarbage('collect');collectgarbage('collect')
            for i=1,3 do assert(WEAK[i]==nil,'retained underlay actor '..i)end
        "#,
                    b"=underlay release",
                )
                .unwrap(),
            )
            .unwrap();
    }
}

#[test]
fn optional_note_field_widgets_use_existing_chart_and_turn_helpers() {
    for live in [false, true] {
        for style in ["single", "double", "solo", "pump-single"] {
            let mut p = Pair::style(
                live,
                [true, false],
                r#"
                local mods=SL.P1.ActiveModifiers
                mods.ColumnCues=true;mods.SubtractiveScoring=true;mods.MeasureCounter='8th'
                mods.ErrorBar='Colorful';mods.ShowFaPlusWindow=true;mods.HideDanger=true
            "#,
                style,
            );
            p.load();
            p.script(r#"
                inspect(TOP:GetChild('Underlay'))
                local map=GetColumnMapping(PLAYER_1);assert(#map==GAMESTATE:GetCurrentStyle():ColumnsPerPlayer())
                log(table.concat(map,','))
                collectgarbage('collect')
                MESSAGEMAN:Broadcast('CurrentSongChanged')
                inspect(TOP:GetChild('Underlay'):GetChild('NoteFieldContainerP1'))
            "#);
        }
    }
}

#[test]
fn required_numeric_color_and_time_helpers_preserve_native_coercion_and_values() {
    let mut p = Pair::new(true, [true, false], "");
    p.script(r#"
        for _,v in ipairs{0,1.25,-1.25,599.99,3600.1,36000.5,'63.75'} do
            log(SecondsToMSS(v),SecondsToMMSS(v),SecondsToHMMSS(v),SecondsToHHMMSS(v))
        end
        log(clamp('1.25',0,1),scale(1.3333333,0,2,5,7),scale(3,1,1,8,9))
        for _,args in ipairs{{5},{5,1},{1,2,0.3},{-0.0,0.0},{1,2,0},{1,3,-1}} do log(table.concat(range(unpack(args)),','))end
        assert(range(false)==nil)
        for _,v in ipairs{1,2.5,'TimingWindow_W3','4.4',false,'bad'} do
            log(GetTimingWindow(v),GetTimingWindow(v,'FA+',true))
        end
        for _,name in ipairs{'Hard','Challenge','Difficulty_Hard','bad',2,false} do log(unpack(CustomDifficultyToColor(name)))end
        log(IsServiceAllowed('anything'),NumJudgmentsAvailable(),CalculateExScore(PLAYER_1,{}))
        assert(not pcall(SecondsToMSS,{}));assert(not pcall(scale,1,2))
    "#);
}

#[test]
fn player_enum_and_other_player_tables_keep_native_aliases_and_reverse_identity() {
    let mut p = Pair::new(true, [true, true], "");
    p.script(
        r#"
        assert(#PlayerNumber==2)
        for i,pn in ipairs(PlayerNumber) do
            assert(PlayerNumber[pn]==pn)
            assert(PlayerNumber:Reverse()[pn]==i-1)
            assert(OtherPlayer[OtherPlayer[pn]]==pn)
        end
        local reverse=PlayerNumber.Reverse
        local saved=reverse()
        assert(saved==reverse(false,{}))
        PlayerNumber=nil;collectgarbage('collect');collectgarbage('collect')
        assert(reverse()==saved);log(saved[PLAYER_1],saved[PLAYER_2])
    "#,
    );
}

#[test]
fn required_brightness_and_alignment_keep_native_values_and_partial_writes() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, false], "");
        p.script(
            r#"
            for _,c in ipairs{{0.2,0.8,0.4,0.3},{0,0,0},{1,1,1,0.25},{1,0,0,0}} do
                for _,v in ipairs{-1,0,0.333333333333,1,2} do log(unpack(Brightness(c,v)))end
            end
            log(HasAlpha({}),HasAlpha({1,0,0}),HasAlpha({1,0,0,'0.2'}),HasAlpha('#abcdef80'))
            assert(not pcall(Brightness,{},0.5))
            local a=TOP:GetChild('Underlay')
            for _,v in ipairs{{0.2,0.8},{left,bottom},{'bad',middle},{right,'bad'},{false,0.1}} do
                assert(a:align(unpack(v))==a);log(a:GetHAlign(),a:GetVAlign())
            end
            local align=a.align
            a.align=false;a.halign=false;a.valign=false
            assert(not pcall(align,{},0,1))
            assert(align(a,0,1)==a);log(a:GetHAlign(),a:GetVAlign())
        "#,
        );
    }
}

#[test]
fn underlay_loaders_preserve_shared_writes_relative_paths_and_missing_resource_errors() {
    for live in [false, true] {
        let mut p = Pair::new(
            live,
            [true, false],
            r#"
            local ctor=Def.ActorFrame
            Def.ActorFrame=function(t)
                if t.Name=='NoteFieldContainerP1' then NOTE_ENV=getfenv(t.OnCommand) end
                return ctor(t)
            end
        "#,
        );
        p.load();
        p.script(r#"
            local env=__songlua_step_statistics_environment
            assert(rawget(env,'LoadActor'))
            env.UnderlaySharedWrite=73;assert(_G.UnderlaySharedWrite==73 and rawget(env,'UnderlaySharedWrite')==nil)
            assert(NOTE_ENV and NOTE_ENV~=env and rawget(NOTE_ENV,'LoadActor'))
            local function fails(f,arg,expected)
                local ok,err=pcall(f,arg)
                assert(not ok and string.find(tostring(err),expected,1,true),tostring(err))
            end
            fails(env.LoadActor,nil,'native LoadActor requires a path')
            fails(env.LoadActor,'missing.lua','unknown native StepStatistics resource missing.lua')
            fails(env.LoadActor,'././DarkBackground.lua','unknown native StepStatistics resource ././DarkBackground.lua')
            fails(NOTE_ENV.LoadActor,false,'native NoteField LoadActor requires a path')
            fails(NOTE_ENV.LoadActor,'missing.lua','unknown native NoteField actor missing.lua')
            local load=env.LoadActor
            local replacement=setmetatable({},getmetatable(env))
            __songlua_step_statistics_environment=replacement
            local ctor=Def.Quad;local observed
            Def.Quad=function(t)observed=getfenv(t.InitCommand);return ctor(t)end
            collectgarbage('collect')
            assert(load('./DarkBackground.lua',{PLAYER_1,80,160}))
            assert(observed==replacement)
            Def.Quad=ctor
            log('shared environment and paths')
        "#);
    }
}

#[test]
fn failed_underlay_on_keeps_partial_construction_owned_until_screen_release() {
    for live in [false, true] {
        let mut p = Pair::new(
            live,
            [true, false],
            r#"
            local ctor=Def.ActorFrame
            Def.ActorFrame=function(t)
                if t.Name=='NoteFieldContainerP1' then
                    t.OnCommand=function(self)FAILED_UNDERLAY=self;error('underlay On failure')end
                end
                return ctor(t)
            end
        "#,
        );

        let rust = test_support::gameplay_underlay(&mut p.rust, p.scene).unwrap_err();

        assert!(rust.to_string().contains("underlay On failure"));
        p.script(r#"
            assert(FAILED_UNDERLAY and FAILED_UNDERLAY:GetParent()==TOP:GetChild('Underlay'))
            WEAK=setmetatable({FAILED_UNDERLAY},{__mode='v'})
            FAILED_UNDERLAY=nil;root=nil;TOP=nil
            __songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil
            GetPlayerAF=nil;MusicRateDisplay=nil;dispP1=nil;dispP2=nil
        "#);
        assert!(p.rust.release_actor_scene(p.scene).unwrap());
        p.rust
            .vm_mut()
            .execute(
                &compile_named(
                    b"collectgarbage('collect');collectgarbage('collect');assert(WEAK[1]==nil)",
                    b"=failed underlay release",
                )
                .unwrap(),
            )
            .unwrap();
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
