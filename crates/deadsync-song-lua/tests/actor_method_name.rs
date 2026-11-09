//! Binary method keys, UTF-8 errors and pinned-string lifetime in the Rust VM.
use deadlib_lua::{Value, Vm, compile};
use deadsync_song_lua::rust_engine::test_support;

#[test]
fn method_name_flags_preserve_binary_keys_utf8_errors_and_gc_roots() {
    let mut vm = Vm::new();
    let classify = test_support::actor_method_flags_probe(&mut vm).unwrap();
    let function_root = vm.pin(classify).unwrap();
    for (bytes, expected) in [
        (b"x".to_vec(), Some((true, false))),
        (b"visible".to_vec(), Some((true, false))),
        (b"__songlua_private".to_vec(), Some((false, false))),
        (b"OnCommand".to_vec(), Some((false, false))),
        (b"SetUpdateFunction".to_vec(), Some((true, true))),
        (b"SetDrawFunction".to_vec(), Some((true, true))),
        (b"GetDrawFunction".to_vec(), Some((true, true))),
        (b"OnCommand\0suffix".to_vec(), Some((true, false))),
        ("long".repeat(256).into_bytes(), Some((true, false))),
        (vec![255, 0], None),
    ] {
        let name = vm.intern(&bytes);
        let key_root = vm.pin(name).unwrap();
        for _ in 0..4 {
            vm.collect_garbage().unwrap();
            match expected {
                Some((public, frame)) => assert_eq!(
                    vm.call(classify, &[name]).unwrap(),
                    &[Value::Boolean(public), Value::Boolean(frame)]
                ),
                None => {
                    let error = vm.call(classify, &[name]).unwrap_err();
                    let expected = std::str::from_utf8(&bytes).unwrap_err().to_string();
                    assert!(error.message().contains(&expected), "{error:?}");
                }
            }
            let function = vm
                .execute(&compile(b"return function(a,b)return a,b end").unwrap())
                .unwrap()[0];
            assert_eq!(
                vm.call(function, &[Value::Number(17.), Value::Number(29.)])
                    .unwrap(),
                &[Value::Number(17.), Value::Number(29.)]
            );
        }
        vm.unpin(key_root).unwrap();
    }
    vm.unpin(function_root).unwrap();
}
