# Save Your Tears whole-song parity

Frozen/local chart `657399e28076dbdf` matches `Save Your Tears/SaveYourTears.ssc`,
dance-single Challenge (Episode 01). All resources came from the local workspace.
No downloads, song edits or frozen manifest changes were made.

The two rain movies previously retained an old queued fade destination when a
later message wrote diffusealpha before appending another tween. Native Actor
setters change the back tween's destination. DeadSync now applies that rule to
diffuse colors as well as player transforms, preserving the current state and
queue duration. The old transform-only restriction was replaced.

The reference harness also omitted the JPEG background's geometry. Its existing
headless texture metadata implementation now reads bounded JPEG frame headers,
including baseline/progressive streams and filename sprite-sheet grids. A
truncated segment fails instead of returning fabricated dimensions. Native
RageTexture accessors retain source, frame and backing sizes. This is header
metadata, not a native JPEG pixel decoder or GPU screenshot.

The retained reference reaches beat 336 (170.84745762711864 seconds) with both
players, cel, 854x480, seed 1 and 60 FPS. It has 602 events, no Lua errors or
dropped events, 10,252 updates and 1,069 projected samples across five drawables.
The rain source is 1280x720; the JPEG background is 3224x2240.

The whole-song test passes 126,766 semantic comparisons at existing tolerances,
including 102,520 alpha/visibility checks on every drawable update. Corrupted
fade samples fail a countercheck. Another 6,808 checks exercise actual production
render-list geometry and tint for every visible movie/JPEG sample. Production
loaders decode the local movie poster and JPEG before registering dimensions;
the headless composer supplies transparent pixels for layout, not pixel sampling.

The 970,777-byte reference compresses to 96,154 bytes with an exact round trip.
Raw SHA-256:
`1e07ce1e36d5294b81111ef027bb9f95085277e2979db673ca3e272debb85e94`.

REWORK and MAIN each independently pass 136 semantic tests and 965 song-Lua
package tests; the reference harness passes 146 tests. MAIN is `0.5.1759`,
exactly one patch above `0.5.1758`, with both Cargo files included in the
curated commit. REWORK keeps its changes uncommitted.

Full movie frames, audible playback, live-input branches and interactive GPU
screenshots remain outside this audit. Frozen/local hash discrepancies remain
unchanged and are listed in [the inventory report](song-lua-mawaru6-progress.md).
