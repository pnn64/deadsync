use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime},
};

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/actors.lua").unwrap())
        .unwrap()
        .to_vec()
}

struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
}
impl Pair {
    fn new(source: &str) -> Self {
        Self::with_init(source, true, false)
    }
    fn constructed(source: &str) -> Self {
        Self::with_init(source, false, true)
    }
    fn with_init(source: &str, initialize: bool, live: bool) -> Self {
        let mut context =
            SongLuaCompileContext::new(std::path::Path::new("."), "preparation startup");
        // Live Def decorates declarations; its materializer owns new instances.
        // Offline Def constructs instances immediately. Keep the existing raw
        // preparation controls for capture and use live mode for this boundary.
        context.live = live;
        Self::with_context(source, initialize, context)
    }
    fn with_context(source: &str, initialize: bool, context: SongLuaCompileContext) -> Self {
        if context.song_dir.is_absolute() {
            reference_test::register_path(&context.song_dir);
        }
        let live = context.live;

        let mut rust = SongRuntime::new(&context).unwrap();
        if !live {
            assert!(rust.begin_actor_preparation().unwrap());
        }
        let declaration = execute(&mut rust, source)[0];
        let scene = if initialize {
            rust.instantiate_actor(declaration)
        } else {
            rust.construct_actor(declaration)
        }
        .unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        Self { rust, scene }
    }
    fn initialize(&mut self) {
        assert!(self.rust.initialize_actor_scene(self.scene).unwrap());
        self.compare_trace();
    }
    fn compare_trace(&mut self) -> String {
        let source = "return table.concat(events, '|')";
        let Value::String(actual) = execute(&mut self.rust, source)[0] else {
            panic!()
        };
        let actual = std::str::from_utf8(self.rust.vm_mut().string_bytes(actual).unwrap()).unwrap();
        reference_test::assert_debug("song/tests/rust_preparation.rs::trace", &actual.as_bytes());
        actual.to_owned()
    }
    fn script(&mut self, source: &str) {
        execute(&mut self.rust, source);
        self.compare_trace();
    }
    fn capture(&mut self, name: &str) {
        let actor = execute(&mut self.rust, &format!("return {name}"))[0];
        let actual = self.rust.actor_command_capture(actor).unwrap();
        reference_test::assert_debug("song/tests/rust_preparation.rs::capture", &(name, &actual));
    }
    fn startup(&mut self) {
        // The compiler's synthetic root has no Begin command. Its normal screen
        // activation/broadcast phase is outside this queue-boundary comparison.
        assert!(self.rust.start_actor_scene(self.scene).unwrap());
        assert!(self.rust.advance_actor_scene(self.scene, 0.).unwrap());
        self.rust.discover_actor_startup_commands().unwrap();
        self.compare_trace();
    }
}

const LOG: &str = r#"
events={};function note(s)events[#events+1]=s end
"#;

#[test]
fn init_on_and_zero_update_finish_before_startup_queue_discovery() {
    let mut pair = Pair::new(&format!(
        r#"{LOG}
        local function layer(name)
            return Def.ActorFrame{{Name=name,
                InitCommand=function(self)
                    note(name..':Init');self:queuecommand('Queued')
                    self:playcommand('Nested');note(name..':InitEnd')
                end,
                NestedCommand=function(self)self:queuecommand('NestedQueued')end,
                BeginCommand=function(self)note(name..':Begin')end,
                OnCommand=function(self)
                    note(name..':On');self:SetUpdateFunction(function(a,dt)
                        assert(dt==0);note(name..':Update');a:SetUpdateFunction(nil)
                    end)
                end,
                QueuedCommand=function(self)
                    note(name..':Queued');assert(root[2].ready)
                    collectgarbage('collect');self:x(20)
                end,
                NestedQueuedCommand=function(self)note(name..':NestedQueued')end,
            }}
        end
        local a,b=layer('a'),layer('b')
        b.OnCommand=function(self)note('b:On');self.ready=true end
        return Def.ActorFrame{{a,b,InitCommand=function(self)note('root:Init')end,
            OnCommand=function(self)note('root:On')end}}
    "#
    ));
    assert_eq!(
        pair.compare_trace(),
        "a:Init|a:InitEnd|b:Init|b:InitEnd|root:Init"
    );
    for actor in ["root", "root[1]", "root[2]"] {
        pair.capture(actor);
    }
    pair.startup();
    assert_eq!(
        pair.compare_trace(),
        "a:Init|a:InitEnd|b:Init|b:InitEnd|root:Init|a:Begin|b:Begin|root:On|a:On|b:On|a:Update|a:Queued|a:NestedQueued|b:Queued|b:NestedQueued"
    );
    pair.rust.discover_actor_startup_commands().unwrap();
    pair.compare_trace();
}

#[test]
fn authored_init_exports_explicit_blocks_steps_and_pending_commands() {
    let mut pair = Pair::new(&format!(
        r#"{LOG}
        return Def.ActorFrame{{
            Def.Quad{{InitCommand=function(self)
                self:xy(4,8):aux(2):diffusealpha(.3):setsize(30,50)
                self:linear(.5):x(20):diffusealpha(.7):queuecommand('Later')
                self:sleep(.25):y(40):queuecommand('Again')
                self:visible(false)
            end}},
            Def.Actor{{InitCommand=function(self)
                self:x(3):linear(.2):x(10):queuecommand('Cancelled')
                self:finishtweening():linear(.4):y(12):queuecommand('Kept')
            end}},
        }}
    "#
    ));
    for actor in ["root", "root[1]", "root[2]"] {
        pair.capture(actor);
    }
    pair.script("assert(#events==0);assert(root[1]:GetX()==4);assert(root[1]:GetDestX()==20)");
}

#[test]
fn delayed_commands_and_zero_time_messages_wait_for_chronological_replay() {
    let mut pair = Pair::new(&format!(
        r#"{LOG}
        return Def.ActorFrame{{
            Def.Actor{{InitCommand=function(self)self:queuecommand('First'):sleep(.25):queuecommand('Future')end,
                FirstCommand=function(self)note('first')end,
                FutureCommand=function(self)note('future')end}},
            Def.Actor{{InitCommand=function(self)self:queuemessage('Mark'):queuecommand('AfterMessage')end,
                MarkMessageCommand=function(self)note('message')end,
                AfterMessageCommand=function(self)note('after-message')end}},
        }}
    "#
    ));
    pair.capture("root[1]");
    pair.capture("root[2]");
    pair.startup();
    assert_eq!(pair.compare_trace(), "first");
    pair.script(
        "root[1]:queuecommand('Future');root[2]:queuecommand('AfterMessage');assert(#events==1)",
    );
    for name in ["root[1]", "root[2]"] {
        let actor = execute(&mut pair.rust, &format!("return {name}"))[0];
        let actual = pair.rust.actor_command_capture(actor).unwrap();
        reference_test::assert_debug(
            "song/tests/rust_preparation.rs::pending_capture",
            &(name, &actual.steps, &actual.queued_commands),
        );
    }
}

#[test]
fn discovery_errors_restore_command_guards_and_discovery_scope() {
    let mut pair = Pair::new(&format!(
        r#"{LOG}
        return Def.ActorFrame{{Def.Actor{{
            InitCommand=function(self)self:queuecommand('Bad'):queuecommand('Good')end,
            BadCommand=function(self)note('bad');collectgarbage('collect');error('discovery failure')end,
            GoodCommand=function(self)note('good')end,
        }}}}
    "#
    ));
    pair.rust.start_actor_scene(pair.scene).unwrap();
    pair.rust.advance_actor_scene(pair.scene, 0.).unwrap();
    let error = pair.rust.discover_actor_startup_commands().unwrap_err();
    assert!(error.to_string().contains("discovery failure"), "{error}");
    assert_eq!(pair.compare_trace(), "bad");
    pair.script("root[1].BadCommand=function(self)note('recovered')end;root[1]:playcommand('Bad')");
    assert_eq!(pair.compare_trace(), "bad|recovered|good");
}

#[test]
fn stop_and_finish_clear_the_deferred_discovery_marker() {
    for method in ["stoptweening", "finishtweening"] {
        let mut pair = Pair::new(&format!(
            r#"{LOG}
            return Def.ActorFrame{{Def.Actor{{
                InitCommand=function(self)self:sleep(.25):queuecommand('Cancelled')end,
                CancelledCommand=function(self)note('cancelled')end,
                FreshCommand=function(self)note('fresh')end,
            }}}}
        "#
        ));
        pair.startup();
        pair.script(&format!(
            "root[1]:{method}():sleep(.5):queuecommand('Fresh')"
        ));
        assert_eq!(pair.compare_trace(), "fresh");
    }
}

#[test]
fn scene_release_removes_startup_owners_before_collection() {
    let context = SongLuaCompileContext::new(std::path::Path::new("."), "startup lifetime");
    let mut rust = SongRuntime::new(&context).unwrap();
    assert!(rust.begin_actor_preparation().unwrap());
    assert!(!rust.begin_actor_preparation().unwrap());
    let bad = execute(
        &mut rust,
        "return Def.Actor{InitCommand=function(self)self:queuecommand('Bad')end,BadCommand=function()error('released actor ran')end}",
    )[0];
    let retired = rust.instantiate_actor(bad).unwrap();
    let good = execute(
        &mut rust,
        "ran=0;return Def.Actor{InitCommand=function(self)self:queuecommand('Good')end,GoodCommand=function()ran=ran+1 end}",
    )[0];
    let scene = rust.instantiate_actor(good).unwrap();
    assert!(rust.release_actor_scene(retired).unwrap());
    execute(&mut rust, "collectgarbage('collect')");
    rust.discover_actor_startup_commands().unwrap();
    execute(&mut rust, "assert(ran==1)");
    assert!(rust.release_actor_scene(scene).unwrap());
    rust.discover_actor_startup_commands().unwrap();
}

#[test]
fn construction_retains_initial_state_and_defers_init_links_and_subscriptions() {
    let mut pair = Pair::constructed(&format!(
        r#"{LOG}
        declaration=Def.ActorFrame{{Name='root',Condition=false,
            Def.Quad{{Name='child',
                InitCommand=function(self)
                    note('child:'..self:GetParent():GetName())
                    self:GetParent():name('loading');self:x(12):diffusealpha(.4)
                end,
                MarkMessageCommand=function(self)note('child-message')end}},
            Def.Actor{{Condition=false,InitCommand=function()error('excluded child')end}},
            InitCommand=function(self)note('root:'..self:GetName());self:y(15)end,
            MarkMessageCommand=function(self)note('root-message')end,
        }}
        return declaration
    "#
    ));
    let identities = reference_test::source_indices(
        "song/tests/rust_preparation.rs::message_ranks",
        b"root,child",
    );
    let mut sorted = identities.clone();
    sorted.sort();
    assert_eq!(sorted, vec![1, 2]);
    assert!(pair.rust.set_actor_message_ranks(pair.scene, &identities));
    let expected_messages = if identities[0] < identities[1] {
        "child:|root:renamed|root-message|child-message"
    } else {
        "child:|root:renamed|child-message|root-message"
    };
    pair.script(
        r#"
        assert(root~=declaration and root[1]~=declaration[1], 'distinct instances')
        assert(root:GetName()=='root', 'root name: '..root:GetName())
        assert(root[1]:GetName()=='child', 'child name: '..root[1]:GetName())
        assert(root:GetChild('child')==root[1], 'named child lookup')
        assert(root:GetParent()==nil, 'root parent')
        assert(root[1]:GetParent()==nil, 'child parent')
        assert(root:GetNumChildren()==1, 'child count')
        MESSAGEMAN:Broadcast('Mark')
        assert(#events==0, 'early subscribers: '..table.concat(events,'|'))
        collectgarbage('collect')
    "#,
    );
    let initial = pair
        .rust
        .actor_scene_render_states(pair.scene, [0., 0.])
        .unwrap();
    reference_test::assert_debug(
        "song/tests/rust_preparation.rs::render",
        &("render", &(initial)),
    );
    assert_eq!(initial.len(), 2);
    assert_eq!(initial[0].y, 0.);
    assert_eq!(initial[1].x, 0.);
    assert_eq!(initial[1].diffuse, [1.; 4]);
    pair.script("root.Name='renamed';assert(root:GetName()=='renamed');declaration[1].InitCommand=function()error('declaration changed')end");
    pair.initialize();
    assert_eq!(pair.compare_trace(), "child:|root:renamed");
    pair.script("assert(root[1]:GetParent()==root);assert(root:GetName()=='renamed');assert(root[1]:GetX()==12);MESSAGEMAN:Broadcast('Mark')");
    assert_eq!(pair.compare_trace(), expected_messages);
    pair.initialize();
    assert_eq!(pair.compare_trace(), expected_messages);
    let after = pair
        .rust
        .actor_scene_render_states(pair.scene, [0., 0.])
        .unwrap();
    assert_eq!(after[0].y, 15.);
    assert_eq!(after[1].x, 12.);
    assert_eq!(initial[1].x, 0.);
    reference_test::assert_debug(
        "song/tests/rust_preparation.rs::render",
        &("render", &(after)),
    );
}

#[test]
fn foreground_parent_is_visible_during_init_before_child_list_attachment() {
    let mut pair = Pair::constructed(&format!(
        r#"{LOG}
        return Def.ActorFrame{{Name='layer',
            Def.Actor{{InitCommand=function(self)
                note('child:'..self:GetParent():GetParent():GetName())
            end}},
            InitCommand=function(self)
                assert(self:GetParent()==foreground)
                assert(foreground:GetNumChildren()==0)
                assert(foreground:GetChild('layer')==nil)
                note('layer:'..self:GetParent():GetName());collectgarbage('collect')
                wrapper=self:AddWrapperState();wrapper:x(8)
            end,
            MarkMessageCommand=function(self)note('retained')end,
        }}
    "#
    ));

    let declaration = execute(
        &mut pair.rust,
        "return Def.ActorFrame{Name='SongForeground'}",
    )[0];
    let parent_scene = pair.rust.instantiate_actor(declaration).unwrap();
    let parent_value = pair.rust.actor_scene_root(parent_scene).unwrap();
    // Populate the old traversal before attachment; its traced roots must be
    // invalidated along with the cached plan when the child list changes.
    pair.rust.advance_actor_scene(parent_scene, 0.).unwrap();
    pair.rust
        .vm_mut()
        .set_global(b"foreground", parent_value)
        .unwrap();
    assert!(
        pair.rust
            .set_actor_scene_parent(pair.scene, Some(parent_value))
            .unwrap()
    );
    // The external parent is intentionally absent from this scene's local indices.
    assert_eq!(
        pair.rust.actor_scene_snapshot(pair.scene).unwrap()[0].parent,
        None
    );
    pair.initialize();
    assert_eq!(
        pair.compare_trace(),
        "child:SongForeground|layer:SongForeground"
    );

    for _ in 0..2 {
        assert!(
            pair.rust
                .append_actor_scene(pair.scene, parent_value)
                .unwrap()
        );
    }
    pair.script("assert(foreground:GetNumChildren()==1);assert(foreground:GetChild('layer')==root);assert(root:GetWrapperState(1)==wrapper)");
    assert_eq!(
        pair.rust.actor_scene_snapshot(parent_scene).unwrap().len(),
        4
    );
    pair.script("late_wrapper=root:AddWrapperState():x(6)");
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    pair.script("collectgarbage('collect');assert(root:GetWrapperState(2)==late_wrapper);assert(foreground:GetChild('layer')==root);assert(root:GetWrapperState(1):GetX()==8);MESSAGEMAN:Broadcast('Mark')");
    assert_eq!(
        pair.compare_trace(),
        "child:SongForeground|layer:SongForeground|retained"
    );
    execute(
        &mut pair.rust,
        "weak=setmetatable({root,root[1],wrapper,foreground,late_wrapper},{__mode='v'});root=nil;wrapper=nil;foreground=nil;late_wrapper=nil",
    );
    assert!(pair.rust.release_actor_scene(parent_scene).unwrap());
    execute(
        &mut pair.rust,
        "collectgarbage('collect');collectgarbage('collect');for i=1,5 do assert(weak[i]==nil)end",
    );
}

#[test]
fn init_failure_keeps_partial_state_and_retry_skips_finished_children() {
    let mut pair = Pair::constructed(&format!(
        r#"{LOG}
        return Def.ActorFrame{{Name='root',
            Def.Actor{{InitCommand=function(self)note('child');self:x(4)end}},
            InitCommand=function(self)
                note('root');self:y(9);collectgarbage('collect')
                if not recovered then error('init failure')end
            end,
        }}
    "#
    ));

    let actual = pair.rust.initialize_actor_scene(pair.scene).unwrap_err();

    assert!(actual.to_string().contains("init failure"), "{actual}");
    assert_eq!(pair.compare_trace(), "child|root");
    pair.script("assert(root:GetY()==9 and root[1]:GetX()==4);recovered=true;root.Name='retry'");
    pair.initialize();
    assert_eq!(pair.compare_trace(), "child|root|root");
    pair.script("assert(root:GetName()=='retry')");
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    assert!(!pair.rust.initialize_actor_scene(pair.scene).unwrap());
}

#[test]
fn uninitialized_scenes_release_callbacks_and_validate_parent_ownership() {
    let context = SongLuaCompileContext::new(std::path::Path::new("."), "construction ownership");
    let mut rust = SongRuntime::new(&context).unwrap();
    let declaration = execute(
        &mut rust,
        "return Def.ActorFrame{Def.Actor{},InitCommand=function()error('must not run')end}",
    )[0];
    let scene = rust.construct_actor(declaration).unwrap();
    let root = rust.actor_scene_root(scene).unwrap();
    rust.vm_mut().set_global(b"root", root).unwrap();
    let child = execute(&mut rust, "return root[1]")[0];
    assert!(
        rust.set_actor_scene_parent(scene, Some(child))
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
    assert!(
        rust.set_actor_scene_parent(scene, Some(root))
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
    let mut other = SongRuntime::new(&context).unwrap();
    assert!(!other.initialize_actor_scene(scene).unwrap());
    assert!(!other.set_actor_scene_parent(scene, None).unwrap());
    let foreign = execute(&mut other, "return {}")[0];
    assert!(rust.set_actor_scene_parent(scene, Some(foreign)).is_err());
    assert!(rust.set_actor_scene_parent(scene, None).unwrap());
    execute(
        &mut rust,
        "weak=setmetatable({root,root[1],root.InitCommand},{__mode='v'});root=nil;collectgarbage('collect');assert(weak[1]~=nil)",
    );
    assert!(rust.release_actor_scene(scene).unwrap());
    execute(
        &mut rust,
        "collectgarbage('collect');collectgarbage('collect');for i=1,3 do assert(weak[i]==nil)end",
    );
}

#[test]
fn offline_constructor_identity_child_ownership_and_values_match_production() {
    let context = SongLuaCompileContext::new(std::path::Path::new("."), "offline constructors");
    let source = r#"
        events={};function note(s)events[#events+1]=s end
        local orphan=Def.Actor{Name='orphan'}
        weak=setmetatable({orphan},{__mode='v'});orphan=nil
        collectgarbage('collect');collectgarbage('collect')
        note('orphan:'..tostring(weak[1]~=nil))
        supplied={Name='child',Class='kept',InitCommand=function(self)note('child-init')end}
        child=Def.Quad(supplied)
        note('same:'..tostring(child==supplied))
        note('class:'..tostring(child.Class))
        note('methods:'..type(child.x)..':'..child:GetName())
        child:x(6)
        parent=Def.ActorFrame{child,child,InitCommand=function(self)note('parent-init')end}
        note('first:'..tostring(parent[1]==child))
        note('copy:'..tostring(parent[2]~=child))
        note('parent:'..tostring(child:GetParent()==nil))
        note('x:'..child:GetX()..':'..parent[2]:GetX())
        child:x(9)
        note('independent:'..child:GetX()..':'..parent[2]:GetX())
        return parent
    "#;

    let mut rust = SongRuntime::new(&context).unwrap();
    assert!(rust.begin_actor_preparation().unwrap());
    let actor = execute(&mut rust, source)[0];
    let scene = rust.construct_actor(actor).unwrap();
    assert_eq!(rust.actor_scene_root(scene), Some(actor));
    rust.vm_mut().set_global(b"root", actor).unwrap();
    let mut pair = Pair { rust, scene };
    assert_eq!(
        pair.compare_trace(),
        "orphan:true|same:true|class:kept|methods:function:child|first:true|copy:true|parent:true|x:6:6|independent:9:6"
    );
    pair.capture("root[1]");
    pair.capture("root[2]");
    pair.initialize();
    pair.script("assert(root==parent and root[1]==supplied);assert(root[2]~=supplied);assert(root[1]:GetParent()==root and root[2]:GetParent()==root)");
    pair.capture("root[1]");
    pair.capture("root[2]");
    execute(
        &mut pair.rust,
        "owned_weak=setmetatable({root,root[1],root[2]},{__mode='v'});root=nil;parent=nil;child=nil;supplied=nil",
    );
    assert!(pair.rust.release_actor_scene(scene).unwrap());
    execute(
        &mut pair.rust,
        "collectgarbage('collect');collectgarbage('collect');for i=1,3 do assert(owned_weak[i]==nil)end;assert(weak[1]~=nil)",
    );
}

#[test]
fn offline_clone_preserves_tween_capture_and_copies_mutable_state() {
    let mut pair = Pair::with_init(
        &format!(
            r#"{LOG}
        original=Def.Quad{{Name='piece',Frames={{{{Frame=0,Delay=.2}}}},
            Payload={{value=1}},InitCommand=function(self)note('init:'..self:GetDestX())end}}
        original:x(6):linear(.5):x(20):queuecommand('Later')
        local parent=Def.ActorFrame{{original,original}}
        copy=parent[2]
        assert(copy~=original and copy.Payload==original.Payload)
        assert(copy.Frames~=original.Frames and copy.Frames[1]~=original.Frames[1])
        assert(copy.InitCommand==original.InitCommand)
        note('pose:'..original:GetX()..':'..copy:GetX()..':'..copy:GetDestX())
        original:stoptweening():x(9)
        note('changed:'..original:GetX()..':'..copy:GetX()..':'..copy:GetDestX())
        return parent
    "#
        ),
        false,
        false,
    );
    pair.compare_trace();
    pair.capture("original");
    pair.capture("copy");
    pair.initialize();
    pair.capture("copy");
}

#[test]
fn offline_concat_shares_children_and_composes_commands_before_init() {
    let mut pair = Pair::with_init(
        &format!(
            r#"{LOG}
        child=Def.Actor{{Name='child',InitCommand=function(self)note('child')end}}
        first=Def.ActorFrame{{child,Name='first',InitCommand=function(self)note('first:'..self:GetName())end}}
        second=first..{{Name='second',InitCommand=function(self)note('second:'..self:GetName())end}}
        assert(first~=second and first[1]==child and second[1]==child)
        assert(child:GetParent()==nil and second:GetParent()==nil)
        assert(#events==0)
        return Def.ActorFrame{{first,second}}
    "#
        ),
        false,
        false,
    );
    pair.compare_trace();
    pair.script("assert(root[1]==first and root[2]==second and first[1]==second[1])");
    pair.initialize();
    pair.script("assert(first[1]==second[1]);note('parent:'..tostring(child:GetParent()==second))");
}

#[test]
fn offline_resource_clones_keep_stream_aliases_and_scroller_state() {
    let mut pair = Pair::with_init(
        &format!(
            r#"{LOG}
        stream=Def.Quad{{InitCommand=function(self)note('stream')end}}
        meter=Def.SongMeterDisplay{{Stream=stream,StreamWidth=72}}
        assert(meter.Stream==stream and stream:GetParent()==meter)
        meter:SetStreamWidth(91)
        meter_copy=meter..{{}}
        assert(meter_copy.Stream==stream and meter_copy:GetChild('Stream')==stream)
        assert(meter_copy:GetStreamWidth()==91)
        graph=Def.GraphDisplay{{}}
        graph_copy=graph..{{}}
        assert(graph_copy:GetChild('Line')==graph:GetChild('Line'))
        display=Def.Actor{{Name='Display',InitCommand=function(self)note('display')end}}
        extra=Def.Actor{{Name='extra',InitCommand=function(self)note('extra')end}}
        course=Def.CourseContentsList{{extra,Display=display}}
        assert(course[1]==extra and course[2]==display and #course==2)
        course:SetCurrentItem(3):SetDestinationItem(5)
        course_copy=course..{{}}
        assert(course_copy[1]==extra and course_copy[2]==display)
        assert(course_copy:GetCurrentItem()==3 and course_copy:GetDestinationItem()==5)
        input=Def.InputList{{}};input:settext('custom')
        input_copy=input..{{}};assert(input_copy:GetText()=='custom')
        return Def.ActorFrame{{meter,meter_copy,graph,graph_copy,course,course_copy,input,input_copy}}
    "#
        ),
        false,
        false,
    );
    pair.compare_trace();
    pair.initialize();
    pair.script(
        "assert(meter_copy:GetChild('Stream')==stream);assert(course_copy:GetCurrentItem()==3)",
    );
    execute(
        &mut pair.rust,
        "resource_weak=setmetatable({root,stream,display,extra,graph:GetChild('Line')},{__mode='v'});root=nil;stream=nil;display=nil;extra=nil;meter=nil;meter_copy=nil;graph=nil;graph_copy=nil;course=nil;course_copy=nil;input=nil;input_copy=nil",
    );
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    execute(
        &mut pair.rust,
        "collectgarbage('collect');collectgarbage('collect');for i=1,5 do assert(resource_weak[i]==nil,'resource retained '..i)end",
    );
}

#[test]
fn offline_constructor_rebinds_methods_and_defers_child_conditions() {
    let mut pair = Pair::with_init(
        &format!(
            r#"{LOG}
        saved_ctor=Def.Quad
        local supplied={{x=17,GetX=function()return -999 end,Name='supplied'}}
        quad=saved_ctor(supplied)
        assert(quad==supplied and type(quad.x)=='function')
        quad:x(4);assert(quad:GetX()==4)
        assert(type(saved_ctor(false))=='table' and type(saved_ctor(1))=='table')
        rejected=Def.Actor{{Condition=false,InitCommand=function()error('excluded child initialized')end}}
        assert(type(rejected.x)=='function')
        first=Def.Actor{{Name='first',InitCommand=function()note('first')end}}
        last=Def.Actor{{Name='last',InitCommand=function()note('last')end}}
        selected={{first,rejected,third=last}}
        parent=Def.ActorFrame{{quad,children=selected}}
        assert(parent[1]==quad and selected[1]==first and selected[2]==rejected)
        return parent
    "#
        ),
        false,
        false,
    );
    pair.compare_trace();
    pair.initialize();
    pair.script("assert(root[1]==first and root[2]==last and root[3]==nil);assert(rejected:GetParent()==nil)");
}

#[test]
fn offline_reused_root_clones_children_and_releases_independent_scene_owners() {
    let mut pair = Pair::with_init(
        &format!(
            r#"{LOG}
        local frame={{Frame=0,Delay=.2}}
        return Def.ActorFrame{{Name='root',InitCommand=function()note('root-init')end,
            Def.Quad{{Name='piece',Frames={{frame,frame}},InitCommand=function(self)
                note('child-init');self:x(8):linear(.25):x(12)
            end}}
        }}
    "#
        ),
        false,
        false,
    );
    pair.initialize();
    let original = pair.scene;
    let original_root = pair.rust.actor_scene_root(original).unwrap();

    pair.rust
        .vm_mut()
        .set_global(b"old_root", original_root)
        .unwrap();
    pair.scene = pair.rust.construct_actor(original_root).unwrap();
    let clone = pair.rust.actor_scene_root(pair.scene).unwrap();
    assert_ne!(clone, original_root);
    pair.rust.vm_mut().set_global(b"root", clone).unwrap();
    pair.script(
        r#"
        assert(root~=old_root and root[1]~=old_root[1])
        assert(root[1]:GetParent()==nil and root:GetParent()==nil)
        assert(rawget(root,'__songlua_init_commands_ran')==nil)
        assert(root[1].Frames~=old_root[1].Frames)
        assert(old_root[1].Frames[1]==old_root[1].Frames[2])
        assert(root[1].Frames[1]~=root[1].Frames[2])
        assert(root[1].InitCommand==old_root[1].InitCommand)
    "#,
    );
    pair.capture("root[1]");
    pair.capture("old_root[1]");
    pair.initialize();
    pair.capture("root[1]");
    pair.script("assert(root[1]:GetParent()==root and old_root[1]:GetParent()==old_root)");
    execute(
        &mut pair.rust,
        "weak=setmetatable({old_root,old_root[1],root,root[1]},{__mode='v'});old_root=nil;root=nil",
    );
    assert!(pair.rust.release_actor_scene(original).unwrap());
    execute(
        &mut pair.rust,
        "collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil and weak[2]==nil);assert(weak[3]~=nil and weak[4]~=nil)",
    );
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    execute(
        &mut pair.rust,
        "collectgarbage('collect');collectgarbage('collect');assert(weak[3]==nil and weak[4]==nil)",
    );
}

#[test]
fn preparation_loader_virtual_media_and_invalid_arguments_match_production() {
    let mut pair = Pair::with_init(
        &format!(
            r#"{LOG}
        sprite=LoadActor('__songlua_theme_path/Graphics/missing 2x1.png')
        movie=LoadActor('/__songlua_theme_path/Graphics/missing.mp4')
        sound=LoadActor('__songlua_theme_path/Sounds/missing.ogg')
        ordinary=LoadActor('__songlua_theme_path/Graphics/not-a-script.lua')
        invalid={{LoadActor(),LoadActor(17),LoadActor(false),LoadActor({{}})}}
        for _,a in ipairs(invalid) do assert(a.__songlua_actor_type=='LoadActor');assert(type(a.x)=='function')end
        assert(sprite.Class==nil and sprite._Dir==nil)
        note('kinds:'..sprite.__songlua_actor_type..':'..sound.__songlua_actor_type..':'..ordinary.__songlua_actor_type)
        note('paths:'..sprite.Texture..':'..movie.Texture..':'..sound.File)
        note('decode:'..tostring(rawget(movie,'__songlua_state_decode_movie')))
        note('metadata:'..tostring(rawget(movie,'__songlua_movie_path')))
        sprite.ProbeMessageCommand=function()note('early-sprite')end
        MESSAGEMAN:Broadcast('Probe')
        return Def.ActorFrame{{sprite,movie,sound,ordinary,invalid[1]}}
    "#
        ),
        false,
        false,
    );
    pair.compare_trace();
    reference_test::assert_debug(
        "song/tests/rust_preparation.rs::render",
        &(
            "render",
            &(pair
                .rust
                .actor_scene_render_states(pair.scene, [0., 0.])
                .unwrap()),
        ),
    );
    pair.initialize();
    pair.script("assert(root[1]==sprite and root[2]==movie)");
}

#[test]
fn preparation_loader_files_preserve_args_first_result_context_and_media_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let nested = directory.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    image::RgbaImage::from_pixel(12, 8, image::Rgba([255, 255, 255, 255]))
        .save(nested.join("image 2x1.png"))
        .unwrap();
    std::fs::write(
        nested.join("child.lua"),
        r#"
        local a,b=...;note('args:'..a..':'..b)
        return Def.Actor{Name='child',InitCommand=function()note('child-init')end}, 'discarded'
    "#,
    )
    .unwrap();
    std::fs::write(
        nested.join("main.lua"),
        r#"
        local before=__songlua_script_dir
        local child=LoadActor('child.lua',...)
        assert(before==__songlua_script_dir)
        local image=LoadActor('image')
        assert(image:GetWidth()==6 and image:GetHeight()==8)
        assert(image.__songlua_script_dir==before)
        note('media:'..image.__songlua_actor_type..':'..tostring(image.Class))
        return Def.ActorFrame{child,image}, nil, 'discarded'
    "#,
    )
    .unwrap();
    let context = SongLuaCompileContext::new(directory.path(), "preparation file loader");
    let mut pair = Pair::with_context(
        &format!(
            r#"{LOG}
        local old=__songlua_script_dir
        local function keep(...)note('results:'..select('#',...));return (...)end
        loaded=keep(LoadActor({:?},7,'forwarded'))
        assert(__songlua_script_dir==old)
        return loaded
    "#,
            deadsync_song_lua::file_path_string(&nested.join("main.lua"))
        ),
        false,
        context,
    );
    pair.compare_trace();
    reference_test::assert_debug(
        "song/tests/rust_preparation.rs::render",
        &(
            "render",
            &(pair
                .rust
                .actor_scene_render_states(pair.scene, [0., 0.])
                .unwrap()),
        ),
    );
    pair.initialize();
    pair.script("assert(root==loaded);note('width:'..root[2]:GetWidth())");
}

#[test]
fn preparation_loader_font_templates_null_actor_and_gc_ownership_match_production() {
    let mut pair = Pair::with_init(
        &format!(
            r#"{LOG}
        font=LoadFont('Common Normal')
        font:x(4):linear(.5):x(9)
        font.ProbeMessageCommand=function()note('font-subscriber')end
        MESSAGEMAN:Broadcast('Probe')
        copy=font..{{Name='copy'}}
        local frame=Def.ActorFrame{{font,copy,NullActor,NullActor}}
        assert(frame[1]==font and frame[2]==copy)
        assert(frame[3]~=NullActor and frame[4]~=NullActor and frame[3]~=frame[4])
        assert(font.Font=='Common Normal' and font.Class==nil)
        return frame
    "#
        ),
        false,
        false,
    );
    pair.compare_trace();
    pair.capture("font");
    pair.capture("copy");
    pair.initialize();
    pair.capture("font");
    pair.capture("copy");
    execute(
        &mut pair.rust,
        "weak=setmetatable({root,font,copy},{__mode='v'});root=nil;font=nil;copy=nil",
    );
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    execute(
        &mut pair.rust,
        "collectgarbage('collect');collectgarbage('collect');for i=1,3 do assert(weak[i]==nil,'retained font '..i)end",
    );
}

#[test]
fn preparation_loader_saved_callbacks_resolve_their_source_and_restore_failed_scopes() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path();
    for name in ["helper", "xml_dir", "priority", "empty_dir"] {
        std::fs::create_dir(dir.join(name)).unwrap();
    }
    for (name, source) in [
        ("child.lua", "error('wrong caller directory')"),
        (
            "helper/child.lua",
            r#"local token=...;assert(marker=='global');collectgarbage('collect');return Def.Actor{Name='child',Token=token,Origin=__songlua_script_dir},'extra'"#,
        ),
        (
            "helper/fail.lua",
            "collectgarbage('collect');error('file rejection')",
        ),
        (
            "helper/factory.lua",
            r#"local load=LoadActor;return function(fail)
            if fail=='tail' then return load('child.lua',{key='tail'})end
            local before=__songlua_script_dir
            local actor=load(fail and 'fail.lua' or 'child.lua',{key='kept'})
            assert(__songlua_script_dir==before);return actor
        end"#,
        ),
        ("alias.REDIR", "  helper/child.lua  \n"),
        ("empty.redir", "   \n"),
        ("loop.redir", "loop.redir"),
        ("multi1.lua", "return 1"),
        ("multi2.lua", "return 2"),
        ("xml_dir/default.xml", "return 'xml-as-lua'"),
        ("priority/default.lua", "return 'lua-first'"),
        ("priority/default.xml", "error('wrong default')"),
        ("opaque.txt", "return 'script-text'"),
    ] {
        std::fs::write(dir.join(name), source).unwrap();
    }
    let context = SongLuaCompileContext::new(dir, "saved preparation loader");
    let mut pair = Pair::with_context(
        &format!(
            r#"{LOG}
        local dir={:?};local helper_dir={:?}
        marker='global'
        saved=LoadActor(dir..'/helper/factory.lua')
        setfenv(saved,setmetatable({{marker='private'}},{{__index=_G}}))
        __songlua_script_dir='preserved-scope'
        local path,env=__songlua_current_script_path,__songlua_current_chunk_env
        local first=saved(false)
        assert(first.Token.key=='kept' and first.Origin==helper_dir, 'actual origin='..tostring(first.Origin)..'; expected='..helper_dir)
        local ok,err=pcall(saved,true)
        assert(not ok and tostring(err):find('file rejection',1,true))
        assert(__songlua_script_dir=='preserved-scope' and __songlua_current_script_path==path and __songlua_current_chunk_env==env)
        local second=saved(false)
        local tail=saved('tail');assert(tail.Token.key=='tail' and tail.Origin==helper_dir)
        local alias=LoadActor(dir..'/alias.REDIR',{{key='redirect'}})
        assert(alias.Token.key=='redirect')
        assert(LoadActor(dir..'/xml_dir')=='xml-as-lua')
        assert(LoadActor(dir..'/priority')=='lua-first')
        assert(LoadActor(dir..'/opaque.txt')=='script-text')
        for _,case in ipairs{{{{'/empty.redir','empty redirect'}},{{'/loop.redir','circular redirect'}},{{'/multi','multiple matches'}},{{'/empty_dir','not found'}},{{'/missing.lua','not found'}}}} do
            local ok,err=pcall(LoadActor,dir..case[1]);assert(not ok and tostring(err):find(case[2],1,true))
            assert(__songlua_script_dir=='preserved-scope')
        end
        note('callbacks-and-errors-restored')
        return Def.ActorFrame{{first,second,alias,tail}}
    "#,
            deadsync_song_lua::file_path_string(dir),
            dir.join("helper").to_string_lossy(),
        ),
        false,
        context,
    );
    pair.compare_trace();
    pair.initialize();
    pair.script("assert(root[1]~=root[2] and root[1].Token~=root[2].Token);assert(__songlua_script_dir=='preserved-scope')");
}

#[test]
fn preparation_loader_host_entry_roots_arguments_and_preserves_scope_after_limits() {
    let directory = tempfile::tempdir().unwrap();
    let entry = directory.path().join("entry.lua");
    let infinite = directory.path().join("infinite.lua");
    std::fs::write(&entry,"local value=...;collectgarbage('collect');assert(value.tag=='retained');return value,nil,'extra'").unwrap();
    std::fs::write(&infinite, "while true do end").unwrap();
    let context = SongLuaCompileContext::new(directory.path(), "host preparation loader");
    let mut pair = Pair::with_context(&format!("{LOG}return Def.ActorFrame{{}}"), false, context);
    let argument = execute(
        &mut pair.rust,
        "saved_loader=LoadActor;LoadActor=function()error('replaced global')end;return {tag='retained'}",
    )[0];
    let values = pair.rust.load_actor(&entry, &[argument]).unwrap().to_vec();
    assert_eq!(values, vec![argument]);
    pair.rust
        .vm_mut()
        .set_global(b"host_result", values[0])
        .unwrap();
    execute(
        &mut pair.rust,
        "weak=setmetatable({host_result},{__mode='v'});host_result=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil)",
    );
    execute(
        &mut pair.rust,
        "LoadActor=saved_loader;saved_loader=nil;__songlua_script_dir='before-limit';__songlua_current_script_path='saved-path';__songlua_current_chunk_env={saved=true}",
    );
    pair.rust.vm_mut().set_instruction_limit(Some(2000));
    let program = compile_named(
        format!(
            "return LoadActor({:?})",
            deadsync_song_lua::file_path_string(&infinite)
        )
        .as_bytes(),
        b"@/fixture/limit.lua",
    )
    .unwrap();
    let error = pair.rust.vm_mut().execute(&program).unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    pair.rust.vm_mut().set_instruction_limit(None);
    execute(
        &mut pair.rust,
        "assert(__songlua_script_dir=='before-limit' and __songlua_current_script_path=='saved-path' and __songlua_current_chunk_env.saved)",
    );
    // Saved host entry loading remains usable after the failed execution.
    let argument = execute(&mut pair.rust, "return {tag='retained'}")[0];
    assert_eq!(
        pair.rust.load_actor(&entry, &[argument]).unwrap(),
        [argument]
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
