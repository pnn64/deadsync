# Scheduled actor tween countdown

ITGmania subtracts float frame deltas from the remaining time of each
actor tween. DeadSync previously interpolated scheduled callback writes
using elapsed time divided by the authored duration. Even when queued
commands dispatched on the correct frame, the intermediate fade values
and zero-time states could differ from native playback.

Scheduled writes now share the native countdown samples of their back
tween. The compiler walks the existing queue clock with the original
float durations and residual frame delta. Static startup blocks and
scheduled callbacks use the same interpolation function. The replaced
duplicate scheduled interpolation is removed.

Sampling and completion comparisons use the native float frame clock.
This also keeps a zero-duration glow write from waiting an extra frame
when its timestamp rounds upward. Immediate actor writes retain their
existing dispatch behavior. Retargeted writes retain the pending tween's
clock. Adjacent expensive easing curves only reuse a factor when their
countdown arrays and clocks match.

## Native regression

The local `scheduled-fade` fixture queues `Begin` after a one-second
sleep. The native harness appends the command's tweens inside the actual
C++ queued dispatch, rather than preloading equivalent-looking tweens.
Four sprites cover chained flashes, a long diffuse fade, a glow fade,
and a zero-time glow flash followed by a short decelerated fade.

The direct comparison checks 9,632 raw diffuse/glow channels at the
existing `f32::EPSILON` threshold, and 1,204 draw counts over 301 frames.
The native Lua trace independently agrees with those C++ channels. The
semantic comparison checks all 4,706 observations, including projected
geometry, draw colors, and vibration. The previous implementation fails
the direct queued-command comparison starting at 1.533 seconds.

`scheduled-fade-provenance.json` records the local reference revision,
native executable and source hashes, fixture inputs, and raw/compressed
capture hashes. Previously published captures remain intact. All song
resources and native reference sources are local; no downloads or
comparison-tolerance changes are part of this fix.

The older `message-queue-reset` capture completed an accelerated move
and its trailing reset at 4.383333 seconds. Replaying the same local Lua
with the current native float frame cadence leaves the move pending on
that frame and resets at 4.4 seconds. The independent C++ queued-command
fixture confirms both frames for the runner and TV actor. DeadSync
matches all 484 sampled position axes using the existing 0.75 geometry
threshold. The fresh semantic reference passes 951/951 checks, including
568 newly captured draw-color checks. The regression uses
`message-queue-reset-clock.json.zst`; the old capture remains untouched.

## Whole-song result

The complete retained Mawaru8 trace improves from 304,886/306,487 to
**304,938/306,502**. Every projected geometry check now passes:
**92,870/92,870**, including the previously failing grade visibility and
flash states. Restored visibility enables additional geometry checks,
which accounts for the larger denominator. All layer-order, final-render,
render-persistence, update-value, player-range, vibration, timeline,
message-command, and runtime-modifier sections pass.

Whole-song parity remains unfinished. Draw colors pass 172,569/174,128;
the remaining failures all report the monitor actor's timer glow effect
(`def-0453`). Five speculative input-message probes still fail because
their song tables or parameters are unavailable in those probes. These
failures remain visible in the full audit. Mawaru5's complete local trace
continues to pass 77,718/77,718 checks.

## Validation and publication

Rework passes 781 song-Lua unit tests, 173 playback tests, two AMV tests,
110 regular semantic tests, and 17 profile/gameplay song-Lua tests.
Publication copies only the curated changes to MAIN and advances its
patch exactly once from `0.5.1747` to `0.5.1748`. Rework remains
uncommitted. MAIN runs the same checks before its commit.

MAIN's initial semantic compilation exhausted LLVM memory. A single-job
`cargo rustc` retry disables debug symbols only for that test binary,
retains the existing dependencies, and runs the same 110 tests directly:

```powershell
cargo rustc --offline -j 1 --test song_lua_itgmania_semantic_parity -- -C debuginfo=0 -C codegen-units=16
```

Source, comparison thresholds, and test selection are identical. The
initial build failure is retained in the investigation logs.

```powershell
$env:RUST_MIN_STACK = '16777216'
$env:ITGMANIA_SONG_LUA_WORKSPACE = 'C:\GitHub\rework'
cargo test --offline -j 2 -p deadsync-song-lua
cargo test --offline -j 2 --test song_lua_itgmania_semantic_parity
cargo test --offline -j 2 -p deadsync-profile-gameplay --lib song_lua
```

## Storage and frame cost

The song-load compiler owns the queue-clock cache on one thread. Each
actor retains at most one cached back-tween array; its capacity is bounded
by the canonical song-frame horizon plus a start sample. Appending a
queue step invalidates that actor's cached array. A miss rebuilds it
during compilation, never during gameplay. Writes to the same back
tween share an immutable `Arc` rather than duplicating its frame data.

Pending scheduled writes hold these arrays only during compilation.
Compilation consumes them into the existing baked output tracks. Queue
clocks and temporary arrays are destroyed when compilation finishes.
There is no gameplay insertion, eviction, pruning, or miss path. The
added worst-case gameplay maintenance cost is therefore zero; existing
track sampling is unchanged. Native comparison and unchanged fixture
check counts provide correctness instrumentation.

## Local chart identity

The frozen project hashes are unchanged. Existing local mismatches are:

| Song | Frozen hash | Local hash |
| --- | --- | --- |
| Mawaru5 | `74765c1936186d20` | `8e0b6274cb33af5e` |
| Mawaru8 | `cadefe09888e9ab8` | `8224fb7e0b05040f` |
| Brain Power | `a73ec5f2f3015620` | `f40ebaf45ea6e26d` |

Get Into It (`9c208360a9b25133`) and Rhythm Hell
(`be38aa9e3c88c32b`) remain absent from the audited local folders and
archives. These resource differences remain separate from engine parity
results so the user can update the frozen list later.
