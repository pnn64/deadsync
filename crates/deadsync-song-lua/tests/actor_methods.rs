//! Authenticated native method identities and weak registry rooting.
#[path = "../src/actor_methods.rs"]
mod actor_methods;
use actor_methods::{Method, identity, register};
use mlua::Lua;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed},
};

#[test]
fn method_identity_is_exact_and_does_not_root_replaced_functions() {
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Relaxed);
        }
    }
    let lua = Lua::new();
    let dropped = Arc::new(AtomicBool::new(false));
    let marker = Dropped(dropped.clone());
    let function = lua
        .create_function(move |_, ()| {
            let _ = &marker;
            Ok(())
        })
        .unwrap();
    register(&lua, &function, Method::Lookup).unwrap();
    let alias = function.clone();
    assert_eq!(identity(&lua, &alias).unwrap(), Some(Method::Lookup));
    let other = lua.create_function(|_, ()| Ok(())).unwrap();
    assert_eq!(identity(&lua, &other).unwrap(), None);
    assert!(!dropped.load(Relaxed));
    drop(function);
    drop(alias);
    lua.gc_collect().unwrap();
    assert!(
        dropped.load(Relaxed),
        "method metadata must not root its weak key"
    );
    for _ in 0..128 {
        let replacement = lua.create_function(|_, ()| Ok(())).unwrap();
        assert_eq!(identity(&lua, &replacement).unwrap(), None);
        drop(replacement);
        lua.gc_collect().unwrap();
    }
}

#[test]
fn position_identities_keep_setter_target_and_weak_function_roots() {
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Relaxed);
        }
    }
    let lua = Lua::new();
    for setter in actor_methods::Setter::ALL {
        let dropped = Arc::new(AtomicBool::new(false));
        let marker = Dropped(dropped.clone());
        let function = lua
            .create_function(move |_, ()| {
                let _ = &marker;
                Ok(())
            })
            .unwrap();
        let method = Method::Bound(setter, 0x1230);
        register(&lua, &function, method).unwrap();
        assert_eq!(identity(&lua, &function).unwrap(), Some(method));
        drop(function);
        lua.gc_collect().unwrap();
        assert!(
            dropped.load(Relaxed),
            "method metadata must not root a bound function"
        );
    }
}
