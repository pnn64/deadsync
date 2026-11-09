//! Production method identities and weak ownership in the Rust VM.
use deadlib_lua::{Value, compile};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{
        ActorScene, RustSongRuntime,
        test_support::{self, ActorMethodIdentity as Method},
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

struct Dropped(Arc<AtomicBool>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Relaxed);
    }
}

fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile(source.as_bytes()).unwrap())
        .unwrap()
        .to_vec()
}

fn scene() -> (RustSongRuntime, ActorScene, deadlib_lua::TableId) {
    let context = SongLuaCompileContext::new(".", "method identity fixtures");
    let mut runtime = RustSongRuntime::new(&context).unwrap();
    let value = execute(&mut runtime, "return Def.ActorFrame{Def.Quad{}}")[0];
    let scene = runtime.instantiate_actor(value).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    let Value::Table(actor) = execute(&mut runtime, "return root[1]")[0] else {
        panic!("child actor")
    };
    (runtime, scene, actor)
}

#[test]
fn method_identity_is_exact_and_does_not_root_replaced_functions() {
    let (mut runtime, _scene, actor) = scene();
    let values = execute(
        &mut runtime,
        "return root[1].__songlua_bound_methods.x,Actor.x,getmetatable(root[1]).__index,getmetatable(root[1]).__newindex,function()end",
    );
    let function = values[0];
    let alias = function;
    let first = runtime.vm_mut().pin(function).unwrap();
    let second = runtime.vm_mut().pin(alias).unwrap();
    assert_eq!(
        test_support::actor_method_identity(&runtime, alias),
        Some(Method::Bound("x", actor))
    );
    assert_eq!(
        test_support::actor_method_identity(&runtime, values[1]),
        Some(Method::Forward("x"))
    );
    assert_eq!(
        test_support::actor_method_identity(&runtime, values[2]),
        Some(Method::Lookup)
    );
    assert_eq!(
        test_support::actor_method_identity(&runtime, values[3]),
        Some(Method::Assign)
    );
    assert_eq!(
        test_support::actor_method_identity(&runtime, values[4]),
        None
    );

    let dropped = Arc::new(AtomicBool::new(false));
    let other = runtime
        .vm_mut()
        .new_host_function_with_data(|_| Ok(()), &[], Dropped(dropped.clone()))
        .unwrap();
    let keep = runtime.vm_mut().pin(other).unwrap();
    assert_eq!(test_support::actor_method_identity(&runtime, other), None);
    runtime.vm_mut().collect_garbage().unwrap();
    assert!(!dropped.load(Relaxed));
    runtime.vm_mut().unpin(keep).unwrap();
    runtime.vm_mut().collect_garbage().unwrap();
    assert!(
        dropped.load(Relaxed),
        "method lookup must not retain an unrelated host closure"
    );

    execute(
        &mut runtime,
        "root[1].__songlua_bound_methods.x=function()end",
    );
    runtime.vm_mut().unpin(first).unwrap();
    runtime.vm_mut().collect_garbage().unwrap();
    assert_eq!(
        test_support::actor_method_identity(&runtime, alias),
        Some(Method::Bound("x", actor))
    );
    runtime.vm_mut().unpin(second).unwrap();
    runtime.vm_mut().collect_garbage().unwrap();
    assert_eq!(
        test_support::actor_method_identity(&runtime, function),
        None
    );
    for _ in 0..128 {
        let replacement = runtime.vm_mut().new_host_function(|_| Ok(()), &[]).unwrap();
        assert_eq!(
            test_support::actor_method_identity(&runtime, replacement),
            None
        );
        runtime.vm_mut().collect_garbage().unwrap();
    }
}

#[test]
fn position_identities_keep_setter_target_and_weak_function_roots() {
    for setter in ["visible", "x", "y"] {
        let (mut runtime, scene, actor) = scene();
        let dropped = Arc::new(AtomicBool::new(false));
        let marker = runtime
            .vm_mut()
            .new_host_function_with_data(|_| Ok(()), &[], Dropped(dropped.clone()))
            .unwrap();
        let key = runtime.vm_mut().intern(b"fixture_marker");
        runtime.vm_mut().raw_table_set(actor, key, marker).unwrap();
        let function = execute(
            &mut runtime,
            &format!("kept=root[1].__songlua_bound_methods.{setter};root=nil;return kept"),
        )[0];
        assert_eq!(
            test_support::actor_method_identity(&runtime, function),
            Some(Method::Bound(setter, actor))
        );
        assert!(runtime.retire_actor_scene(scene).unwrap());
        test_support::reset_actor_subscriptions(&mut runtime).unwrap();
        execute(
            &mut runtime,
            "collectgarbage('collect');collectgarbage('collect')",
        );
        assert!(!dropped.load(Relaxed), "saved setter must retain its actor");
        assert_eq!(
            test_support::actor_method_identity(&runtime, function),
            Some(Method::Bound(setter, actor))
        );
        runtime.vm_mut().set_global(b"kept", Value::Nil).unwrap();
        runtime.vm_mut().collect_garbage().unwrap();
        runtime.vm_mut().collect_garbage().unwrap();
        assert!(
            dropped.load(Relaxed),
            "method metadata must not retain the discarded bound actor"
        );
        assert_eq!(
            test_support::actor_method_identity(&runtime, function),
            None
        );
    }
}
