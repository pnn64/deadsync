# Native draw colors and queued color effects

The reference harness now samples diffuse and glow through the linked
ITGmania `Actor::PreDraw`, including local effects, wrapper colors, and
ancestor tint/glow. Projected rows append diffuse and glow RGBA at indexes
9 and 10. Visibility includes a visible glow pass when diffuse alpha is
zero. The schema advertises `projected_draw_color_samples`; the global
layout is derived from the actual track layout. Older captures and their
provenance remain unchanged. This captures the first diffuse corner, not
all four gradient colors.

DeadSync's semantic comparator checks all eight sampled color channels on
native-visible rows. Old captures keep their existing comparison path.
Color tolerance is 0.0001; existing geometry and alpha tolerances are not
widened. The independent renderer test compares all four corners of each
actual ITGmania Sprite draw, permitting one byte quantization step.

The new coverage found three production gaps:

- Native actors start with white RGB and zero-alpha glow. Black RGB changes
  the result when an ancestor adds glow. Both compiled and Lua default
  states now use the native default.
- The six diffuse/glow macros reset the timer when the effect changes and
  retain it when the same macro is repeated. Their native timer phase is
  now captured and used during playback, as it already was for pulse and
  rainbow. The former motion-only phase function is replaced by the shared
  effect-phase function.
- Startup queues reconstructed individual durations by subtracting rounded
  cumulative cursors. A chain of 0.4 and four 0.5-second sleeps consequently
  stopped an effect at 2.4167 instead of native frame 2.4. The compiler now
  retains the original float durations, including zero-time queue entries,
  and feeds them to its existing native float countdown. Startup dispatch,
  queue clearing, and hurry scaling retain the same queue ownership. This
  storage exists at song load; gameplay still consumes compiled tracks.

`late-colors.lua` exercises the six color macros, repeated selection,
switching to rainbow and back, and stopping at the exact queue boundary.
It also covers actors with default glow beneath a glowing ancestor, and a
transparent leaf whose glow still draws. Actual native draws are captured
at all 241 frames from zero through four seconds at 60 Hz.

Before the default/phase fixes, the semantic fixture was 798/1,218. Those
fixes passed all 1,218 sparse semantic checks, but the independent dense
renderer check still caught the late stop at 2.4 seconds. After retaining
the original queue durations, all 1,218 semantic checks and all 57,840
native draw-color channels pass. The prior color-inheritance fixture's
54,656 native channels also pass. This is why a sparse passing trace alone
is insufficient evidence of visual parity.

Capture provenance, executable/source/input hashes, raw SHA-256 hashes,
and compressed hashes are stored in
`tests/fixtures/itgmania-song-lua-micro/late-colors-provenance.json`.
Both compressed captures have verified exact round trips. The native draw
capture is byte-identical to the earlier capture before the metadata
layout correction. Reference files under `itgmania/` were not edited.
All source resources and captures are local; nothing was downloaded.

Reproduce the dense native draw capture with the local harness:

```powershell
itgmania-harness-rs/target/debug/itgmania-harness-rs.exe actor-conformance deadsync/tests/fixtures/itgmania-song-lua-micro/late-colors-input.json --out late-colors-native.json
```

Run the production check from DeadSync:

```powershell
$env:RUST_MIN_STACK = '16777216'
cargo test -j 2 --test song_lua_itgmania_semantic_parity late_colors_native_draws -- --nocapture
```

Full validation passes 781 song-Lua unit tests, 172 playback tests, two
AMV tests, and 106 regular semantic tests in both rework and MAIN. The
local harness passes 135 tests; its two native color regressions also
pass after the layout assertion was added. MAIN advances exactly once
from 0.5.1745 to 0.5.1746 for this pass.

## Remaining whole-song gaps

The fresh complete local Mawaru8 trace now checks rendered colors:
**304,886/306,487**. Sections are compile 7/12, layer order 3/3, final
render 1,088/1,088, render persistence 3,150/3,150, update values
6,854/6,854, player ranges 10/10, projected geometry 92,843/92,855,
draw colors 172,544/174,128, vibration 27,784/27,784, timeline 135/135,
message commands 214/214, and runtime modifiers 254/254.

Remaining failures include five speculative input-message probes,
twelve fade-boundary geometry/visibility checks, and 1,584 rendered-color
checks. These include small fade-alpha differences and a glow difference;
their tolerances remain unchanged. The fresh native run has no runtime
errors or dropped events. Its raw trace SHA-256 is
`f60f4bbfa608b54ff56ed73330f2748bae68a208f78091dbfabc250607264830`.
It contains 462 projected tracks and 27,784 sampled rows. The old retained
trace has less coverage and is not a substitute for this result.

Mawaru5 remains **4,861/4,873**, with four fade-end visibility gaps
accounting for twelve failed checks. Native float countdown leaves a
small positive alpha on the final fade frame; those gaps are not fixed
by the startup-duration change. Neither chart is declared whole-song
parity.

Frozen project hashes are unchanged. Local mismatches remain:

| Chart | Frozen hash | Local hash |
| --- | --- | --- |
| Mawaru5 | 74765c1936186d20 | 8e0b6274cb33af5e |
| Mawaru8 | cadefe09888e9ab8 | 8224fb7e0b05040f |
| Brain Power | a73ec5f2f3015620 | f40ebaf45ea6e26d |

Get Into It (`9c208360a9b25133`) and Rhythm Hell
(`be38aa9e3c88c32b`) have no matching local chart in the audited folders
and archives. No resources or frozen hashes were replaced to conceal this.
