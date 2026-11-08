//! A bare embedding used by the public C library-initializer comparison.
#[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
fn run() {
    use deadlib_lua::{NativeCAbiLoader, NativeLibraryLoader, Value, Vm, compile_named};
    use std::io::Write;
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [module, source, order] = args.as_slice() else {
        panic!("library_initializers module source.lua order")
    };
    let loader = NativeCAbiLoader::default();
    let mut vm = Vm::new_bare();
    let library = loader.open(module.as_bytes()).unwrap();
    let symbol = library.symbol(b"luaopen_library_initializers").unwrap();
    let function = vm.new_native_function(symbol).unwrap();
    vm.call(function, &[]).unwrap();
    vm.set_global(b"library_order", Value::Number(order.parse().unwrap()))
        .unwrap();
    let path = vm.intern(module.as_bytes());
    vm.set_global(b"library_module_path", path).unwrap();
    let program = compile_named(
        &std::fs::read(source).unwrap(),
        b"@library-initializers.lua",
    )
    .unwrap();
    let result = vm.execute(&program).unwrap();
    let [Value::String(text)] = result else {
        panic!("comparison must return its transcript")
    };
    let text = *text;
    std::io::stdout()
        .write_all(vm.string_bytes(text).unwrap())
        .unwrap();
    drop(vm);
    drop(loader);
    let stats = NativeCAbiLoader::statistics();
    assert_eq!(stats.states_created, stats.states_destroyed);
    assert_eq!(stats.groups_created, stats.groups_destroyed);
    assert_eq!(stats.operations, stats.returned);
    assert_eq!(stats.libraries_opened, stats.libraries_closed);
    eprintln!(
        "LIBRARY_INIT operations={} states={} groups={} libraries={}",
        stats.operations, stats.states_created, stats.groups_created, stats.libraries_opened
    );
}
fn main() {
    #[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
    #[cfg(not(all(feature = "native-c-api", any(windows, target_os = "linux"))))]
    panic!("native C ABI is required for the library-initializer comparison");
}
