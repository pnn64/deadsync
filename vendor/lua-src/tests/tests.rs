//! This is standalone tests that executed at runtime (not during build)

#[test]
fn test_build() {
    let host = target_lexicon::Triple::host().to_string();
    let outdir = tempfile::tempdir().expect("Failed to create temp dir");
    let mut build = lua_src::Build::new();
    build.target(&host);

    for version in [
        lua_src::Lua51,
        lua_src::Lua52,
        lua_src::Lua53,
        lua_src::Lua54,
        lua_src::Lua55,
    ] {
        // Use a dedicated output dir for each Lua version
        let version_dir = outdir.path().join(format!("{:?}", version));
        std::fs::create_dir(&version_dir).expect("Failed to create version dir");

        build.out_dir(&version_dir);
        let _artifacts = build.build(version);
    }
}
