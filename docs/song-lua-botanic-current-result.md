# Botanic Panic current result (0.5.1797)

The frozen project's dance-single Challenge chart in
`Cuphead [TaroNuke]/botanic.sm` passes **655,940/655,940** existing semantic
and runtime-modifier checks. Hash `30448d01c606fce9`, description
`TaroNuke` and seed 1 are pinned by the corpus test.

The completion audit found 432,684/435,866 checks passing against the
retained reference. A complete current local headless capture resolves
the stale countdown, actor-update and visibility observations, leaving
655,931/655,940 passing: nine RGB checks fail on the Wallop sprite's
0.1-second diffuse blink.

Zero-duration scheduled setters were evaluated using subtraction and
addition as though they were tweens. Copy their destination exactly.
Interpolating an immediate effect period from 1 to 0.1 at factor 1
changes its native float bits, accumulating a timer phase difference
and selecting the wrong blink color at an exact boundary. Preserve
the existing interpolation for actual tweens. No chart-specific logic,
tolerance changes or comparator changes are introduced.

The small native `queued-blink` fixture fails 21 color checks before
the fix (1,126/1,147), then passes all 1,147, including 392 color checks.
It covers a queued fade, short repeating blink and effect stop. Existing
queued-command, current-position, retarget and update-order regressions
also pass. Temporary clock diagnostics are removed.

The full passing result includes compile information 8, layers 3,
final render 322, player proxies 6, textures 118, persistence 52,662,
updates 146,162, player ranges 16, geometry 46,923, colors 92,920,
crops 48,312, shadows 72,468, vibration 12,078, timeline 135,
messages 65 and runtime modifiers 183,742.

Retain all 8,154 frames, ending at beat
362 and 135.87825149525844 seconds,
with zero Lua errors and zero dropped events. Settings: cel, seed 1,
beat step 0.125, max events 2,000,000. Raw capture SHA-256:
`202a0bee8bb4b62335f618eb10ff9ef23f8bcbaedf338f52696b60adec6a9105`. Compression has an exact verified byte round trip.
Source, harness and executable hashes are recorded in the manifest.
Delete both superseded selected raw and compressed references.

Validation: the full direct comparison and pinned corpus test pass in
MAIN's isolated checkout, alongside the five focused regressions.
No downloads, visible ITGmania instance, song edits or frozen-project
edits. Comparison scope stays unchanged and does not claim whole-game
pixel parity. Both Cargo files bump exactly once from 0.5.1796 to 0.5.1797.
