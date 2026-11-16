use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, ActorSnapshot, SongRuntime},
};

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/wrappers.lua").unwrap())
        .unwrap()
        .to_vec()
}
struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
    seconds: f32,
}
impl Pair {
    fn new(definition: &str) -> Self {
        let source = format!(
            r#"
            events={{}}
            function log(...) for i=1,select('#',...) do
                local v=select(i,...)
                events[#events+1]=type(v)=='number' and string.format('%.17g',v) or tostring(v)
            end end
            function observe()
                local function visit(a)
                    log(a:GetName(),a:GetX(),a:GetDestX(),a:GetRotationZ(),a:GetTweenTimeLeft(),a:GetSecsIntoEffect())
                    for i=1,#a do visit(a[i]) end
                    for i=1,a:GetNumWrapperStates() do visit(a:GetWrapperState(i)) end
                end
                visit(root)
            end
            {definition}
        "#
        );
        let mut context = SongLuaCompileContext::new(".", "wrapper migration");
        context.live = true;
        let mut rust = SongRuntime::new(&context).unwrap();
        let definition = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self {
            rust,
            scene,
            seconds: 0.0,
        };
        pair.compare("construction");
        pair
    }
    fn compare(&mut self, phase: &str) {
        let Value::String(actual) = execute(&mut self.rust, "return table.concat(events,'|')")[0]
        else {
            panic!("string")
        };
        let actual = String::from_utf8_lossy(self.rust.vm_mut().string_bytes(actual).unwrap());
        let actual: Vec<_> = actual.split('|').collect();
        reference_test::assert_source_debug(
            "song/tests/rust_wrappers.rs::Pair::compare::events",
            phase,
            &actual,
        );
        let clock = [self.seconds, self.seconds * 2.0];
        let states = self.rust.actor_scene_snapshot(self.scene).unwrap();
        let render = self
            .rust
            .actor_scene_render_states(self.scene, clock)
            .unwrap();
        fn visit(index: usize, states: &[ActorSnapshot], order: &mut Vec<usize>) {
            order.push(index);
            for &child in states[index].children.iter().chain(&states[index].wrappers) {
                visit(child, states, order);
            }
        }
        let mut order = Vec::new();
        visit(0, &states, &mut order);
        let actual = order
            .into_iter()
            .map(|index| format!("{:?}", render[index]))
            .collect::<Vec<_>>();
        reference_test::assert_source_debug(
            "song/tests/rust_wrappers.rs::Pair::compare::render",
            phase,
            &(self.seconds.to_bits(), actual),
        );
    }
    fn script(&mut self, source: &str) {
        execute(&mut self.rust, source);
        self.compare(source);
    }
    fn advance(&mut self, delta: f64) {
        self.seconds += delta as f32;
        self.rust
            .set_values(self.seconds * 2.0, self.seconds)
            .unwrap();
        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.script("observe()");
    }
}

#[test]
fn wrapper_membership_indices_and_frame_methods_match_production() {
    let mut pair = Pair::new("return Def.ActorFrame{Def.Quad{Name='child'}}");
    pair.script(
        r#"
        assert(root:GetNumWrapperStates()==0)
        w1=root:AddWrapperState('ignored'):name('first'):x(10)
        w2=root:AddWrapperState():name('second'):SetUpdateRate(2):diffusealpha(.5)
        assert(w1:GetParent()==root and w2:GetParent()==root)
        assert(w1:GetNumChildren()==0 and root:GetNumChildren()==1 and #root==1)
        assert(root:GetChild('first')==nil and root:GetChildren().first==nil)
        assert(root:GetWrapperState(1)==w1 and root:GetWrapperState(2)==w2)
        for _,v in ipairs{0,-1,3,1/0,-1/0,0/0,'1',false,{}} do
            assert(root:GetWrapperState(v)==nil)
        end
        assert(root:GetWrapperState(1.9)==w1 and root:GetWrapperState()==nil)
        assert(w1:GetWrapperState(1)==nil)
        nested=w1:AddWrapperState():name('nested')
        assert(nested:GetParent()==w1)
        ActorFrame.wrapper_probe=function(self) return self:GetName() end
        assert(w1:wrapper_probe()=='first')
        observe()
    "#,
    );
    pair.advance(0.25);
}

#[test]
fn wrappers_advance_before_owner_rate_and_obey_ancestor_hibernation() {
    let mut pair = Pair::new(
        r#"return Def.ActorFrame{
        Def.Quad{Name='child',InitCommand=function(self) self:linear(2):x(40) end},
        InitCommand=function(self)
            self:name('root'):SetUpdateRate(3):linear(2):x(20)
            local a=self:AddWrapperState():name('a'):SetUpdateRate(2):bob():linear(2):x(10)
            local b=self:AddWrapperState():name('b'):spin():linear(2):x(30)
            local c=a:AddWrapperState():name('c'):linear(2):x(50)
            c:SetUpdateFunction(function(s,dt) log('c',dt,s:GetX()) end)
            a:SetUpdateFunction(function(s,dt) log('a',dt,self:GetX()); self:SetUpdateRate(4) end)
            b:SetUpdateFunction(function(s,dt) log('b',dt,self:GetX()) end)
            self:SetUpdateFunction(function(s,dt) log('root',dt,s:GetX()) end)
        end}"#,
    );
    for delta in [0.0, 0.125, 0.125] {
        pair.advance(delta);
    }
    pair.script("root:hibernate(.5); root:GetWrapperState(1):hibernate(.125); observe()");
    for delta in [0.25, 0.25, 0.0, 0.125, 0.25] {
        pair.advance(delta);
    }
}

#[test]
fn wrapper_queues_and_messages_remain_separate_from_child_commands() {
    let mut pair = Pair::new("return Def.ActorFrame{Def.Quad{}}");
    pair.script(r#"
        w=root:AddWrapperState():name('wrapper'):linear(2):x(20):queuecommand('Own')
        w.OwnCommand=function(self) log('own'); self:linear(.25):x(40) end
        w.GoCommand=function(self) log('propagated') end
        w.PingMessageCommand=function(self) log('message'); self:addx(5); collectgarbage('collect') end
        root:linear(.25):x(5); root[1]:linear(.5):x(10)
        root:playcommand('Go'):RunCommandsOnChildren(function(self) self:playcommand('Go') end)
        assert(#events==0 and root:GetTweenTimeLeft()==.5 and w:GetTweenTimeLeft()==2)
        root:hurrytweening(2):finishtweening()
        assert(root:GetTweenTimeLeft()==0 and w:GetTweenTimeLeft()==2)
        MESSAGEMAN:Broadcast('Ping')
        observe()
    "#);
    for delta in [0.25, 1.75, 0.125, 0.25] {
        pair.advance(delta);
    }
}

#[test]
fn callback_wrapper_creation_invalidates_next_plan_and_keeps_gc_roots() {
    let mut pair = Pair::new("return Def.ActorFrame{}");
    pair.script(
        r#"
        root:SetUpdateFunction(function(self,dt)
            if not self.added then
                self.added=true
                local w=self:AddWrapperState():name('late'):linear(1):x(8)
                w:SetUpdateFunction(function(s,delta)
                    log('wrapper',delta,s:GetX()); collectgarbage('collect')
                    if not s.added then
                        s.added=true
                        s:AddWrapperState():name('nested'):linear(1):x(16)
                    end
                end)
                collectgarbage('collect')
            end
            log('root',dt,self:GetWrapperState(1):GetX())
        end)
    "#,
    );
    for delta in [0.25, 0.25, 0.25, 0.25] {
        pair.advance(delta);
    }
}

#[test]
fn wrapper_tag_reads_reenter_and_fail_without_losing_owned_actors() {
    let mut pair = Pair::new("return Def.Actor{}");
    pair.script(
        r#"
        root.__songlua_player1_enabled=true; root.__songlua_player2_enabled=false
        root.__songlua_player_index='2'; root.__songlua_compile_layer=3
        root.__songlua_main_title='title'; root.__songlua_style_name='dance-single'
        w=root:AddWrapperState()
        assert(w.__songlua_player1_enabled==true and w.__songlua_player2_enabled==false)
        assert(w.__songlua_player_index==2 and w.__songlua_compile_layer==3)
        assert(w.__songlua_main_title=='title' and w.__songlua_style_name=='dance-single')
        for _,value in ipairs{{},1/0,-1/0,0/0,1e30,-1e30} do
            root.__songlua_compile_layer=value
            assert(not pcall(root.AddWrapperState,root))
            assert(root:GetNumWrapperStates()==1)
        end
        root.__songlua_compile_layer=nil; root.__songlua_player_index=nil
        root.__songlua_player1_enabled=0; root.__songlua_player2_enabled='false'
        local mt=getmetatable(root); local lookup=mt.__index; local entered=false
        mt.__index=function(self,key)
            if key=='__songlua_player_index' then
                collectgarbage('collect')
                if not entered then entered=true; self:AddWrapperState():name('inner') end
                return 1
            end
            return lookup(self,key)
        end
        root:AddWrapperState():name('outer')
        mt.__index=lookup
        assert(root:GetNumWrapperStates()==3)
        assert(root:GetWrapperState(2):GetName()=='inner')
        assert(root:GetWrapperState(3):GetName()=='outer')
        assert(root:GetWrapperState(3).__songlua_player1_enabled==true)
        assert(root:GetWrapperState(3).__songlua_player2_enabled==true)
        observe()
    "#,
    );
    pair.advance(0.25);
}

#[test]
fn wrapper_directories_are_inherited_from_current_scope_across_gc() {
    let mut pair = Pair::new("return Def.Actor{}");
    pair.script(r#"
        local old_mt=getmetatable(_G)
        __songlua_script_dir=nil; __songlua_song_dir=nil
        setmetatable(_G,{__index=function(_,key)
            if key=='__songlua_script_dir' then collectgarbage('collect'); return '/fixture/current/' end
            if key=='__songlua_song_dir' then collectgarbage('collect'); return '/fixture/song/' end
        end})
        w=root:AddWrapperState()
        setmetatable(_G,old_mt)
        assert(w.__songlua_script_dir=='/fixture/current/')
        assert(w.__songlua_song_dir=='/fixture/song/')
        w.LocalCommand=function(self)
            log(_DIR,__songlua_script_dir,self.__songlua_song_dir)
            collectgarbage('collect')
        end
        _DIR='outside'; __songlua_script_dir='outside'
        w:playcommand('Local')
        assert(_DIR=='outside' and __songlua_script_dir=='outside')
        observe()
    "#);
    pair.advance(0.25);
}

#[test]
fn releasing_scene_retires_nested_wrappers_and_their_callbacks() {
    let mut pair = Pair::new("return Def.ActorFrame{}");
    pair.script("w=root:AddWrapperState(); nested=w:AddWrapperState(); nested.PingMessageCommand=function(self) error('retired wrapper ran') end");
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    execute(
        &mut pair.rust,
        "collectgarbage('collect'); assert(not pcall(w.GetX,w)); assert(not pcall(nested.AddWrapperState,nested)); MESSAGEMAN:Broadcast('Ping')",
    );
    assert!(pair.rust.actor_scene_snapshot(pair.scene).is_none());
    assert!(!pair.rust.advance_actor_scene(pair.scene, 0.25).unwrap());
}

#[test]
fn failed_directory_and_tag_reads_preserve_partial_registration() {
    let mut pair = Pair::new("return Def.Actor{}");
    pair.script(
        r#"
        for _,value in ipairs{false,{},string.char(255)} do
            __songlua_script_dir=value
            assert(not pcall(root.AddWrapperState,root))
        end
        __songlua_script_dir=nil
        for _,value in ipairs{false,{},string.char(255)} do
            __songlua_song_dir=value
            assert(not pcall(root.AddWrapperState,root))
        end
        __songlua_song_dir=nil
        for _,value in ipairs{false,{},string.char(255)} do
            root.__songlua_main_title=value
            assert(not pcall(root.AddWrapperState,root))
        end
        root.__songlua_main_title=nil
        assert(root:GetNumWrapperStates()==0)
        collectgarbage('collect')
    "#,
    );
    let actual = pair
        .rust
        .actor_scene_snapshot(pair.scene)
        .unwrap()
        .into_iter()
        .filter(|actor| actor.class == "WrapperState" && actor.parent.is_none())
        .count();
    assert_eq!(actual, 3, "only tag reads occur after registration");
    pair.script("root:AddWrapperState():x(7); observe()");
    pair.advance(0.25);
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
