from pathlib import Path
p = Path('/mnt/c/GitHub/deadsync/docs/testing/2026-11-04-rust-lua-production-migration/wire_song_observation_owners.py')
s = p.read_text()
head = s[:s.index('# Root-owned')]
tail = s[s.index("edit('src/rust_engine/managers/catalog.rs','    context:"):]
tail = tail.replace("context: &crate::SongLuaCompileContext,\\n) -> Result<deadlib_lua::Root, Error>", "context: &SongLuaCompileContext,\\n) -> Result<deadlib_lua::Root, Error>")
# The previous exact edit stopped before this signature: the file uses the imported name.
exec(compile(head + tail, str(p) + ':resume', 'exec'))
