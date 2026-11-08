# Native actor effect clock

Mawaru8's monitor uses a 0.2-second timer glow with a red component of
11. DeadSync baked the difference between song time and the actor's
native float clock, then subtracted that phase during rendering. Large
song timestamps lost the low bits of the short effect clock. The glow
also used subtraction-based interpolation and fused sine scaling,
where ITGmania combines weighted colors with ordinary float operations.

Compilation now retains `[source clock, actor clock]` samples. Playback
advances the actor clock from the most recent sample. Wraps and restarts
are steps rather than interpolated clock changes. The compiler retains
an earlier reference only when it predicts the exact native float bits.
Timer and beat clocks use the same representation; inactive effects
do not reuse a stale clock. The replaced phase correction is deleted.

Color shifts and ramps now follow the local `Actor::PreDraw` arithmetic:
`color1 * mix + color2 * (1 - mix)`. The song-Lua sine mix follows the
native operation order. Generic presentation effects remain unchanged.

## Independent regression

The local `short-glow` fixture uses the monitor's original glow settings.
The C++ actor oracle and native Lua semantic capture independently agree
on all 3,848 raw diffuse/glow channels over 481 frames. The production
composer compares those channels at the existing `f32::EPSILON`
threshold and checks 481 draw counts. All 8,601 semantic observations
are required. The old implementation fails at 0.016666668 seconds:
native glow red is 10.330127 and DeadSync is 10.330128.

A playback regression checks both clock domains between samples, at a
wrap, after a wrap, and when seeking backward. It also checks that
stopping the effect restores the source clock. The fixture provenance
records local source and capture hashes. Existing captures, frozen
project hashes, and comparison thresholds are retained. No resources
were downloaded.

## Storage and frame cost

The existing song-load compiler owns these samples on one thread.
There is no new cache or gameplay allocation. Each effect track stores
at most the existing canonical frame horizon plus intervening hold
samples; unchanged references use the existing track deduplication.
Every retained reference must predict the exact native clock bits.
Compilation misses only append to the baked track at song load.

The resulting tracks are immutable for the song lifetime. Gameplay
samples them with the existing cursors and performs a subtraction and
addition for an active actor's clock. There is no insertion, eviction,
pruning, disk I/O, or GPU work. Tracks are destroyed with the song's
compiled resources. Native channel checks and the whole-song audit
provide correctness instrumentation.

## Validation

The complete retained local Mawaru8 audit improves from
304,938/306,502 to **306,497/306,502**. All 1,559 remaining monitor color
failures are resolved: **174,128/174,128 draw colors** and
**92,870/92,870 projected geometry** pass. Layer order, final render,
render persistence, update values, player ranges, vibration, timeline,
message commands, and runtime modifiers also pass.

Whole-song parity remains unfinished. Five speculative input-message
compilation probes lack their song tables or parameters: `KeyHitP1`,
`RyukoCheckP1/P2`, and `RyukoExplodeP1/P2`. The full audit continues to
fail on those five observations; they are not suppressed. Its retained
native trace has no live runtime errors or dropped events.

Rework passes 781 song-Lua unit tests, 174 playback tests, two AMV tests,
111 regular semantic tests, and 17 profile/gameplay tests: 1,085 selected
checks. Mawaru5's complete local trace retains all 77,718 passing checks.
The scheduled-fade and queued-reset regressions retain their counts and
native tolerances.

Publication copies 15 curated source, fixture, and documentation files
to MAIN, plus Cargo.toml and Cargo.lock. MAIN advances exactly once from
`0.5.1748` to `0.5.1749`. Both repositories pass the same 1,085 selected
checks. MAIN's full Mawaru8 audit also reports 306,497/306,502, with the
same five input-message probes still failing and every other section
passing. Rework remains uncommitted. Semantic builds use a single job and
omit debug symbols only for the large test binary, following the prior
pass's successful workaround for LLVM memory exhaustion:

```powershell
$env:RUST_MIN_STACK = '16777216'
$env:ITGMANIA_SONG_LUA_WORKSPACE = 'C:\GitHub\rework'
cargo test --offline -j 2 -p deadsync-song-lua
cargo rustc --offline -j 1 --test song_lua_itgmania_semantic_parity -- -C debuginfo=0 -C codegen-units=16
cargo test --offline -j 1 -p deadsync-profile-gameplay --lib song_lua
```

The newly built semantic binary runs all 111 regular tests directly.
Source, test selection, and thresholds are unchanged by these build
flags.

## Frozen chart identity

The 63-chart frozen project list remains untouched: 58 local exact
matches, three local hash mismatches, and two missing local charts.

| Song | Frozen hash | Local hash |
| --- | --- | --- |
| Mawaru5 | `74765c1936186d20` | `8e0b6274cb33af5e` |
| Mawaru8 | `cadefe09888e9ab8` | `8224fb7e0b05040f` |
| Brain Power | `a73ec5f2f3015620` | `f40ebaf45ea6e26d` |

Get Into It (`9c208360a9b25133`) and Rhythm Hell
(`be38aa9e3c88c32b`) remain absent from the audited local folders and
archives. These identity differences are retained for the user to edit
later and are separate from engine parity results.
