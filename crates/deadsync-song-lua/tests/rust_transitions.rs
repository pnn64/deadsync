use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime, test_support},
};
use std::path::Path;

const DEFAULTS: &str = r#"
ThemePrefs={Get=function(key)assert(key=='VisualStyle');return 'Arrows' end}
IsSpooky=function()return false end
GetCurrentColor=function(s)assert(s==true);return {0.2,0.4,0.6,1} end
SL.Global.GameplayReloadCheck=false
"#;
struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
}
impl Pair {
    fn new(name: &str, live: bool, setup: &str) -> Self {
        Self::with_construction(name, live, setup, "")
    }
    fn with_construction(name: &str, live: bool, setup: &str, construction: &str) -> Self {
        Self::with_construction_mode(name, live, setup, construction, false)
    }
    fn constructor(name: &str, construction: &str) -> Self {
        Self::with_construction_mode(name, false, "", construction, true)
    }
    fn with_construction_mode(
        name: &str,
        live: bool,
        setup: &str,
        construction: &str,
        _before_frames: bool,
    ) -> Self {
        let mut context = SongLuaCompileContext::new(Path::new("."), "generated transitions");
        context.live = live;
        context.screen_width = 854.;
        context.screen_height = 480.;
        context.players[1].enabled = true;

        let mut rust = RustSongRuntime::new(&context).unwrap();
        rust.vm_mut()
            .execute(&compile_named(DEFAULTS.as_bytes(), b"=defaults").unwrap())
            .unwrap();
        rust.vm_mut()
            .execute(&compile_named(setup.as_bytes(), b"=transition setup").unwrap())
            .unwrap();
        let scene = test_support::screen_part(&mut rust, name).unwrap();
        let actor = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"part", actor).unwrap();
        rust.vm_mut()
            .execute(&compile_named(construction.as_bytes(), b"@/fixture/constructor.lua").unwrap())
            .unwrap();
        let mut pair = Self { rust, scene };
        pair.script(r#"
            root=part;observations={};function log(...)
                local t={};for i=1,select('#',...) do local v=select(i,...)
                    t[i]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
                end;observations[#observations+1]=table.concat(t,',')
            end
            function pose(a)log(a:GetX(),a:GetY(),a:GetWidth(),a:GetHeight(),a:GetZoomX(),a:GetZoomY(),a:GetVisible(),a:GetDiffuseAlpha())end
        "#);
        pair
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/transitions.lua").unwrap())
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
            "song/tests/rust_transitions.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }
}

const CAPTURE_PROBE: &str = r#"
capture_events={}
function sample(a,label)
    local t={label,a:GetX(),a:GetY(),a:GetZ(),a:GetDestX(),a:GetDestY(),a:GetDestZ(),a:getaux(),
        a:GetZoomX(),a:GetZoomY(),a:GetRotationZ(),a:GetDiffuseAlpha(),a:GetTweenTimeLeft(),a:GetWidth(),a:GetHeight(),a:GetVisible()}
    for i,v in ipairs(t)do t[i]=type(v)=='number' and string.format('%.9g',v) or tostring(v)end
    capture_events[#capture_events+1]=table.concat(t,',')
end
"#;

fn compare_constructor(source: &str) {
    let mut p = Pair::constructor("Cancel", &format!("{CAPTURE_PROBE}\n{source}"));
    p.script("log(table.concat(capture_events,'|'))");
    p.captures();
}

#[test]
fn capture_stop_cancels_pending_commands_and_keeps_direct_and_immediate_blocks() {
    compare_constructor(
        r#"
        local a=part[1];a:xy(12,16):z(4):aux(5):zoom(2):rotationz(9):diffusealpha(.3)
        a.DropCommand=function()dropped=true;error('cancelled command ran')end
        a.EditCommand=function(self)
            self:linear(1):xy(100,200):z(30):aux(9):zoom(4):rotationz(80):diffusealpha(.7)
            self:visible(false):setsize(20,30):queuecommand('Drop');sample(self,'queued')
            self:stoptweening();sample(self,'stopped')
            self:addx(2):aux(6);sample(self,'relative')
        end
        a:playcommand('Edit');assert(not dropped);sample(a,'returned')
    "#,
    );
}

#[test]
fn capture_finish_merges_blocks_and_discards_pending_callbacks_without_advancing_getters() {
    compare_constructor(
        r#"
        local a=part[1];a:xy(12,16):aux(5):rotationz(9):diffusealpha(.3)
        a.DropCommand=function()dropped=true;error('finished command ran')end
        a.EditCommand=function(self)
            self:linear(.5):x(80):aux(9):rotationz(40):diffusealpha(.7)
            self:sleep(.25):x(90):setsize(20,30):queuecommand('Drop')
            self:finishtweening();sample(self,'finished')
            self:linear(.2):y(50);sample(self,'new-tween')
        end
        a:playcommand('Edit');assert(not dropped);sample(a,'returned')
    "#,
    );
}

#[test]
fn capture_hurry_scales_history_and_clock_but_retains_queued_dispatch_starts() {
    for factor in [0., 0.25, 2.] {
        compare_constructor(&format!(
            r#"
            local a=part[1];a:x(12):hibernate(.4)
            a.MarkCommand=function(self)self:visible(false):setsize(80,90);sample(self,'mark')end
            a.EditCommand=function(self)
                self:sleep(.5):linear(.25):x(50):queuecommand('Mark')
                self:hurrytweening({factor});sample(self,'hurried')
            end
            a:playcommand('Edit');assert(not dropped);sample(a,'returned')
        "#
        ));
    }
}

#[test]
fn capture_zero_duration_getters_and_cancellation_keep_explicit_writes() {
    for tween in ["linear(0)", "sleep(0)", "linear(.00000001)"] {
        for end in ["stoptweening", "finishtweening"] {
            compare_constructor(&format!(
                r#"
                local a=part[1];a:x(12):aux(5):rotationz(9)
                a:{tween}:x(30):aux(8):rotationz(70);sample(a,'queued')
                a:{end}();sample(a,'cancelled')
                a:xy(31,32):aux(9);sample(a,'after')
            "#
            ));
        }
    }
}

#[test]
fn capture_queue_overflow_finishes_history_before_the_next_tween_or_command() {
    for count in [50, 51, 52, 104] {
        compare_constructor(&format!(
            r#"
            local a=part[1];calls=0
            a.DoneCommand=function(self)calls=calls+1;self:setsize(20+calls,30);sample(self,'done')end
            a.EditCommand=function(self)
                for i=1,{count} do self:linear(0):x(i)end
                self:queuecommand('Done');sample(self,'before-drain')
            end
            a:playcommand('Edit');assert(calls==1);sample(a,'returned')
        "#
        ));
    }
    compare_constructor(
        r#"
        local a=part[1];calls=0
        a.LoopCommand=function(self)
            calls=calls+1
            for i=1,51 do self:linear(.01):x(i)end
            self:queuecommand('Loop');sample(self,'self-queue')
        end
        a:playcommand('Loop');assert(calls==1);sample(a,'returned')
    "#,
    );
}

#[test]
fn capture_finish_retains_detached_immediate_cache_until_its_scope_changes() {
    compare_constructor(
        r#"
        local a=part[1]
        a.GoCommand=function(self)
            self:setsize(20,30):finishtweening():hurrytweening(2):setsize(40,50)
            sample(self,'detached-cache')
            self:sleep(.25):queuecommand('Next')
        end
        a.NextCommand=function(self)self:setsize(60,70);sample(self,'new-scope')end
        a:sleep(.5):queuecommand('Go');sample(a,'returned')
    "#,
    );
}

#[test]
fn capture_parent_finish_and_hurry_visit_children_while_stop_only_changes_self() {
    for operation in ["stoptweening()", "finishtweening()", "hurrytweening(.5)"] {
        compare_constructor(&format!(
            r#"
            local a,b=part,part[1]
            a:x(3):linear(1):x(8);b:x(12):linear(2):x(40)
            a:{operation};sample(a,'parent');sample(b,'child')
        "#
        ));
    }
}

#[test]
fn capture_cancellation_uses_the_spin_queue_created_at_first_spin_selection() {
    for setup in [
        "a:spin():linear(1):rotationz(70)",
        "a:linear(1):rotationz(70):spin()",
        "a:spin():linear(1):rotationz(70):stopeffect()",
    ] {
        for operation in ["stoptweening()", "finishtweening()"] {
            let mut p = Pair::constructor(
                "Cancel",
                &format!(
                    r#"{CAPTURE_PROBE}
                local a=part[1];a:rotationz(9);{setup};sample(a,'before')
                a:{operation};sample(a,'after')
            "#
                ),
            );
            // Full effect capture/publication remains a separate compiler port;
            // this checks the spin queue's influence on cancellation getters.
            p.script("log(table.concat(capture_events,'|'))");
        }
    }
}

impl Pair {
    fn captures(&mut self) {
        let source = "local t={};local function visit(a)t[#t+1]=a;for _,c in ipairs(a)do visit(c)end end;visit(part);return unpack(t)";
        let actual = self
            .rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=actors").unwrap())
            .unwrap()
            .to_vec();
        reference_test::assert_debug(
            "song/tests/rust_transitions.rs::capture_count",
            &actual.len(),
        );
        for (index, actual) in actual.iter().copied().enumerate() {
            let actual = test_support::captured_blocks(&self.rust, actual).unwrap();
            reference_test::assert_debug(
                "song/tests/rust_transitions.rs::capture_blocks",
                &(index, &actual),
            );
        }
    }

    fn start(&mut self) {
        let actor = self.rust.actor_scene_root(self.scene).unwrap();
        test_support::start_actor_transition(&mut self.rust, actor).unwrap();
    }
    fn advance(&mut self, second: f64, delta: f64) {
        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.script("log(part.__songlua_transition_state,part:GetTweenTimeLeft(),part[1]:GetTweenTimeLeft())");
    }
}

fn inspect_constructor(live: bool) {
    for name in ["In", "in", "Out", "Cancel"] {
        let mut p = Pair::new(name, live, "");
        p.script(r#"
                assert(part:GetParent()==nil and #part==1)
                assert(part.__songlua_transition_state==0 and part:GetTweenTimeLeft()==0)
                assert(part.__songlua_transition_child==part[1] and part[1]:GetParent()==part)
                function inspect(a)
                    log(a.__songlua_actor_type,a:GetName(),a.Text,a:GetTweenTimeLeft());pose(a)
                    local texture=a.GetTexture and a:GetTexture()
                    if texture then log(texture:GetPath(),texture:GetImageWidth(),texture:GetImageHeight())end
                    log(type(a.InitCommand),type(a.OnCommand),type(a.OffCommand),type(a.StartTransitioningCommand))
                    for _,child in ipairs(a)do assert(child:GetParent()==a);inspect(child)end
                end
                inspect(part);log(SL.Global.GameplayReloadCheck)
            "#);
    }
}

#[test]
fn live_constructors_keep_real_children_commands_geometry_and_textures() {
    inspect_constructor(true);
}

#[test]
fn preparation_constructors_keep_captured_queue_effects_and_shared_state() {
    inspect_constructor(false);
}

#[test]
fn preparation_exports_constructor_blocks_with_timing_easing_and_explicit_writes() {
    for name in ["Cancel", "Out", "In"] {
        let mut p = Pair::new(name, false, "");
        p.captures();
    }
}

#[test]
fn preparation_queues_preserve_order_shared_state_and_handoff_without_duplicate_callbacks() {
    let mut p = Pair::with_construction(
        "Cancel",
        false,
        "",
        r#"
        calls=0;events={};local function record(s)events[#events+1]=s..calls end;local a=part[1]
        a.FirstCommand=function(self)
            calls=calls+1;record('first');collectgarbage('collect')
            self:x(30):aux(4):sleep(.25):queuecommand('Second');record('body-end')
        end
        a.SecondCommand=function(self)
            calls=calls+1;self:visible(false):linear(.5):x(60):aux(8):diffusealpha(.2)
            collectgarbage('collect');record('second')
        end
        a:sleep(.5):queuecommand('First');record('after')
        assert(calls==2 and not a:GetVisible(),table.concat(events,'|'))
    "#,
    );
    p.script("log(table.concat(events,'|'));assert(calls==2)");
    p.captures();

    p.rust.begin_actor_playback();
    p.script("log(part[1]:GetTweenTimeLeft(),part[1]:GetDestX(),part[1]:getaux());pose(part[1]);assert(calls==2)");
    p.start();
    p.advance(4., 4.);
    p.script("assert(calls==2)");
}

#[test]
fn incoming_style_spooky_reload_and_stage_variants_match() {
    for setup in [
        "ThemePrefs.Get=function()return 'Hearts'end",
        "IsSpooky=function()return true end",
        "SL.Global.GameplayReloadCheck=true",
        "GAMESTATE.IsEventMode=function()return true end",
        "GAMESTATE.IsEventMode=function()return false end;GAMESTATE.GetCurrentStageIndex=function()return 6 end",
        "GAMESTATE.IsCourseMode=function()return true end;GAMESTATE.GetCurrentCourse=function()return {GetCourseEntries=function()return {1,2,3}end}end",
    ] {
        let mut p = Pair::new("In", true, setup);
        p.script(r#"
            local child=part[1];local visuals=child[1]
            log(#child,#visuals,visuals:GetVisible(),SL.Global.GameplayReloadCheck)
            if child[2] then log(child[2].Text)end
            for _,a in ipairs(visuals)do local t=a:GetTexture();if t then log(t:GetPath(),a:GetNumStates())end end
            assert(type(visuals.HideCommand)=='function' and type(visuals.OffCommand)=='function')
            local old=SL; SL={Global={}};visuals:playcommand('Off');assert(SL.Global.GameplayReloadCheck==false)
            visuals:playcommand('Hide');assert(SL.Global.GameplayReloadCheck==true and not visuals:GetVisible())
            SL=old;collectgarbage('collect')
        "#);
    }
}

#[test]
fn cancel_gates_children_and_callbacks_after_base_queue_and_completes_before_final_update() {
    let mut p = Pair::new("Cancel", true, "");
    p.script(r#"
        child=part[1];initial_x=child:GetX();starts=0
        local start=child.StartTransitioningCommand
        child.StartTransitioningCommand=function(self)
            starts=starts+1;log('start',part.__songlua_transition_state);collectgarbage('collect');start(self)
        end
        part:SetUpdateFunction(function(self,dt)log('owner-update',self.__songlua_transition_state,dt)end)
        part:linear(.25):x(20):queuecommand('Own')
        part.OwnCommand=function(self)log('own',self:GetX());collectgarbage('collect')end
        child:linear(.2):x(10)
    "#);
    p.advance(0.3, 0.3);
    p.script(
        "assert(starts==0 and part:GetX()==20 and child:GetX()==initial_x);log(child:GetTweenTimeLeft())",
    );
    p.start();
    p.start();
    p.script("assert(starts==1);log(part:GetTweenTimeLeft())");
    for (sec, dt) in [(0.5, 0.2), (0.8, 0.3), (1.1, 0.3), (1.2, 0.1), (1.3, 0.1)] {
        p.advance(sec, dt);
    }
    p.script("assert(part.__songlua_transition_state==2 and part:GetTweenTimeLeft()==0);log(child:GetDiffuseAlpha())");
    p.start();
    p.script("assert(starts==1)");
}

#[test]
fn incoming_hide_queue_and_off_survive_collection_and_scene_release() {
    let mut p = Pair::new("In", true, "");
    p.script(r#"
        weak=setmetatable({}, {__mode='v'});local visuals=part[1][1]
        weak[1]=part;weak[2]=part[1];weak[3]=visuals;weak[4]=part[1][2]
        for i,a in ipairs(visuals)do weak[i+4]=a end
        local hide=visuals.HideCommand
        visuals.HideCommand=function(self)collectgarbage('collect');hide(self);log('hide',SL.Global.GameplayReloadCheck)end
        part:SetUpdateFunction(function(self,dt)collectgarbage('collect');log('update',self.__songlua_transition_state)end)
    "#);
    p.start();
    for (sec, dt) in [
        (0.5, 0.5),
        (1., 0.5),
        (1.5, 0.5),
        (2.1, 0.6),
        (2.2, 0.1),
        (2.3, 0.1),
    ] {
        p.advance(sec, dt);
    }
    p.script(
        r#"
        assert(part.__songlua_transition_state==2 and not part[1][1]:GetVisible())
        assert(SL.Global.GameplayReloadCheck==true)
        part:playcommand('Off');assert(SL.Global.GameplayReloadCheck==false)
        part=nil;root=nil;collectgarbage('collect');assert(weak[1]~=nil)
    "#,
    );
    p.rust.release_actor_scene(p.scene).unwrap();
    p.rust
        .vm_mut()
        .execute(
            &compile_named(
                b"collectgarbage('collect');collectgarbage('collect');assert(next(weak)==nil)",
                b"=release transitions",
            )
            .unwrap(),
        )
        .unwrap();
}

#[test]
fn outgoing_off_preserves_white_score_and_routine_updates() {
    let mut p = Pair::new("Out", true, "");
    p.script(r#"
        SL.Global.GameMode='ITG';SL.Global.Stages.PlayedThisGame=0
        SL.P1.Stages.Stats[1]={ex_counts={W0_total=3}}
        SL.P2.Stages.Stats[1]={ex_counts={W0_total=5}}
        local pss={};for i=1,2 do pss[i]={
            GetTapNoteScores=function(self,k)assert(k=='TapNoteScore_W1');return 20*i end,
            GetPercentDancePoints=function()return 1 end,
            SetScore=function(self,n)log('score',i,n)end,
        }end
        local routine={GetPercentDancePoints=function()return 1 end,SetScore=function(self,n)log('routine',n)end}
        GAMESTATE.GetHumanPlayers=function()return {'PlayerNumber_P1','PlayerNumber_P2'}end
        GAMESTATE.GetMasterPlayerNumber=function()return 'PlayerNumber_P1'end
        GAMESTATE.GetCurrentSong=function()return 'song'end
        GAMESTATE.GetCurrentSteps=function(self,pn)return pn end
        STATSMAN={};PROFILEMAN={}
        STATSMAN.GetCurStageStats=function()return {
            GetPlayerStageStats=function(self,pn)return pss[pn=='PlayerNumber_P1' and 1 or 2]end,
            GetRoutineStageStats=function()return routine end,
        }end
        PROFILEMAN.IsPersistentProfile=function()return true end
        PROFILEMAN.GetProfile=function()return {GetHighScoreList=function(self,song,steps)
            assert(song=='song');return {GetHighScores=function()return {
                {GetPercentDP=function()return .9 end,GetScore=function()return 1 end},
                {GetPercentDP=function()return 1 end,GetScore=function()return 14 end},
            }end}
        end}end
        IsRoutine=function()return true end
        part:playcommand('Off');collectgarbage('collect')
        assert(table.concat(observations,'|')=='score,1,14|score,2,14|routine,14', table.concat(observations,'|'))
        part[1]:stoptweening();part:playcommand('On');log(part[1]:GetTweenTimeLeft())
    "#);
    p.start();
    p.advance(0.75, 0.75);
    p.advance(1.6, 0.85);
    p.advance(1.7, 0.1);
    p.script("assert(part.__songlua_transition_state==2)");
}

#[test]
fn transition_time_uses_retained_child_and_raw_state_not_owner_queue() {
    let mut p = Pair::new("Cancel", true, "");
    p.script(r#"
        child=part[1];part:sleep(9);child:hibernate(.5):sleep(.75)
        assert(part:GetTweenTimeLeft()==0)
        part.__songlua_transition_state='1';assert(part:GetTweenTimeLeft()==1.25)
        child.__songlua_theme_hibernating=true;assert(part:GetTweenTimeLeft()==math.huge)
        child.__songlua_theme_hibernating=nil
        part.__songlua_transition_child={__songlua_capture_tween_time_left=3,[1]={__songlua_hibernate_seconds=4}}
        assert(part:GetTweenTimeLeft()==4)
        part.__songlua_transition_child=nil;assert(part:GetTweenTimeLeft()==0)
        part.__songlua_transition_child=false;assert(not pcall(function()return part:GetTweenTimeLeft()end))
        part.__songlua_transition_state=2;assert(part:GetTweenTimeLeft()==0)
        for _,value in ipairs({-1,256,0/0,math.huge,false,'invalid'})do
            part.__songlua_transition_state=value;assert(not pcall(function()return part:GetTweenTimeLeft()end))
        end
        part.__songlua_transition_child=child;part.__songlua_transition_state=0
        child.StartTransitioningCommand=function(self)
            part.__songlua_transition_child=nil;collectgarbage('collect');self:sleep(.25)
            log('detached',self:GetTweenTimeLeft())
        end
    "#);
    p.start();
    p.script("assert(part.__songlua_transition_state==1 and part:GetTweenTimeLeft()==0)");
}

#[test]
fn screen_geometry_uses_current_globals_typed_dimensions_and_owner_binding() {
    let mut p = Pair::new("Out", true, "");
    p.script(r#"
        local a=part[1];a:stoptweening()
        SCREEN_WIDTH='801.9';SCREEN_HEIGHT='601.2';SCREEN_CENTER_X=nil;SCREEN_CENTER_Y=nil
        a:FullScreen();pose(a);a:Center();pose(a)
        SCREEN_CENTER_X=10.25;SCREEN_CENTER_Y=-9.75
        a:CenterX();pose(a);a:CenterY();pose(a)
        a:linear(1);a:FullScreen();pose(a);a:Center();pose(a)
        local x=Actor.x;Actor.x=function()error('public x override')end
        a:Center();Actor.x=x
        SCREEN_WIDTH=false;assert(not pcall(function()a:Center()end));assert(not pcall(function()a:FullScreen()end))
        SCREEN_WIDTH=2147483648;assert(not pcall(function()a:CenterY()end))
        SCREEN_WIDTH=nil;SCREEN_HEIGHT=nil;SCREEN_CENTER_X=nil;SCREEN_CENTER_Y=nil
        a:Center():FullScreen();pose(a)
        local ok=pcall(function()local center=a.Center;center(nil)end);assert(not ok);log(ok)
    "#);
}

#[test]
fn transition_start_owns_plain_callback_children_snapshot_through_gc() {
    let mut p = Pair::new("Cancel", true, "");
    p.script(r#"
        weak=setmetatable({}, {__mode='v'})
        local one={StartTransitioningCommand=function(self)
            weak[1][1]=nil;weak[1][2]=nil;collectgarbage('collect')
            assert(weak[3]~=nil);log('first')
        end}
        local two={StartTransitioningCommand=function(self)collectgarbage('collect');assert(self==weak[3]);log('second')end}
        local child={one,two,StartTransitioningCommand=function(self)
            part.__songlua_transition_child=nil;collectgarbage('collect');log('parent')
        end}
        weak[1]=child;weak[2]=one;weak[3]=two
        part.__songlua_transition_child=child
    "#);
    p.start();
    p.script("assert(part.__songlua_transition_state==1);collectgarbage('collect');assert(next(weak)==nil)");
}

#[test]
fn transition_owner_hibernation_and_rate_control_child_delta_and_last_callback() {
    let mut p = Pair::new("Cancel", true, "");
    p.script(
        r#"
        part[1].StartTransitioningCommand=function(self)self:sleep(.5)end
        part:hibernate(.25):sleep(8):SetUpdateRate(2)
        part:SetUpdateFunction(function(self,dt)log('owner',self.__songlua_transition_state,dt)end)
    "#,
    );
    p.start();
    p.script("assert(part:GetTweenTimeLeft()==.5)");
    p.advance(0.125, 0.125);
    p.script("assert(part:GetTweenTimeLeft()==.5)");
    p.advance(0.5, 0.375);
    p.script("assert(part:GetTweenTimeLeft()==0 and part.__songlua_transition_state==1)");
    p.advance(0.625, 0.125);
    p.script("assert(part.__songlua_transition_state==2)");
    p.advance(0.75, 0.125);
}

#[test]
fn existing_iterator_and_enum_helpers_keep_indexing_conversion_and_alias_identity() {
    let mut p = Pair::new("Cancel", true, "");
    p.script(r#"
        assert(pname==ToEnumShortString)
        for _,v in ipairs({'PlayerNumber_P1','A_B_C','bare','_','',123})do log(ToEnumShortString(v))end
        for _,v in ipairs({false,{},function()end,string.char(255)})do assert(not pcall(ToEnumShortString,v))end
        local t=setmetatable({1,false,3},{__index=function(self,i)log('lookup',i);return i==4 and 9 or nil end})
        local iter=ivalues(t);collectgarbage('collect');log(iter());t[2]=7;log(iter(),iter(),iter(),iter())
        local invalid=ivalues(false);assert(type(invalid)=='function' and not pcall(invalid))
    "#);
}

#[test]
fn incoming_init_and_on_handlers_use_native_color_helpers_and_current_class_methods() {
    let mut p = Pair::new("In", true, "");
    p.script(r#"
        local visuals=part[1][1]
        for _,a in ipairs(visuals)do a:stoptweening();a:diffuse(1,1,1,1);a:playcommand('Init');log(unpack(a:GetDiffuse()));pose(a)end
        assert(visuals[1]:GetDiffuse()[1]==0 and visuals[2]:GetDiffuseAlpha()<1)
        for _,a in ipairs(visuals)do a:playcommand('On');log(a:GetTweenTimeLeft())end
        for _,name in ipairs({'Black','Blue','Green','HoloBlue','HoloDarkBlue','HoloDarkGreen','HoloDarkOrange','HoloDarkPurple','HoloDarkRed','HoloGreen','HoloOrange','HoloPurple','HoloRed','Invisible','Orange','Outline','Purple','Red','Stealth','White','Yellow'})do
            assert(Color(name)==Color[name]);log(unpack(Color(name)))
        end
        for _,value in ipairs({'#123456','bad',false,123})do local c=Color(value);if c then log(unpack(c))else log(nil)end end
        for _,value in ipairs({Color.Black,{'0.25','0.5','0.75'}, {},false,'#987654','invalid'})do log(unpack(Color.Alpha(value,.3)))end
        assert(not pcall(Color.Alpha,Color.Black,false))
        local original=Color;Color={Black={.5,.5,.5,1}};visuals[1]:stoptweening():playcommand('Init');log(unpack(visuals[1]:GetDiffuse()));Color=original
        collectgarbage('collect');assert(Color('Black')==Color.Black)
    "#);
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
