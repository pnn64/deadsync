//! Differential coverage for the captured setters used by existing input snapshots.
use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {SongRuntime, test_support},
};

fn run(runtime: &mut SongRuntime, script: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(script.as_bytes(), b"@/fixture/setters.lua").unwrap())
        .unwrap()
        .to_vec()
}
fn compare(probe: &str) {
    compare_definition(
        "return Def.ActorFrame{Def.Quad{Name='left'},Def.Quad{Name='right'}}",
        probe,
    )
}
fn compare_definition(source: &str, probe: &str) {
    for live in [false, true] {
        let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "Actor setters");
        context.live = live;
        let mut rust = SongRuntime::new(&context).unwrap();
        let declaration = run(&mut rust, source)[0];
        let scene = rust.instantiate_actor(declaration).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let Value::String(actual) = run(&mut rust, probe)[0] else {
            panic!("probe string")
        };
        reference_test::assert_source_debug(
            "song/tests/rust_actor_setters.rs::compare_definition",
            format!("{source}\n{probe}"),
            &rust.vm_mut().string_bytes(actual).unwrap(),
        );
        assert!(rust.take_actor_diagnostics().is_empty());
    }
}

#[test]
fn cloned_templates_get_fresh_setters_for_the_new_actor() {
    compare_definition(
        "local template=Def.Quad{InitCommand=function(self)self:y(9)end};return Def.ActorFrame{template,template..{Name='clone'}}",
        r#"
            local a,b=root[1],root[2]
            assert(a~=b and a.__songlua_bound_methods.x~=b.__songlua_bound_methods.x)
            assert(a.__songlua_bound_methods.x(b,11)==a)
            assert(b.__songlua_bound_methods.x(a,23)==b)
            assert(a:GetX()==11 and b:GetX()==23 and a:GetY()==9 and b:GetY()==9)
            return tostring(a:GetX())..':'..tostring(b:GetX())
        "#,
    );
}

#[test]
fn constructor_method_attributes_match_native_installation() {
    compare_definition(
        "return Def.ActorFrame{Def.Quad{x=function()return 'authored' end,y=math.sin,visible=false},Def.Quad{}}",
        r#"
            local a=root[1]
            local function observe(name)
                local method=a[name]
                local ok,value=pcall(method,a,0.5)
                return type(method)..':'..tostring(ok)..':'..(value==a and 'target' or type(value)=='string' and value or type(value))
            end
            return observe('x')..'|'..observe('y')..'|'..observe('visible')
        "#,
    );
}

#[test]
fn bound_setters_keep_their_target_and_consume_the_second_argument() {
    compare(
        r#"
        local a,b=root[1],root[2]
        local bound=a.__songlua_bound_methods
        for _,name in ipairs{'x','y','visible'} do
            assert(rawget(a,name)==nil and type(bound[name])=='function')
        end
        assert(bound.x(b,17)==a and a:GetX()==17 and b:GetX()==0)
        assert(bound.x(false,19)==a and a:GetX()==19)
        assert(bound.x(23)==a and a:GetX()==19)
        assert(bound.x()==a and bound.x(nil,21)==a and a:GetX()==21)
        assert(bound.y({},'16777217')==a and a:GetY()==16777216)
        assert(bound.visible(b,false)==a and not a:GetVisible() and b:GetVisible())
        assert(bound.visible(true)==a and not a:GetVisible())
        assert(bound.visible(nil,true)==a and a:GetVisible())
        return tostring(a:GetX())..':'..tostring(a:GetY())..':'..tostring(a:GetVisible())
    "#,
    );
}

#[test]
fn assigned_and_raw_copied_setters_return_the_captured_actor_for_chaining() {
    compare(
        r#"
        local a,b=root[1],root[2]
        a.visible=b.__songlua_bound_methods.visible
        assert(rawget(a,'visible')==nil)
        assert(a.__songlua_bound_methods.visible==b.__songlua_bound_methods.visible)
        assert(a:visible(false)==b and a:GetVisible() and not b:GetVisible())
        a.x=b.__songlua_bound_methods.y
        assert(a:x(31):y(43)==b and b:GetY()==43 and a:GetX()==0)
        assert(Actor.x(a,17)==b and b:GetY()==17)
        rawset(a,'x',b.__songlua_bound_methods.x)
        assert(a:x(29)==b and a:GetX()==0 and b:GetX()==29)
        return tostring(a:GetVisible())..':'..tostring(b:GetVisible())..':'..tostring(b:GetX())..':'..tostring(b:GetY())
    "#,
    );
}

#[test]
fn public_forwarders_observe_bound_overrides_and_preserve_all_results() {
    compare(
        r#"
        local a,b=root[1],root[2]
        local bound=a.__songlua_bound_methods
        local calls=0
        bound.visible=function(self,...) calls=calls+1; return b,nil,select('#',...),... end
        local target,hole,count,v,hole2,last=a:visible(false,nil,42)
        assert(target==b and hole==nil and count==3 and v==false and hole2==nil and last==42)
        assert(calls==1)
        bound.visible=false
        assert(not pcall(Actor.visible,a,false))
        rawset(a,'__songlua_bound_methods',nil)
        rawset(a,'x',function(self,v)assert(self==a);return b,nil,v end)
        local x,y,z=Actor.x(a,35)
        assert(x==b and y==nil and z==35)
        assert(not pcall(Actor.x,false,1))
        return tostring(calls)..':'..tostring(z)
    "#,
    );
}

#[test]
fn lookup_uses_current_raw_kind_and_missing_public_class_falls_back_to_bound() {
    compare(
        r#"
        local a=root[1]
        local old=Sprite
        Sprite=nil
        assert(a.x==a.__songlua_bound_methods.x and a:x(7)==a)
        Sprite=old
        local original=Actor.x
        Actor.x=function(self,v)return 'actor',self,v end
        rawset(a,'__songlua_actor_type','Actor')
        local tag,target,n=a:x(13)
        assert(tag=='actor' and target==a and n==13)
        rawset(a,'__songlua_actor_type','Quad')
        Actor.x=original
        assert(a:GetX()==7)
        return tag..':'..tostring(n)..':'..tostring(a:GetX())
    "#,
    );
}

#[test]
fn missing_public_classes_preserve_other_existing_methods() {
    compare(
        r#"
        local a=root[1]
        a:x(17):y(29):visible(false)
        local sprite,actor,frame=Sprite,Actor,ActorFrame
        Sprite=nil;Actor=nil;ActorFrame=nil
        assert(type(a.GetX)=='function' and a:GetX()==17 and a:GetY()==29)
        assert(not a:GetVisible() and root:GetNumChildren()==2)
        local result=tostring(a:GetX())..':'..tostring(a:GetY())..':'..tostring(root:GetNumChildren())
        Sprite=sprite;Actor=actor;ActorFrame=frame
        return result
    "#,
    );
}

#[test]
fn sound_initialization_preserves_common_actor_setters_and_sound_methods() {
    compare_definition(
        "return Def.ActorFrame{Def.Sound{Name='left'},Def.Sound{Name='right'}}",
        r#"
            local a,b=root[1],root[2]
            local bound=a.__songlua_bound_methods
            assert(type(bound.x)=='function' and type(bound.y)=='function' and type(bound.visible)=='function')
            assert(type(bound.Load)=='function' and type(bound.get)=='function' and type(bound.stop)=='function')
            assert(a:x(17):y(29):visible(false)==a)
            assert(a:GetX()==17 and a:GetY()==29 and not a:GetVisible())
            rawset(a,'x',b.__songlua_bound_methods.x)
            assert(a:x(41)==b and a:GetX()==17 and b:GetX()==41)
            local handle=a:get()
            assert(type(handle)=='table' and handle:volume(0.25)==handle and a:stop()==a)
            return tostring(a:GetX())..':'..tostring(a:GetY())..':'..tostring(b:GetX())..':'..tostring(a:GetVisible())
        "#,
    );
}

#[test]
fn custom_assignment_hooks_observe_writes_to_the_captured_target() {
    for live in [false, true] {
        let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "Actor setters");
        context.live = live;
        let mut runtime = SongRuntime::new(&context).unwrap();
        let declaration = run(
            &mut runtime,
            "return Def.ActorFrame{Def.Quad{Name='left'},Def.Quad{Name='right'}}",
        )[0];
        let scene = runtime.instantiate_actor(declaration).unwrap();
        let root = runtime.actor_scene_root(scene).unwrap();
        runtime.vm_mut().set_global(b"root", root).unwrap();
        run(
            &mut runtime,
            r#"
            local a,b=root[1],root[2]
            local writes=0
            local mt=getmetatable(b)
            local assign=mt.__newindex
            assert(type(assign)=='function')
            mt.__newindex=function(self,key,value)
                assert(self==b, 'assignment must reach the captured actor')
                if key=='__songlua_visible' then
                    assert(value==false)
                    writes=writes+1
                end
                return assign(self,key,value)
            end
            rawset(b,'__songlua_visible',nil)
            rawset(a,'visible',b.__songlua_bound_methods.visible)
            assert(a:visible(false)==b and not b:GetVisible() and a:GetVisible())
            assert(writes==1, 'the visibility assignment must invoke the hook once')
        "#,
        );
        assert!(runtime.take_actor_diagnostics().is_empty());
    }
}

#[test]
fn saved_bound_setter_roots_retired_target_then_releases_the_scene() {
    let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "Saved setter");
    context.live = true;
    let mut rust = SongRuntime::new(&context).unwrap();
    let baseline = test_support::actor_ownership_counts(&mut rust);
    let definition = run(&mut rust, "return Def.ActorFrame{Def.Quad{}}")[0];
    let scene = rust.instantiate_actor(definition).unwrap();
    let root = rust.actor_scene_root(scene).unwrap();
    rust.vm_mut().set_global(b"root", root).unwrap();
    run(
        &mut rust,
        "saved=root[1].__songlua_bound_methods.x;weak=setmetatable({root,root[1]},{__mode='v'});root=nil",
    );
    assert!(rust.retire_actor_scene(scene).unwrap());
    test_support::reset_actor_subscriptions(&mut rust).unwrap();
    run(
        &mut rust,
        "collectgarbage('collect');local a=saved(false,37);assert(a==weak[2] and a:GetX()==37 and a:GetParent()==weak[1])",
    );
    run(
        &mut rust,
        "saved=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil and weak[2]==nil)",
    );
    assert_eq!(test_support::actor_ownership_counts(&mut rust), baseline);
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
