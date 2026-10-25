//! Safe method-key classification against the frozen stack-reading implementation.
#[path = "support/method_name_before.rs"]
mod before;
#[path = "../src/actor_method_name.rs"]
mod current;
#[path = "../src/raw_vm.rs"]
mod raw_vm;

#[test]
fn method_name_flags_preserve_binary_keys_utf8_errors_and_gc_roots() {
    let lua = mlua::Lua::new();
    for bytes in [
        b"x".to_vec(),
        b"visible".to_vec(),
        b"__songlua_private".to_vec(),
        b"OnCommand".to_vec(),
        b"SetUpdateFunction".to_vec(),
        b"SetDrawFunction".to_vec(),
        b"GetDrawFunction".to_vec(),
        b"OnCommand\0suffix".to_vec(),
        "long".repeat(256).into_bytes(),
        vec![255, 0],
    ] {
        let name = lua.create_string(bytes).unwrap();
        let expected = before::flags(&lua, &name).map_err(|e| e.to_string());
        for _ in 0..4 {
            lua.gc_collect().unwrap();
            assert_eq!(
                current::flags(&lua, &name).map_err(|e| e.to_string()),
                expected
            );
            let function: mlua::Function = lua
                .load("return function(a,b)return a,b end")
                .eval()
                .unwrap();
            assert_eq!(function.call::<(i32, i32)>((17, 29)).unwrap(), (17, 29));
        }
    }
}
