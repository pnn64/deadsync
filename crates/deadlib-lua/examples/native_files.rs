//! Run public FILE** interoperability probes in a single C runtime image.
#[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
fn run() {
    use deadlib_lua::{NativeCAbiLoader, NativeLibraryLoader, Value, Vm, compile_named};
    use std::io::Write;
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [image, source, ..] = args.as_slice() else {
        panic!("native_files image source.lua [rust]")
    };
    let rust_libraries = args.get(2).is_some_and(|mode| mode == "rust");
    // SAFETY: the fixture image is built from this build's boundary protocol.
    let loader =
        std::sync::Arc::new(unsafe { NativeCAbiLoader::from_library(image.as_bytes()) }.unwrap());
    let mut vm = if rust_libraries {
        Vm::new()
    } else {
        Vm::new_bare()
    };
    vm.set_native_library_loader(Some(loader.clone()));
    if rust_libraries {
        vm.execute(&compile_named(b"early=assert(io.tmpfile());assert(early:write('early\\n')and early:seek('set')==0 and early:read(2)=='ea')",b"=before C activation").unwrap()).unwrap();
        assert_eq!(NativeCAbiLoader::statistics().states_created, 0);
    } else {
        let all = vm
            .new_native_function(loader.open_libraries().unwrap())
            .unwrap();
        vm.call(all, &[]).unwrap();
    }
    let library = loader.open(image.as_bytes()).unwrap();
    let entry = vm
        .new_native_function(library.symbol(b"luaopen_native_files").unwrap())
        .unwrap();
    let module = vm.call(entry, &[]).unwrap()[0];
    vm.set_global(b"native_files", module).unwrap();
    let program = compile_named(&std::fs::read(source).unwrap(), b"@native-files.lua").unwrap();
    let path = vm.intern(source.as_bytes());
    vm.set_global(b"native_files_source", path).unwrap();
    let result = vm.execute(&program).unwrap();
    let [Value::String(text)] = result else {
        panic!("transcript expected")
    };
    let text = *text;
    let bytes = vm.string_bytes(text).unwrap().to_vec();
    drop(vm);
    drop(loader);
    let stats = NativeCAbiLoader::statistics();
    assert_eq!(stats.operations, stats.returned);
    assert_eq!(stats.states_created, stats.states_destroyed);
    assert_eq!(stats.groups_created, stats.groups_destroyed);
    assert_eq!(stats.libraries_opened, stats.libraries_closed);
    std::io::stdout().write_all(&bytes).unwrap();
    eprintln!(
        "NATIVE_FILES operations={} states={} groups={} libraries={}",
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
    panic!("native C API required");
}
