# Chart Lua ownership

`deadsync-song-lua` owns DeadSync's chart-Lua compatibility semantics. A theme
change must not change compilation, modifier windows, actor transforms, proxy
targets, capture ordering, or scheduled chart media.

- The compiler and `playback/prepare.rs` select song layers, preserve shared-session
  compilation and independent-layer fallback, and prepare runtime commands.
- `gameplay.rs` translates compiled commands into the deterministic windows
  consumed by `deadsync-gameplay`. The gameplay crate never depends on this
  presentation-aware crate and never builds actors.
- `playback.rs` owns actor execution, proxies, offscreen captures, media schedules,
  frame ordering, and song-lifetime scratch storage. It uses the existing
  `NoteskinSlot` contract, not the concrete asset loader or profile types.
- `deadsync-notefield` continues to own note rendering and player transform math.
- `deadsync-assets` supplies file/resource compilation. `deadsync-profile-gameplay`
  supplies profile/session compilation context. Neither supplies Lua semantics.
- `deadsync-shell` initiates preparation, coordinates media and warmup, and runs
  composition for gameplay and practice. Simply Love supplies presentation
  fragments through `frame_layers` and its existing field/HUD adapter. Shared
  playback decides which fragments a chart captures and how it transforms them.

The renderer reads gameplay state without advancing judgment or chart timing.
Buffers and caches retain their song lifetime and are prepared before live play.

Playback fixtures live in `tests/playback.rs`. They include the production source
so they can exercise private capture/cache paths with real asset slots without a
library-unit-test type-identity cycle through the asset crate. Full-frame gameplay
and practice regressions live with the shell orchestration. Native ITGmania actor
and Lua-semantic fixtures remain in the workspace tests.

## Runtime

The public song compiler, preparation, live scene, reload and fallback paths use
`deadlib-lua` directly. The core owns parsing, bytecode, GC,
standard libraries and the Rust host API. This crate owns song/actor bindings,
commands, managers, timing/RNG, input/messages, media and renderer-facing output.

Application callers use `compile_song_lua`,
`compile_song_lua_layers`, `reload_song_lua`
and the shared `playback` entry points. `SongRuntime` owns the song VM and host
bindings. Preparation can emit owned native data or retain a live scene;
reloads keep the song's VM and shared state. Fallback uses the same compiler.

## Ownership and lifetime

A retained song session owns one `SongRuntime` through `SongLiveRuntime`.
Gameplay callers release its mutex guard before applying writes that may invoke
another callback. Host callbacks use `HostCall` for nested Lua execution.

The runtime's VM owns the actor store. Host callbacks and runtime entry points
borrow it through their VM context, so a borrow ends before Lua can run again.

Retained Lua objects use VM roots, traced captures or private table attachments.
Copying a `Value` alone does not root it. Actor retirement removes active scene
ownership while allowing authored references and bound callbacks to retain their
objects until collection. End commands, sound stops, message cleanup and reload
handoff remain adapter responsibilities. Dropping an external retained callback
queues its root release; the next VM access drains the queue without reentering
Lua or acquiring the VM mutex from `Drop`.

Compiled song preparation and VM state are owned in memory. They do not persist
legacy registry handles. Noteskin disk artifacts are owned by `deadsync-noteskin`,
whose compiler identity changed from 29 to 30 for the engine cutover.

## Regression controls

The compiler and live-runtime tests exercise the same implementation as the
application. `test-support` exposes fixture controls and observations. Native
ITGmania snapshots provide independent regression expectations.

```sh
cargo test --locked -p deadsync-song-lua --test production_routing
cargo test --locked -p deadsync-song-lua --tests --features test-support --no-fail-fast -- --test-threads=1
```
