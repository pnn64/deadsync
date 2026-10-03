# Mawaru9 compatibility checkpoint

Updated 2026-10-03. Song: `lua-songs/mawaru9/mawaru9.sm`, dance-single
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
`123fdd5c599807f307d133aa66b4b1edfe327495de88e09db636072fb0ee419d`.
Compression was checked with an exact byte-for-byte round trip.

Regenerate from the workspace root after a relevant oracle or song change:

```powershell
$env:ITGMANIA_SONG_LUA_NOTESKIN_ROOT = (Resolve-Path deadsync/assets/noteskins).Path
$env:ITGMANIA_SONG_LUA_NOTESKIN = 'cyber'
cargo run --release --manifest-path itgmania-harness-rs/Cargo.toml -- song-lua-semantic-baseline lua-songs/mawaru9 --out .tmp/mawaru9-native-instances --beat-step 0.25 --max-events 2000000
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
- Semantic harness checks: 73 regular tests pass in both repositories. The
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
contain `allowed/` and `lua-songs/`; all 73 current tests pass with the existing workspace data.

## Complete song audit

After these fixes: **106,321 / 117,464 checks pass (90.51%)**. The ignored
full-song test still fails, correctly identifying the remaining gaps.
Fixture status `ok` describes the complete native capture, not a passing
DeadSync comparison.

| Comparator | Passed / total |
| --- | --- |
| Compile info | 12 / 12 |
| Layer order | 4 / 4 |
| Final render | 3,566 / 3,748 |
| Render persistence | 6,507 / 6,587 |
| Update values | 23,065 / 23,197 |
| Player ranges | 13 / 14 |
| Projected geometry | 53,275 / 64,018 |
| Projected vibration | 19,385 / 19,387 |
| Timeline | 223 / 224 |
| Message commands | 225 / 227 |
| Runtime modifiers | 46 / 46 |

The former Sprite/Model ordering difference came from unpinned noteskins;
all four layer-order checks pass with Cyber. The complete capture and
instance-aware comparison still report the remaining gaps rather than
changing expected values or omitting checks. The native capture is unchanged.
The previous result was 67,430/88,316; more visible state and runtime message
effects now enter the comparison, so the totals differ. Remaining failing
checks are now 11,143, with 1,025 detailed gap reports. The immediately
preceding audit used the same 117,464 checks and passed 104,988; the affine
fix adds 1,333 passing geometry checks without changing the reference.

The queued `Start -> SpawnPlayers -> SetControlling` sequence now records
`BodyRotateBuildings` at its dispatch beat near 89.701, rather than the
trigger beat near 87.050. Its state changes are now deferred as well. Remaining
gaps include trail opacity/visibility, projected sprite bounds, one player
range, two vibration mismatches near beat 104.012, the `ChanceTime` broadcast near
beat 540.076 and stateful `ToshiUp` and `TVGrow` target writes.

Local detailed audit output: `.tmp/mawaru-parent-rotation-full.log` at the workspace
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

## Project scope

`tests/fixtures/itgmania-song-lua-project.json` preserves all 63 items from the
public project page, captured on 2026-10-03, including chart hashes and pack
names. Board check counts are historical worklist values, not current proof
of passing parity. Mawaru9 remains failing; the overall 63-chart goal is
still active.
