use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaHostClockInput, {ActorScene, SongRuntime, test_support},
};
use std::path::Path;

struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
    context: SongLuaCompileContext,
}
impl Pair {
    fn context(live: bool, enabled: [bool; 2], probe: bool) -> SongLuaCompileContext {
        let mut c = SongLuaCompileContext::new(Path::new("."), "overlay migration");
        c.live = live;
        c.capture_overlay_probe = probe;
        c.screen_width = 854.;
        c.screen_height = 480.;
        for (p, enabled) in c.players.iter_mut().zip(enabled) {
            p.enabled = enabled;
        }
        c
    }
    fn new(live: bool, enabled: [bool; 2], probe: bool, setup: &str) -> Self {
        Self::with_context(Self::context(live, enabled, probe), setup)
    }
    fn with_context(context: SongLuaCompileContext, setup: &str) -> Self {
        let mut rust = SongRuntime::new(&context).unwrap();
        let scene = test_support::gameplay_screen(&mut rust, &context).unwrap();
        assert_eq!(rust.begin_actor_preparation().unwrap(), !context.live);

        assert!(test_support::gameplay_combo(&mut rust, scene).unwrap());

        assert!(test_support::gameplay_underlay(&mut rust, scene).unwrap());
        let mut p = Self {
            rust,
            scene,
            context,
        };
        p.script(
            r#"
            TOP=SCREENMAN:GetTopScreen();root=TOP;observations={}
            function log(...)
                local t={};for i=1,select('#',...) do local v=select(i,...)
                    t[i]=type(v)=='number' and string.format('%.17g',v) or tostring(v)
                end;observations[#observations+1]=table.concat(t,',')
            end
            function inspect(a)
                log(a.__songlua_actor_type,a:GetName(),a:GetVisible(),a:GetX(),a:GetY(),
                    a:GetWidth(),a:GetHeight(),a:GetZoomX(),a:GetZoomY(),a:GetDiffuseAlpha(),
                    a:GetTweenTimeLeft(),a:GetRotationZ(),a.Text,
                    rawget(a,'__songlua_state_draw_order'),rawget(a,'__songlua_hibernate_seconds'))
                for _,child in ipairs(a) do assert(child:GetParent()==a);inspect(child)end
            end
            RUNTIME=__songlua_compile_song_runtime
        "#,
        );
        p.script(setup);
        p
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/gameplay-overlay.lua").unwrap())
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
            "song/tests/rust_gameplay_overlay.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }

    fn overlay(&mut self) {
        let result = test_support::gameplay_overlay(
            &mut self.rust,
            self.scene,
            self.context.capture_overlay_probe,
        );
        let diagnostics = self.rust.take_actor_diagnostics();
        assert!(result.is_ok(), "{result:?}\n{diagnostics:#?}");
        assert!(result.unwrap());
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }
    fn decorations(&mut self) {
        let result = test_support::gameplay_decorations(&mut self.rust, self.scene);
        let diagnostics = self.rust.take_actor_diagnostics();
        assert!(result.is_ok(), "{result:?}\n{diagnostics:#?}");
        assert!(result.unwrap());
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }
}

#[test]
fn overlay_replaces_its_placeholder_and_preserves_capture_probe_tree_and_layer() {
    for live in [false, true] {
        for probe in [false, true] {
            for enabled in [[true, false], [false, true], [true, true]] {
                let mut p = Pair::new(
                    live,
                    enabled,
                    probe,
                    "OLD=TOP:GetChild('Overlay');OLD:draworder(77)",
                );
                if probe && !live {
                    let rust =
                        test_support::gameplay_overlay(&mut p.rust, p.scene, true).unwrap_err();

                    assert!(rust.to_string().contains("Overlay requires its actor host"));
                    assert!(p.rust.take_actor_diagnostics().is_empty());
                    p.script(
                        r#"
                        local o=TOP:GetChild('Overlay');assert(o==OLD and #o==1)
                        assert(o[1]:GetName()=='');inspect(o)
                        log(SL.P1.Stages.Stats[1]~=nil,SL.P2.Stages.Stats[1]~=nil)
                    "#,
                    );
                    continue;
                }
                p.overlay();
                p.script(if probe {r#"
            local o=TOP:GetChild('Overlay');assert(o==OLD and #o==2)
            assert(o[1]:GetName()=='');assert(o[2]:GetName()=='ITGmaniaHarnessTraceProbe')
            inspect(o);log(SL.P1.Stages.Stats[1]~=nil,SL.P2.Stages.Stats[1]~=nil)
        "#} else {r#"
            local o=TOP:GetChild('Overlay');assert(o~=OLD and o:GetName()=='Overlay')
            assert(o:GetParent()==TOP and rawget(o,'__songlua_top_screen_child_name')=='Overlay')
            assert(rawget(o,'__songlua_state_draw_order')==77)
            local seen=0;for _,child in ipairs(TOP)do assert(child~=OLD);if child==o then seen=seen+1 end end;assert(seen==1)
            inspect(o);log(SL.P1.Stages.Stats[1]~=nil,SL.P2.Stages.Stats[1]~=nil)
        "#});
            }
        }
    }
}

#[test]
fn decorations_load_real_files_merge_metric_on_and_sort_after_init() {
    for live in [false, true] {
        for style in ["SRPG10", "Technique", "default"] {
            let mut p = Pair::new(
                live,
                [true, true],
                false,
                &format!(
                    "ThemePrefs.Set('VisualStyle','{style}');OLD_H=TOP:GetChild('Header');OLD_F=TOP:GetChild('Footer')"
                ),
            );
            p.overlay();
            p.decorations();
            p.script(r#"
            local h=TOP:GetChild('Header');local f=TOP:GetChild('Footer')
            assert(h~=OLD_H and f~=OLD_F and h:GetParent()==TOP and f:GetParent()==TOP)
            assert(rawget(h,'__songlua_hibernate_seconds')==math.huge)
            assert(rawget(f,'__songlua_hibernate_seconds')==math.huge)
            assert(rawget(f,'__songlua_state_draw_order')==90)
            local last=-math.huge
            for _,a in ipairs(TOP)do local order=rawget(a,'__songlua_state_draw_order') or 0;assert(order>=last);last=order end
            inspect(h);inspect(f)
            MESSAGEMAN:Broadcast('SetHeaderText',{Text='updated header'})
            assert(h:GetChild('HeaderText'):GetText()=='updated header')
            collectgarbage('collect');MESSAGEMAN:Broadcast('ColorSelected');inspect(h);inspect(f)
        "#);
        }
    }
}

#[test]
fn overlay_root_is_replaced_before_init_and_direct_begin_on_keep_native_order() {
    for live in [false, true] {
        let mut p = Pair::new(
            live,
            [true, false],
            false,
            r#"
            local ctor=Def.ActorFrame;local first=true
            Def.ActorFrame=function(t)
                if first then
                    first=false
                    t.InitCommand=function(self)log('Init',self==TOP:GetChild('Overlay'),self:GetParent()==TOP)end
                    t.BeginCommand=function(self)log('Begin')end
                    t.OnCommand=function(self)log('On')end
                end
                return ctor(t)
            end
        "#,
        );
        p.overlay();
        p.script("assert(table.concat(observations,'|')=='Init,true,true|Begin|On')");
    }
}

#[test]
fn original_judgment_listeners_keep_stage_data_and_off_callbacks_across_gc() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, false], false, "");
        p.overlay();
        p.script(
            r#"
            collectgarbage('collect');collectgarbage('collect')
            NOTE={GetTapNoteType=function()return 'TapNoteType_Tap'end,
                GetTapNoteResult=function()return {GetHeld=function()return false end}end}
        "#,
        );
        // Actual gameplay messages do not apply authored Broadcast defaults:
        // that path inserts a truthy HoldNoteScore_None and skips offset tracking.
        for judgment in ["TapNoteScore_W1", "TapNoteScore_Miss"] {
            p.script(&format!("PARAMS={{Player=PLAYER_1,TapNoteScore='{judgment}',TapNoteOffset=0.005,Notes={{[1]=NOTE}}}}"));

            let actual = p.rust.vm_mut().get_global(b"PARAMS").unwrap();
            p.rust
                .broadcast_native_message("Judgment", Some(actual))
                .unwrap();
        }
        p.script(
            r#"
            RUNTIME.__songlua_music_seconds=14.25;RUNTIME.__songlua_uptime_seconds=18.5
            TOP:GetChild('Overlay'):playcommand('Off')
            local stats=SL.P1.Stages.Stats[1]
            assert(#stats.sequential_offsets==2 and stats.sequential_offsets[2][2]=='Miss')
            assert(stats.column_judgments[1].W1+stats.column_judgments[1].W0==1)
            assert(stats.column_judgments[1].Miss==1 and stats.ex_counts.Miss==1)
            log(stats.duration,stats.sequential_offsets[1][2],stats.ex_counts.W0,stats.ex_counts.W1)
            for _,col in ipairs(stats.column_judgments)do log(col.W0,col.W1,col.Miss)end
        "#,
        );
        assert!(p.rust.take_actor_diagnostics().is_empty());
    }
}

#[test]
fn uptime_keeps_native_precision_runtime_replacement_and_error_rules() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, false], false, "");
        p.script(
            r#"
            for _,v in ipairs{1.333333333333,16777217.125,'9.333333333333',-0.0}do
                RUNTIME.__songlua_music_seconds=v;RUNTIME.__songlua_uptime_seconds=v
                log(GetTimeSinceStart());if tonumber(v)==0 then log(1/GetTimeSinceStart())end
            end
            __songlua_compile_song_runtime=setmetatable({}, {__index=function(_,key)
                if key=='__songlua_uptime_seconds' then return 23.1234567890123 end
                if key=='__songlua_music_seconds' then return 17.3333333333 end
            end})
            log(GetTimeSinceStart())
            __songlua_compile_song_runtime={__songlua_music_seconds={},__songlua_uptime_seconds={}}
            local ok,value=pcall(GetTimeSinceStart);log(ok,ok and value)
            __songlua_compile_song_runtime=nil
            local ok,value=pcall(GetTimeSinceStart);log(ok,ok and value)
            __songlua_compile_song_runtime=false;local ok=pcall(function()GetTimeSinceStart()end);assert(not ok);log(ok)
        "#,
        );
    }
}

#[test]
fn observed_clock_inputs_use_original_source_lines_and_fifo_values() {
    let source = include_str!(
        "../assets/simply-love/BGAnimations/ScreenGameplay overlay/TrackTimeSpentInGameplay.lua"
    );
    let on = source
        .lines()
        .position(|line| line.contains("start_time = GetTimeSinceStart()"))
        .unwrap() as i32
        + 1;
    let off = source
        .lines()
        .position(|line| line.contains("duration = GetTimeSinceStart()"))
        .unwrap() as i32
        + 1;
    for live in [false, true] {
        let mut c = Pair::context(live, [true, true], false);
        for (line, value) in [(on, 20.25), (on, 30.5), (off, 22.75), (off, 36.25)] {
            c.host_clock_inputs.push(SongLuaHostClockInput {
                clock: "GetTimeSinceStart".into(),
                source: "@theme:/BGAnimations/ScreenGameplay overlay/TrackTimeSpentInGameplay.lua"
                    .into(),
                line,
                value,
            });
        }
        let mut p = Pair::with_context(c, "debug.getinfo=function()error('authored debug')end");
        p.overlay();
        p.script(
            r#"
            RUNTIME.__songlua_music_seconds=100;RUNTIME.__songlua_uptime_seconds=200
            TOP:GetChild('Overlay'):playcommand('Off')
            assert(SL.P1.Stages.Stats[1].duration==2.5 and SL.P2.Stages.Stats[1].duration==5.75)
            log(SL.P1.Stages.Stats[1].duration,SL.P2.Stages.Stats[1].duration)
        "#,
        );
    }
}

#[test]
fn overlay_resource_loader_uses_current_shared_environment_and_closed_paths() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, false], false, "");
        p.overlay();
        p.script(r#"
            local env=__songlua_overlay_environment;env.OverlayWrite=47
            assert(rawget(env,'OverlayWrite')==nil and _G.OverlayWrite==47)
            local load=env.LoadActor
            local function fails(value,expected)
                local ok,err=pcall(load,value);assert(not ok and string.find(tostring(err),expected,1,true))
            end
            fails(nil,'native LoadActor requires a path')
            fails('missing.lua','unknown native Overlay resource missing.lua')
            fails('././default.lua','unknown native Overlay resource ././default.lua')
            local replacement=setmetatable({},getmetatable(env));__songlua_overlay_environment=replacement
            local ctor=Def.Actor;local seen
            Def.Actor=function(t)seen=getfenv(t.OnCommand);return ctor(t)end
            collectgarbage('collect');assert(load('./TrackTimeSpentInGameplay.lua',PLAYER_1));assert(seen==replacement)
            Def.Actor=ctor;log('shared overlay environment')
        "#);
    }
}

#[test]
fn screen_release_retires_overlay_and_decoration_owners() {
    for live in [false, true] {
        let mut p = Pair::new(live, [true, true], false, "");
        p.overlay();
        p.decorations();
        p.script(r#"
            WEAK=setmetatable({TOP:GetChild('Overlay'),TOP:GetChild('Header'),TOP:GetChild('Footer')},{__mode='v'})
            assert(WEAK[1] and WEAK[2] and WEAK[3]);root=nil;TOP=nil
            __songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil
            GetPlayerAF=nil;MusicRateDisplay=nil;dispP1=nil;dispP2=nil
        "#);
        assert!(p.rust.release_actor_scene(p.scene).unwrap());
        p.rust.vm_mut().execute(&compile_named(b"collectgarbage('collect');collectgarbage('collect');for i=1,3 do assert(WEAK[i]==nil,'retained actor '..i)end",b"=overlay release").unwrap()).unwrap();
    }
}

#[test]
fn overlay_on_failure_preserves_replacement_and_releases_partial_construction() {
    for live in [false, true] {
        let mut p = Pair::new(
            live,
            [true, false],
            false,
            r#"
            local ctor=Def.ActorFrame;local first=true
            Def.ActorFrame=function(t)
                if first then first=false;t.OnCommand=function(self)FAILED=self;error('overlay On failure')end end
                return ctor(t)
            end
        "#,
        );

        assert!(
            test_support::gameplay_overlay(&mut p.rust, p.scene, false)
                .unwrap_err()
                .to_string()
                .contains("overlay On failure")
        );
        p.script(r#"
            assert(FAILED==TOP:GetChild('Overlay') and FAILED:GetParent()==TOP)
            WEAK=setmetatable({FAILED},{__mode='v'});FAILED=nil;root=nil;TOP=nil
            __songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil
            GetPlayerAF=nil;MusicRateDisplay=nil;dispP1=nil;dispP2=nil
        "#);
        assert!(p.rust.release_actor_scene(p.scene).unwrap());
        p.rust
            .vm_mut()
            .execute(
                &compile_named(
                    b"collectgarbage('collect');collectgarbage('collect');assert(WEAK[1]==nil)",
                    b"=failed overlay release",
                )
                .unwrap(),
            )
            .unwrap();
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
