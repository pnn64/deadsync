# ITGmania whole-song archives

This directory contains one content-addressed `.tar.zst` per chart in the
`lua-songs` corpus. `index.json` maps each source simfile to its archive SHA-256.

Every archive contains:

- `manifest.json` with schema versions, ITGmania source revision, display and
  update context, random-state provenance, texture metadata, required assets,
  and hashes for every member;
- `song/` with the simfile, reachable Lua closure, and locally referenced assets;
- `trace/semantic.json` with the complete native semantic/render trace.

The Rust test opens these files from disk and stream-decompresses them. It does
not use `include_bytes!`, launch ITGmania, or depend on the external `lua-songs`
directory after extraction.

Validate all archive hashes and members:

```powershell
cargo test --test song_lua_itgmania_semantic_parity `
  whole_song_archive_index_and_streamed_members_are_valid
```

Run full compilation, full-duration composition, and exact trace comparison for
one chart:

```powershell
$env:DEADSYNC_SONG_ARCHIVE="Cuphead"
cargo test --test song_lua_itgmania_semantic_parity `
  whole_song_archives_compile_compose_and_match_native_trace -- `
  --ignored --nocapture
```

Unset `DEADSYNC_SONG_ARCHIVE` to audit the complete corpus. The full audit is
ignored by default because it is intentionally expensive and reports real
remaining parity gaps; archive integrity stays in the normal test suite.
