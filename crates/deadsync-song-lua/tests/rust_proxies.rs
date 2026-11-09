use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaProxyTarget,
    rust_engine::{ActorScene, RustSongRuntime},
};

fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/proxies.lua").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"))
        .to_vec()
}
struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
}
impl Pair {
    fn new(live: bool) -> Self {
        let source = r#"
            observations={}
            function log(...) for i=1,select('#',...) do
                local v=select(i,...);observations[#observations+1]=tostring(v)
            end end
            return Def.ActorFrame{Def.ActorProxy{Name='p'},Def.Actor{Name='a'},Def.ActorProxy{Name='q'}}
        "#;
        let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "proxy migration");
        context.live = live;

        let mut rust = RustSongRuntime::new(&context).unwrap();
        let definition = execute(&mut rust, source)[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self { rust, scene };
        pair.script("p=root[1];a=root[2];q=root[3]", false);
        pair
    }
    fn script(&mut self, source: &str, error: bool) {
        execute(&mut self.rust, source);
        self.compare(source, error);
    }
    fn advance(&mut self, delta: f64) {
        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.compare("advance", false);
    }
    fn compare(&mut self, phase: &str, error: bool) {
        let actor = self.rust.vm_mut().get_global(b"p").unwrap();
        let actual = self.rust.actor_proxy_target(actor);
        if error {
            assert!(actual.is_err(), "{phase}: Rust read must fail");
        } else {
            fn normalize(target: Option<SongLuaProxyTarget>) -> Option<SongLuaProxyTarget> {
                target.map(|target| match target {
                    SongLuaProxyTarget::Actor { overlay_index } => {
                        assert_ne!(overlay_index, 0);
                        SongLuaProxyTarget::Actor { overlay_index: 0 }
                    }
                    target => target,
                })
            }
            reference_test::assert_debug(
                "song/tests/rust_proxies.rs::target",
                &(phase, &normalize(actual.unwrap())),
            );
        }
        let actual = self
            .rust
            .actor_scene_snapshot(self.scene)
            .unwrap()
            .into_iter()
            .map(|actor| actor.render)
            .collect::<Vec<_>>();
        reference_test::assert_debug("song/tests/rust_proxies.rs::render", &(phase, &actual));
        let observations = "return table.concat(observations,'|')";
        let Value::String(actual) = execute(&mut self.rust, observations)[0] else {
            panic!("observations")
        };
        reference_test::assert_debug(
            "song/tests/rust_proxies.rs::observations",
            &(phase, &self.rust.vm_mut().string_bytes(actual).unwrap()),
        );
    }
}

#[test]
fn frozen_native_proxy_inventory_identity_and_inheritance() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-song-lua-micro/proxy-methods-input.json"
    ))
    .unwrap();
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script("native_proxy=p;native_target=a", false);
        pair.script(fixture["lua_assertions"].as_str().unwrap(), false);
        let p = pair.rust.vm_mut().get_global(b"p").unwrap();
        let a = pair.rust.vm_mut().get_global(b"a").unwrap();
        assert_eq!(
            pair.rust.actor_proxy_target(p).unwrap(),
            Some(SongLuaProxyTarget::Actor {
                overlay_index: pair.rust.vm_mut().object_pointer(a).unwrap(),
            })
        );
    }
}

#[test]
fn target_classification_priorities_and_visibility_match_production() {
    let mut pair = Pair::new(true);
    for child in [
        "P1Score", "P2Score", "p1score", "Underlay", "oVeRlAy", "Unknown", "",
    ] {
        pair.script(&format!("a.__songlua_top_screen_child_name={child:?};a.__songlua_player_index=1;p:SetTarget(a);assert(p:GetTarget()==a);log(p.__songlua_proxy_target_kind,p.__songlua_proxy_player_index)"), false);
        pair.script("a:visible(false)", false);
        pair.script("a:visible(true)", false);
    }
    pair.script("a.__songlua_top_screen_child_name=nil", false);
    for child in ["NoteField", "judgment", "CoMbO", "Other", ""] {
        pair.script(&format!("a.__songlua_player_child_name={child:?};p:SetTarget(a);log(p.__songlua_proxy_target_kind,p.__songlua_proxy_player_index)"), false);
    }
    for index in [
        "nil",
        "0",
        "-1",
        "1.9",
        "'0x1'",
        "'-0.75'",
        "'9223372036854774784'",
    ] {
        pair.script(&format!("a.__songlua_player_index={index};p:SetTarget(a);log(p.__songlua_proxy_target_kind,p.__songlua_proxy_player_index)"), false);
    }
    pair.script("assert(a.SetTarget==nil and a.GetTarget==nil);assert(ActorProxy.SetTarget(a,q)==a);assert(a.__songlua_proxy_target_actor==q);assert(not pcall(ActorProxy.GetTarget,a))", false);
    pair.script("local target={__songlua_actor_type=17};assert(p:SetTarget(target)==p and p:GetTarget()==target)", false);
}

#[test]
fn invalid_targets_preserve_old_values_and_partial_classification_writes() {
    let mut pair = Pair::new(true);
    pair.script("p:SetTarget(a)", false);
    for value in [
        "nil",
        "false",
        "12",
        "'target'",
        "{}",
        "{__songlua_actor_type=false}",
        "{__songlua_actor_type=string.char(255)}",
        "setmetatable({}, {__index=function() return 'Actor' end})",
    ] {
        pair.script(&format!("local old=p:GetTarget();assert(not pcall(p.SetTarget,p,{value}));assert(p:GetTarget()==old)"), false);
    }
    for field in [
        "__songlua_top_screen_child_name",
        "__songlua_player_index",
        "__songlua_player_child_name",
    ] {
        pair.script(
            &format!(
                r#"
            p:SetTarget(a);local target={{__songlua_actor_type='Actor'}}
            if '{field}'=='__songlua_player_child_name' then target.__songlua_player_index=1 end
            target['{field}']=false
            assert(not pcall(p.SetTarget,p,target));assert(p:GetTarget()==target)
            log(p.__songlua_proxy_target_kind,p.__songlua_proxy_player_index)
        "#
            ),
            false,
        );
    }
    pair.script("p:SetTarget(a);local mt=getmetatable(p);local assign=mt.__newindex;p.__songlua_proxy_target_actor=nil;mt.__newindex=function(self,key,value) if key=='__songlua_proxy_target_actor' then collectgarbage();error('publication failure') end;rawset(self,key,value) end;assert(not pcall(p.SetTarget,p,a));assert(p:GetTarget()==nil);log(p.__songlua_proxy_target_kind,p.__songlua_proxy_player_index);mt.__newindex=assign", false);
}

#[test]
fn collecting_reentrant_getters_keep_target_identity_and_write_order() {
    let mut pair = Pair::new(true);
    pair.script(
        r#"
        local target={__songlua_actor_type='Actor'}
        setmetatable(target,{__index=function(self,key)
            log(key);collectgarbage('collect');assert(p:GetTarget()==self)
            if key=='__songlua_top_screen_child_name' then q:SetTarget(a) end
            if key=='__songlua_player_index' then return '1.75' end
            if key=='__songlua_player_child_name' then return 'NoteField' end
        end})
        assert(p:SetTarget(target)==p);assert(p:GetTarget()==target and q:GetTarget()==a)
        log(p.__songlua_proxy_target_kind,p.__songlua_proxy_player_index)
    "#,
        false,
    );
    pair.script(r#"
        local target=p:GetTarget();rawset(target,'__songlua_visible',nil)
        setmetatable(target,{__index=function(self,key)
            if key=='__songlua_visible' then p.__songlua_proxy_target_actor=nil;collectgarbage('collect');log('visibility');return false end
        end})
        p.__songlua_proxy_target_kind='underlay'
    "#, false);
    pair.script("assert(p:GetTarget()==nil);p:SetTarget(a)", false);
    pair.script("local target={__songlua_actor_type='Actor'};setmetatable(target,{__index=function(self,key) if key=='__songlua_visible' then collectgarbage();log('ignored error');error('visible failure') end end});p:SetTarget(target);p.__songlua_proxy_target_kind='overlay'", false);
}

#[test]
fn retained_bound_methods_and_targets_are_traced_and_released() {
    let mut pair = Pair::new(true);
    pair.script(r#"
        local target={__songlua_actor_type='Actor'};weak=setmetatable({target},{__mode='v'})
        p:SetTarget(target);target=nil;collectgarbage();assert(weak[1]==p:GetTarget())
        setter=p.__songlua_bound_methods.SetTarget;getter=p.__songlua_bound_methods.GetTarget
        assert(getter()==p:GetTarget());assert(setter(false,a)==p);collectgarbage();collectgarbage();assert(weak[1]==nil)
        local originals=p.__songlua_bound_methods
        p.__songlua_bound_methods={GetTarget=function(self,...) return self,select('#',...),... end}
        local x,n,y,z=p:GetTarget(3,nil);assert(x==p and n==2 and y==3 and z==nil)
        p.__songlua_bound_methods=originals
    "#, false);
    let actor = pair.rust.vm_mut().get_global(b"p").unwrap();
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    assert!(pair.rust.actor_proxy_target(actor).is_err());
    execute(
        &mut pair.rust,
        r#"
        local target={__songlua_actor_type='Actor'};setter(nil,target)
        assert(getter()==target);weak=setmetatable({p,target},{__mode='v'})
        p=nil;root=nil;a=nil;q=nil;target=nil;collectgarbage();assert(weak[1] and weak[2])
        setter=nil;getter=nil;collectgarbage();collectgarbage();assert(weak[1]==nil and weak[2]==nil)
    "#,
    );
}

#[test]
fn target_changes_remain_immediate_and_queued_callbacks_retain_owners() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script(
            r#"
            p:SetTarget(a):linear(.5):x(20):SetTarget(q)
            assert(p:GetTarget()==q)
            p:addcommand('Swap',function(self)
                collectgarbage('collect');log('Swap',self:GetX())
                self:SetTarget(a);assert(self:GetTarget()==a)
            end):queuecommand('Swap')
        "#,
            false,
        );
        for delta in [0.1, 0.2, 0.4] {
            pair.advance(delta);
        }
        pair.script(
            "assert(p:GetTarget()==a);p:sleep(.2):SetTarget(q);assert(p:GetTarget()==q)",
            false,
        );
    }
}

#[test]
fn proxy_template_instances_rebind_methods_and_release_independently() {
    let mut pair = Pair::new(true);
    pair.script("p:SetTarget(a)", false);
    let template = pair.rust.vm_mut().get_global(b"p").unwrap();
    let clone_scene = pair.rust.instantiate_actor(template).unwrap();
    let clone = pair.rust.actor_scene_root(clone_scene).unwrap();
    pair.rust.vm_mut().set_global(b"cloned", clone).unwrap();
    execute(
        &mut pair.rust,
        r#"
        assert(cloned~=p and cloned:GetTarget()==a)
        assert(cloned.__songlua_bound_methods~=p.__songlua_bound_methods)
        local setter=cloned.__songlua_bound_methods.SetTarget
        assert(setter(nil,q)==cloned and cloned:GetTarget()==q and p:GetTarget()==a)
        cloned:visible(false);assert(p:GetVisible())
    "#,
    );
    assert!(pair.rust.release_actor_scene(clone_scene).unwrap());
    execute(
        &mut pair.rust,
        "cloned=nil;collectgarbage();collectgarbage();assert(p:GetTarget()==a)",
    );
    pair.compare("after clone retirement", false);
}

#[test]
fn malformed_renderer_fields_fail_and_recover_without_stale_output() {
    let mut pair = Pair::new(false);
    pair.script("p:SetTarget(a)", false);
    for field in [
        "__songlua_proxy_target_kind",
        "__songlua_proxy_player_index",
        "__songlua_proxy_target_actor",
    ] {
        pair.script(&format!("p.{field}=false"), true);
        pair.script("p:SetTarget(a)", false);
    }
    for index in ["'no number'", "0/0", "1/0", "9223372036854775808"] {
        pair.script(&format!("p.__songlua_proxy_player_index={index}"), true);
        pair.script("p:SetTarget(a)", false);
    }
    pair.script("p.__songlua_proxy_target_kind=nil;p.__songlua_proxy_player_index=false;p.__songlua_proxy_target_actor=false", false);
    pair.script(
        "p:SetTarget(a);p.__songlua_proxy_target_kind='Actor'",
        false,
    );
    pair.script("p:SetTarget(a)", false);
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
