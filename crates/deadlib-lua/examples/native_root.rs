//! Execute a public-header C embedding driver on the persistent main stack.
#[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
fn run() {
    use deadlib_lua::{NativeCAbiLoader, Vm};
    let image = std::env::args().nth(1).expect("native_root image");
    // SAFETY: the fixture is built with this build's protocol and public headers.
    let loader = unsafe { NativeCAbiLoader::from_library(image.as_bytes()) }.unwrap();
    let first = unsafe { loader.driver(image.as_bytes(), b"root_first") }.unwrap();
    let second = unsafe { loader.driver(image.as_bytes(), b"root_second") }.unwrap();
    let mut vm = Vm::new_bare();
    println!("first.return\t{}", first.run(&mut vm).unwrap());
    // Closures pushed by the first entry retain their originating image.
    drop(first);
    vm.collect_garbage().unwrap();
    println!("second.return\t{}", second.run(&mut vm).unwrap());
    drop(second);
    drop(loader);
    vm.with_native_stack(|stack| {
        stack.push(stack.value(2).unwrap())?;
        stack.call_unprotected(0, Some(1))?;
        assert_eq!(stack.to_bytes(-1)?, Some(b"rooted value".as_slice()));
        stack.set_top(2)
    })
    .unwrap();
    drop(vm);
    let stats = NativeCAbiLoader::statistics();
    assert_eq!(stats.operations, stats.returned);
    assert_eq!(stats.states_created, stats.states_destroyed);
    assert_eq!(stats.groups_created, stats.groups_destroyed);
    assert_eq!(stats.libraries_opened, stats.libraries_closed);
    eprintln!(
        "NATIVE_ROOT operations={} states={} groups={} libraries={}",
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
