use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime, test_support},
};
fn run(runtime: &mut SongRuntime, script: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(script.as_bytes(), b"@/fixture/retirement.lua").unwrap())
        .unwrap()
        .to_vec()
}
fn runtime(live: bool) -> SongRuntime {
    let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "Retirement");
    context.live = live;
    SongRuntime::new(&context).unwrap()
}
fn scene(r: &mut SongRuntime, source: &str) -> ActorScene {
    let value = run(r, source)[0];
    let scene = r.instantiate_actor(value).unwrap();
    let root = r.actor_scene_root(scene).unwrap();
    r.vm_mut().set_global(b"root", root).unwrap();
    scene
}
fn collect(r: &mut SongRuntime) {
    run(r, "collectgarbage('collect');collectgarbage('collect')");
}

#[test]
fn saved_actor_state_and_late_wrappers_match_native_after_retirement() {
    let source = "return Def.ActorFrame{Name='parent',Def.Quad{Name='child',InitCommand=function(self)self:x(17)end}}";

    let mut rust = runtime(true);
    let baseline = test_support::actor_ownership_counts(&rust);
    let scene = scene(&mut rust, source);
    let save = "saved=root[1];weak=setmetatable({root,saved},{__mode='v'});root=nil";

    run(&mut rust, save);
    assert!(rust.retire_actor_scene(scene).unwrap());
    assert!(!rust.retire_actor_scene(scene).unwrap());
    assert!(!rust.release_actor_scene(scene).unwrap());
    assert!(rust.actor_scene_root(scene).is_none());
    assert!(rust.actor_scene_snapshot(scene).is_none());
    assert!(!rust.advance_actor_scene(scene, 1.).unwrap());
    let probe = "collectgarbage('collect');assert(saved:GetX()==17);assert(saved:GetParent():GetName()=='parent');saved:addx(5);local w=saved:AddWrapperState();w:x(9);collectgarbage('collect');assert(saved:GetWrapperState(1):GetX()==9);assert(saved:GetX()==22)";

    run(&mut rust, probe);
    assert_eq!(test_support::actor_ownership_counts(&rust)[2], 1);
    run(
        &mut rust,
        "saved=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1] and weak[2])",
    );
    test_support::reset_actor_subscriptions(&mut rust).unwrap();
    run(
        &mut rust,
        "collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil and weak[2]==nil)",
    );
    assert_eq!(test_support::actor_ownership_counts(&rust), baseline);
    run(&mut rust, "MESSAGEMAN:Broadcast('AfterCollection')");
}

#[test]
fn unused_retired_scenes_release_rust_metadata_in_repeated_cycles() {
    for live in [false, true] {
        let mut rust = runtime(live);
        let baseline = test_support::actor_ownership_counts(&rust);
        for _ in 0..8 {
            let scene = scene(
                &mut rust,
                "return Def.ActorFrame{Def.Quad{},Def.ActorFrame{Def.Quad{}}}",
            );
            assert!(rust.end_actor_scene(scene).unwrap());
            assert!(rust.retire_actor_scene(scene).unwrap());
            run(&mut rust, "root=nil");
            collect(&mut rust);
            assert_eq!(test_support::actor_ownership_counts(&rust), baseline);
        }
    }
}

#[test]
fn detached_fields_metatables_and_collecting_callbacks_keep_native_state() {
    let mut rust = runtime(true);
    let baseline = test_support::actor_ownership_counts(&rust);
    let scene = scene(
        &mut rust,
        "return Def.ActorFrame{Name='parent',Def.Quad{Name='child'}}",
    );
    run(
        &mut rust,
        "saved=root[1];PROBE=function(self)collectgarbage('collect');Actor.x(self,31)end;saved:addcommand('Probe',PROBE);root[1]=nil;setmetatable(saved,{__mode='kv'})",
    );
    assert!(rust.retire_actor_scene(scene).unwrap());
    run(
        &mut rust,
        "root=nil;collectgarbage('collect');Actor.playcommand(saved,'Probe');assert(Actor.GetX(saved)==31,'collected command state');assert(Actor.GetName(Actor.GetParent(saved))=='parent','retained parent');saved=nil;PROBE=nil",
    );
    collect(&mut rust);
    assert_eq!(test_support::actor_ownership_counts(&rust), baseline);
}

#[test]
fn retirement_unsubscribes_old_messages_but_keeps_direct_commands_callable() {
    let mut rust = runtime(true);
    let scene = scene(
        &mut rust,
        "hits=0;return Def.ActorFrame{PulseMessageCommand=function(self)hits=hits+1 end,PingCommand=function(self)hits=hits+10 end}",
    );
    run(&mut rust, "MESSAGEMAN:Broadcast('Pulse');assert(hits==1)");
    assert!(rust.retire_actor_scene(scene).unwrap());
    run(
        &mut rust,
        "collectgarbage('collect');MESSAGEMAN:Broadcast('Pulse');assert(hits==1);root:playcommand('Ping');assert(hits==11)",
    );
}

#[test]
fn failed_end_still_allows_retained_state_and_eventual_cleanup() {
    let mut rust = runtime(true);
    let baseline = test_support::actor_ownership_counts(&rust);
    let scene = scene(
        &mut rust,
        "return Def.ActorFrame{Def.Quad{EndCommand=function(self)self:x(23);error('End failure')end}}",
    );
    assert!(
        rust.end_actor_scene(scene)
            .unwrap_err()
            .to_string()
            .contains("End failure")
    );
    assert!(rust.retire_actor_scene(scene).unwrap());
    run(
        &mut rust,
        "collectgarbage('collect');assert(root[1]:GetX()==23);root=nil",
    );
    collect(&mut rust);
    assert_eq!(test_support::actor_ownership_counts(&rust), baseline);
}

#[test]
fn shared_actors_transfer_after_the_last_active_owner_retires_or_releases() {
    for hard_release in [false, true] {
        let mut rust = runtime(true);
        let baseline = test_support::actor_ownership_counts(&rust);
        let parent = scene(&mut rust, "return Def.ActorFrame{}");
        let parent_value = rust.actor_scene_root(parent).unwrap();
        let child = scene(
            &mut rust,
            "return Def.Quad{InitCommand=function(self)self:x(13)end}",
        );
        assert!(rust.append_actor_scene(child, parent_value).unwrap());
        assert!(rust.retire_actor_scene(child).unwrap());
        if hard_release {
            assert!(rust.release_actor_scene(parent).unwrap());
        } else {
            assert!(rust.retire_actor_scene(parent).unwrap());
        }
        run(
            &mut rust,
            "collectgarbage('collect');assert(root:GetX()==13);root:addx(2);assert(root:GetX()==15);root=nil",
        );
        collect(&mut rust);
        assert_eq!(test_support::actor_ownership_counts(&rust), baseline);
    }
}

#[test]
fn retained_sound_handle_keeps_its_actor_and_native_parameters() {
    let mut rust = runtime(true);
    let baseline = test_support::actor_ownership_counts(&rust);
    let scene = scene(&mut rust, "return Def.Sound{}");
    run(
        &mut rust,
        "sound=root:get();sound:volume(0.25);weak=setmetatable({root},{__mode='v'});root=nil",
    );
    assert!(rust.retire_actor_scene(scene).unwrap());
    run(
        &mut rust,
        "collectgarbage('collect');assert(weak[1]);sound:pitch(1.5);sound:volume(0.75);sound=nil",
    );
    collect(&mut rust);
    run(&mut rust, "assert(weak[1]==nil)");
    assert_eq!(test_support::actor_ownership_counts(&rust), baseline);
}

#[test]
fn weak_authored_command_fields_still_follow_native_collection_rules() {
    let source = "return Def.Quad{}";

    let mut rust = runtime(true);
    let scene = scene(&mut rust, source);
    let script = "local bound=rawget(root,'__songlua_bound_methods');local play,getx=root.playcommand,root.GetX;root.ProbeCommand=function(self)self:x(31)end;setmetatable(root,{__mode='kv'});collectgarbage('collect');collectgarbage('collect');assert(rawget(root,'ProbeCommand')==nil);assert(rawget(root,'__songlua_bound_methods')==bound);play(root,'Probe');assert(getx(root)==0)";

    assert!(rust.retire_actor_scene(scene).unwrap());
    run(&mut rust, script);
}

#[test]
fn screen_reconstruction_keeps_saved_native_actors_until_lua_drops_them() {
    for live in [false, true] {
        let mut context =
            SongLuaCompileContext::new(std::path::Path::new("."), "screen retirement");
        context.live = live;

        let mut rust = SongRuntime::new(&context).unwrap();
        let (old, selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(old, &selected).unwrap();
        let save = "root=SCREENMAN:GetTopScreen();SAVED_TOP=root;SAVED=root:GetChild('Overlay');SAVED_GET=GetPlayerAF;weak=setmetatable({root},{__mode='v'})";

        run(&mut rust, save);

        assert!(rust.end_actor_scene(old).unwrap());
        assert!(rust.retire_actor_scene(old).unwrap());

        let (new, selected) = rust.reconstruct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(new, &selected).unwrap();
        let probe = "root=nil;collectgarbage('collect');assert(SAVED_TOP~=SCREENMAN:GetTopScreen());SAVED:x(33);assert(SAVED:GetX()==33);assert(SAVED_GET(PLAYER_1)~=GetPlayerAF(PLAYER_1));SAVED:AddWrapperState():x(6);collectgarbage('collect');assert(SAVED:GetWrapperState(1):GetX()==6)";

        run(&mut rust, probe);
        assert_eq!(test_support::actor_ownership_counts(&rust)[2], 1);
        let drop_refs = "SAVED=nil;SAVED_TOP=nil;SAVED_GET=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]);root=SCREENMAN:GetTopScreen()";

        run(&mut rust, drop_refs);

        assert!(rust.end_actor_scene(new).unwrap());
        assert!(rust.retire_actor_scene(new).unwrap());

        run(&mut rust, "root=nil");

        let (replacement, selected) = rust.reconstruct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(replacement, &selected).unwrap();
        run(
            &mut rust,
            "collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil)",
        );
        assert_eq!(test_support::actor_ownership_counts(&rust)[2], 0);
        assert!(rust.actor_scene_root(new).is_none());
        assert!(rust.actor_scene_root(replacement).is_some());
    }
}

#[test]
fn scene_handles_from_another_vm_cannot_retire_or_release_local_actors() {
    let mut first = runtime(true);
    let mut second = runtime(true);
    let a = scene(&mut first, "return Def.Quad{}");
    let b = scene(&mut second, "return Def.Quad{}");
    assert!(
        second.actor_scene_root(a).is_none(),
        "scene identity must include originating VM"
    );
    assert!(!second.retire_actor_scene(a).unwrap());
    assert!(!second.release_actor_scene(a).unwrap());
    assert!(!second.end_actor_scene(a).unwrap());
    assert!(second.actor_scene_root(b).is_some());
    assert!(first.actor_scene_root(a).is_some());
}

#[test]
fn late_wrapper_subscriptions_remain_rooted_until_the_next_registry_replacement() {
    let source = "return Def.ActorFrame{}";

    let mut rust = runtime(true);
    let baseline = test_support::actor_ownership_counts(&rust);
    let scene = scene(&mut rust, source);

    assert!(rust.retire_actor_scene(scene).unwrap());
    let script = "hits=0;local wrapper=root:AddWrapperState();wrapper.PulseMessageCommand=function(self)hits=hits+1 end;weak=setmetatable({root,wrapper},{__mode='v'});root=nil;wrapper=nil;collectgarbage('collect');collectgarbage('collect');MESSAGEMAN:Broadcast('Pulse');assert(hits==1,'late registry must retain its subscriber');assert(weak[1] and weak[2])";

    run(&mut rust, script);

    test_support::reset_actor_subscriptions(&mut rust).unwrap();
    let stopped = "collectgarbage('collect');collectgarbage('collect');MESSAGEMAN:Broadcast('Pulse');assert(hits==1)";

    run(&mut rust, stopped);
    run(&mut rust, "assert(weak[1]==nil and weak[2]==nil)");
    assert_eq!(test_support::actor_ownership_counts(&rust), baseline);
}
