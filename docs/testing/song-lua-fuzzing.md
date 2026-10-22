# Generative song Lua differential testing

The fuzzer generates bounded song folders and compares ITGmania observations
with deadsync's **compiled** song Lua. Lua runs during song loading; the resulting
tracks and draw data are evaluated without a retained Lua VM. The native mode
uses the production context builder and `playback::prepare_song_lua`, with the
capture's external clocks supplied to the offline compiler. The runner rejects any layer
that retains a VM. There is no live replay option in this fuzzer.

## Run

Python 3.11+, Cargo, and the built sibling `../itgmania-harness` are required.
From the deadsync workspace:

```powershell
python scripts/fuzz_song_lua.py --count 1000 --seed 42 --out target/fuzz-compiled
```

The default native oracle launches the instrumented ITGmania engine with an
invisible graphics context, fixed 60 Hz updates and eight-beat capture. The
harness stages an isolated installation. Defaults point to the sibling release
harness, `target/native-engine-source/Program/ITGmania-harness-trace.exe`, and the
`Harness Simply Love` theme. Override them with `--harness`, `--itgmania-bin`,
and `--fallback-theme`.

For faster compiled semantic checks:

```powershell
python scripts/fuzz_song_lua.py --oracle semantic --count 1000 --seed 42 --out target/fuzz-semantic
```

This oracle uses ITGmania's bundled Lua and the harness's modeled actor host;
it is not execution of native Actor objects. It checks compiled transforms,
commands, modifiers and spline tracks through the existing semantic comparator.
Its sample interval defaults to 1/48 beat. Both sides share the workspace cyber
noteskin assets, whose hashes are checked.

Both modes build `song_lua_itgmania_semantic_parity` once. `--runner PATH` accepts
that prebuilt test executable. `--generate-only` writes cases without running
either engine. `--timeout SECONDS`, `--max-events`, and `--fail-fast` bound runs.
Use a new output directory for every run; existing results are never overwritten.

## Seeds and coverage

A generation seed reproduces inputs; different seeds explore different programs.
One fixed seed is useful for regression testing, but is not sufficient exploration.
A separate saved Lua seed controls randomized Lua branches. Native broadcasts
visit subscribers in pointer order, so native comparison also supplies the
capture's neutral actor creation identities to reproduce subscriber order.
A seed alone cannot determine which subscriber receives each random value.
Sharing that seed does not align native vibration's random draw stream with deadsync's hashed
vibration. New strict-equality cases exclude vibration and list it as a coverage
gap requiring a bounded-motion or statistical oracle. Other effects remain enabled.

Version 2 generates:

- 19 transform families: position/addition, rotation/addition/base rotation,
  heading/pitch/roll, axis/base zoom, size/zoom-to, stretch/fit/cover, both skews,
  numeric/named alignment, all four crops and fades. Selected transforms are
  also applied during tweens with changed targets.
- 13 tween choices: linear, accelerate, decelerate, spring, their enum forms,
  one- and two-dimensional Bezier, bouncebegin, bounceend and smooth; zero/short
  durations, sleep, queues, stop/finish and hurry.
- Diffuse, alpha, glow, inherited colors, corner/edge colors, diffusecolor and
  basealpha. Unsupported engine implementations become explicit failures.
- Pulse, spin, wag, bounce, bob, diffuse/glow blink/shift/ramp, rainbow and stopped
  effects; timer, beat, music and offset-free clocks.
- Quads, textured Sprites and nested ActorFrames; child ordering; Init/On, queued
  commands and broadcasts; closures, loops, seeded branches and bounded callbacks.
- Selected global/per-column modifiers, signed percentages, column transforms,
  position splines, BPM changes, stops, delays and beat-boundary changes.

The root sleeps for 12 seconds to keep the foreground alive through the capture.
Without this, native Foreground hides the entire animation as soon as the startup
queue expires, preventing later effects and callbacks from being observed.

This is **not all song Lua APIs or all combinations**. `summary.json` includes
`api_coverage`: authored method names checked against the pinned native class
inventory, every class's methods not authored, and explicit exclusions. Method
name presence is not proof that a branch ran, that a class used it, or that it
passed. `scripts/song_lua_fuzz_catalog.py` owns the generated families.

The pairwise strategy measures coverage of pairs of factor values, then continues
with random rows. `--strategy random` skips the covering prefix. Neither covers
all numeric arguments or higher-order combinations. Generated, compared and
passing pair coverage are reported separately. `--start N` produces reproducible
shards; requesting a longer campaign preserves its prefix.

## Comparison scope

Native mode validates the capture, compiles via production preparation, then
samples the compiled actor tracks at recorded frame clocks and uses the shared
renderer to compare per-actor submissions: pass/vertex counts, clip coordinates,
UVs, colors, texture identity, sampler and depth state. Diffuse/glow submissions
are compared separately. Identical-state native fade quads may match one batched
mesh, with every quad matched once and all vertices still checked. Colors use native RGBA8 quantization without widening
geometry tolerances. Texture identities resolve Windows path spellings. An offscreen native draw can match a culled compiled
draw when all its vertices lie outside one clip plane. Extra or missing visible
passes fail. Missing captures and runtime/compile errors never count as passes.

The native music clock and screen update delta are separate inputs:
`--fixed-update-fps` fixes the latter only. ITGmania's `Foreground::Update` uses
music time since the layer's start beat, then successive music-time differences.
The diagnostic `test-support` context supplies those clocks and neutral actor
identities to compilation. Draw results and actor states are never compiler inputs. Raw music time is converted
to elapsed song time for command playback, while rendering retains raw music
time. Normal production compilation still uses its 60 Hz reference schedule;
this comparison establishes equality for the recorded input schedule, not
independence from update rate or arbitrary runtime clock jitter.

The native check does not yet cover framebuffer pixels, cross-actor draw order,
blend/cull state, or native modifier/notefield output. Use the semantic mode for
compiled modifier and spline checks, while retaining that oracle's limitation.
Runtime Lua call sequence equality is not the contract for compiled playback.
A passing report establishes only its listed observations, not full gameplay
or pixel parity. Mutation fuzzing, automatic minimization, more actor classes
and assets, and the remaining API families are future work.

## Results and reproduction

Every case retains `case.json`, `default.lua`, `song.sm`, a PNG and silent WAV.
Results contain native/semantic traces, subprocess logs, `deadsync.json`, source
and executable hashes, commands, environment inputs, and `summary.json`.
The console prints failure categories and a report/log path.

```powershell
python scripts/fuzz_song_lua.py --replay target/fuzz-native/case-000001/case.json --out target/fuzz-recheck
```

Replay executes the saved source unchanged, including version 1 cases, using the
new compiled comparator. Version 2 changes generated inputs for a given campaign
seed. Old version 1 captures/results describe the former live diagnostic runner
and must not be treated as compiled parity results.

Statuses distinguish mismatches from oracle/compiler errors, incomplete captures,
and timeouts. Timeouts terminate the process tree; Ctrl+C does likewise. Reports
are written incrementally. Mutation fuzzing is deliberately separate.

## Checks

```powershell
python -B -m unittest discover -s scripts -p test_fuzz_song_lua.py -v
cargo test --test song_lua_itgmania_semantic_parity --no-run
```

The generated Rust test is ignored unless explicitly invoked by the Python
runner with trace, simfile, oracle and report environment variables. Its pass
requires `execution_path=compiled-song-lua` and `retained_lua_vm=false`.

Validation of this revision: seed-42 native cases 0 through 24 pass against
saved captures; cases 13, 16, and 23 also pass fresh native execution. All runs
use compiled playback with zero draw mismatches and no retained Lua VM. The 19
targeted Rust regressions pass, including named/text alignment inside queued
tweens, queued rotation writes with active or stopped spin, and immediate
same-frame callback writes.

The fixes follow native `Actor.cpp`, `Actor.h`, `RageMath.cpp`, and `Sprite.cpp`:
size/alignment/basealpha setters bypass tween destinations; heading/pitch/roll
accumulate a separate quaternion; offset-free beat clocks exclude global offset;
glow replaces vertex colors; queued rotations change the destination without
overwriting the current spin snapshot; and fade quads may be batched without
discarding geometry. The comparator still rejects changed vertices, missing/duplicate quads,
and batching across different native render states.

An earlier 1,000-case generation run authored 113 method names and all 3,239
catalog pairs. This was generation, not a 1,000-case engine parity run. Passing
these 25 native cases does not establish complete Lua API or pixel coverage.
