# Fof Guys interactive playback

Fof Guys needs a running Lua game. Production gameplay selected the baked
compiler, discarded `AddInputCallback`/`RemoveInputCallback`, and transferred
poses without transferring changing meshes or text. A successful score replay
therefore did not establish that the ball could move or that the level rendered.

The repair enables the retained Lua scene in normal, preloaded, and practice
preparation. The screen owns native-shaped input callbacks, including repeats,
removal during dispatch, callback truthiness, and physical keyboard identity.
Held inputs survive the song's gameplay reload and are released when input is
cleared. Press and release edges received during asynchronous loading are queued;
the replacement screen reconciles old held inputs with current physical state.
Input has an explicit phase after update and before authored drawing;
callback-requested screen changes survive output draining until the next update.
Production preparation defers the first Lua update until the screen supplies the
real song clock, including when gameplay is preloaded.

Visual slots now retain initially empty AMVs and hidden sprites. Live updates
transfer vertices, text, and sprite bindings. AMV vertices retain Z and deliberate
NaNs without shifting primitive boundaries. Rendering uses the native camera,
texture translation, wrapping, and filtering. Sprite culling preserves visible
edge slivers. Relative nested media paths are resolved before being rebound.

## Native evidence

The sibling `itgmania-harness` now injects controls through the real native
InputMapper/InputFilter and records the table delivered to screen callbacks.
Its observer records RageDisplay submissions, resources, texture allocation
dimensions, and render state. It observes engine output without implementing
Fof Guys physics or copying expected vertices into DeadSync.

The retained captures and input scripts are in
[`tests/fixtures/interactive_song_lua`](../tests/fixtures/interactive_song_lua).
They use the pinned native engine and Simply Love, the unchanged Fof Guys source,
actual frame clocks, and a declared maximum capture rate of 60 Hz.

| Case | Scenes | Input events | Draw samples | Compared vertices | Draw mismatches | Lua errors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Single | 1 | 59 | 15,387 | 699,774 | 0 | 0 |
| Versus, join and reload | 2 | 102 | 16,343 | 734,094 | 0 | 0 |
| Single, input handler deliberately disabled | 1 | 59 | 15,387 | 699,774 | 11,854 | 0 |

The negative control changes only the handler in a private copy of the song.
It keeps the positive native expectations intact. This demonstrates that lost
controls produce observable failures even when the scene still loads and runs.

The comparison invokes production gameplay and presentation composition. It
checks homogeneous clip coordinates, complete quad coverage, sampling
coordinates normalized to image dimensions, color, texture identity, depth,
mesh samplers, and changing text values. Thirty-five fully clipped native draws
in Versus are equivalent to production culls; visible edge geometry is checked.
The neutral native factory identities establish ownership, not expected poses.

These captures end at beat 34. They prove the recorded interactive checks, not
whole-song or pixel-for-pixel parity. Glyph rasterization, sampled pixels, audio
output, every input device, and every song-Lua API are outside this comparison.
`archive_accepted`, `whole_song_parity_proven`, and `pixel_parity_proven` remain
false. The manifest records hashes and the existing source archive.

## Reproduce

With the original song available in the sibling corpus:

```powershell
# MSVC's large debug PDB hit its limit on this machine. This affects linking,
# not the simulation or comparison rules, and is local to this shell.
$env:_LINK_ = '/DEBUG:NONE'
cargo test --offline --test live_song_lua -- tests/fixtures/interactive_song_lua/fof-single.trace.json.zst ../rework/lua-songs/fof_guys/fof.ssc target/fof-single-report.json
cargo test --offline --test live_song_lua -- tests/fixtures/interactive_song_lua/fof-versus.trace.json.zst ../rework/lua-songs/fof_guys/fof.ssc target/fof-versus-report.json
```

Passing scoped interactive checks return exit code 0. Incomplete coverage or a
mismatch returns 2; invalid captures return 1. No partial capture can certify a
full-song archive.

Build the observer executable using the sibling harness's
`native/trace_adapter.cmake`, then capture with `song-lua-trace`,
`--input-script`, and `--drawable-samples`. Use distinct native keys for P1/P2.
Versus joins both native sides and selects dance-single charts.

## Other validation

The hardware mapping/callback/repeat/release regression, callback registry tests,
dynamic-content and relative-media tests, visible-edge culling test, input suite
(including its allocation test), and 13 harness runtime tests pass.

The broad Lua library run reports 638 passes and 15 failures. Every failing test
also fails in the retained pre-change executable; names and its hash are recorded
in `baseline-failures.json`. These failures are not reported as passing checks.

The Windows app links successfully with PDB generation disabled. The final
executable is delivered as `target/debug/deadsync-interactive.exe`, allowing an
existing game session to remain open. Its hash is recorded in `results.json`.
