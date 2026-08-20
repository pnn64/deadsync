fn main() {
    println!("cargo:rerun-if-changed=src/pipewire_bridge.c");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    let pipewire = pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("libpipewire-0.3")
        .expect("Cannot find PipeWire development headers");

    let mut build = cc::Build::new();
    build.file("src/pipewire_bridge.c");
    build.warnings(true);
    for include in pipewire.include_paths {
        build.include(include);
    }
    build.compile("deadlib_pipewire_bridge");
}
