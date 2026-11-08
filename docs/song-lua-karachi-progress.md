# Karachi whole-song parity

Frozen/local chart `702b3a5f5aba3b98` matches `Karachi/Jorts - Karachi.ssc`,
dance-single Challenge, `Mods BR+ FS+ SS- FL-`. The Edit chart has the same note
hash but takes the no-mods branch; this audit selects Challenge explicitly.
All resources came from the local workspace. No downloads, song edits or frozen
manifest changes were made.

The earlier reference omitted six sprites whose Texture property names the
`ScreenTex` render target. ActorFrameTexture::Create registers that resource in
ITGmania's texture manager. The headless Lua host now resolves named handles
from texture properties, SetTexture and Load, retaining their source dimensions
and projected geometry. A local fixture verifies all three binding paths.
The implementation models registration metadata, not native GPU target pixels.

The refreshed reference reaches beat 324 (126.23376623376625 seconds), with both
players, cel, 854x480, seed 1 and 60 FPS. It has 87,250 events, no Lua errors or
dropped events, 7,576 update frames and 970 projected samples across eight
drawables. The local background is 1280x720; ScreenTex is 854x480.

The whole-song audit retains 200,040 semantic comparisons at existing
tolerances. This includes 121,216 alpha/visibility comparisons across every
drawable update, eight HUD/combo/judgment/notefield proxy bindings and twelve
capture resource checks. A corrupted glow track must fail the audit.

Another 7,600 comparisons exercise actual production PNG and render-target
sprite geometry/tint. Native homogeneous corners are cropped before perspective
division. The same check is shared with Goodbye's previous cropped image
verification. The existing test-support composer now calls the production
render-target sprite builder too. It verifies that all six effects sample the
same render-target handle and retain the native additive/alpha blend modes.

The harness's full regression run also exposed a Windows temporary-directory
collision when parallel tests shared the same clock tick. Atomic IDs and
AlreadyExists handling replace the clock-only names; all 147 harness tests pass.

The 7,136,784-byte reference compresses to 502,120 bytes with an exact round trip.
Raw SHA-256:
`5ed6e28354dbbb7a5fd64b4ab068641c2be95b039b336108ba5d57818d8e3697`.

REWORK and MAIN each independently pass 137 semantic tests and 965 song-Lua
package tests; the reference harness passes 147 tests. MAIN is `0.5.1760`,
exactly one patch above `0.5.1759`, with both Cargo files included in the
curated commit. REWORK keeps its changes uncommitted.

Captured render-target pixels, audible playback, live-input branches and
interactive GPU screenshots remain outside this audit. Frozen/local hash
discrepancies remain unchanged and are listed in
[the inventory report](song-lua-mawaru6-progress.md).
