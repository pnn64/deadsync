from pathlib import Path
r=Path('/mnt/c/GitHub/deadsync/crates/deadsync-song-lua')
p=r/'src/live.rs';b=p.read_bytes();assert b'migration_finish_health_update' not in b
b+=b'\n#[cfg(all(feature = "test-support", feature = "rust-engine"))]\npub(crate) fn migration_finish_health_update(lua: &Lua) -> mlua::Result<()> {\n    finish_health_update(lua)\n}\n'
p.write_bytes(b)
p=r/'src/rust_engine/test_support.rs';b=p.read_bytes();b+=b'\npub fn legacy_live_scene_lua(scene: &crate::SongLuaLiveScene) -> &mlua::Lua { &scene.lua }\npub fn finish_legacy_health_update(lua: &mlua::Lua) -> mlua::Result<()> { crate::live::migration_finish_health_update(lua) }\n';p.write_bytes(b)
p=r/'Cargo.toml';b=p.read_bytes();b+=b'\n[[test]]\nname = "rust_live_frames"\nrequired-features = ["rust-engine", "test-support"]\n';p.write_bytes(b)
print('Added test-only native delegates and one isolated live-phase target')
