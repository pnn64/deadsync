use deadlib_lua::{Value, Vm, compile};

#[test]
fn bare_states_execute_core_language_without_standard_library_globals() {
    let mut vm = Vm::new_bare();
    for name in [
        b"_G".as_slice(),
        b"_VERSION",
        b"assert",
        b"math",
        b"string",
        b"table",
        b"debug",
        b"coroutine",
        b"io",
        b"os",
        b"package",
    ] {
        assert_eq!(vm.global(name), Value::Nil);
    }
    assert_eq!(vm.heap_stats().tables, 2);
    assert_eq!(
        vm.execute(&compile(b"local t={value=6};return t.value*7,('x').sub,_G").unwrap())
            .unwrap_err()
            .kind(),
        deadlib_lua::ErrorKind::Runtime
    );
    assert_eq!(
        vm.execute(&compile(b"local t={value=6};return t.value*7,_G,math").unwrap())
            .unwrap(),
        &[Value::Number(42.), Value::Nil, Value::Nil]
    );
    vm.collect_garbage().unwrap();
}
