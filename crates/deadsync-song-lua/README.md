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
