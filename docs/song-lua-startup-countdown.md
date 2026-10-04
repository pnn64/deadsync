# Native startup tween countdown

The local Mawaru5 chart passes the fresh complete rendered-color trace:
**77,718/77,718**. This supersedes the remaining Mawaru5 fade failures
recorded in `song-lua-draw-colors.md`. Frozen project hashes remain
unchanged; the result applies to the local chart with hash
`8e0b6274cb33af5e`, rather than the frozen `74765c1936186d20`.

## Cause and change

ITGmania's `Actor::UpdateTweening` subtracts a float delta from the time
left on each frame. `CalcPercentThroughTween` evaluates
`1 - time_left / duration`. DeadSync evaluated elapsed time divided by
duration and completed startup tweens at their authored end time.
Rounding can leave native alpha slightly positive on that frame, so
DeadSync hid a sprite that ITGmania still drew.

Startup compilation now retains the original queue durations and bakes
time-left samples using the same float countdown as queued-command
dispatch. Cached playback, uncached playback, command completion, and
sprite-state activation consume that shared progress. Their duplicate
elapsed-time calculations have been removed. Existing easing and
comparison tolerances are unchanged.

The compiler owns these immutable arrays on the loading thread. They
live for the song and are shared through `Arc` when commands are cloned.
Each tween holds at most one sample per canonical update frame plus its
initial sample, bounded by the compilation's song horizon. Sampling uses
binary search without allocation, insertion, eviction, disk access, or
GPU work. Arrays are released with the compiled song at a transition.

## Independent regressions

`fade-clock-native.json.zst` comes from the actual local C++ Actor and
Sprite implementations through `actor-conformance`. Its three sprites
cover chained sleep/linear flashes, a long diffuse fade, and decelerated
glow. The regression checks 5,784 raw diffuse/glow channels at
`f32::EPSILON`, and 723 native draw counts over 241 frames. Before this
fix the first raw-color mismatch occurred at 0.433 seconds. At 1.6
seconds native flash alpha is exactly `5.125999450683594e-6`; DeadSync
previously returned zero. The playback regression also checks that value
after backward and forward seeks.

The old near-camera native draw capture updated only at zero and
1.683333 seconds. Its endpoint geometry was included in the old semantic
fixture, although the semantic evaluator updates at 60 Hz. Repeated
float subtraction now exposes that cadence difference. The new
`near-camera-clock` references use the local native implementation and
102 consecutive 60 Hz actor updates through that endpoint. The semantic
comparison passes 2,374 checks; the direct C++ comparison retains exact
local-depth bits and the original 0.75-pixel tolerance across 816
projected coordinates. Old reference files remain intact. The harness's
independent `tween_depth_matches_native` test also passes.

## Whole-song coverage

The fresh Mawaru5 native run uses the local simfile and all 38 local Lua
files. It has no runtime errors or dropped events, 81 projected tracks,
6,046 sampled rows, and 14,644 update frames. The final native position
is beat `981.0208129882812` at `244.0354766845703` seconds. Its timing
warps make that larger than the authored maximum beat
`693.8333129882812`; this is the same full replay endpoint as the retained
older trace.

| Section | Result |
| --- | --- |
| Compile info | 164/164 |
| Layer order | 42/42 |
| Final render | 206/206 |
| Projected geometry | 23,676/23,676 |
| Native draw colors | 47,584/47,584 |
| Projected vibration | 6,046/6,046 |

The retained older Mawaru5 trace also passes **4,909/4,909**. Its check
count increases because recovered visibility enables geometry checks
that were previously skipped after a visibility failure.

Mawaru8 remains **304,886/306,487** against its fresh whole-song trace.
Scheduled callback tweens have a separate playback path; this startup
change does not resolve those remaining color and visibility gaps.

## Reproduction and publication

All resources and captures are local. `fade-clock-provenance.json`
records the reference revision, native executable and source hashes,
micro-fixture inputs, the Mawaru5 context, all local Lua source hashes,
and raw/compressed capture hashes. No web files were downloaded.

```powershell
$env:RUST_MIN_STACK = '16777216'
$env:ITGMANIA_SONG_LUA_WORKSPACE = 'C:\GitHub\rework'
cargo test -j 2 -p deadsync-song-lua
cargo test -j 2 --test song_lua_itgmania_semantic_parity
cargo test -j 2 -p deadsync-profile-gameplay --lib song_lua
```

This pass validates 781 song-Lua unit tests, 173 playback tests, two AMV
tests, and 108 regular semantic tests in rework and MAIN, plus the
profile/gameplay song-Lua tests. MAIN advances exactly once from
`0.5.1746` to `0.5.1747`; rework remains uncommitted.

Frozen/local hash differences remain Mawaru5
`74765c1936186d20` / `8e0b6274cb33af5e`, Mawaru8
`cadefe09888e9ab8` / `8224fb7e0b05040f`, and Brain Power
`a73ec5f2f3015620` / `f40ebaf45ea6e26d`. Get Into It
(`9c208360a9b25133`) and Rhythm Hell (`be38aa9e3c88c32b`) remain absent
from the audited local folders and archives.
