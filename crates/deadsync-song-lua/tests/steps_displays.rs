use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime, test_support},
};
use std::path::Path;

struct Pair {
    rust: SongRuntime,
    scenes: Vec<ActorScene>,
}

impl Pair {
    fn new(live: bool) -> Self {
        let mut context = SongLuaCompileContext::new(Path::new("."), "steps display migration");
        context.live = live;
        context.players[1].enabled = true;

        let rust = SongRuntime::new(&context).unwrap();
        let mut pair = Self {
            rust,
            scenes: Vec::new(),
        };
        for (name, global) in [("StepsDisplayP1", "part"), ("StepsDisplayP2", "part2")] {
            let scene = test_support::screen_part(&mut pair.rust, name).unwrap();
            let actor = pair.rust.actor_scene_root(scene).unwrap();
            pair.rust
                .vm_mut()
                .set_global(global.as_bytes(), actor)
                .unwrap();
            pair.scenes.push(scene);
        }
        pair.script(r#"
            root=part;observations={}
            assert(PLAYER_1=='PlayerNumber_P1' and PLAYER_2=='PlayerNumber_P2')
            function log(...)local t={};for i=1,select('#',...)do
                local v=select(i,...);t[i]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
            end;observations[#observations+1]=table.concat(t,',')end
            function pose(a)log(a:GetX(),a:GetY(),a:GetZoomX(),a:GetZoomY(),a:GetVisible(),a:GetDiffuseAlpha())end
            screen={__songlua_children={StepsDisplayP1=part,StepsDisplayP2=part2}}
            part.__songlua_preferred_difficulty='Difficulty_Hard'
            part2.__songlua_preferred_difficulty='Difficulty_Challenge'
            function steps(diff,kind,meter,description,autogen)
                return {
                    GetDifficulty=function(self)collectgarbage('collect');return diff end,
                    GetStepsType=function(self)return kind end,
                    GetMeter=function(self)return meter end,
                    GetDescription=function(self)return description end,
                    IsAutogen=function(self)return autogen end,
                }
            end
        "#);
        pair
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/steps_displays.lua").unwrap())
            .unwrap_or_else(|e| panic!("Rust: {source}\n{e:?}"));
        self.compare(source);
    }
    fn compare(&mut self, _phase: &str) {
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
            "song/tests/rust_steps_displays.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }
    fn initialize(&mut self) {
        let screen = self.rust.vm_mut().get_global(b"screen").unwrap();
        test_support::initialize_steps_displays(&mut self.rust, screen).unwrap();
        self.compare("gameplay display initialization");
    }
}

#[test]
fn owned_children_defaults_class_overrides_and_typed_receivers_match() {
    for live in [false, true] {
        let mut p = Pair::new(live);
        p.script(r#"
            assert(StepsDisplay==nil and part.__songlua_actor_type=='StepsDisplay')
            assert(part:GetNumChildren()==4 and part:GetChild('missing')==nil)
            for i,name in ipairs{'Frame','Meter','Description','Autogen'}do
                local c=part:GetChild(name);assert(part[i]==c and c:GetParent()==part);pose(c)
                log(name,c.Text,c.Font,type(c.GetTexture))
                if c.GetTexture then local t=c:GetTexture();log(t:GetPath(),t:GetImageWidth(),t:GetImageHeight())end
            end
            assert(part.GetText==nil and part.SetUpdateFunction~=nil and part:GetVisible())
            assert(part:SetFromSteps(nil)==part and not part:GetVisible())
            assert(part2:SetFromTrail()==part2 and not part2:GetVisible())
            local set=part.SetFromSteps;local other={};assert(set(other,nil)==other and other.__songlua_visible==false)
            assert(not pcall(set,42,nil) and not pcall(set,part,false))
            ActorFrame.SetFromSteps=function(self,v)assert(self==part and v=='class');return 19 end
            assert(part:SetFromSteps('class')==19);ActorFrame.SetFromSteps=nil
            assert(set(part2,nil)==part2);log(__songlua_side_effect_count)
            part[1]:name('renamed');assert(part:GetChild('Frame')==nil and part:GetChild('renamed')==part[1])
            assert(part.__songlua_children.Frame==part[1])
        "#);
    }
}

#[test]
fn setters_keep_parameter_identity_mapping_and_two_child_passes() {
    for live in [false, true] {
        let mut p = Pair::new(live);
        p.script(r#"
            part:propagate(false)
            function observe(self,p)
                assert(p==current or current==nil);current=p;collectgarbage('collect')
                log(self:GetName(),p.CustomDifficulty,p.DisplayDescription,p.Meter,p.StepsType,p.Steps==chosen,p.Trail==chosen,self:GetVisible(),part:GetVisible())
                p.visits=(p.visits or 0)+1
            end
            part.SetCommand=observe;for _,c in ipairs(part)do c.SetCommand=observe end
            for _,case in ipairs{
                {'Difficulty_Hard','StepsType_Pump_Single','8.75','unused',true},
                {'Difficulty_Medium','StepsType_Pump_Double',0,'unused',false},
                {'Difficulty_Hard','StepsType_Pump_Double',9,'unused',0},
                {'Difficulty_Medium','StepsType_Pump_Halfdouble',-3,'unused',nil},
                {'Difficulty_Beginner','StepsType_Dance_Single',2,'unused',false},
                {'Difficulty_Challenge','StepsType_Dance_Single',18,'unused',true},
                {'Difficulty_Edit','StepsType_Dance_Single',10,'author description',true},
                {'Difficulty_Invalid','StepsType_Invalid',0,'unused',false},
            }do
                for _,method in ipairs{'SetFromSteps','SetFromTrail'}do
                    chosen=steps(unpack(case));current=nil
                    assert(part[method](part,chosen)==part and part:GetVisible())
                    assert(current.visits==(case[3]==0 and 8 or 9))
                    log(part[2]:GetText(),part[3]:GetText(),part[4]:GetVisible(),__songlua_side_effect_count)
                end
            end
            local original=part.SetFromSteps;assert(original(part2,steps('Hard','Other',4,'',true))==part2)
            assert(part2[2]:GetText()=='4' and part2[3]:GetText()=='Hard')
        "#);
    }
}

#[test]
fn getter_failures_preserve_partial_writes_and_reentrant_visibility() {
    let mut p = Pair::new(true);
    p.script(r#"
        part:SetFromSteps(nil);local before=__songlua_side_effect_count
        local value=steps('Difficulty_Edit','StepsType_Dance_Single',6,'editing',true)
        for _,bad in ipairs{false,'bad',math.huge,2147483648,-2147483649}do
            value.GetMeter=function()return bad end
            assert(not pcall(part.SetFromSteps,part,value) and not part:GetVisible())
            assert(__songlua_side_effect_count==before)
        end
        value.GetMeter=function()return 6 end
        local order={};local function note(s)order[#order+1]=s end
        value.GetDifficulty=function()note('difficulty');return 'Difficulty_Edit'end
        value.GetStepsType=function()note('type');return 'StepsType_Dance_Single'end
        value.GetMeter=function()note('meter');return 6 end
        value.GetDescription=function()note('description');return 'editing'end
        value.IsAutogen=false
        for _,c in ipairs(part)do c.SetCommand=function(self,p)note(self:GetName());collectgarbage('collect')end end
        part.SetCommand=function()error('must not reach parent before IsAutogen')end
        assert(not pcall(part.SetFromSteps,part,value))
        assert(part:GetVisible() and part[2].Text=='6' and part[3].Text=='editing')
        assert(__songlua_side_effect_count==before)
        assert(table.concat(order,',')=='difficulty,type,meter,description,Frame,Meter,Description,Autogen')
        value.IsAutogen=function()note('autogen');return false end
        part.SetCommand=function(self,p)note('parent');log(self:GetVisible())end
        local entered=false
        part[1].SetCommand=function(self,p)
            note('Frame');if not entered then entered=true;part:SetFromSteps(nil)end
        end
        order={};assert(part:SetFromSteps(value)==part)
        assert(not part:GetVisible() and not part[4]:GetVisible())
        log(table.concat(order,','),__songlua_side_effect_count-before)
    "#);
}

#[test]
fn game_state_route_uses_current_object_and_nil_preferred_difficulty() {
    for live in [false, true] {
        let mut p = Pair::new(live);
        p.script(r#"
            local original=GAMESTATE
            for _,course in ipairs{false,true,0}do
                local selected=steps('Difficulty_Edit','StepsType_Dance_Double',12,'custom edit',true)
                local game={};GAMESTATE=game
                game.IsCourseMode=function(self)assert(self==game);GAMESTATE={};collectgarbage('collect');return course end
                game.GetCurrentSteps=function(self,player)assert(self==game and player=='opaque');log('steps');return selected end
                game.GetCurrentTrail=function(self,player)assert(self==game and player=='opaque');log('trail');return selected end
                assert(part:SetFromGameState('opaque')==part);log(part[3].Text,part[4]:GetVisible())
            end
            GAMESTATE={IsCourseMode=function()return false end,GetCurrentSteps=function()return nil end}
            part:SetFromGameState(nil);assert(part:GetVisible() and part[2].Text=='?' and part[3].Text=='Hard')
            GAMESTATE.GetCurrentSteps=function()return false end
            local count=__songlua_side_effect_count;assert(not pcall(part.SetFromGameState,part,{}))
            assert(__songlua_side_effect_count==count)
            GAMESTATE=original;log(count)
        "#);
    }
}

#[test]
fn gameplay_initialization_keeps_metrics_player_order_and_real_commands() {
    for live in [false, true] {
        let mut p = Pair::new(live);
        p.script(r#"
            choices={steps('Difficulty_Edit','StepsType_Dance_Single',7,'edit chart',true),steps('Difficulty_Hard','StepsType_Pump_Single',0,'',false)}
            GAMESTATE.GetCurrentSteps=function(self,p)log('current',p);collectgarbage('collect');return choices[p=='PlayerNumber_P1' and 1 or 2]end
            CustomDifficultyToColor=function(diff)log('color',diff);collectgarbage('collect');return {.2,.4,.6,1}end
            local broadcast=MESSAGEMAN.Broadcast
            MESSAGEMAN.Broadcast=function(self,name,...)log('broadcast',name);return broadcast(self,name,...)end
            for _,display in ipairs{part,part2}do
                display.visible=function()error('native visibility must bypass public override')end
                for _,c in ipairs(display)do c.x=function()error('native placement must bypass public override')end end
            end
            oldFrameLoad=part[1].LoadCommand
        "#);
        p.initialize();
        p.script(
            r#"
            assert(part:GetVisible() and part2:GetVisible())
            assert(part[1]:GetZoomX()==1 and part2[1]:GetZoomX()==-1)
            assert(part[1].LoadCommand==part2[1].LoadCommand)
            assert(part[1].LoadCommand~=oldFrameLoad)
            assert(not part[2]:GetVisible() and not part2[2]:GetVisible())
            assert(part[3]:GetVisible() and not part2[3]:GetVisible())
            assert(part[3].Text=='edit chart' and part2[3].Text=='Crazy')
            assert(part[2].Text=='7' and part2[2].Text=='?')
            for _,display in ipairs{part,part2}do
                for _,c in ipairs(display)do pose(c)end
                log(display[3]:GetHAlign(),display[3]:GetVAlign(),__songlua_side_effect_count)
            end
            oldFrameLoad=part[1].LoadCommand;GAMESTATE.GetCurrentSteps=function()return nil end
        "#,
        );
        p.initialize();
        p.script("assert(not part:GetVisible() and not part2:GetVisible());assert(part[1].LoadCommand~=oldFrameLoad)");
    }
}

#[test]
fn detached_setters_accept_table_receivers_and_keep_removed_callback_children_alive() {
    let mut p = Pair::new(true);
    p.script(r#"
        local set=part.SetFromSteps
        local raw={__songlua_actor_type='StepsDisplay',__songlua_children={}}
        local order={};local function note(v)order[#order+1]=v end
        for _,name in ipairs{'Frame','Meter','Description','Autogen'}do
            raw.__songlua_children[name]={SetCommand=function(self,params)note(name);collectgarbage('collect')end}
        end
        raw.SetCommand=function(self,params)
            assert(self==raw);note('parent');assert(self.__songlua_visible)
        end
        local value=steps('Difficulty_Edit','StepsType_Dance_Single',3,'raw description',true)
        assert(set(raw,value)==raw)
        assert(raw.__songlua_children.Meter.Text=='3' and raw.__songlua_children.Description.Text=='raw description')
        assert(raw.__songlua_children.Autogen.__songlua_visible)
        -- Hash order of the second pass is allocator-dependent for a named-only
        -- plain table. Verify the ordered first pass and the complete visit set.
        assert(table.concat(order,',',1,5)=='Frame,Meter,Description,Autogen,parent')
        local count={};for _,name in ipairs(order)do count[name]=(count[name]or 0)+1 end
        assert(count.Frame==2 and count.Meter==2 and count.Description==2 and count.Autogen==2 and count.parent==1)
        order={};local a={SetCommand=function()note('a')end};local b={SetCommand=function()note('b');collectgarbage('collect')end}
        raw[1]=a;raw[2]=b
        raw.SetCommand=function(self,p)self.__songlua_children={};collectgarbage('collect')end
        a.SetCommand=function()note('a');raw[2]=nil;b=nil;collectgarbage('collect')end
        set(raw,value);assert(order[#order-1]=='a' and order[#order]=='b')
        log(raw.__songlua_visible,__songlua_side_effect_count)
    "#);
}

#[test]
fn removed_set_children_remain_rooted_through_later_getters_and_parent() {
    let mut p = Pair::new(true);
    p.script(
        r#"
        local raw={__songlua_children={}}
        local weak=setmetatable({}, {__mode='v'})
        for index,name in ipairs{'Frame','Meter','Description','Autogen'}do
            local child={};weak[index]=child;raw.__songlua_children[name]=child
            child.SetCommand=function(self)
                raw.__songlua_children[name]=nil;collectgarbage('collect')
                for previous=1,index do assert(weak[previous]~=nil)end
                assert(weak[index]==self)
            end
        end
        local value=steps('Difficulty_Hard','StepsType_Dance_Single',2,'',true)
        value.IsAutogen=function()
            collectgarbage('collect');for i=1,4 do assert(weak[i]~=nil)end;return true
        end
        raw.SetCommand=function(self)
            collectgarbage('collect');for i=1,4 do assert(weak[i]~=nil)end
            assert(weak[4].__songlua_visible)
        end
        assert(part.SetFromSteps(raw,value)==raw)
        collectgarbage('collect');assert(next(weak)==nil)
        log(raw.__songlua_visible,__songlua_side_effect_count)
    "#,
    );
}

#[test]
fn parameters_survive_gc_and_display_queues_advance_before_scene_release() {
    let mut p = Pair::new(true);
    p.script(r#"
        weak=setmetatable({}, {__mode='v'})
        GAMESTATE.IsCourseMode=function()return false end
        GAMESTATE.GetCurrentSteps=function()
            local v=steps('Difficulty_Hard','StepsType_Dance_Single',5,'',true);weak[1]=v;return v
        end
        part.SetCommand=function(self,params)assert(params.Steps==weak[1]);collectgarbage('collect');retained=params end
        local calls=0
        part[2].SetCommand=function(self,p)
            calls=calls+1;if calls==1 then self:linear(.5):x(100):queuecommand('Done')end
        end
        part[2].DoneCommand=function(self)log('done',self:GetX());collectgarbage('collect')end
        part:SetFromGameState('PlayerNumber_P1');collectgarbage('collect');assert(retained.Steps==weak[1])
        retained=nil;collectgarbage('collect');assert(weak[1]==nil)
    "#);
    for (_second, dt) in [(0.25, 0.25), (0.5, 0.25), (0.51, 0.01)] {
        p.rust.advance_actor_scene(p.scenes[0], dt).unwrap();
        p.script("log(part[2]:GetX(),part[2]:GetTweenTimeLeft())");
    }
    p.script(
        r#"
        for i,a in ipairs(part)do weak[i]=a end;weak[5]=part;weak[6]=part2
        part=nil;part2=nil;root=nil;screen=nil;collectgarbage('collect');assert(weak[5]~=nil)
    "#,
    );
    for scene in p.scenes {
        p.rust.release_actor_scene(scene).unwrap();
    }
    p.rust
        .vm_mut()
        .execute(
            &compile_named(
                b"collectgarbage('collect');collectgarbage('collect');assert(next(weak)==nil)",
                b"=release displays",
            )
            .unwrap(),
        )
        .unwrap();
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
