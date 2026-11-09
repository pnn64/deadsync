use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime},
};

fn runtime() -> RustSongRuntime {
    RustSongRuntime::new(&SongLuaCompileContext::new(
        std::path::Path::new("."),
        "Rust actor instance tests",
    ))
    .unwrap()
}
fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/actors.lua").unwrap())
        .unwrap()
        .to_vec()
}
fn instantiate(runtime: &mut RustSongRuntime, source: &str) -> ActorScene {
    let definition = execute(runtime, source)[0];
    let scene = runtime.instantiate_actor(definition).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    scene
}
fn assert_clean(runtime: &RustSongRuntime) {
    assert_eq!(runtime.take_actor_diagnostics(), Vec::<String>::new());
}

#[test]
fn instances_are_distinct_and_child_first_init_observes_native_loading_order() {
    let mut r = runtime();
    let scene = instantiate(
        &mut r,
        r#"
        seen={}
        local function observe(self,params)
            assert(type(params)=='table' and next(params)==nil)
            local parent=self:GetParent()
            seen[#seen+1]=self:GetName()..':'..(parent and parent:GetName() or '-')..':'..(parent and parent:GetNumChildren() or self:GetNumChildren())
        end
        shared=Def.Quad{Name='same',InitCommand=function(self,p)
            observe(self,p); self:GetParent():name('temporary'); self:draworder(-1)
        end}
        definition=Def.ActorFrame{Name='root', InitCommand=observe,
            Def.Quad{Name='first', InitCommand=observe},shared,shared,
            Def.Actor{Condition=false,InitCommand=function()error('excluded')end}}
        return definition
    "#,
    );
    execute(
        &mut r,
        r#"
        assert(table.concat(seen,'|')=='first::0|same:temporary:1|same:temporary:2|root:-:3')
        assert(root~=definition and root.Class==nil and root.Name==nil and root.InitCommand==nil)
        assert(root:GetCommand('Init')==definition.InitCommand)
        local group=root:GetChild('same')
        assert(#group==2 and group[1]~=group[2] and group[1]~=shared)
        assert(group[1].ctx~=group[2].ctx and getmetatable(group[1].ctx)==group[1].ctx)
        root.ctx.answer={42}; assert(group[1].ctx.answer==root.ctx.answer)
        root.Name='field'; assert(root:GetName()=='root')
        assert(getmetatable(root)=='(hidden)' and not pcall(setmetatable,root,{}))
    "#,
    );
    let state = r.actor_scene_snapshot(scene).unwrap();
    assert_eq!(state.len(), 4);
    assert_eq!(state[0].children, vec![2, 3, 1]);
    assert_eq!(state[2].parent, Some(0));
    assert_clean(&r);
}

#[test]
fn full_declaration_snapshot_precedes_init_and_preserves_shared_callback_upvalues() {
    let mut r = runtime();
    instantiate(
        &mut r,
        r#"
        local count=0
        local next=Def.Quad{Name='before',InitCommand=function(self) count=count+1; self:aux(count) end}
        return Def.ActorFrame{Def.Quad{InitCommand=function(self)
            next.Name='after'; next.InitCommand=function()error('mutated declaration')end
            collectgarbage()
        end},next,next}
    "#,
    );
    execute(
        &mut r,
        "local g=root:GetChild('before');assert(#g==2 and g[1]:getaux()==1 and g[2]:getaux()==2 and root:GetChild('after')==nil)",
    );
    assert_clean(&r);
}

#[test]
fn commands_propagate_in_sorted_order_and_preserve_parameter_identity() {
    let mut r = runtime();
    instantiate(
        &mut r,
        r#"
        log={}; params={}
        local function command(self,p) assert(p==params); log[#log+1]=self:GetName() end
        return Def.ActorFrame{Name='root',ProbeCommand=command,
            Def.ActorFrame{Name='frame',ProbeCommand=command,Def.Quad{Name='leaf',ProbeCommand=command}},
            Def.Quad{Name='first',InitCommand=function(self)self:draworder(-1)end,ProbeCommand=command}}
    "#,
    );
    execute(
        &mut r,
        r#"
        assert(root:playcommand('Probe',params)==root)
        assert(table.concat(log,',')=='root,first,frame,leaf')
        log={}; root:RunCommandsRecursively(function(self,p)assert(p==params);log[#log+1]=self:GetName()end,params)
        assert(table.concat(log,',')=='first,leaf,frame,root')
        log={}; root:RunCommandsOnChildren(function(self,p)assert(p==params);log[#log+1]=self:GetName()end,params)
        assert(table.concat(log,',')=='first,frame')
        log={}; root:runcommandsonleaves(function(self,p)assert(p==nil);log[#log+1]=self:GetName()end)
        assert(table.concat(log,',')=='first,leaf')
        local frame=root:GetChild('frame');frame:propagate(true)
        log={};frame:playcommand('Probe',params)
        assert(table.concat(log,',')=='leaf,leaf')
    "#,
    );
    assert_clean(&r);
}

#[test]
fn named_children_commands_use_the_parents_handler_and_empty_frames_are_not_leaves() {
    let mut r = runtime();
    instantiate(
        &mut r,
        r#"
        log={}
        return Def.ActorFrame{CustomCommand=function(self,p) assert(p==nil);log[#log+1]=self:GetName()end,
            Def.ActorFrame{Name='empty'},Def.Quad{Name='leaf',CustomCommand=function()error('wrong handler')end}}
    "#,
    );
    execute(
        &mut r,
        r#"
        root:playcommandonchildren('Custom'); assert(table.concat(log,',')=='empty,leaf')
        log={}; root:playcommandonleaves('Custom'); assert(table.concat(log,',')=='leaf')
        root:GetChild('empty'):runcommandsonleaves(function()error('empty frame is not a leaf')end)
        assert(not pcall(ActorFrame.GetChild,root:GetChild('leaf'),'x'))
    "#,
    );
    assert_clean(&r);
}

#[test]
fn duplicate_child_groups_are_snapshots_with_class_lookup_and_open_results() {
    let mut r = runtime();
    instantiate(
        &mut r,
        "return Def.ActorFrame{Def.Quad{Name='same'},Def.Quad{Name='same'}}",
    );
    execute(
        &mut r,
        r#"
        local g=root:GetChild('same'); assert(#g==2 and g~=root:GetChild('same'))
        assert(g:x(17)==g[2] and g[1]:GetX()==0 and g[2]:GetX()==17)
        local getx=g.GetX; g[3]=g[1]; assert(getx(g)==0 and g['3']==nil)
        g[2].GetX=function()return 999 end
        assert(root:GetChild('same'):GetX()==17)
        Actor.multi=function(self,...)return self,... end
        local t={}; local a,b,c,d=root:GetChildren().same:multi(t,nil,42)
        assert(a==g[2] and b==t and c==nil and d==42)
        assert(select('#',root:GetChild('same'):multi(nil))==2)
        g[1]:name('changed');assert(root:GetChild('changed')==g[1] and root:GetChild('same')==g[2])
    "#,
    );
    assert_clean(&r);
}

#[test]
fn scene_ownership_survives_gc_and_release_invalidates_methods_without_leaking_cycles() {
    let mut r = runtime();
    let scene = instantiate(
        &mut r,
        r#"
        weak=setmetatable({},{__mode='v'})
        return Def.ActorFrame{InitCommand=function(self)weak[1]=self end,
            Def.Quad{InitCommand=function(self)weak[2]=self;local cycle={self};self:addcommand('Cycle',function()return cycle end)end}}
    "#,
    );
    execute(
        &mut r,
        "root=nil;collectgarbage();collectgarbage();assert(weak[1] and weak[2]);saved=weak[2]",
    );
    assert!(r.release_actor_scene(scene).unwrap());
    assert!(!r.release_actor_scene(scene).unwrap());
    assert!(r.actor_scene_root(scene).is_none());
    execute(
        &mut r,
        "assert(not pcall(saved.GetName,saved));saved=nil;collectgarbage();collectgarbage();assert(weak[1]==nil and weak[2]==nil)",
    );
    assert_clean(&r);
}

#[test]
fn failed_build_releases_partial_instances_and_later_scenes_remain_usable() {
    let mut r = runtime();
    let d = execute(
        &mut r,
        r#"
        weak=setmetatable({},{__mode='v'})
        return Def.ActorFrame{Def.Quad{InitCommand=function(self)weak[1]=self;weak[2]=self:GetParent()end},Def.Model{}}
    "#,
    )[0];
    assert!(
        r.instantiate_actor(d)
            .unwrap_err()
            .to_string()
            .contains("no Rust instance implementation")
    );
    execute(
        &mut r,
        "collectgarbage();collectgarbage();assert(weak[1]==nil and weak[2]==nil)",
    );
    let scene = instantiate(&mut r, "return Def.Actor{Condition=false}");
    assert_eq!(r.actor_scene_root(scene), Some(Value::Nil));
    assert!(r.actor_scene_snapshot(scene).unwrap().is_empty());
    let scene = instantiate(&mut r, "return Def.Quad{Name='healthy'}");
    let mut other = runtime();
    assert!(!other.release_actor_scene(scene).unwrap());
    execute(&mut r, "assert(root:GetName()=='healthy')");
    assert_clean(&r);
}

#[test]
fn ordinary_command_errors_report_and_continue_while_fatal_limits_abort() {
    let mut r = runtime();
    instantiate(
        &mut r,
        "return Def.ActorFrame{ProbeCommand=function()error({reason='stop'})end,Def.Quad{ProbeCommand=function(self)self:x(42)end}}",
    );
    execute(
        &mut r,
        "assert(root:playcommand('Probe')==root);assert(root:GetChildren()['']:GetX()==42)",
    );
    assert_eq!(r.take_actor_diagnostics().len(), 1);
    execute(
        &mut r,
        "root:addcommand('Forever',function()while true do end end)",
    );
    r.vm_mut().set_instruction_limit(Some(400));
    let error = r
        .vm_mut()
        .execute(&compile_named(b"root:playcommand('Forever')", b"=limit").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    r.vm_mut().set_instruction_limit(None);
    execute(&mut r, "assert(root:GetNumChildren()==1)");
    assert_clean(&r);
}

#[test]
fn scalar_methods_preserve_native_float_rounding_and_lua_fields_do_not_alias_state() {
    let mut r = runtime();
    let scene = instantiate(&mut r, "return Def.Quad{}");
    execute(
        &mut r,
        r#"
        assert(root:x('16777217')==root and root:GetX()==16777216)
        root:addx(1); assert(root:GetX()==16777216)
        root:xy(0.1,0.2):z(-3):zoom(2):zoomy(-4):rotationz(12):addrotationz(0.5):setsize(5,6):aux(7)
        assert(root:GetZoomedWidth()==10 and root:GetZoomedHeight()==-24 and root:GetRotationZ()==12.5 and root:getaux()==7)
        root.x='authored';assert(root:GetX()>0 and root.x=='authored');Actor.x(root,3);assert(root:GetX()==3)
        root:visible(0.5);assert(not root:GetVisible());root:visible(true);assert(root:GetVisible())
        assert(not pcall(root.visible,root,'1'))
        root:draworder(16777217)
    "#,
    );
    let s = &r.actor_scene_snapshot(scene).unwrap()[0];
    assert_eq!(s.draw_order, 16777217);
    assert_eq!(s.position[0], 3.);
    assert_clean(&r);
}
