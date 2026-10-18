# 7th Gear: compiled playback (0.5.1872)

Production gameplay compiles song Lua during loading and releases the VM before
playback. The preparation API no longer accepts a live-mode flag; normal entry,
preload, and practice all use compiled data. Explicit live scenes remain a
headless diagnostic backend.

The saved Mini option now reaches Lua before song construction. Previously the
renderer used the profile's 60% Mini while `PlayerOptions:Mini()` returned zero
to the song. At M730, 7th Gear therefore built 10 reverse-path knots instead of
15. Future notes clamped to the last knot at logical Y=37.79, near the score;
native ITGmania places that endpoint above the screen at Y=-86.5484. The reversed
receptor was also at Y=334.5 instead of Y=375.

The original 31 direction/modifier checks still passed with that incorrect
path. The focused `saved_mini_reaches_song_lua_constructor_and_reverse_path`
regression now covers constructor reads, all four option levels, P1/P2 profile
mapping, and compiled sample positions against the retained
[native PlayerOptions and CubicSpline controls](../tests/fixtures/itgmania-song-lua-micro/profile-mini/README.md).
The actual-chart checker also verifies that receptor positions agree with the
selected Mini and accepts optional MMod/Mini arguments:

```powershell
cargo test --locked -p deadsync-profile-gameplay saved_mini_reaches_song_lua_constructor_and_reverse_path
cargo run --locked -p deadsync-shell --example seventh_compiled -- 'C:\GitHub\rework\lua-songs\7th Gear\7th Gear.ssc' target/seventh-mini60.json 730 60
```

These checks cover the profile-dependent path; a passing semantic replay alone
does not establish final gameplay pixel parity for other player options.
The M730/60% Mini production check passes all 31 sampled reversal observations,
retains no Lua VM, and confirms 15 knots, receptor Y=375, and endpoint
Y=-86.54837 at beat 115. The native-backed profile regression and the focused
notefield composition regression also pass. The normal debug application was
rebuilt successfully; no full fixture corpus was replayed for this repair.

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
