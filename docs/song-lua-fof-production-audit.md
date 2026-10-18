# Fof Guys compiled-path repair ? 2026-10-19

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
