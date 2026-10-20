# Song Lua execution audit â€” 2026-10-19

## Current execution policy

The source-verified FOF adapter described below has been removed. It was a
handwritten implementation of that program, not a general Lua compiler.
Changing even a comment in a verified system failed its source check; changing
the wrapper could silently fall back to a non-interactive sampled scene.

Production `gameplay_entry::prepare_song_lua` now prepares actors and resources
on the loading worker and transfers the shared Lua scene to gameplay. Lua owns
the authored update functions, delayed commands, input callbacks, state,
physics, controls, timers, resource paths and navigation requests. Screen
replacement preserves the VM and arbitrary Lua globals. No chart identity,
source hash, ECS layout or game-specific constants select the implementation.
This follows ITGmania's `ActorFrame::UpdateInternal` and
`Screen::PassInputToLua`, which invoke the authored Lua functions at runtime.

Production uses this scene for all song scripts: a program can register input
callbacks after an arbitrary delay, so startup inspection cannot safely prove
that it will remain non-interactive. The explicit offline compiler remains
available for deterministic timeline tools. It rejects encountered input
registrations instead of reporting successful compilation after dropping them.

Actor `Sound:play`, `pause` and `stop` emit ordered operations with actor
identity and Lua-resolved paths. Voices are controlled independently even when
two actors use the same file. An overlapping play follows native RageSound's
detached-copy behavior. Runtime-selected files can be loaded if not prewarmed.

The focused headless replay mode is now `--production`, reporting
`production-live-lua` and `retained_lua_vm=true`. Diagnostic replay remains
separately labelled. Existing native captures and expected observations are
unchanged; the older `--compiled` reports below describe the removed adapter.

The initial runtime migration missed two `test-support` gates in the theme:
the application rejected scene attachment and skipped the pre-player Lua
frame phase. The native replay enabled that feature through dev-dependencies,
so its passing result did not establish that the normal application could
start the scene. Both paths now share attachment and frame preparation.

The separate `song_lua_execution` smoke binary exercises production preparation,
chart selection, gameplay initialization and 241 updates (four seconds). Run
it as a shell binary, not a test or example, to exclude dev-dependency feature
unification. It fails when the screen drops the prepared scene, callbacks
fail, or visuals do not change on multiple frames, and reports actor geometry
and authored audio operations. This smoke check targets scenes that animate
within their first four seconds. It
does not open a window or audio device and is not a native/pixel parity claim:

```powershell
cargo run --locked --offline -j 2 -p deadsync-shell --bin song_lua_execution -- ../rework/lua-songs/fof_guys/fof.ssc target/fof-application-startup.json
```

The application-build check reproduced FOF's failure before the gate removal
(`retained_lua_vm=false`, zero frames, exit 1). With the fix it passed 241
frames, with changing visuals on 239 frames, seven authored Sound operations,
64 mesh actors containing 3,114 vertices at the final frame, and zero callback
errors. Evidence is in `target/song-lua-portability-audit/fof-application-{before,after}.json`.
The dependency feature graph in `application-features.txt` contains no
`test-support`. Native replay reports now explicitly state that their build
enables test support and requires this separate application-build check.

The focused `live_lua_screen_frames` regression now loads its background and
foreground declarations through normal gameplay initialization. Its old
manual-injection setup expected foreground On during loading and retained
background Init properties across activation. The local reference instead
dispatches foreground On in `Foreground::Update` and calls recursive
`InitState` before background On (`Background.cpp:850`, `ActorFrame.cpp:83`).
The synthetic background therefore initializes its visual in On, and dormant
child actors give the foregrounds an authored lifetime without tweening their
moving parents. Rendered positions, frame counts, score propagation and
transition assertions are retained. This test and the explicitly offline
spline regression both pass.

## Current focused verification

### Replay source identity correction (2026-10-20)

The performance follow-ups replayed the native Single capture against the
installed `target/debug/Songs/lua-songs/fof_guys/fof.ssc`. That simfile has a
locally changed offset of `0.036000`; the captured source archive has `0.000`.
This invalid comparison produced the reported 12,519 draw mismatches beginning
at frame 226. They do not establish an engine defect. Replaying the original
source with the same executable produces zero mismatches.

ITGmania's `Foreground::Update` uses song timing to choose the activation frame
and the first update delta. Changing the offset therefore changes how often
the authored queued commands run, including this minigame's simulation.

The replay harness now checks a capture's sibling `manifest.json` when it
declares `captures` and `source_archive`. It verifies the capture hash, the
content-addressed source archive, and every recorded song file before gameplay
preparation. This includes the simfile, Lua dependencies and media; relocated
identical sources remain valid. Reports include `source_identity`. Standalone
traces without this provenance explicitly report `not-recorded`.

Use the original source directory in the commands below. A changed source
returns exit code 1 as an invalid replay input; a valid replay with behavior
mismatches still returns 2. Deliberate modified-source controls also require
their own explicit provenance instead of reusing this positive capture's
manifest. The native observations and installed song files remain unchanged.

The unchanged native captures pass through public production preparation with
`retained_lua_vm=true` and `production_gameplay_checks_passed=true`:

| Capture | Frames | Scenes | Inputs | Draw samples | Submitted vertices | Mismatches/errors |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Single | 448 | 1 | 59 | 15,387 | 699,774 | 0 / 0 |
| Versus | 698 | 2 | 102 | 16,343 | 734,094 | 0 / 0 |

The independent runtime regressions change control buttons, motion constants,
registration delay, timer duration and sound paths in Lua. They verify delayed
input registration, callback-driven motion, ordered play/pause/stop, authored
screen reloads and preservation of arbitrary globals. A mixer regression checks
independent actors using the same file and native overlapping-play behavior.
The offline regression covers immediate and delayed input registration followed
by removal. Isolated copies of FOF also load and advance successfully after
relocation, comment edits to the wrapper and physics system, and level edits.

The production replay exposed a comparator error: the native draw hook records
projection coordinates before `glDepthRange`, while DeadSync encodes that range
in its submitted matrix. The comparator now applies the native recorded bias
using `RageDisplay_Legacy::SetZBias` and the implicit NoEffect bias in
`Actor::SetGlobalRenderStates`. It retains homogeneous W, the same tolerances,
and the original vertices. A regression rejects the old unadjusted depth and
checks explicit bias, implicit mask bias, and depth-disabled draws.

Focused commands (the opt-level override only reduces iteration build time):

```powershell
cargo test --locked --offline -j 2 --config 'profile.dev.package.deadsync-song-lua.opt-level=0' -p deadsync-song-lua --lib interactive_runtime
cargo test --locked --offline -j 2 --config 'profile.dev.package.deadsync-song-lua.opt-level=0' -p deadsync-song-lua --lib offline_compile_rejects_input_callbacks_instead_of_dropping_them
cargo test --locked --offline -j 2 -p deadsync-audio-stream --lib actor_sound_stop_pause_and_overlapping_play_are_independent
cargo test --locked --offline -j 2 --config 'profile.dev.package.deadsync-song-lua.opt-level=0' --test live_song_lua -- tests/fixtures/interactive_song_lua/fof-single.trace.json.zst ../rework/lua-songs/fof_guys/fof.ssc target/fof-production-single.json --production
cargo test --locked --offline -j 2 --config 'profile.dev.package.deadsync-song-lua.opt-level=0' --test live_song_lua -- tests/fixtures/interactive_song_lua/fof-versus.trace.json.zst ../rework/lua-songs/fof_guys/fof.ssc target/fof-production-versus.json --production
```

No all-fixture sweep or graphical game launch was used. These captures cover
short interactive sections, not full-course completion or framebuffer pixels.
They do not certify every ITGmania Lua API or arbitrary future game content.

## Historical compiled adapter (superseded)


The missing ball is repaired on the normal compiled gameplay path. FOF now
has a source-verified Rust execution program for physics, controls, camera
follow, collisions, checkpoints, goals, projected geometry and countdowns.
Lua initializes the authored scene on the loading worker; all Lua values and
the VM are dropped before gameplay. Screen replacements carry plain player,
chart, countdown and PRNG state into another loading-worker compilation.

The frontend checks the actual wrapper, driver and supported system sources,
and extracts entity parameters, mesh topology, materials and actor bindings
from the initialized scene. It does not choose behavior from a song title or
copy expected poses from a native capture. The source revisions are pinned in
`compiled_scene.rs`; this is support for the verified FOF program, not a
general lowering of arbitrary interactive Lua.

The executable program updates the ball, course meshes and camera together.
It consumes the raw song clock, matching native foreground updates. The HUD's
smoothed display clock delayed the first tick and shifted subsequent geometry;
the focused comparisons caught that implementation error. Their expected
values and tolerances were unchanged. Dynamic actor plans invalidate the
compositor's immutable geometry assumptions. Windows sound paths are prepared
in both authored and native spellings, without frame-time normalization.

## Focused native verification

Both captures exercise the public normal `gameplay_entry::prepare_song_lua`
path and assert that gameplay has a compiled program and no retained Lua VM.
The harness's `--compiled` mode is separate from its diagnostic live-Lua mode.

| Capture | Frames | Scenes | Inputs | Drawable samples | Submitted vertices | Mismatches/errors |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Single | 448 | 1 | 59 | 15,387 | 699,774 | 0 / 0 |
| Versus | 698 | 2 | 102 | 16,343 | 734,094 | 0 / 0 |

The checks include live text, textures, depth/sampler state, submitted geometry,
and 869 stage fields in total. Versus includes the join screen and replacement
with Beginner note data while the wrapper restores the displayed selection to
Challenge. Its 35 equivalent offscreen culls retain the existing comparator's
clip-plane rule.

- [Single compiled result](../tests/fixtures/interactive_song_lua/fof-compiled-single-result.json)
- [Versus compiled result](../tests/fixtures/interactive_song_lua/fof-compiled-versus-result.json)
- [Capture provenance](../tests/fixtures/interactive_song_lua/manifest.json), native engine
  `5c737928d93778c2c9f68e276052b220b43a468f`

Three targeted Rust regressions also pass: the countdown, the delayed goal
reload with source chart choices, and the result screen's single transition
after eight seconds. The countdown replays the captured clock up to its first transition and
requests `SM_DoNextScreen` at the exact native music second,
`119.07389831542969`. The small
[clock fixture](../tests/fixtures/interactive_song_lua/fof-timeout-clock.json.zst)
retains 7,378 native clock inputs and the native transition event from the
existing full-song archive, with provenance; it
contains no expected ball poses or meshes. The goal/result tests are focused
lifecycle regressions, not independent full-course native captures.

Focused replay and unit builds used an opt-level=0 override for the song-Lua
crate during iteration. The normal application is built with the repository's
unmodified default profile, which optimizes that crate at level 3.

```powershell
cargo test --locked --offline -j 2 --test live_song_lua -- tests/fixtures/interactive_song_lua/fof-single.trace.json.zst ../rework/lua-songs/fof_guys/fof.ssc target/test-audit/fof-compiled-single.json --compiled
cargo test --locked --offline -j 2 --test live_song_lua -- tests/fixtures/interactive_song_lua/fof-versus.trace.json.zst ../rework/lua-songs/fof_guys/fof.ssc target/test-audit/fof-compiled-versus.json --compiled
cargo test --locked --offline -j 2 -p deadsync-song-lua --lib compiled_scene::runtime::tests
cargo build --locked --offline -j 2 --bin deadsync
```

No all-fixture sweep ran. Full-song completion, sampled framebuffer parity and
compiled command/query instrumentation remain unverified. The scoped reports
therefore set `compiled_gameplay_checks_passed=true` while keeping whole-song,
pixel and unrestricted production-parity certificates false.

## Why the earlier passes missed it

Before this repair, the normal preparation path released the compiler VM and
retained 64 static course meshes (3,114 vertices), alongside baked changing ball
pose tracks. It had no compiled representation of input callbacks or recurring
vertex uploads. The ball actor existed and had a captured `visible(true)` write,
yet the [normal Vulkan application showed no ball](../tests/fixtures/interactive_song_lua/compiled-missing-ball.png).
The exact pixel/depth operation hiding it was not isolated; an inconsistent
course/camera pose was a working explanation.

The old full replay passed its implemented stage/command comparisons but
explicitly excluded drawable state and input callbacks. The separate interactive
pass retained a diagnostic Lua scene and bypassed normal compilation. Both were
narrower than production gameplay parity. Their historical acceptance rules
and native captures are unchanged. Live replay reports now label their path as
`diagnostic-live-lua` and state that production preparation was not exercised.

The [machine-readable audit](../tests/fixtures/interactive_song_lua/compiled-production-audit.json)
retains the earlier reproduction and the new compiled-path results. Full details
of the earlier stage-only acceptance remain in
[song-lua-fof-result.md](song-lua-fof-result.md).
