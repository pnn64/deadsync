//! Bootstrap the engine, then run C programs which own their Lua state lifetimes.
#[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
fn run() {
    use deadlib_lua::NativeCAbiLoader;
    let image = std::env::args().nth(1).expect("native_owned image");
    // SAFETY: the fixture and both int(void) entries match this build's C ABI.
    let loader = if std::env::args().nth(2).as_deref() == Some("sdk") {
        NativeCAbiLoader::default()
    } else {
        unsafe { NativeCAbiLoader::from_library(image.as_bytes()) }.unwrap()
    };
    if let Some(symbol) = std::env::args().nth(3) {
        let program = unsafe { loader.program(image.as_bytes(), symbol.as_bytes()) }.unwrap();
        let result = program.run();
        panic!("terminating C program returned {result}");
    }
    let first = unsafe { loader.program(image.as_bytes(), b"owned_first") }.unwrap();
    let second = unsafe { loader.program(image.as_bytes(), b"owned_second") }.unwrap();
    let a = first.run();
    drop(first);
    let b = second.run();
    println!("program.first\t{a}\nprogram.second\t{b}");
    drop(second);
    drop(loader);
    let stats = NativeCAbiLoader::statistics();
    assert_eq!(stats.operations, stats.returned);
    assert_eq!(stats.states_created, stats.states_destroyed);
    assert_eq!(stats.groups_created, stats.groups_destroyed);
    assert_eq!(stats.libraries_opened, stats.libraries_closed);
    eprintln!(
        "NATIVE_OWNED operations={} states={} groups={} libraries={}",
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
