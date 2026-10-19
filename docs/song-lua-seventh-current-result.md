# 7th Gear: compiled playback (0.5.1872)

## Independent native rendering reference

The harness needed correcting. Its existing authored-actor drawable channel
omitted native hold strips without song declarations and never observed Cel's
compiled Model geometry. Passing that channel did not establish note-rendering
parity.

Harness 0.1.94 now has a headless `--notefield-samples` mode. It observes actual
`NoteDisplay` submissions, model meshes (including skinning and bone transforms),
homogeneous projection anchors, native update/draw clocks and render-target IDs.
It uses a lightweight observer without wrapping authored functions or deriving
poses from their Lua writes. Missing meshes, anchors, target IDs, draw boundaries
or dropped samples invalidate the capture. A small native control verifies both
normal and reversed Position-spline caps. The ordinary semantic recorder remains
a separate capture product.

[seventh-notefield-native.json](../tests/fixtures/itgmania-song-lua-micro/seventh-notefield-native.json)
is the unmodified CLI export: Cel, M730, Mini60, single Challenge, autoplay,
default variable update clock and an offscreen window. It contains 1,556 actual
model draws and 60 native hold-strip submissions; the first right tap at beat
130.75 is observed on 32 distinct beats around the direction switch. Provenance
includes native executable, harness executable, observer, raw trace and simfile
hashes. Regenerate from `C:/GitHub/itgmania-harness`:

```powershell
target/debug/itgmania-harness-rs.exe song-lua-trace 'C:/GitHub/rework/lua-songs/7th Gear/7th Gear.ssc' `
  --itgmania-bin target/native-engine-source/Program/ITGmania-harness-trace.exe `
  --headless --notefield-samples --style single --difficulty Challenge `
  --fallback-theme 'Harness Simply Love' --player-options 'm730, 60% mini, cel' `
  --drawable-from-beat 130.2 --until-beat 130.74 --capture-fps 120 --timeout 180 `
  --out target/seventh-notefield-native.json
```

Do not add `--fixed-update-fps` to this comparison. A fixed actor delta does not
lock the native audio clock. The earlier, much heavier recorder could put those
clocks out of step. The same arrow is drawn both under the original Player
perspective (target zero) and into the AFT (a nonzero target, orthographic).
The compiled AFT regression must compare the matching native AFT submissions.

This exposed two rendering errors beyond the shared spline-clock fixes described
below. Absolute spline Y used screen center rather than the midpoint of the
native receptor metrics, shifting this chart by ten design pixels. Canonical
note models also applied a fabricated local perspective before entering their
actual camera, including orthographic AFT draws. They now preserve local XYZ and
use the inherited camera. These changes stay on the compiled gameplay path;
playback retains no Lua VM.

Some reported behavior is present in the independent native reference. Across
beats 130.4730 to 130.5069, the first right arrow's AFT pivot moves backward from
Y 328.866 to 320.875 while its scale falls from 0.7954 to 0.7305. The chart rebuilds
a cubic bend on a 0.125-beat knot grid; the first incoming note crosses the bend
while later notes use its straight part. Native Position-spline hold caps also
advance just one local tangent unit, and Cel's separate tail actor is blank.
Those observations must not be replaced with manually enlarged caps or a
monotonic-motion expectation. The regression additionally compares actual
projected model vertices, rather than only pivots or bounding rectangles.

The recorded executable is the project's pinned ITGmania
`1.2.0-BETA-git-5c73792` reference, not the installed stock 1.3 executable.
`NoteDisplay.cpp`, `CubicSpline.cpp`, `Model.cpp`, `ActorProxy.cpp`,
`ActorFrame.cpp` and `RageTextureRenderTarget.cpp` are unchanged between that
revision and the local `v1.3.0` tag; the relevant Player draw math is unchanged.
This is focused geometry and clock coverage, not whole-song pixel certification.
No full fixture corpus was run. Native captures for this reference are headless.

Validation: all 32 native AFT first-arrow frames pass through the real compiled
three-AFT path, together with the steady receptor and normal/reversed hold
probes. Position tolerance remains four design pixels, absolute scale tolerance
0.021, and normalized projected-vertex error 0.2 model units. Nine spline tests,
21 model tests, 44 native-vector/composition tests (including the native cap and
spline observations), and the signed global-offset regression pass. Harness
validation, CLI/config checks and the native normal/reverse control pass.
`target/debug/deadsync.exe` was rebuilt successfully; it was not launched.

## Earlier investigations

## Native scroll-swap clock follow-up

The previous checks did not establish consecutive-frame parity for the first
incoming arrow. A native ScreenGameplay run now retains 129 spline observations
and the actual drawn cap rows around beats 114.5 and 122.5. It uses the unchanged
Challenge chart, Cel, M730, Mini60, 1x rate, autoplay and seed 1. The executable,
song, Lua and observer hashes are in
[seventh-swap-native.json](../tests/fixtures/itgmania-song-lua-micro/seventh-swap-native.json).
The [observer](../tests/fixtures/itgmania-song-lua-micro/seventh-swap-native-probe.lua)
runs only in an isolated native harness install, with the end beat set to 123.25
and those player options applied before entering gameplay. Installed songs and
profiles are untouched.

Two timing errors remained in compiled playback:

- The captured music origin already included the global offset, but playback
  subtracted the offset again. Controls therefore reached a spline bend on a
  different beat from the note sampling them. The origin is now applied once.
- Position and hold tangents still used a frozen capture with the current beat
  while zoom blended between captures. Incoming-note positions and hold paths
  now blend evaluated poses using the same reference clocks as zoom. Receptor
  positions and pinned hold heads retain their captured `SetPoint` anchors;
  interpolating these jumps invented a sweep across the middle of the field.
  Sparse intervals and enable/disable boundaries keep their existing steps.
  Reference beats use the native float clock.

The shell fixture also compiled with the cabinet's default -8 ms offset while
its chart and playback used zero. The fixture now supplies zero consistently
and finalizes judged rows when recreating autoplay visibility. Setting only
individual note results had left past arrows in the earlier GPU probes.
The offset regression covers positive, negative and zero offsets, including the
fallback origin. The inter-frame regression covers position, tangent, zoom,
held heads and receptors at 240 Hz and three playback rates.

The native cap result is unexpected but explicit: both normal and reversed
Position-spline caps advance only one local tangent unit. ITGmania's
`NoteDisplay::DrawHoldPart` fixes their beat and clears ArrowEffects Y. Cel's
separate HoldTail actor is blank. The flat caps in this section therefore also
occur in this reference build; enlarging them would change native behavior.
The cap regression compares both directions against the recorded native rows.

The actual-chart check now includes 21 consecutive native incoming-arrow poses
after the two direction switches, isolating each tap through the real AFT chain.
It checks position and relative scale rather than just visible pixels or body
width, and rejects receptor positions between the two native anchors. The
incoming-origin tolerance is four design pixels and the relative-scale tolerance
is 0.03, allowing different native and compiled update phases. These remain
focused 60 Hz compiled-playback checks; arbitrary native
update phases and whole-song pixel identity are not certified.

Validation: nine focused spline tests, both native observation tests, the signed
offset regression, and all 33 actual-chart Vulkan GPU probes pass. The current
application is rebuilt. Gameplay preparation retains no Lua VM. No full fixture
corpus was run.

## Earlier scroll-swap note and hold geometry

Two remaining rendering errors were outside the earlier receptor-position
checks. They affect the compiled path even when its spline data is correct:

- Zoom controls were captured at 60 Hz but evaluated against the current
  rendering beat. That mixed two clocks: a note's scale could fall on one
  render frame, then jump back when the next captured controls arrived.
  Compiled zoom playback now evaluates each adjacent reference with its own
  song beat and interpolates the resulting scale. Held heads and receptors
  use their corresponding reference beats too. Sparse intervals and spline
  enable/disable boundaries retain their authored steps. Position splines
  keep their existing direction-swap boundaries.
- Hold cross-sections always added ArrowEffects' +Y direction to the normalized
  spline tangent. ITGmania's `NoteDisplay.cpp` does this for Offset mode, but
  removes that contribution for Position mode. On a straight reversed absolute
  path the two directions cancelled, collapsing the body and cap widths.
  Hold samples now retain the spline mode and use the native basis in both
  directions, including the cap's forward step. Absolute caps also discard
  their extra screen-Y offset: native `DrawHoldPart` fixes the cap beat and
  clears ArrowEffects Y before advancing along the tangent.

The clock regression models native linear spline evaluation
`z(t,b) = 1+t+(b-t) = 1+b`. The previous renderer fails at the first 240 Hz
intermediate frame (1.9958334 instead of the constant scale 2), reproducing the
alternating-frame scale error. It now checks stable incoming-note size,
smooth pinned-head/receptor size, and sparse/disabled boundaries.
The native CubicSpline oracle independently returns scale 2 at all four control
clocks in [zoom-clock-native.json](../tests/fixtures/itgmania-song-lua-micro/zoom-clock-native.json).
Its [request](../tests/fixtures/itgmania-song-lua-micro/zoom-clock-request.json) can be
replayed with `itgmania-harness-rs actor-conformance`.

Focused regressions also cover normal/reversed absolute hold geometry, cap
positions and direction, and Offset mode's existing native reference vectors.
Full field composition checks straight absolute holds and rolls
under both ordinary scroll directions and several field zooms. The actual-chart
test additionally searches incoming chart notes at every compiled reversal
frame for the smallest scale and isolates active holds in both spline directions
through the real three-AFT chain. Optional Vulkan GPU captures are enabled with:

```powershell
$env:DEADSYNC_SEVENTH_CAPTURE = '1'
cargo test --locked -p deadsync-shell --lib seventh_saved_mini_reaches_aft_receptors -- --ignored --nocapture
```

The eight focused spline tests and 43 hold tests pass (three tests belong to
both filters). The M730/Mini60 actual-chart regression also passes all 12 GPU
probes, with visible Cel notes, full-width active holds in both directions,
and the expected receptor positions through the three-AFT chain. Preparation
retains no Lua VM. `target/debug/deadsync.exe` was rebuilt successfully.
No full fixture corpus was run. These are targeted clock and geometry checks,
not a whole-song native pixel comparison.

## Earlier profile Mini repair

Production gameplay compiles song Lua during loading and releases the VM before
playback. The preparation API no longer accepts a live-mode flag; normal entry,
preload, and practice all use compiled data. Explicit live scenes remain a
headless diagnostic backend.

The saved Mini option now reaches Lua before song construction. Previously the
renderer used the profile's 60% Mini while `PlayerOptions:Mini()` returned zero
to the song. At M730, 7th Gear therefore built 10 reverse-path knots instead of
15. Future notes clamped to the last knot at logical Y=37.79, near the score;
native ITGmania places that endpoint above the screen at Y=-86.5484. The reversed
receptor was also at Y=334.5 instead of Y=375. That constructor repair alone
did not fix gameplay: the compiled modifier runtime still added the profile
Mini to every authored Mini amount.

The updated actual-chart check reproduced that second bug at beat 112 with
M730/60% Mini: Current Mini=119.986176%, field zoom=0.40006912,
receptor Y=162.84381, and spline endpoint Y=426.63132. Future notes clamped at
that visible endpoint form the reported feeding row. Reversing mirrors the
same placement error. ITGmania's native option control confirms that Song and
Current Mini setters, and `FromString`, assign absolute amounts. Compiled
constants, direct eases and approached Song targets now follow that behavior.

The original 31 direction/modifier checks still passed with that incorrect
path, and its runtime modifier loop used a zero-Mini base. The focused
`saved_mini_reaches_song_lua_constructor_and_reverse_path`
regression now covers constructor reads, all four option levels, P1/P2 profile
mapping, compiled setter playback using the saved profile, and sample positions against the retained
[native PlayerOptions and CubicSpline controls](../tests/fixtures/itgmania-song-lua-micro/profile-mini/README.md).
The actual-chart checker now advances the real compiled modifier windows with
the selected profile, checks Current Mini, and verifies receptor and endpoint
positions in both directions. A separate focused shell test composes the real
three-AFT branch and checks the final receptor sprite coordinates. The checker
accepts optional MMod/Mini arguments:

```powershell
cargo test --locked -p deadsync-profile-gameplay saved_mini_reaches_song_lua_constructor_and_reverse_path
cargo test --locked -p deadsync-gameplay mini
cargo test --locked -p deadsync-shell --lib seventh_saved_mini_reaches_aft_receptors -- --ignored --nocapture
cargo run --locked -p deadsync-shell --example seventh_compiled -- 'C:\GitHub\rework\lua-songs\7th Gear\7th Gear.ssc' target/seventh-mini60.json 730 60
```

These checks cover the profile-dependent path; a passing semantic replay alone
does not establish final gameplay pixel parity for other player options.
The updated M730/60% Mini production check passes all 34 observations and retains
no Lua VM. Its final pulse sample leaves Current Mini=59.986176%, matching the
compiled 60 Hz sampling. At beat 115 the actual runtime gives receptor
Y=375.01334 and endpoint Y=-86.58063; at beats 112 and 123 the receptor is
Y=104.986664 and the endpoint is Y=566.5806. The native-backed profile
regression and all 17 focused Mini tests pass. The final AFT composition test
also passes: all four receptor sprites project to the authored positions at
beats 112, 115 and 123 through the three-texture chain. The normal application
at `target/debug/deadsync.exe` was rebuilt successfully; no full fixture corpus
was replayed for this repair. These checks cover final draw coordinates, not a
whole-song native pixel comparison.

The before/after values and chart/source hashes are retained in
[`seventh-mini-runtime.json`](../tests/fixtures/interactive_song_lua/seventh-mini-runtime.json).

## Earlier compiled-path audit

The live prototype dropped note-column position and zoom splines when transferring
its scene into gameplay. That explains the missing reversal behavior beginning
at beat 111.5. The compiled path already retains those immutable spline tracks.
A regular headless production regression checks direction reversal, zoom changes,
reset to normal, and release of the compiler VM before playback.

The actual-chart check uses the normal production preparation API and samples
all four Rust spline tracks through both reversal sections. All 31 checks pass:
direction changes at beats 112, 115 and 123, reset at beat 175, lane reversals
at beats 206.75 and 207.5, and the handoff to ordinary Reverse at beat 208.
All three AFT stages are retained and the prepared output contains no VM.
Load-time compilation took 90.26 seconds in the expanded check (78.74 seconds
in the earlier run); the loading delay remains unresolved.

The Reverse check advances Rust playback in 1/120-beat steps before inspecting
the result. Song-level targets approach the previous frame's sampled value;
quarter-beat stepping incorrectly under-sampled the transition and reported
0.404 at beat 207.5. Frame-sized steps report 0.721 against the authored 0.75
target (0.05 tolerance allows the compiler's 60 Hz samples), followed by 1.0
at beat 208.5. This checks authored behavior, not a native pixel comparison.

```powershell
cargo run --offline --release --config 'profile.release.lto=false' -p deadsync-shell --example seventh_compiled -- 'C:\GitHub\rework\lua-songs\7th Gear\7th Gear.ssc' target/seventh-compiled/production-track-check.json
```

The retained observations and source hashes are in
[`seventh-compiled-reversal.json`](../tests/fixtures/interactive_song_lua/seventh-compiled-reversal.json).
Five headless notefield spline tests also pass, covering composed notes,
receptors, hold orientation, zoom assignment, and track changes/rewinds.

Both normal development and optimized production executables are rebuilt. The
optimized build is also available at `target/debug/deadsync-seventh-compiled.exe`
alongside the installed song assets. It uses compiled playback; the older live
prototype binaries described below are superseded. No application window was
launched for this verification, and full application FPS was not measured.

The retained older compiled archive run passes all 2,304 column-spline checks,
but still has two final ActorProxy visibility mismatches (881,665 passing
comparisons). It does not establish whole-song rendering parity. The historical
live CPU measurements and binaries below are superseded and do not represent
the production execution model.

## Earlier live prototype investigation

The previous application check covered startup visibility only. It did not
establish correct effects after startup or acceptable frame time. Its recorded
79.4 ms average frame time was already a performance failure.

The old `--headless --drawable-samples` reference used the null renderer. 7th
Gear checks `VideoRenderers` and disables its AFT branch for that renderer. That
capture contained two actor definitions and zero drawable samples. It cannot
validate the effects seen during gameplay. Live replay also hardcoded that
renderer preference; it now retains the capture's actual preference list.

The harness now supports hidden OpenGL contexts on Windows. It refuses to launch
headless graphics against an older adapter without that capability, and validates
the native window visibility observation. The new capture records
`headless_graphics=true`, `window_visible=false`, 485 actor definitions, three
AFTs, 120,966 drawable samples, 700 update frames through beat 24.035, and zero
dropped events or drawable samples. No visible game is needed for this workflow.

The observed opening strip poses are retained in
[`seventh-headless-opening-poses.json`](../tests/fixtures/interactive_song_lua/seventh-headless-opening-poses.json).
The fixture preserves actual native frame clocks and observed setter values;
it does not supply expected results to the live Lua VM. Run the unchanged song
through the headless regression/profile example:

```powershell
cargo run --offline --release --config 'profile.release.lto=false' -p deadsync-song-lua --example seventh_profile -- 'C:\GitHub\rework\lua-songs\7th Gear' tests/fixtures/interactive_song_lua/seventh-headless-opening-poses.json
```

At low frame rates the short opening wave can retain a large distortion: the
script applies the previous wave sample before evaluating easing callbacks, then
clears its horizontal wave list at completion. Missing the final decay samples
leaves the last applied transform. The slow application retained HStrip1 at
x=247.80, skew=0.599 after the first effect. The native and live strip transforms
agree when driven by the same native frame clocks. The song is unchanged.

The live bridge was republishing an unchanged full tween pose for every scalar
setter and idle actor, then reading the complete scene again during Draw even
without authored draw callbacks. These redundant copies and allocations are
removed. Scalar reads and capture resets batch VM access. Traversal timing
scratch stays in Rust, and private callback/rate reads bypass public method
lookup. Tests retain coverage of queued tween ownership, callback order, and
custom Draw.

Using the same development profile and the same 645 recorded frame clocks, the
Lua-only workload fell from 65.87 ms to 16.54 ms per frame (P95 from 96.35 ms to
20.89 ms). The optimized headless run averages 12.74 ms (P95 19.68 ms) over the
new 700-frame capture, with 10,148 opening-pose comparisons passing. These are
component timings, not measurements of full application FPS.

The full Lua suite records 646 passes and 15 failures. An older binary records
640 passes and the exact same 15 failures, with no newly introduced failures.
Current scalar/getter, update rate, callback, hibernation and custom Draw checks
pass, as do all 14 harness runtime tests. The native drawable comparator still
reports AFT viewport/startup-phase differences; full rendering parity and
whole-song acceptance remain unproven.

The normal executable and an optimized build are rebuilt without launching a
visible application. The retained headless receipt records build hashes and
measured scope in
[`seventh-headless-result.json`](../tests/fixtures/interactive_song_lua/seventh-headless-result.json).

## Previous startup proxy repair

Foreground `OnCommand` runs after the application prepares its visual slots.
7th Gear binds its NoteField, Combo, Judgment, Underlay and Overlay proxies there.
The retained Lua actors had the targets, but presentation kept the unbound
startup targets, leaving only the captured background visible.

Live visual writes now refresh proxy targets and resolve local actor identities
to their own visual layer. Source visibility follows live retargeting. Screen
entry also reserves enough direct-proxy destinations for a late whole-Player
binding, whose three source runs can exceed an initially unbound slot.

The new regression fails before the fix and passes afterward. It checks the
actual prepared visual slot after foreground activation, then retargets it to
a hidden local actor and a hidden screen layer. Fifteen distinct focused proxy
and foreground checks pass.

The normal Windows application reproduces the missing playfield before the fix.
The rebuilt application shows all four receptor arrows and the HUD through
7th Gear's three ActorFrameTexture stages. It completes 120 warmup and 120 measured
frames on Single/Challenge, chart hash `477fed829d289df2`, with gameplay preparation
at 1.243 seconds. A separate Single/Expert Fof Guys application check also
completes and shows its course, ball and countdown. The installed songs, sync
and user profiles are unchanged. Both checks use isolated data directories.

Hashes, screenshots and application results are recorded in
[`seventh-windows.json`](../tests/fixtures/interactive_song_lua/seventh-windows.json).
These checks establish startup proxy visibility only. They did not cover the
opening effects or certify development-build performance or native pixel parity.

## Earlier semantic result (0.5.1800)

The frozen project chart `7th Gear/7th Gear.ssc` passes **1,410,424/1,410,424** existing
semantic and runtime-modifier checks. Pin hash `477fed829d289df2`,
dance-single Challenge, description `FS+ XO BR- BU- MODS`,
and seed 1.

The original completion audit reports 228,018/228,019 against
the superseded reference. The earlier local semantic capture passes
without production changes, comparator changes or tolerance changes. The
reference records the current native float beat and countdown behavior;
delete the superseded fixture and retain every emitted observation.

Passing sections:

```text
  compile info         4/4 ok
  layer order          2/2 ok
  final render         450/450 ok
  sprite textures      1/1 ok
  column splines       4584/4584 ok
  projected geometry   235075/235075 ok
  draw colors          469784/469784 ok
  draw crops           235624/235624 ok
  draw shadows         353436/353436 ok
  projected vibration  58906/58906 ok
  runtime modifiers    52558/52558 ok
```

Retain 7,815 update frames, ending at beat
238.75 and 130.22727272727272 seconds,
with zero Lua errors and zero dropped events. Raw capture SHA-256:
`662382dcb034b156b5bb2f0a3dc1dd0b57ae6032da39e7e1a0fda3575b743277`. Compression has an exact verified byte round trip.
The manifest records simfile, Lua, noteskin, harness and executable
provenance. No songs, supplied resources or frozen project data are edited.

The direct current-trace comparison and complete pinned corpus test pass
in MAIN's isolated checkout. This preserves the existing semantic and
modifier scope, not whole-game pixel parity. No downloads or visible
ITGmania instance. Both Cargo files bump exactly once from 0.5.1799
to 0.5.1800.
