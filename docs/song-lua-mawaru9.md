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

- Harness: 119 regular tests pass, including separate instances, unique IDs,
  parent-specific draw order, chart identity and Init queue regressions.
- DeadSync Lua and profile gameplay: 961 tests pass. The real repository also
  passes all 942 Lua crate tests using its own fixture files.
- Model parser: 19 tests pass, including separate files with a materials
  directory and a bone rotation animation.
- Selected corpus coverage passes with Mawaru9 registered.
- Semantic harness checks: 68 regular tests pass. The shared-actor native
  regression passes all 20 checks, including each instance's final alpha and
  projected position. It failed before the loader and verifier fixes.

## Complete song audit

After these fixes: **67,429 / 88,316 checks pass (76.35%)**. The ignored
full-song test still fails, correctly identifying the remaining gaps.
Fixture status `ok` describes the complete native capture, not a passing
DeadSync comparison.

| Comparator | Passed / total |
| --- | --- |
| Compile info | 12 / 12 |
| Layer order | 4 / 4 |
| Final render | 3,477 / 3,748 |
| Render persistence | 3,529 / 3,917 |
| Update values | 18,611 / 23,197 |
| Player ranges | 13 / 14 |
| Projected geometry | 21,930 / 37,540 |
| Projected vibration | 19,383 / 19,387 |
| Timeline | 209 / 224 |
| Message commands | 215 / 227 |
| Runtime modifiers | 46 / 46 |

The former Sprite/Model ordering difference came from unpinned noteskins;
all four layer-order checks pass with Cyber. The complete capture and
instance-aware comparison still report the remaining gaps rather than
changing expected values or omitting checks.

Start further investigation with the queued `Start -> SpawnPlayers ->
SetControlling` sequence in `lua/body/default.lua`: DeadSync is missing the
resulting `BodyRotateBuildings` message near beat 89.701. The trail actors
`def-0227` through `def-0258` finish transparent in ITGmania and opaque in
DeadSync; their positions and visibility also differ during play. Other
remaining gaps include gameplay message sequences such as `MawaWrongP1/P2`,
`KillPatient3P1/P2` and `AndersDieFrontP1/P2`.

Local detailed audit output: `.tmp/mawaru9-instance-parity-final.log` at the workspace
root. Rerun the per-song command above to reproduce every comparison. No
reference song files or ITGmania source files were modified.

## Project scope

`tests/fixtures/itgmania-song-lua-project.json` preserves all 63 items from the
public project page, captured on 2026-10-03, including chart hashes and pack
names. Board check counts are historical worklist values, not current proof
of passing parity. Mawaru9 remains failing; the overall 63-chart goal is
still active.
