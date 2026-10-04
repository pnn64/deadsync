# Mawaru9 compatibility checkpoint

Updated 2026-10-04. Song: `lua-songs/mawaru9/mawaru9.sm`, dance-single
Challenge, description `TaroNuke`.

## Resolved blockers

- The harness now enumerates every native chart in `Song:GetAllSteps()` and
  filters `GetStepsByStepsType()` with native enum validation. The current
  chart shares object identity with its entry in those arrays. Each call
  returns a fresh array in native load order.
- Player and NoteField feature probes select the ITGmania branch, including
  the absence of SM5.2-only `set_skin` and `SetNoteData` APIs.
- A per-frame instruction guard permits a long, finite replay while still
  stopping runaway callbacks. The existing loading quota remains bounded.
- The harness preserves session-local `TimingWindowAdd` preference writes
  with native float conversion; it does not change saved preferences.
- DeadSync leaves Init queue commands until after the tree's On commands,
  including queues submitted through nested `playcommand` calls.
- Model loading retains separate mesh, material and bone paths. Textures
  resolve relative to the materials file and animations read the bones file.
  The former single-path loader has been replaced; single-file call sites
  pass the same path for all three sections.
- Overlay capture storage covers the highest overlay index, even when shared
  actor references replace earlier entries in the pointer lookup.
- Message probes restore command queues and recurring command state as well
  as actor properties. This prevents a probed RoadGlitchLite loop from running
  before its real message initializes the globals it needs.
- Reused Lua definitions create separate actor instances. Each instance runs
  Init and has its own parent, transforms and render state; command functions
  retain their shared Lua upvalues.
- The oracle assigns unique runtime IDs per occurrence and derives each
  parent's draw order from its actual children. Snapshot lookups and the
  DeadSync verifier no longer collapse instances into one definition ID.
- Init queue and shared-actor regression fixtures live inside the Lua crate,
  so the real repository's tests do not depend on a sibling harness checkout.
- Broadcasts from finite queued commands use the dispatch frame's actual beat
  rather than the callback beat that scheduled them. The frame map retains
  the sampled song clock, including timing stops.
- Finite commands now execute their Lua state changes on the dispatch frame.
  Queue clocks keep pending commands until the owning actor advances, including
  child commands submitted through nested `playcommand` calls.
- A queued command starts from the remaining tween tail, not an old callback's
  capture cursor. A newly activated recurring command consumes the current
  frame's remaining delta before its next update.
- Stop and finish clear pending Lua commands and recurring loops. Stop keeps
  the current interpolated position; finish applies the destination.
- Static message probes identify queued render blocks. When runtime replay
  supplies the command's writes, those speculative blocks are removed so they
  cannot override the correctly timed render state.
- Indirect command probes restore shared local cells and tables after the
  capture. Stateful command detection retains changes across its two probe
  runs and restores them after the pair.
- Two-dimensional parent composition applies scale before skew, matching
  native `Actor::BeginDraw`. The affine decomposition derives X shear from
  X scale. The previous ordering widened sprites under nonuniform scale.
- Child translations follow the parent's X, Y and Z rotation, including
  retained ancestor/local scale order. Previously only Z rotation affected
  child offsets, so the road's 90-degree X rotation left offsets on Y rather
  than moving them into depth.
- Unrotated descendants retain an ancestor's nonuniform scale before the
  parent's rotation. Dropping the scale factors at an intermediate frame
  moved ancestor Z scale after the road's X rotation, distorting both the
  sprite and the next child offset.
- Recurring commands consume their pending marker before dispatch. A command
  that finishes without self-queuing stops; its finite tween tail still runs.
- Pre-queue snapshots retain untouched siblings until the initial getter
  callback runs, preventing copied queued state from appearing at beat zero.
- A recurring command's first replay deadline includes its initial queued
  delay. Startup queue time retains 64-bit precision, and cancellation clears
  that time before another loop starts.

## Native reference

`tests/fixtures/itgmania-song-lua-selected/mawaru9/mawaru9.sm.semantic.json.zst`
contains the complete capture: three layers, both players, 854 x 480,
17,930 frames at 60 Hz through beat 836, with Cyber pinned in both engines.
There are 2,431 unique runtime actors and 443 draw orders; every recorded child
was checked against its actual parent. There are zero runtime errors and
zero dropped events. Capture sampling is 0.25 beats with a 2,000,000-event
limit. Its per-entry provenance is recorded in the selected manifest;
older entries keep their original provenance.

Decompressed SHA256:
`d56bc4ea2aef3f9ec87e291466e134e1e6e6925b094a7b5f0443f8dc2bc7b503`.
Compression was checked with an exact byte-for-byte round trip.

Regenerate from the workspace root after a relevant oracle or song change:

```powershell
$env:ITGMANIA_SONG_LUA_NOTESKIN_ROOT = (Resolve-Path deadsync/assets/noteskins).Path
$env:ITGMANIA_SONG_LUA_NOTESKIN = 'cyber'
cargo run --release --manifest-path itgmania-harness-rs/Cargo.toml -- song-lua-semantic-baseline lua-songs/mawaru9 --out .tmp/mawaru9-native-receptor-metrics --beat-step 0.25 --max-events 2000000
```

Run the complete comparison from `deadsync/`:

```powershell
cargo test --test song_lua_itgmania_semantic_parity corpora::lua_songs::mawaru9 -- --exact --ignored --nocapture
```

## Verification

`parent-rotation.lua` reproduces the road's rotated frame and nested offsets,
plus tilted parents with nonuniform and reflected scales, child skew and
noncentral alignment. The request and actual native Actor/Sprite draw output
are retained under `tests/fixtures/itgmania-song-lua-micro/`. The regression
failed before the fix: the first road corner's Y was 1104 instead of 480.
All 36 world-coordinate comparisons now pass within 0.0001 units. This fixture
uses native drawing rather than the semantic host's matrix reconstruction.

The 0.5.1717 translation pass reran all 962 Lua/profile gameplay tests and all
74 regular semantic tests successfully in both repositories. The real game's
running executable blocked Cargo from replacing `target/debug/deadsync.exe`.
Its newly compiled semantic test executable
`song_lua_itgmania_semantic_parity-bc8cc7dd1e331be6.exe` was run directly with
the workspace override; all 74 tests passed. Close the running game before
rebuilding the game executable to try this source change.

The complete 0.5.1717 audit took 706.64 seconds: 469.72 seconds compiling,
202.70 seconds comparing projected geometry and 25.34 seconds comparing
vibration. It still passes 106,321/117,464 checks, with the same 1,025 gap
reports. The nine road sprites' reported coordinates changed, but their
remaining differences still fail the native comparison at beat 87.273.
The translation regression is fixed; full road-scene parity is not established.

Regenerate its native drawing from the workspace root:

```powershell
./itgmania-harness-rs/target/release/itgmania-harness-rs.exe actor-conformance deadsync/tests/fixtures/itgmania-song-lua-micro/parent-rotation.request.json --out deadsync/tests/fixtures/itgmania-song-lua-micro/parent-rotation-native.json
```

`ancestor-scale.lua` adds nonuniform ancestor scales before those rotated
parents, including the road's X/Z scale of 1.334375 and a reflected ancestor.
Its actual native drawing regression failed before the scale propagation fix:
the first road corner's Z was -1669.85 instead of -2006.9. All 36 coordinates
now pass within 0.0001 units. The previous drawing fixtures are unchanged.

The 0.5.1718 scale pass reran all 962 Lua/profile gameplay tests and all 75
regular semantic tests in both repositories. Its complete Mawaru9 audit took
721.50 seconds: 480.29 seconds compiling, 201.92 seconds comparing projected
geometry and 29.96 seconds comparing vibration. It adds 25 passing geometry
checks with the same reference and denominator. At that checkpoint, the
portable road-loop test still reproduced a gap; the subsequent fix below
now enables it as a regular regression.

Regenerate this additional native drawing from the workspace root:

```powershell
./itgmania-harness-rs/target/release/itgmania-harness-rs.exe actor-conformance deadsync/tests/fixtures/itgmania-song-lua-micro/ancestor-scale.request.json --out deadsync/tests/fixtures/itgmania-song-lua-micro/ancestor-scale-native.json
```

The 0.5.1715 probe pass reran all 962 Lua/profile gameplay tests and all 71
regular semantic tests successfully in both repositories. The stateful
cross-actor regression now checks both direct and indirect commands.

The 0.5.1716 affine pass reran all 962 Lua/profile gameplay tests and all 73
regular semantic tests successfully in both repositories. Its full Mawaru9
audit took 635.76 seconds: 422.97 seconds compiling, 177.04 seconds comparing
geometry and 28.07 seconds comparing vibration. The audit remains failing.

The affine regression uses actual native Actor/Sprite drawing, rather than
the Lua semantic host's transform reconstruction. Its input is retained as
`tests/fixtures/itgmania-song-lua-micro/affine-skew.request.json` and its
native output as `affine-skew-native.json`. Before the fix, the curtain's
first corner X was 103.02903 rather than native 101.60125. All 36 world
coordinates now pass, covering nonuniform scale, skew, rotation, reflection
and top alignment.

- Harness: 119 regular tests pass, including separate instances, unique IDs,
  parent-specific draw order, chart identity and Init queue regressions.
- DeadSync Lua and profile gameplay: 962 tests pass in both repositories,
  including all 943 Lua crate tests using repository-local fixture files.
- Model parser: 19 tests pass, including separate files with a materials
  directory and a bone rotation animation.
- Selected corpus coverage passes with Mawaru9 registered.
- Semantic harness checks: 77 regular tests pass in both repositories. The
  shared-actor native
  regression passes all 20 checks, including each instance's final alpha and
  projected position. It failed before the loader and verifier fixes.
- Nested queued-broadcast fixtures pass 39/39 checks with a continuous clock
  and 34/34 with a timing stop. The continuous fixture failed its message
  timing check before the fix. Both compare exact native dispatch beats.
- The former ignored queued Lua state regression now passes as a regular test.
  Native queue-control and recurring-loop fixtures pass 69/69 and 107/107
  checks. They cover appending behind a pending command, ordered state changes,
  cancellation, tween endpoints and the first recurring update's remaining delta.
- Queued parent vibration passes 38/38 native checks, including exact
  on/off dispatch beats 1.516667 and 1.8 rather than predicted static blocks.

The real repository uses the external song packs without copying them:

```powershell
$env:ITGMANIA_SONG_LUA_WORKSPACE = 'C:/GitHub/rework'
cargo test --test song_lua_itgmania_semantic_parity
```

Without the override, the default remains the repository's parent. The original
real-repository attempt failed seven corpus checks because its parent did not
contain `allowed/` and `lua-songs/`; use the workspace override for those tests.

## Complete song audit

After restoring nested global probe state: **118,632 / 118,890 checks pass (99.78%)**. The ignored
full-song test still fails, correctly identifying the remaining gaps.
Fixture status `ok` describes the complete native capture, not a passing
DeadSync comparison.

| Comparator | Passed / total |
| --- | --- |
| Compile info | 12 / 12 |
| Layer order | 4 / 4 |
| Final render | 3,748 / 3,748 |
| Render persistence | 6,639 / 6,639 |
| Update values | 23,197 / 23,197 |
| Player ranges | 13 / 14 |
| Projected geometry | 65,138 / 65,392 |
| Projected vibration | 19,385 / 19,387 |
| Timeline | 224 / 224 |
| Message commands | 226 / 227 |
| Runtime modifiers | 46 / 46 |

The former Sprite/Model ordering difference came from unpinned noteskins;
all four layer-order checks pass with Cyber. The complete capture and
instance-aware comparison still report the remaining gaps rather than
changing comparator tolerances or omitting checks. The native capture was
regenerated for the receptor metric correction documented below.
The previous result was 67,430/88,316; more visible state and runtime message
effects now enter the comparison, so the totals differ. Remaining failing
checks are now 258, with 72 detailed gap reports. Before the affine
fix, the same 117,464 checks passed 104,988; the affine fix added 1,333 passing
geometry checks. The parent-translation pass retained 106,321 passing checks;
the ancestor-scale pass added another 25 without changing the reference.
The 0.5.1719 recurring-tween pass added **5,511**: 5,434 in projected geometry and 77 in
render persistence. Its final audit retains the previous 23,065/23,197 raw
update checks after correcting the 320 visibility-write mismatches found
in its first complete run. That checkpoint passed 111,857/117,464.

The 0.5.1720 conditional-loop correction makes all final alpha/visibility
checks pass, including the formerly persistent trail pools. `ToshiUp` also
matches. The denominator decreases by 162 with unchanged native data and
comparison code: persistence probes require an active compiled update track,
and projected alpha/bounds checks require visibility in both engines. Stopped
loops remove stale tracks and alter which geometry checks run. That pass's
audit therefore had 300 more passing checks and 462 fewer failing checks;
these are not 462 identical comparisons newly passing. Persistence and raw
update failures increased by 7 and 13 respectively, primarily in Reisen's
pooled arrows. Investigate their lifecycle and dispatch timing independently.

The queued `Start -> SpawnPlayers -> SetControlling` sequence now records
`BodyRotateBuildings` at its dispatch beat near 89.701, rather than the
trigger beat near 87.050. Its state changes are now deferred as well. Remaining
gaps include 254 projected geometry checks, one player range, two vibration
mismatches near beat 104.012 and stateful `TVGrow` target writes. All raw
updates, render persistence and the complete timeline, including `ChanceTime`,
now pass.

Local detailed audit output: `.tmp/mawaru-global-probes-final-full.log` at the workspace
root. Rerun the per-song command above to reproduce every comparison. No
reference song files or ITGmania source files were modified.

## Next investigations

The full debug comparison took 2,530.60 seconds (42 minutes), versus roughly
five minutes before deferred execution. A bounded 55-second replay, without
comparators, took 40.15 seconds: 28.88 seconds in frame replay, including
24.54 seconds in command/update execution, and 6.19 seconds reading overlays.
These timings used `DEADSYNC_SONG_LUA_TIMING_STDERR=1` on the 0.5.1714
implementation. Profile the full replay separately before attributing the
remaining audit time to gameplay or song loading.

The same implementation's full replay, without comparators, took 422.46
seconds. Frame replay took 410.08 seconds, including 321.84 seconds in
command/update execution and 63.83 seconds capturing overlay states. The
earlier 2,530.60-second full audit therefore needs comparator profiling too;
these separate runs establish the scale but are not a subtraction of timings
from the same process.

The verifier now resolves active update tracks once per frame while retaining
the original write order and last-active-track precedence. Its regression
checks duplicate, empty, future and out-of-range tracks across a BPM change.
The ignored `frame_track_sampling_benchmark` compares every state field for
2,000 actors with 16,000 tracks. The per-actor sampler took 2.880 seconds;
the frame sampler took 21.15 milliseconds, with identical output. This is
a verifier benchmark, not a gameplay frametime measurement.

After the probe fix and before the affine rendering fix, the complete
Mawaru9 audit took 566.30 seconds and passed 104,988/117,464 checks. Its
measured stages were 344.23 seconds compiling, 185.43 seconds comparing
projected geometry and 29.24 seconds comparing vibration. Enabling
`DEADSYNC_SONG_LUA_TIMING_STDERR=1` now reports each semantic section too.
Native expected values and comparator tolerances are unchanged.

The highest remaining failure count is projected geometry. The two vibration
mismatches and `ChanceTime` give smaller, frame-specific reproductions to
investigate alongside the trail fades.

The parent-rotation pass fixes child offsets independently against native
drawing but does not reduce the complete Mawaru9 failing-check count. Inspect
the road actors' local animation values and scale order through their nested
frames before attributing the remaining road coordinates to the camera or to
the corrected offset rotation. Adding parent Euler angles still cannot
represent arbitrary nested 3D rotations; use a native drawing reproduction
before replacing that composition path.

A temporary bounded 48-second Mawaru9 replay found that `actor_nf` retained
ancestor scales `[2.0015626, 1, 1.334375]` and local scales `[2, 1, 1]`, but
`actor_of` and its sprite dropped those factors. The scale propagation fix
addresses that loss. The same diagnostic reported the loop's local Y as
-1022.9318 at beat 87.273/second 47.85, so inspect the loop independently too.

The portable `road-loop` fixture isolates the exact looping tween:
`linear((480/115)/2):y(-1024):sleep(0):y(0):queuecommand("Loop")`.
It also has a following sibling that reads `road:GetY()`. The independently
captured native trace covers 12 four-beat measures through beat/second 47,
with zero dropped events or runtime errors. Before this fix, it passed
1,164/1,900: at beat 2.25 the native road Y was -80 while DeadSync returned
-73.6; at beat 0.25 the witness's native X was -122.666664 while DeadSync
returned zero. It now passes **1,900/1,900**, including every projected sample.
The original native capture is unchanged. Its decoded SHA256 remains
`308aa5e04ab50c002bbb4fcea9ec753f4752134c10889f44acb8ecf08c5e00ea`.

Startup tween commands now enter the per-frame replay before update callbacks
run. Recurring commands retain the delta left after the preceding cycle,
and retire that cycle's captured queue tail rather than advancing it twice.
Getter sampling and tween completion use the same advanced clock as rendering.
The redundant separate render-only sampling loop has been removed.

`road-loop-order` adds a preceding sibling, which must see the road before
its own update. At beat 0.25 the native earlier sibling sees -114.48889,
while the following sibling sees -122.666664. The fixed replay retains the
preceding frame's positions for actors whose update has not yet run, and
advances newly queued position tweens before later callbacks read them.
This independently captured native fixture passes **2,847/2,847**; it has
zero errors or dropped events. Its decoded SHA256 is
`dd567084ba8945375237248e43c521efbc5a5e4bebbc86f9d8d09a466fe20e11`.
Compression was checked with an exact round trip: 354,531 bytes to 36,197.
`road-loop-parent` additionally checks a parent's callback after its children
have advanced. It passes **3,794/3,794**, including the same-frame restart.
Its decoded SHA256 is
`eae76349c6662d8247e25389f17209f949412ac9f7a1b14d42d0ccbecf6aaad2`;
it has zero errors or dropped events, and compression was verified by an
exact round trip (438,908 bytes to 36,852).
All three fixtures' Lua/simfiles and compressed native captures live inside
DeadSync, so the test works without the sibling harness checkout.

The formerly ignored road-loop regression is now enabled and checks all
**8,541** comparisons across all three fixtures:

```powershell
cargo test --test song_lua_itgmania_semantic_parity recurring_road_loop_matches_native -- --exact --nocapture
```

The native trace runs `WallCommand` on `def-0441` at beat 103.820473 and
`StopVibCommand` at 104.203804. The child vibration mismatch at 104.012138
occurs before that stop; investigate the intervening `TVShrink` and body-score
state changes and their projection rather than assuming early queue dispatch.

Static indirect message probes previously leaked local Lua upvalue changes.
The new `queued-local-state` fixture reproduces the failure at beat 0.017,
before the queued command's native dispatch. Probe scopes now preserve locals
in commands reached through `playcommand`, broadcasts and queues, and restore
snapshots in reverse order. The fixture also checks nested tables, cycles,
aliases and cells shared by different commands. ITGmania's independently
captured trace passes 27/27 checks, as does the existing global-state fixture.
The two runs used to identify stateful cross-actor commands share an outer
scope, allowing their locals to evolve before restoring them after the pair.
This regression is covered by the regular queue-state test below. The full
Mawaru9 tally above includes both the probe fix and the affine rendering fix.

The previously failing queue-state regression is now enabled:

```powershell
cargo test --test song_lua_itgmania_semantic_parity queued_lua_state_matches_native_dispatch -- --exact --nocapture
```

Its Lua, simfile and native trace are retained inside DeadSync. The command
passes without an ignore flag and without changing the native expected trace.

The first complete audit of the recurring-tween fix found 320 additional raw
visibility-write mismatches for five actors: `def-0442` (36), `def-0447` (15),
`def-0450` (51), `def-1818` (109) and `def-1835` (109). The native
`Actor::SetVisible` setter changes `m_bVisible` immediately. Recording an
immediate setter through a queued-command scope must therefore preserve the
last same-frame write, rather than the first scheduled destination.

The independent `recurring-visible` fixture reproduces `visible(false)` then
`visible(true)` on siblings before and after a recurring driver. It failed
26/114 before the capture correction, including 88 of its 90 update-write
checks. It now passes **114/114** in both repositories. This correction is in
`test-support` raw-write capture; the chronological render samples retain
actual queue-dispatch timing. Its native trace has zero runtime errors or
dropped events. Compression was verified by an exact round trip (59,067 bytes
to 6,362); decoded SHA256:
`2412809986827b848a1b61e4e0275fb04cb2c14e7ed275c3e109b870e35031de`.

```powershell
cargo test --test song_lua_itgmania_semantic_parity recurring_visibility_matches_native -- --exact --nocapture
```

The 0.5.1719 recurring-tween pass retains the original native road capture,
adds sibling/parent callback coverage and enables the formerly ignored test.
The additional visibility regression preserves immediate raw-write semantics.
All 962 Lua/profile tests and all 77 regular semantic tests pass in both
repositories. The curve replay test retains signed-zero behavior when its
frame advance is zero; the retarget fixture still passes all 1,640 native
checks, including raw destinations written by recurring callbacks.

The final 0.5.1719 audit took **751.51 seconds**: 501.61 seconds compiling,
209.90 seconds comparing projected geometry and 30.46 seconds comparing
vibration. It passes 111,857/117,464 with the same native capture and
comparison tolerances. The intermediate run, before the immediate-write
capture correction, passed 111,537/117,464 in 780.10 seconds. Keep that
intermediate result separate from the final checkpoint above.

At the 0.5.1719 checkpoint, remaining geometry reports started with two background bounds at beat zero
(`def-0002` and `def-0006`) and then body actors such as `def-0263` near
beat 100.275. The backgrounds were newly reported by this pass and need
investigation of startup placement. Trail alpha/visibility, stateful
message results, one player range, two vibration samples and `ChanceTime`
still require independent reproductions. The road-loop regression passes;
the complete Mawaru9 test remains correctly ignored and failing.

The trail script's reproduction target is a reused actor that receives
`finishtweening`, immediate visibility/color/position writes, then
`linear(0.3):addz(50):diffusealpha(0):zoom(-0.1):queuecommand("Hide")`.
`HideCommand` sets visibility false and queues `aux(0)` after `sleep(0)`.
Check this pool lifecycle independently against native before changing
capture or queue behavior for the remaining trail failures.

The isolated pooled-trail lifecycle passes 565/565 native comparisons. The
remaining trail failure instead reproduces when the driver conditionally
stops requeuing itself. Body `UpdateCommand` only queues its next cycle while
`mawaru_curgame == 2`. The compiled recurring-command runner previously kept
its old pending-command marker when a callback finished without another queue,
so it continued spawning trails after native stopped.

The runner now consumes that marker before dispatch. A callback must actually
self-queue to schedule another cycle. When it stops, the update plan is
invalidated while any finite tween and queued tail command remain intact.
The old startup-state filter also discarded untouched siblings before the
initial getter callback copied a queued actor's state into them. Retaining
their pre-queue snapshots prevents that first result appearing at beat zero;
the later startup-command construction still filters actors without changes.

The portable `recurring-stop` fixture checks two reused pools, a visible getter
witness, termination without `stoptweening`, exactly 151 driver calls, and the
final finite tween. It failed 712/737 before both corrections and now passes
**737/737** against the unchanged native capture. Native reports zero errors
or dropped events. Compression was verified by an exact round trip (172,371
bytes to 15,965); decoded SHA256:
`3365340d046a26cac3d21a9a74680fb9b72e5280b610c86a37e130905420b152`.

```powershell
cargo test --test song_lua_itgmania_semantic_parity recurring_stop_matches_native -- --exact --nocapture
```

The 0.5.1720 complete audit took **419.22 seconds**: 332.73 seconds compiling,
54.91 seconds comparing projected geometry and 23.18 seconds comparing
vibration. The regular regression passes alongside all 962 Lua/profile tests
and all 78 regular semantic tests. The synthetic dispatch benchmark fixtures
now explicitly schedule their next cycle instead of assuming a command
repeats without another queue; their frozen comparison implementations remain
unchanged. These timings measure the debug audit, not gameplay performance.

The Reisen investigation isolates its original Lua scene and chart tables in
`.tmp/reisen-probe`. Its setter comparisons all pass, but the original
comparison failed 2,096 geometry checks (7,962/10,058 overall). Two independent
capture errors explain that motion discrepancy:

- An appended tween must begin from the previous queued destination, as native
  `Actor::BeginTweening` copies the back tween's state. Using the current
  interpolated pose adds lag when a recurring driver writes an earlier sibling.
- Repeated setters in one tween mutate that state's destination. Reisen writes
  `y(...)` then `addy(...)`; treating the second write as another interpolation
  flattens the motion. Capture now retains the first starting pose and replaces
  the destination for writes with the same target and queue timing.

The portable `recurring-follow` fixtures place targets before and after the
driver, with four visible witnesses reading their current Y before and after
each write. Both include a repeated Y setter: `addy(0)` in the first fixture,
and `y(y - 7):addy(7)` in the second. Against the 0.5.1720 executable they failed
694/874 and 490/874 respectively. Both now pass **874/874**, including all 96
getter writes and 608 geometry checks in each case. The isolated original
Reisen scene now passes **10,058/10,058** against its unchanged native capture.

The portable fixtures and their native captures are retained inside DeadSync.
Both native captures have zero errors or dropped events. Compression was
verified with exact byte-for-byte round trips:

- `recurring-follow`: 143,440 bytes to 12,748, decoded SHA256
  `3f01e56721681a5a9b70a1005132255195ce76c1f0cd002e4a0b9f58e054e836`.
- `recurring-follow-offset`: 143,764 bytes to 12,718, decoded SHA256
  `527a91ff5aed31f3d70f329f8b24c913f101eac31808f8823430ce049ce2abe3`.

```powershell
cargo test --test song_lua_itgmania_semantic_parity recurring_follow_matches_native -- --exact --nocapture
```

The 0.5.1721 complete audit took **376.80 seconds**: 296.69 seconds compiling,
50.29 seconds comparing projected geometry and 21.50 seconds comparing
vibration. It passes 112,165/117,302, adding only eight passing geometry checks
to the complete song with the same original native reference and denominator. The
isolated Reisen result does not establish parity for its full-song context:
pooled arrows still differ in visibility, selected targets and raw writes.
Investigate the preceding shared state and random-call sequence before
attributing those remaining differences to the corrected interpolation.

All 962 Lua/profile gameplay tests and all 79 regular semantic tests pass in
both repositories. The regular follow test checks both fixtures, totaling
1,748 independent native comparisons. Native expectations, comparator
tolerances and the full-song reference are unchanged. These timings measure
the debug audit, not gameplay performance.

## Receptor metric reference correction

The semantic host advertised Simply Love but omitted
`Player/ReceptorArrowsYStandard` and `Player/ReceptorArrowsYReverse`.
Its `THEME:GetMetric` fallback returned zero. The checked-out
`Simply-Love-SM5-8ms-iamchris4life/metrics.ini`, lines 2344-2345, defines
**-125 and 145**. Its SHA256 is
`21216ff26ae7687a2faa39cf317a031a84948e1403ed4ca512f0f65d32ed6d00`.

Mawaru9 computes `mawaru9_receptmove = standard + 125` and broadcasts
`SetNoteField`. The missing metric therefore moved the native reference's
Reisen receptors and their arrows down by 125 pixels. DeadSync already
exposes the checked-out theme's values. This is an oracle correction; changing
DeadSync to match the zero fallback would introduce a gameplay error.

The harness now exposes both metrics through `GetMetric` and `HasMetric`.
Its modifier-query regression checks their presence and exact values. All
119 regular harness tests pass. The portable `receptor-metrics` fixture
checks standard and reverse Lua field offsets against a new native capture;
all **20/20** comparisons pass. Both its native capture and the theme source
are independent of DeadSync's implementation. The capture has zero errors
or dropped events and an exact compression round trip: 16,512 bytes to 3,332,
decoded SHA256
`981b279dd0f5041aa2d4c93dc7d866766996f670f4d22603c7c2653346395142`.

```powershell
cargo test --test song_lua_itgmania_semantic_parity simply_love_receptor_metrics_match_native -- --exact --nocapture
```

Other metric-reading song references, including Someone Special, Igaku and
kaboooooom!, require review after this host correction. Their previous
passing status does not prove parity with the corrected theme environment.

The replacement full-song capture retains all 2,431 unique actors and 17,930
frames through beat 836, with zero errors or dropped events. The exact
compression round trip is 32,629,977 bytes to 1,642,932. Its decoded SHA256
is recorded above; the former capture's decoded SHA256 was
`123fdd5c599807f307d133aa66b4b1edfe327495de88e09db636072fb0ee419d`.
Only four of 8,431 tween tracks and 140 of 1,671 projected vertex tracks
change; all other trace sections are identical. At beat 141.526, the native
`def-0762` receptor center moves from Y=240 to the correct Y=115.
The selected manifest records the metric source, its hash and both values.
No expected coordinates were edited by hand. The new local capture is
`.tmp/mawaru9-native-receptor-metrics`; the previous one remains in
`.tmp/mawaru9-native-instances` for comparison.

The 0.5.1722 complete audit passes **114,093/117,302**, with 60,813/63,862
projected geometry checks and 393 gap reports. The corrected reference removes
**1,928 false geometry failures** with the same denominator and unchanged
DeadSync runtime code and comparator tolerances. It does not resolve the
remaining Reisen pool writes or visibility: all other comparator tallies are
unchanged. Overall, 3,209 checks still fail. This full audit took **326.81
seconds**: 248.56 seconds compiling, 49.50 seconds comparing geometry and
20.58 seconds comparing vibration. All 80 regular semantic tests pass in both
repositories, including the new 20-check fixture. Fresh selected-corpus
coverage checks pass in both copies after the reference replacement.

## Initial recurring-command deadline

The main body's `PulseCommand` shares Lua's random generator with Reisen's
pooled arrows. Its first queue sleeps `30/115` seconds, and subsequent queues
sleep `60/115`. Startup capture already runs the first queued callback, but
the recurring runner previously initialized its next deadline with only the
repeat interval. It now includes the initial delay before that callback.
This preserves the phase of the shared random sequence without changing
the random generator or the song.

Queue durations retain 64-bit precision through startup, as they already do
in frame replay. The first implementation used the render cursor's 32-bit
sum and regressed 88 getter checks in `recurring-follow`. Preserving the
exact queue time restores all 874 checks in both follow fixtures. Stop and
finish discard the accumulated startup time, and hurry scales it with the
captured queue.

Two portable fixtures reproduce those exact fractional delays at 90 BPM,
with and without a parent update callback. They record each pulse's count
and beat, then record random choices after beat one, including repeated
draws that reject the previous choice. Their On commands first cancel a
two-second sleep with stop and finish respectively, verifying that a
discarded queue does not delay the new loop. The native captures fail
**89/105** checks each before the fix and pass **105/105** after it. Both
captures have zero errors or dropped events, 281 frames through beat seven,
and verified exact compression round trips:

- `recurring-delay`: 85,701 bytes to 11,516, decoded SHA256
  `5e68268434d690f26bf3910c318a41e641f0a988c1d2e61e21cb4e4711685d0d`.
- `recurring-delay-callback`: 86,455 bytes to 11,590, decoded SHA256
  `0227ecd1813377b2ba35fe45c1b265da3236f41f688dc623169fb9b8cfbb2cf9`.

```powershell
cargo test --test song_lua_itgmania_semantic_parity recurring_initial_delay_matches_native -- --exact --nocapture
```

A temporary copy of the original song also records random-call counts and
results through beat 162.976. An intermediate replay matches all 192 positive
native frames, including Reisen's arrow selection. The first random-driven
pulse now runs at beat 1.175, matching native, instead of beat 1.575.
The startup pulse itself still lacks a replay sample at native beat 0.4 in
this diagnostic; the later 159 pulse samples match. This was a bounded
diagnostic replay, not a substitute for the full parity comparison. Its
temporary test instrumentation was removed from the source.

The 0.5.1723 full audit uses the same native reference as 0.5.1722 and passes
**116,578/118,890**. The failing-check count decreases by 897, from 3,209 to
2,312; detailed gap reports decrease from 393 to 160. All persistence checks
and the `ChanceTime` timeline check now pass. Raw update failures decrease
from 145 to 38. Geometry has 2,270 failures, down from 3,049.

The denominator increases by 1,588: 58 persistence checks and 1,530 geometry
checks. Those comparators depend on active update tracks and visibility in
both engines. Correcting the random sequence changes those states, so the
897 fewer failures are not 897 identical comparisons newly passing. The
native capture and all comparison tolerances remain unchanged.

The final debug audit took **418.56 seconds**, including 334.16 seconds
compiling, 53.34 seconds comparing geometry and 22.32 seconds comparing
vibration. These are audit timings, not gameplay performance measurements.
The intermediate 355.31-second audit had identical comparator tallies;
the final audit reruns the precision and cancellation corrections too.

All 962 Lua/profile tests and all 81 regular semantic tests pass in both
repositories. The new regular regression retains 210 independent native
comparisons, including stop/finish cancellation; both existing follow
fixtures still pass their 1,748 comparisons. Before resetting the exact
startup clock on finish, the cancellation fixture failed 73/105 checks.

The raw-write reproduction at 0.5.1723 was the stair scene's `UpdateCommand` in
`lua/sbahj/default.lua`, line 72. The first mismatch moves both brother
sprites to Y=167.8 instead of native Y=193 at beat 274.289. Later random
rotation and base-zoom writes diverge near beat 275.753. Check the first
collision's getter state and branch before diagnosing those later random
values. The separate player-range failure is P2 X: native [503.059, 773.938]
versus DeadSync [0.000, 745.438]. The remaining message report is Aya's
`TVGrowMessageCommand` in `lua/default.lua`, lines 2356-2357.

## Nested global probe state

An isolated copy of the original stair scene passes all 729 native checks
when started directly. Using its original `ShowGame7` message instead
reproduces the full-song movement and random-choice differences. The parent
callback case fails 436/730 checks before restoring nested global state;
an earlier sibling callback fails 426/686. Both pass all their checks after
the correction, against the unchanged native captures.

The command probes preserved global bindings shallowly and restored local
upvalue tables, but mutations inside global tables survived. The stair
scene stores gravity, floors, bounce counts and timers in those tables.
Speculative message execution therefore advanced its physics before the
real message. That early movement later changed collision branches and
the shared random-call sequence.

Probe snapshots now follow referenced global tables and Lua helper functions,
as well as local cells. They include the function's own environment when it
differs from the globals. The existing graph traversal preserves identities,
cycles and aliases; actor state remains owned by the action-capture scope,
and C closures retain their host state handling. A referenced name is selected
conservatively from the function's bytecode, so the snapshot need not walk
the entire host environment.

The portable `global-probe` fixtures start a falling actor by message from
parent and earlier-sibling callbacks. They exercise nested global tables,
a shared alias, a self-cycle, random choices and a global helper with a
private local counter. The original executable fails **197/342** and
**197/343** comparisons. Restoring tables alone still fails 19 and 18 checks
of the helper's private counter, establishing why helper traversal is needed.
Both now pass **342/342** and **343/343**, totaling 685 native comparisons.
Both native captures have zero errors or dropped events and verified exact
compression round trips:

- `global-probe`: 104,869 bytes to 12,708, decoded SHA256
  `b0975e22b1a316f7aed169c7399aa7a7178f91c952243c36afa1eb7cccba76ce`.
- `global-probe-sibling`: 105,765 bytes to 12,663, decoded SHA256
  `409d891ab774d802ebe42baf07f8b9d49c55b44f22fc5cd12c913da18fee2d66`.

```powershell
cargo test --test song_lua_itgmania_semantic_parity nested_global_probes_match_native -- --exact --nocapture
```

The cross-actor message-discovery test now gives its synthetic target a
`Quad` actor type, matching production actor metadata. Its original
one-command and stable-effect assertions remain unchanged. Without that
type, the untagged target was traversed as ordinary song data, including
its capture bookkeeping.

The intermediate table-only full audit passes **118,632/118,890**, leaving
258 failing checks and 72 gap reports. It adds 2,054 passing checks over
0.5.1723 with the same native reference, denominator and tolerances: all 38
raw-write failures now pass, alongside 2,016 additional geometry checks.
This intermediate audit took 423.82 seconds, including 63.62 seconds in
geometry and 22.19 seconds in vibration. The final audit also verifies
helper-local and environment preservation. The final 0.5.1724 audit has
identical tallies: **118,632/118,890** passing, with **258 failures** and
72 detailed gap reports. Every raw update now matches, including both stair
sprites' physics and random choices. The full native reference and comparator
tolerances remain unchanged.

The final debug audit took **433.52 seconds**: 348.77 seconds compiling,
54.46 seconds comparing geometry and 22.05 seconds comparing vibration.
All 962 Lua/profile tests and all 82 regular semantic tests pass in both
repositories. The complete song test remains correctly ignored and failing.
The outstanding 254 geometry checks include initial background fitting,
body-scene projection and tween results; P2 range, two vibration samples and
the single Aya message report remain independent gaps. These audit timings
do not measure live gameplay performance.

## Width and height zoom fits

The initial `bg5.png` and `bg8.png` bounds, and the one-pixel Glados Quad,
were reference errors. The headless host recorded `zoomtowidth` and
`zoomtoheight` without applying them. These methods now set the destination
zoom axis, matching `Actor::ZoomToWidth` and `Actor::ZoomToHeight` in
ITGmania's `Actor.h`. They preserve unzoomed dimensions. Zoomed-size getters
include destination zoom and base scale.

DeadSync also changed unzoomed dimensions for those methods. Sprites, Quads
and other geometric actors now write the zoom axes instead. This preserves
`GetWidth`/`GetHeight`, interpolates pending fits and lets later `zoomx` or
`zoomy` replace the fit. BitmapText retains its existing deferred font-fit
bounds representation; this pass does not establish native text sizing.

The portable `zoom-axis-fit` fixture exercises a resized Quad with base zoom,
a 64x32 Sprite with negative horizontal fit, and an unsized Quad, followed by
plain zoom resets. ITGmania's compiled actor fixture calls `ZoomToWidth` and
`ZoomToHeight` directly for both immediate and tweened destinations. The
corrected headless host matches all 120 independently drawn native corner
coordinates. DeadSync matches those same 120 coordinates, plus all 267
semantic comparisons. The previous executable fails 39/267 comparisons,
including interpolated bounds and zoom resets.

The compressed semantic capture has zero errors or dropped events and an
exact round trip: 37,222 bytes to 6,851, decoded SHA256
`17e65c4baf681948e571ba317bd309b2d67b917e2e8b6f26b51e59958dc52d7c`.
The full Mawaru9 reference was regenerated from the original unmodified song
with the same Cyber noteskin under `deadsync/assets/noteskins`. Actor
definitions, loaded files, layer roots, random seed, noteskin context, BPMs,
end position and all 17,930 update frames match the previous capture. It has
2,431 unique actor occurrences and zero errors or dropped events. The exact
compression round trip is 32,633,322 bytes to 1,643,448, decoded SHA256
`d56bc4ea2aef3f9ec87e291466e134e1e6e6925b094a7b5f0443f8dc2bc7b503`.
The headless host SHA256 is
`c8177bd08168affde567445f0dd42f7432bc5c0cf774dfdcc32b05f865d1a047`.
Its corrected geometry contributes additional comparisons, so differences
from the old full-song totals are not solely DeadSync improvements.

## Stopped tween positions

A bounded replay of the original body scene found a checkpoint collision
near beat 97.815. Its cancelled queue ended at Z=350. DeadSync retained that
destination for `addz(60)`, producing Z=410; native `StopTweening` clears the
queue and leaves the current position as the new destination. At beat
98.007, the native rendered depth was approximately 58.216 while DeadSync
was approximately 425.750.

Cancellation now restores current X/Y/Z destinations, respecting an earlier
sibling callback's access to a later child's previous-frame position. The
captured replacement tween starts from that position without inventing Lua
setter writes. Cancellation also removes active startup/message block replay:
otherwise, the original movement resumes after the replacement tween ends.

The portable `stop-position` fixture queues three movements, then interrupts
the target from an earlier sibling. A later witness copies its destination
getters. The original executable passes only 73/85 comparisons. Restoring
destination fields alone leaves eight failures; seeding the replacement
tween still leaves two endpoint failures until startup replay is cancelled.
The final implementation passes all 85 comparisons and an additional 60
direct native world-depth coordinates. Those depth assertions matter because
orthographic screen bounds cannot expose an incorrect Z value.

The native capture has zero errors or dropped events and an exact compression
round trip: 28,285 bytes to 4,894, decoded SHA256
`1183fa2f3277138e95f0e17886ec748fa107077ce8551691dadc853189573c9c`.

```powershell
cargo test --test song_lua_itgmania_semantic_parity stopped_position_matches_native -- --exact --nocapture
```

The full 0.5.1725 audit against the corrected reference passes
**118,680/118,929**, leaving **249 failures** and 67 detailed gap reports:

| Comparison | Result |
|---|---:|
| Compile info | 12/12 |
| Layer order | 4/4 |
| Final render | 3,748/3,748 |
| Render persistence | 6,639/6,639 |
| Update values | 23,197/23,197 |
| Player ranges | 13/14 |
| Projected geometry | 65,173/65,418 |
| Projected vibration | 19,398/19,400 |
| Timeline | 224/224 |
| Message commands | 226/227 |
| Runtime modifiers | 46/46 |

The checkpoint's bounds and center now match throughout the full sampled
trace. The fitted backgrounds and Glados Quad match the corrected reference.
Remaining geometry includes MMM balls, Sans bones, Aya/TV, pulse initialization,
alpha and perspective precision. P2 X range, two vibration samples and
`TVGrowMessageCommand` still fail. The complete song test remains ignored and
failing; tolerances and comparison coverage have not been relaxed.

All 962 Lua/profile tests and 84 regular semantic tests pass in both
repositories. The 60 depth assertions also pass in both; all 120 harness tests
pass in rework. The full debug audit took 493.14 seconds, with 372.05 seconds
in update replay, 57.45 seconds comparing geometry and 22.85 seconds comparing
vibration. These measurements include concurrent regression/build work and
do not measure gameplay performance. The main repository receives this pass
as version 0.5.1725, exactly one patch increment from 0.5.1724.

## Queued state order

The original Aya/TV sequence reproduced the same off-screen reset failure
in a portable fixture. After shrinking, Aya should return to X=1120 and the
TV frame to X=832. DeadSync held Aya at X=-392 and the frame at X=-680.
Both commands end their accelerated movement with `sleep(0)` and an
absolute position reset.

Captured cursors use f32 values. Promoting a tween's start and duration to
f64 can place its calculated endpoint slightly after the next captured
zero-time cursor. Sorting completed samples by those endpoints reversed
their native queue order, applying the reset before the off-screen movement.
Frame replay now retains capture order when draining completed actor states.
The chronological merge used outside frame replay retains its existing path.

The small Aya/TV fixture originally passed 356/363 checks, with six geometry
failures and one message report. Preserving queue order fixes all six
geometry failures, reaching 362/363. The remaining message report was a
checker error: `Actor::QueueCommand`, `QueueMessage` and Sleep's implicit
tail use zero-duration native linear objects. They apply instantaneous
states, represented without interpolation by DeadSync.

The command checker now treats those states as instantaneous, retains setter
writes in queued command states and includes writes in Sleep's implicit
tail. The replaced skip flag and filters were deleted. Explicit tween
curves, durations and property comparisons remain strict; no numeric
tolerance was changed. The Aya/TV fixture now passes all 363 checks.
A second fixture writes position and alpha after both `queuecommand` and
`queuemessage`, proving both native states are compared; it passes 33/33,
including two message-command checks. Both captures have zero errors or
dropped events and verified exact compression round trips:

- `message-queue-reset`: 63,587 bytes to 8,457, decoded SHA256
  `1969ed32d970883f46f9d470d4387c6f51b26cb5e82a6b37cac81c1d402ec49a`.
- `queued-command-state`: 16,881 bytes to 3,324, decoded SHA256
  `0ac14f4534047e2092f1016f78727102fc741f35b30e6c1cdbf2d1413d4d7d91`.

```powershell
cargo test --test song_lua_itgmania_semantic_parity queued_message_states_match_native -- --exact --nocapture
```

The final 0.5.1726 full audit uses the unchanged native reference with decoded
SHA256 `d56bc4ea2aef3f9ec87e291466e134e1e6e6925b094a7b5f0443f8dc2bc7b503`.
It passes **118,751/118,933**, leaving **182 failures** and 60 gap reports:

| Comparison | Result |
|---|---:|
| Compile info | 12/12 |
| Layer order | 4/4 |
| Final render | 3,748/3,748 |
| Render persistence | 6,639/6,639 |
| Update values | 23,197/23,197 |
| Player ranges | 13/14 |
| Projected geometry | 65,239/65,418 |
| Projected vibration | 19,398/19,400 |
| Timeline | 224/224 |
| Message commands | 231/231 |
| Runtime modifiers | 46/46 |

This fixes 66 geometry checks with unchanged geometry coverage and resolves
the message checker report. The additional four message-command checks
explain the changed total denominator. Aya, TV static and TV frame now match
throughout the sampled trace; every message-command comparison passes.
The remaining failures are 179 geometry checks, two vibration samples and
P2's X range. Mawaru9's complete test remains ignored and failing.

All 962 Lua/profile tests and 85 regular semantic tests pass in both
repositories. The full debug audit took 424.16 seconds, including 61.57
seconds comparing geometry and 22.57 seconds comparing vibration. These
load-time audit measurements include concurrent validation work and do not
measure gameplay performance. The main repository receives this pass as
0.5.1726, exactly one patch increment from 0.5.1725.

## Rendered updates during timing stops (0.5.1727)

At beat 651.5, Sans has three distinct update frames at the same song beat:
247.4666667, 247.4833333 and 247.5 seconds. Its recurring 20 ms command keeps
moving the bones during that stop. Beat-keyed render tracks collapsed those
frames, so the first native pause sample received the later position: a
seven-unit local displacement, or 9.340625 logical screen pixels.

Captured render updates now retain absolute song seconds when song timing
is available. Their shared time unit survives splitting into background and
foreground layers, queued startup uses the same clock, and gameplay converts
the samples with the calibration offset. The sample coordinate is named
`time` rather than `beat`; raw setter audit tracks still use beats. Sampled
player transforms and column transform windows also retain seconds instead
of dropping frames with identical start/end beats. The original beat-only
capture path remains for compile contexts that have no song timing.

`paused-updates.ssc` contains a 0.4-second stop, a 0.3-second delay and a
one-beat warp. A recurring command moves one Quad; a SetUpdateFunction clock
moves another. Native ITGmania applies all three timing segments. The earlier
staging `.sm` ignored its warp tag, so it was replaced before freezing the
fixture. The final capture contains 644 update frames, zero Lua runtime
errors and zero dropped events. Its raw 78,769 bytes compress to 10,417 bytes
with an exact round trip. Decoded SHA256:
`6e08ed8864f866970142fc8766a63b9197654df61b3c69d44616d160edb51f61`.

The regular semantic regression compares the complete native fixture:
362/370 before the change and 370/370 after it. Before the change, the walker
was 40 units too far along and the frame clock actor was eight units too far
along at the first stopped frame. The gameplay regression additionally
checks 72 native actor positions through
the production compiler, runtime conversion and cached playback, with two
Lua layers, offsets of zero and 0.25 seconds, and a cursor rewind. A separate
fixture checks sampled player X and the rendered column spline during the
stop at music rates 0.5, 1.0 and 1.5. The checker selects render samples with
native seconds; seconds-frame rounding uses the existing gameplay precision
of 0.0001 seconds, while the previous beat tolerance stays unchanged.

The final full-song audit uses the same frozen Mawaru9 native reference:

| Category | Passing checks |
| --- | --- |
| Complete result | 118,780/118,933 |
| Compile info | 12/12 |
| Layer order | 4/4 |
| Final render | 3,748/3,748 |
| Render persistence | 6,639/6,639 |
| Update values | 23,197/23,197 |
| Player ranges | 13/14 |
| Projected geometry | 65,268/65,418 |
| Projected vibration | 19,398/19,400 |
| Timeline | 224/224 |
| Message commands | 231/231 |
| Runtime modifiers | 46/46 |

This resolves all 28 Sans bone geometry failures and the alpha mismatch for
def-2254 at the beat-120 stop. Coverage is unchanged. Mawaru9 still fails
150 geometry checks, two vibration samples and P2's X range, for 153 failed
checks and 31 distinct reports. Its complete test remains ignored and
failing; passing micro-fixtures do not establish full-song parity.

All 963 Lua/profile tests and 86 regular semantic tests pass in both
repositories.
The final full debug audit took 366.88 seconds. The main repository receives
this pass as 0.5.1727, exactly one patch increment from 0.5.1726. The original
song, native Mawaru9 reference and harness sources are unchanged in this pass.

## Recurring tween cycles (0.5.1728)

MMM's four ball sprites build a decelerating upward tween followed by an
accelerating return, then queue `Bounce` again. DeadSync previously counted
only the first tween: the self-queue branch did not flush the last capture
block, and exact interval accumulation included sleeps but omitted tweens.
The next callback consequently restarted the bounce before its return had
finished. Both players then bounced from Y=240, including P2's first ball
whose initial return destination is Y=-40.

Queue-step recording now accumulates exact durations for both sleeps and
tweens. Self-queuing flushes the final tween before selecting its interval.
The sleep-specific accumulator was removed in favor of the shared path.
This remains load-time compiler work; gameplay consumes the sampled tracks.

The native `queued-bounce.sm` fixture reproduces the two different first
bounces and several recurring cycles. Before the fix, the next callback ran
at beat 4.55 instead of ITGmania's 4.9583333. At beat 4.55, both DeadSync balls
were at Y=232.90425; native positions were 150.4157 and -128.29106. The full
micro-fixture improves from **367/543** to **543/543**, resolving all 176
failed geometry checks without changing comparison tolerances.

The frozen capture has 190 update frames, zero Lua runtime errors and zero
dropped events. Its 69,914 raw bytes compress to 9,146 bytes with an exact
round trip. Decoded SHA256:
`edbd97f0fc19cdbd287f6a5c0c29ef5bd3bdf790ff7030de37d1fd80291a771d`.
The gameplay regression checks all 106 native ball positions through the
production compiler, time conversion and cached playback, with offsets of
zero and 0.25 seconds and a backward seek through the first bounces.

The complete song uses the unchanged frozen native reference, decoded
SHA256 `d56bc4ea2aef3f9ec87e291466e134e1e6e6925b094a7b5f0443f8dc2bc7b503`:

| Category | Passing checks |
| --- | --- |
| Complete result | 118,900/118,933 |
| Compile info | 12/12 |
| Layer order | 4/4 |
| Final render | 3,748/3,748 |
| Render persistence | 6,639/6,639 |
| Update values | 23,197/23,197 |
| Player ranges | 13/14 |
| Projected geometry | 65,388/65,418 |
| Projected vibration | 19,398/19,400 |
| Timeline | 224/224 |
| Message commands | 231/231 |
| Runtime modifiers | 46/46 |

All 120 MMM geometry failures are resolved, with unchanged full-song
coverage. Mawaru9 now has 33 failed checks and 23 distinct reports: 30
geometry checks, two vibration samples and P2's X range. Its complete test
remains ignored and failing. The remaining geometry reports concern the
body projections and stars, Chike game-over banners, Patients' pulsing
sprites, and the initial pulse actor.

All 964 Lua/profile tests and 87 regular semantic tests pass in both
repositories. The full debug audit took 446.28 seconds alongside build and
validation work; this is not a gameplay frametime measurement. Main receives
exactly one patch increment from 0.5.1727 to 0.5.1728. The original song and
the harness sources are unchanged.

## Collapsed affine transforms (0.5.1729)

Chike's game-over banners start inside a parent with zero Y zoom and have a
rotated child sprite. Patients' rotated sprites later set base X zoom to
zero. Both produce rank-deficient affine matrices. The decomposition helper
rejected zero X or Y scale and used the fallback that multiplies scalar
zooms before the child's rotation. This reversed the authored transform
order: Chike's horizontal line gained height, and Patients' collapsed
sprites had the wrong width.

The decomposition now preserves zero Y scale and its X shear. When the X
column is zero, it aligns the transformed Y column with local Y; a matrix
with both columns zero remains collapsed. The former zero-axis rejection
and its small-scale threshold were removed. This uses the existing composed
state and rendering path, without adding another transform representation.

`collapsed-transform.lua` covers a zero-Y parent, a zero-X child beneath a
nonuniform parent, and a reflected zero-X parent with rotation and skew.
The frozen song-Lua fixture improves from **24/27** to **27/27**. Its 241
update frames complete without Lua runtime errors or dropped events. The
22,665-byte capture compresses to 3,963 bytes with an exact round trip.
Decoded SHA256:
`d3ecef96621823436ef85bd5f78a4ce7ed653a7230eb35ae0c33df4256d3e21f`.

An independent native C++ actor-conformance fixture describes the same
three transforms. All 36 world vertex coordinates agree within the existing
0.0001 tolerance. The original input is retained alongside the native
output, whose 82,572 bytes have SHA256
`e1f5585de39c8fd5626ee2a667634f4458d1f0a46109a0f42cc7456ca9b6f6ec`.
Both native captures use the unchanged ITGmania reference sources.

The complete song uses the same frozen native Mawaru9 reference, decoded
SHA256 `d56bc4ea2aef3f9ec87e291466e134e1e6e6925b094a7b5f0443f8dc2bc7b503`:

| Category | Passing checks |
| --- | --- |
| Complete result | 118,912/118,933 |
| Compile info | 12/12 |
| Layer order | 4/4 |
| Final render | 3,748/3,748 |
| Render persistence | 6,639/6,639 |
| Update values | 23,197/23,197 |
| Player ranges | 13/14 |
| Projected geometry | 65,400/65,418 |
| Projected vibration | 19,398/19,400 |
| Timeline | 224/224 |
| Message commands | 231/231 |
| Runtime modifiers | 46/46 |

All 12 Chike and Patients geometry failures are resolved. Coverage and
comparison tolerances are unchanged. Mawaru9 has 21 failed checks and 19
distinct reports: 18 geometry checks, two vibration samples and P2's X
range. Its complete test remains ignored and failing. A separate staging
reproduction confirms that `AddWrapperState()` placement is lost, matching
the remaining initial pulse actor error; that gap is not fixed in this pass.

All 964 Lua/profile tests and 89 regular semantic tests pass in both
repositories. The full debug audit took 406.99 seconds alongside validation;
this is not a gameplay frametime measurement. Main receives exactly one
patch increment from 0.5.1728 to 0.5.1729. The original song, full native
reference, and harness sources are unchanged.

## Animated wrapper states (0.5.1730)

Mawaru9's initial pulse creates a wrapper for its placement and nonuniform
scale, then rotates and skews the sprite itself. DeadSync created that Lua
state but omitted it from the compiled overlay tree. Wrapper states now
enter the tree as distinct, independently animated parents. Highest-index
wrappers are outermost, matching ITGmania's draw order. Startup tweens and
wrapper-owned update callbacks are captured and replayed, including after
a backward seek. The wrapped actor retains its sibling draw order. Actor
proxies carry the target's wrapper states while a wrapper targeted directly
has no draw children; hiding a wrapper still hides the proxied owner.

The reference harness had a separate bug: `AddWrapperState()` returned the
owner rather than the new wrapper, and projected geometry did not visit
wrappers. That incorrect reference placed wrapper writes on the sprite
itself. The harness now returns the new state, composes wrappers in native
order, and inherits their visibility and alpha. The previous pulse reference
is replaced. Authored actors remain the audit's operation targets; synthetic
wrapper parents are checked through the final projected geometry.

`wrapper-transform.lua` covers the pulse's mixed actor/wrapper transform,
two stacked wrappers, a one-second wrapper tween, a wrapper-owned update
function, one-based queries, and inherited alpha. It improves from
**81/127** on the 0.5.1729 binary to **127/127**, with 241 update frames, no
Lua errors and no dropped events. The 44,300-byte capture compresses to
7,960 bytes with an exact round trip. Decoded SHA256:
`55360987ebb4c6bc0dd39e494a9242acc2c3cafbe1d7b66de90a4431f69539c0`.

The independent C++ fixture calls ITGmania's actual `Actor::AddWrapperState`
and `GetWrapperState`, then `Actor::Update` and `Actor::Draw`. Its input is
retained in `tests/fixtures/itgmania-actors/wrapper-transform-input.json`.
All 216 world coordinates agree with the harness within 0.002. DeadSync
checks 324 coordinates through the same production transform functions,
including a backward seek, and reuses the warmed textured-sprite builder
to verify drawing and inherited alpha. Native color comparisons allow one
byte of quantization. The native C++ output repeats byte-for-byte; SHA256:
`c09bd6cf1c5a22132f20582a711b2afba54ffb6e58affd70f26d6642ca678d78`.

The refreshed full-song reference uses the same chart, three layers, 50
loaded Lua files, asset/noteskin hashes, random seed, 17,930 update frames,
2,431 unique actor instances and complete beat-836 endpoint. There are no
runtime errors or dropped events. Only operation, tween and projected
vertex tracks change. The 32,633,951-byte capture compresses to 1,644,450
bytes. Decoded SHA256:
`422da087837313cd4dd9fd5986eb81676f153f19ca9ed8b7a01092fbf9c6c508`.
The corrected semantic host SHA256 is
`961051344cc6b9fd28987025a99fa21568265444e7c38e8e367dfa38cb7a7c3e`.
The final corrected harness reproduces the full capture byte-for-byte.
The complete final audit takes 416.85 seconds; this is a debug verification
time, not a gameplay frametime measurement.

| Category | Passing checks |
| --- | --- |
| Complete result | 118,914/118,933 |
| Compile info | 12/12 |
| Layer order | 4/4 |
| Final render | 3,748/3,748 |
| Render persistence | 6,639/6,639 |
| Update values | 23,197/23,197 |
| Player ranges | 13/14 |
| Projected geometry | 65,402/65,418 |
| Projected vibration | 19,398/19,400 |
| Timeline | 224/224 |
| Message commands | 231/231 |
| Runtime modifiers | 46/46 |

Both initial pulse geometry failures are resolved. The total denominator,
geometry coverage and comparison tolerances are unchanged. Mawaru9 still
has 19 failed checks and 17 distinct reports: 16 near-camera-plane geometry
checks for the body projections and stars, two vibration samples and P2's
X range. Its complete test remains ignored and failing.

All 966 Lua/profile tests and 91 regular semantic tests pass in both
repositories. The corrected harness passes 90 unit tests, with one ignored.
The new production order-cache and proxy tests cover static/runtime sibling
order, owner visibility, wrapper visibility and an empty wrapper target.
No gameplay-time Lua execution or growing cache is added. Main receives
exactly one patch increment from 0.5.1729 to 0.5.1730. Rework stays
uncommitted, and the original song and ITGmania sources remain unchanged.

## Player position retention and vibration restarts (0.5.1731)

The remaining P2 range failure was a compiler reset error. The song moves
P2 from X=503.059375 to X=640.5, crossing the Simply Love baseline X=612.
The tail cleanup interpreted the smaller distance as an unfinished return
and replaced the explicit destination with 612. At beat 467.7, addx(133.4375)
therefore reached 745.4375 instead of native 773.9375. Linear transform
tails now retain destinations that cross the baseline. The existing
ordinary and cyclic return rules remain covered by the boundary test.
The native player-tail micro-fixture improves from 6/7 to 7/7 with the same
checks; it also asserts the exact resulting P2 maximum.

The two vibration failures were reference errors. Native Actor.cpp resets
the magnitude to (10,10,10) on every SetEffectVibrate call, while StopEffect
only clears the active effect. The semantic host incorrectly retained the
zero magnitude introduced by its StopEffect handling. The host now follows
both native rules. Actual native Actor commands validate custom strength,
stopping, restarting and subsequent overrides at five sample times. Their
capture repeats byte-for-byte. The vibration micro-fixture passes 38/38
full checks, plus 20 explicit native effect-state comparisons in DeadSync.

The refreshed full trace retains 17,930 update frames, 2,431 runtime actor
occurrences, 50 loaded Lua files, zero runtime errors and zero dropped
events. Its only data changes are two effect-chain magnitudes from zero
to (10,10,10); geometry, commands, tweens, timing and all sample counts are
unchanged. Its decoded SHA-256 is
`d0847e980a77118eeef360d4e200f2216a4c14595070b6d90b18f8fdddefe3be`.
The semantic host SHA-256 is
`3764b719dc1c23a517a503446ea5c47e9a954abc7137caed6dd3d9dfd93c03fa`.

The final complete Mawaru9 audit took 447.27 seconds and improves from
118,914/118,933 to **118,917/118,933**. Player ranges pass 14/14 and projected
vibration passes 19,400/19,400. Compile info, layer order, final render,
render persistence, update values, timeline, message commands and runtime
modifiers remain fully passing. Projected geometry remains 65,402/65,418:
16 failed checks with 14 distinct reports, all involving the previously
identified body/star projections close to the perspective plane.

Both repositories pass 967 Lua/profile checks and all 93 regular semantic
tests; 71 corpus/native audit tests remain ignored by the regular command.
The harness passes 91 tests with its local-corpus startup test ignored.
No comparison tolerance is widened and no check is removed. Only changed
DeadSync source, tests, fixtures and this document are copied to main.
Main receives exactly one patch increment, 0.5.1730 to 0.5.1731, including
Cargo.toml and Cargo.lock. Rework remains uncommitted. Mawaru9 and the
complete 63-chart goal remain unfinished.

## Native float tween interpolation (0.5.1732)

The near-camera failure has a native arithmetic component. Actor.cpp's
TweenState::MakeWeightedAverage uses RageUtil::lerp: a float subtraction,
multiply and addition, with rounding between operations. DeadSync instead
used explicit fused multiply-add for command properties and scheduled
scalar/vector tween values. That produces Z=85.55552673339844 at 101/60
seconds of the star's -700 to +700, three-second tween. Native Actor
produces Z=85.5555419921875. Under the 1.334375 parent depth scale and
FOV=150 camera, this one-ULP local-depth difference becomes a roughly
10-pixel projection difference near W=0.

Both production interpolation paths now use the native float operation
order. The replaced fused property/vector paths are removed. The semantic
host had its own independent error: Lua double interpolation. It now
calls native RageUtil::lerp through the harness's actor-math binding.
Tween queue scheduling and the semantic replay clock remain unchanged.
This does not claim to emulate accumulated native float delta-time drift
across arbitrary frame sequences.

A portable near-camera fixture contains one quad and one camera. Its
actual native C++ Actor capture has two samples and repeats byte-for-byte.
Against the corrected reference, the previous DeadSync executable fails
911/913 full checks; current DeadSync passes 913/913. The regression also
compares exact native depth bits at both samples and 16 projected corner
coordinates using the existing 0.75-pixel native drawing tolerance.
The scheduled scalar/vector path has an exact native-depth regression.
The harness independently checks native world depth and projected
coordinates as exact float bits, accounting only for JSON decimal
serialization. No comparison tolerance is widened or check removed.

The full reference retains 17,930 update frames, 2,431 runtime actor
occurrences, 50 loaded Lua files, zero runtime errors and zero dropped
events. Only projected vertices, runtime render-state samples and tween
state snapshots change: native float rounding affects 436 projection
tracks, 444 runtime actor records and 44 tween records. Projection
deduplication now retains 19,494 samples instead of 19,400; actor render
state has 18,530 samples instead of 18,531. Operation tracks, player
render tracks, actor definitions and the replay timeline remain unchanged.
Its decoded SHA-256 is
`5760ee840c82e154517f68e47e3d03ffff3c84db1592df1d13eee557b8d6ae82`.
The semantic host SHA-256 is
`b775fd560bc9bfe882647bfd3b2dc60b79dbffbbcac7ec1b1f9b021834613b8c`.
The near-camera semantic fixture's decoded SHA-256 is
`077594ae12b66ba3277cc044bd34bc460e7405a7ae6435f58dd8dbdf12e7b26a`.
The native Actor fixture SHA-256 is
`ab4d1c14c063542f819a58bde30e4eff723638cef7caf4088740aca9197d60a3`.

The complete Mawaru9 audit took 359.92 seconds and reports
**119,025/119,025**, with 0 failed checks. Projected geometry is
65,416/65,416. The previous total was 118,917/118,933. Float rounding
collapses 35 visible projection samples and adds 129 samples, including
three visible samples. This yields 94 more vibration checks and two fewer
geometry checks under the existing visibility/alpha/bounds comparisons.
All previously failing body/star sample times remain in the new trace.
The comparator code and tolerances are unchanged.

Both repositories pass 968 Lua/profile checks and all 94 regular semantic
tests, with 71 explicit corpus/native audits ignored by the regular
command. Both refreshed fixture manifest checks pass. The harness passes
92 unit tests and 31 integration tests, with one local-corpus startup test
ignored. Only changed DeadSync files are copied to main, which receives
exactly one patch increment, 0.5.1731 to 0.5.1732, including Cargo.toml
and Cargo.lock. Rework remains uncommitted.

A separate native drawing probe also exposes a reference coverage gap:
the semantic projection model records pulse effect descriptors but does
not apply pulse scaling to vertices. The actual native star's pulse
changes its world corners from [642.9908447, -3.1333351, Z] to
[610.8045654, -27.2542267, Z] at the same sample. That deterministic effect
geometry needs a dedicated production-renderer comparison and reference
correction. Passing the present comparators will therefore not close the
63-chart goal by itself. The full project remains active.

## Project scope

`tests/fixtures/itgmania-song-lua-project.json` preserves all 63 items from the
public project page, captured on 2026-10-03, including chart hashes and pack
names. Board check counts are historical worklist values, not current proof
of passing parity. Mawaru9 passes the current semantic comparators; native
deterministic effect geometry and the overall 63-chart audit remain active.
