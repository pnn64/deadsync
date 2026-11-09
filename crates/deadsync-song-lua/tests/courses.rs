use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime},
};

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/courses.lua").unwrap())
        .unwrap_or_else(|e| panic!("{source}\n{e:?}"))
        .to_vec()
}
struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
}
impl Pair {
    fn new(source: &str) -> Self {
        let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "course migration");
        context.live = true;
        let source = format!(
            r#"
            seen={{}}
            function log(...) for i=1,select('#',...) do
                local v=select(i,...);seen[#seen+1]=type(v)=='number' and string.format('%.9g',v) or tostring(v)
            end end
            {source}
        "#
        );

        let mut rust = SongRuntime::new(&context).unwrap();
        let value = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(value).unwrap();
        let mut pair = Self { rust, scene };
        pair.compare("construction");
        pair
    }
    fn compare(&mut self, phase: &str) {
        let source = "return table.concat(seen,'|')";
        let Value::String(actual) = execute(&mut self.rust, source)[0] else {
            panic!("observations")
        };
        reference_test::assert_debug(
            "song/tests/rust_courses.rs::observations",
            &(phase, &self.rust.vm_mut().string_bytes(actual).unwrap()),
        );
    }
    fn script(&mut self, source: &str) {
        execute(&mut self.rust, source);
        self.compare(source);
    }
}
const SIMPLE: &str = "return Def.CourseContentsList{InitCommand=function(self)c=self end}";

#[test]
fn scroller_defaults_coercion_clamps_and_private_methods() {
    let mut p = Pair::new(SIMPLE);
    p.script(r#"
        assert(CourseContentsList==nil and c:GetNumItems()==1)
        assert(c:GetCurrentItem()==0 and c:GetDestinationItem()==0 and not c:GetLoop())
        assert(c:SetCurrentAndDestinationItem('-3.5')==c)
        log(c:GetCurrentItem(),c:GetDestinationItem())
        c:SetDestinationItem(-7):SetSecondsPerItem('0.3'):SetSecondsPauseBetweenItems(.2)
        log(c:GetSecondsToDestination(),c:GetFullScrollLengthSeconds())
        c.__songlua_scroller_num_items='5.7'
        c:ScrollWithPadding('1.25',2.5)
        log(c:GetNumItems(),c:GetCurrentItem(),c:GetDestinationItem(),c:GetFullScrollLengthSeconds())
        c:ScrollThroughAllItems();log(c:GetDestinationItem())
        c:SetNumSubdivisions('3.7'):SetNumItemsToDraw(-5):SetPauseCountdownSeconds(-2)
        log(c.__songlua_scroller_num_subdivisions,c.__songlua_scroller_num_items_to_draw,c.__songlua_scroller_pause_countdown)
        c:SetLoop(0);c:SetLoop();assert(c:GetLoop());c:SetLoop(nil);assert(not c:GetLoop())
        c:SetWrap(''):SetFastCatchup(false):SetMask();log(c.__songlua_scroller_wrap,c.__songlua_scroller_fast_catchup,c.__songlua_scroller_mask)
        local bound=c.SetCurrentItem;bound({},'17');assert(c:GetCurrentItem()==17)
        ActorFrame.SetCurrentItem=function()return 'override'end
        assert(c:SetCurrentItem(8)=='override');bound(false);bound('bad');bound()
        ActorFrame.SetCurrentItem=nil;assert(c:GetCurrentItem()==17)
        for _,value in ipairs{'-4','0','1.9','bad','inf'} do
            c.__songlua_scroller_num_items=value
            local ok,n=pcall(c.GetNumItems);log(ok,ok and n)
        end
    "#);
}

#[test]
fn raw_display_identity_child_first_init_and_startup() {
    let mut p = Pair::new(
        r#"
        d=Def.ActorFrame{
            Def.Actor{Name='leaf',InitCommand=function(self)log('leaf init',self.GetX==nil,self.__songlua_parent==nil)end,
                BeginCommand=function()log('leaf begin')end,OnCommand=function()log('leaf on')end},
            InitCommand=function(self)log('display init',self.GetX==nil,self.__songlua_parent.Name)end,
            BeginCommand=function()log('display begin')end,OnCommand=function()log('display on')end}
        return Def.CourseContentsList{Display=d,Name='course',
            Def.Actor{Name='authored',InitCommand=function()log('authored init')end},
            InitCommand=function(self)c=self;assert(self.Display==d and self[1]==d);log('course init',self:GetNumChildren(),self:GetChild('Display')==d)end,
            OnCommand=function()log('course on')end}
    "#,
    );
    p.script("assert(d.Name=='Display' and d.GetX==nil and d[1].GetX==nil)");

    p.rust.start_actor_scene(p.scene).unwrap();
    p.compare("startup");
    p.rust.advance_actor_scene(p.scene, 0.125).unwrap();
}

#[test]
fn positioning_preserves_transform_priority_and_collecting_callbacks() {
    let mut p = Pair::new(
        "return Def.CourseContentsList{Display={},InitCommand=function(self)c=self;d=self.Display end}",
    );
    p.script(
        r#"
        c:SetTransformFromWidth(30):PositionItems();assert(d.X==0 and d.Y==nil)
        c:SetTransformFromHeight(-20):PositionItems();log(1/d.Y)
        c:SetTransformFromFunction(function(self,offset,index,count)
            assert(self==d);collectgarbage('collect');log('transform',offset,index,count)
            c:SetCurrentItem(7)
        end)
        c.__songlua_scroller_num_items=-2
        c:PositionItems();c:SetTransformFromFunction(nil):SetTransformFromWidth(99):PositionItems()
        assert(c:GetCurrentItem()==7)
        c.__songlua_scroller_transform_function={}
        local ok,err=pcall(function()c:PositionItems()end)
        assert(not ok and string.find(tostring(err),'function'));log(ok)
    "#,
    );
}

#[test]
fn set_song_params_are_per_player_shared_with_leaves_and_reentrant_safe() {
    let mut p = Pair::new(
        r#"
        d=Def.ActorFrame{
            Def.Actor{SetSongCommand=function(self,p)log('leaf',p.PlayerNumber,p.mark,p.Meter,p.Difficulty);assert(p==last)end},
            SetSongCommand=function(self,p)
                assert(p~=last);last=p;p.mark='changed'
                collectgarbage('collect');log('display',p.Number,p.PlayerNumber,p.Song.name,p.Steps.tag)
            end}
        return Def.CourseContentsList{Display=d,InitCommand=function(self)c=self end}
    "#,
    );
    p.script(r#"
        GAMESTATE={GetCurrentSong=function(self)assert(self==GAMESTATE);return {name='song'}end,
            GetCurrentSteps=function(self,pn)return {tag=pn,GetMeter=function()return pn=='PlayerNumber_P1' and 12 or false end,
                GetDifficulty=function()return 'Difficulty_Hard'end}end}
        assert(c:SetFromGameState()==c and c.__songlua_course_contents_from_gamestate)
        assert(c:GetNumItems()==1)
        local method=c.SetFromGameState;c.__songlua_actor_type='Actor';method();c.__songlua_actor_type='CourseContentsList'
        GAMESTATE={GetCurrentSong=function()error('manager failure')end}
        c.__songlua_scroller_num_items=7
        local ok=pcall(method);log(ok,c:GetNumItems(),c.__songlua_course_contents_from_gamestate)
    "#);
}

#[test]
fn leaf_display_receives_both_set_song_passes_and_missing_steps_defaults() {
    let mut p = Pair::new(
        r#"
        return Def.CourseContentsList{Display={SetSongCommand=function(self,p)
            log(p.PlayerNumber,p.Meter,p.Difficulty,p==last,p.Song==nil,p.Steps==nil);last=p
        end},InitCommand=function(self)c=self end}
    "#,
    );
    p.script("GAMESTATE=nil;c:SetFromGameState();collectgarbage('collect')");
}

#[test]
fn shared_display_keeps_identity_and_initializes_once() {
    let mut p = Pair::new(
        r#"
        d={InitCommand=function(self)log('display init',self.__songlua_parent.Name)end,
            OnCommand=function(self)log('display on',self.__songlua_parent.Name)end}
        return Def.ActorFrame{
            Def.CourseContentsList{Name='first',Display=d,InitCommand=function(self)a=self end},
            Def.CourseContentsList{Name='second',Display=d,InitCommand=function(self)b=self end},
            InitCommand=function(self)assert(a.Display==b.Display and a[1]==d and b[1]==d);log(d.__songlua_parent==b)end}
    "#,
    );

    p.rust.start_actor_scene(p.scene).unwrap();
    p.compare("shared startup");
    p.script("assert(d.__songlua_init_commands_ran and d.__songlua_startup_commands_ran)");
    p.rust.release_actor_scene(p.scene).unwrap();
    let value = execute(&mut p.rust, "return Def.CourseContentsList{Display=d}")[0];
    let scene = p.rust.instantiate_actor(value).unwrap();
    p.compare("reattachment does not repeat Init");
    p.rust.release_actor_scene(scene).unwrap();
}

#[test]
fn existing_display_actor_and_duplicate_array_declaration_keep_native_names() {
    let mut p = Pair::new(
        r#"
        d=LoadFont('Common Normal'):settext('display'):x(4)
        d.InitCommand=function(self)log('text init',self:GetName(),self:GetX())end
        return Def.CourseContentsList{Display=d,InitCommand=function(self)c=self
            assert(self.Display==d and self:GetChild('Display')==d)
        end}
    "#,
    );
    p.script(
        "assert(d.SetUpdateFunction==nil);d.__songlua_update_function=function(self,dt)log('text update',dt,self:GetParent():GetName())end",
    );

    p.rust.advance_actor_scene(p.scene, 0.125).unwrap();
    p.compare("existing display update");

    let mut p = Pair::new(
        r#"
        d=Def.Actor{InitCommand=function(self)log('init',self.Name,self.GetX==nil)end,
            SetSongCommand=function(self,p)log('song',self.GetX==nil,p.PlayerNumber)end}
        return Def.CourseContentsList{Display=d,d,InitCommand=function(self)c=self
            assert(self[1]==d and self[2]~=d and self[2].Name=='Display')
            local group=self:GetChild('Display');assert(group[1]==d and group[2]==self[2])
        end}
    "#,
    );
    p.script("GAMESTATE=nil;c:SetFromGameState()");
}

#[test]
fn raw_children_are_selected_at_init_and_conditions_keep_constructor_side_effects() {
    let mut p = Pair::new(
        r#"
        d={children={leaf={InitCommand=function()log('old leaf')end}}}
        excluded={Condition=false,InitCommand=function()error('excluded')end}
        return Def.ActorFrame{
            Def.Actor{InitCommand=function()
                d.children.leaf={InitCommand=function()log('new leaf')end}
            end},
            Def.CourseContentsList{Display=d,InitCommand=function(self)c=self end},
            Def.CourseContentsList{Display=excluded,InitCommand=function(self)
                assert(self:GetNumChildren()==0 and excluded.Name=='Display' and excluded.__songlua_parent==self)
            end}}
    "#,
    );
    p.script("assert(d[1]==d.children.leaf)");
}

#[test]
fn shared_scene_release_keeps_actor_and_later_wrappers_until_last_owner() {
    let context = SongLuaCompileContext::new(std::path::Path::new("."), "shared course lifetime");
    let mut runtime = SongRuntime::new(&context).unwrap();
    let value = execute(
        &mut runtime,
        "return Def.ActorFrame{InitCommand=function(self)d=self end}",
    )[0];
    let original = runtime.instantiate_actor(value).unwrap();
    let value = execute(
        &mut runtime,
        "d.Name='Display';return Def.CourseContentsList{Display=d,InitCommand=function(self)c=self end}",
    )[0];
    let course = runtime.instantiate_actor(value).unwrap();
    execute(
        &mut runtime,
        "d:AddWrapperState();w=d:GetWrapperState(1);w:x(19);d:x(7);weak=setmetatable({d,w,c},{__mode='v'})",
    );
    runtime.release_actor_scene(original).unwrap();
    execute(
        &mut runtime,
        "collectgarbage('collect');assert(d:GetX()==7 and w:GetX()==19 and c.Display==d)",
    );
    runtime.advance_actor_scene(course, 0.125).unwrap();
    runtime.release_actor_scene(course).unwrap();
    execute(
        &mut runtime,
        "assert(not pcall(function()d:x(8)end));d=nil;w=nil;c=nil;collectgarbage('collect');collectgarbage('collect');assert(next(weak)==nil)",
    );
}

#[test]
fn raw_load_name_and_draw_order_survive_child_callbacks() {
    let mut p = Pair::new(
        r#"
        d={Name=42,
            {Name='first',InitCommand=function(self)self.__songlua_state_draw_order=5;d.Name='changed'end},
            {Name='second',InitCommand=function(self)self.__songlua_state_draw_order=-2 end},
            InitCommand=function(self)log(self.Name,type(self.Name),self.__songlua_native_name,self.__songlua_load_name,self[1].Name,self[2].Name)end}
        return Def.CourseContentsList{Display=d,InitCommand=function(self)c=self end}
    "#,
    );
    p.script("assert(d.Name==42 and d[1].Name=='second' and d[2].Name=='first')");
}

#[test]
fn malformed_display_cleanup_leaves_the_runtime_usable() {
    let context = SongLuaCompileContext::new(std::path::Path::new("."), "course cleanup");
    let mut runtime = SongRuntime::new(&context).unwrap();
    let mut raw = Pair::new(
        "return Def.CourseContentsList{Display={{Name=true}},InitCommand=function(self)c=self;d=self.Display end}",
    );
    raw.script("assert(d[1].Name==true and d[1].__songlua_native_name==true);local ok=pcall(Actor.GetName,d[1]);assert(not ok);log(ok)");
    for source in [
        "return Def.CourseContentsList{Display=7}",
        "return Def.CourseContentsList{Display={Name=true}}",
        "return Def.CourseContentsList{Display={InitCommand=function()error('display init failure')end}}",
    ] {
        let value = execute(&mut runtime, source)[0];
        assert!(runtime.instantiate_actor(value).is_err(), "{source}");
        let value = execute(&mut runtime, SIMPLE)[0];
        let scene = runtime.instantiate_actor(value).unwrap();
        execute(
            &mut runtime,
            "collectgarbage('collect');assert(c:GetNumItems()==1)",
        );
        runtime.release_actor_scene(scene).unwrap();
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
