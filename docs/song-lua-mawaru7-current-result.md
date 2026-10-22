# Mawaru7 Titan scene depth follow-up

The flat green Titan screen at beats 308 and 320 is a second valid rendering
failure. The production capture reproduces the reported image: the sky and
instruction text draw, while the trees, ground and characters disappear.
The previous note-visibility fix did not establish parity for this scene.

ITGmania keeps `ztest`, `ztestmode` and `zwrite` independent. An Actor starts
with testing off and writing disabled; a Model enables both. `zbuffer` changes
both settings. ActorFrame propagates each setter separately, preserving a
child's later override. `WriteOnPass` compares LEQUAL and `WriteOnFail`
compares GREATER; neither comparison mode implicitly enables writing.
See `Actor.cpp`, `Actor.h`, `ActorFrame.cpp`, `Model.cpp` and
`RageDisplay_OGL.cpp` at native revision
`704cdd5624155f743b59b8b64fedeb0588dae067`.

The Titan frames (`actor_jw` and `actor_ix`) enable testing. Their sky sprites
must not write depth. The invisible quad (`actor_jx`) explicitly enables
writing to mask the other player's half. Combining testing and writing made
the sky occlude subsequently painted scenery, including geometry farther
from the camera. Retain both settings and the comparison direction through
capture, compiled state/deltas, playback, composition and every renderer.
Write-only geometry also participates in depth reset boundaries. Ordinary
noteskin meshes retain their previous combined test/write behavior.

The focused renderer regression covers the six compare/write combinations,
NoEffect masks, model isolation and offscreen depth retention on DirectX,
Vulkan (wgpu), OpenGL and native Vulkan. Native semantic comparisons now
check depth writing and comparison direction separately instead of assuming
that writing matches testing. The small compiled-state regression covers
frame propagation, child overrides, write-only masks and WriteOnFail.

The actual-chart regression now captures beats 308, 320 and 338 as well as
the earlier gameplay samples. At 308 and 320 it removes Titan ground/tree/
character draws and asserts that they contribute more than 1,000 pixels.
Thus a flat sky can no longer pass merely because the scene compiled or
produced geometry. This follow-up uses `DEADSYNC_MAWARU7_UNTIL_BEAT=342`;
it checks the production compiled path with no surviving Lua VM. It does
not establish whole-song pixel or interactive-minigame parity. No complete
fixture sweep is run.

The focused actual-chart run passes in 239.81 seconds. Removing scenery and
character draws changes 487,486 pixels at beat 308 and 483,118 at beat 320;
the Titan and player are visible in the retained beat-308 capture. Earlier
tap contributions remain 24,281 pixels at beat 70 and 10,255 at beat 94.
The four-backend GPU regression passes in 5.32 seconds, and all three small
compiled-state/delta checks pass. Before/after captures are in
`target/mawaru7-titan-before` and `target/mawaru7-titan-after`. The normal
`cargo build --locked --bin deadsync` succeeds.

All four selected software depth checks pass, including independent modes
in retained/direct and indexed/unindexed raster paths. Type checking the
affected renderer, notefield and root test targets also passes. That check
found four stale parity-harness calls to the old six-argument attack-ease
API; remove their obsolete Mini-baseline argument to match the current
five-argument API. No fixture executions are added by this compile check.

# Mawaru7 gameplay integration follow-up (0.5.1872)

The retained semantic results below do not cover the production handoff from
compiled song layers to gameplay, GPU blend state, or recursive depth setters.
The reported missing notes are valid: at beats 70 and 94 the normal gameplay
path produces tap geometry, but the later `doors/3_*` sky and runner draw over
it. With the song's image resources resident, removing tap draws changes zero
pixels at both beats before the depth fixes.

The foreground starts at beat 1, with no primary foreground at beat zero.
The shared compiler stores player/modifier tracks on its first output (the
first background in this session). The gameplay builder previously imported
those tracks only from a primary foreground. Import shared tracks from the
actual session owner, and retain only the shared ScreenGameplay pose at the
root; keep the graphics in their authored timed layers.

The door uses `BlendMode_NoEffect`, a depth-writing quad, and `ztest(1)` on
its ActorFrame. Capture the blend mode instead of silently retaining alpha
blending. Preserve destination RGB and alpha in every renderer backend.
ITGmania's ActorFrame propagates depth setters to its children; capture those
child writes, while retaining subsequent explicit child overrides. Numeric
zero uses native BIArg conversion rather than Lua truthiness. Match the
implicit NoEffect Z bias and native depth-range mapping for projected Lua
geometry so coplanar artwork cannot draw through the invisible mask.

Native source references at ITGmania revision
`704cdd5624155f743b59b8b64fedeb0588dae067`:

- `src/ActorFrame.cpp`: propagated `SetZTestMode` and `SetZWrite` commands.
- `src/Actor.cpp`: `SetGlobalRenderStates` gives NoEffect masks implicit ZBias 1.
- `src/RageDisplay_OGL.cpp`: NoEffect uses ZERO/ONE for both RGB and alpha;
  `SetZBias` maps the depth range to `[.05*(1-bias), 1-.05*bias]`.
- `src/Foreground.cpp`: foreground activation is timed independently of global
  Player and PlayerOptions writes.

The retained small native control is
`tests/fixtures/itgmania-song-lua-micro/late-shared-runtime/native-result.json`.
It ran embedded ITGmania sources at embedded source revision
`5c737928d93778c2c9f68e276052b220b43a468f`, with no script errors or diagnostics.

Validation is scoped to Mawaru7 and the affected small regressions. The GPU
probe loads Lua image resources before actor construction, uses the unmodified
Challenge chart `f7fdd8fafaee6188`, Cel, M730, saved Mini 60%, filter 95%, design
space 854x480 and Vulkan (wgpu) at 1600x900. The integration path asserts that
no Lua VM survives preparation. Installed songs and profiles are unchanged.

The full normal preparation and GPU regression passes (467.02 seconds). At
beats 70 and 94, removing tap draws changes 24,281 and 10,255 pixels respectively;
the leaking door artwork is masked. Beats 116 and 196 correctly have no tap
pixels because those minigame intervals contain no ordinary note rows. The
shorter diagnostic through beat 200 produces the same pixel counts. The small
shared-track regression passes with and without a background owner. The
NoEffect color/depth check passes on DirectX, Vulkan (wgpu), OpenGL and native
Vulkan. Both targeted projection regressions pass, including the retained
Step Your Game Up geometry. Its test helper now requires the workspace lockfile
before treating a local `assets` directory as the workspace root.

Retain the observations in
`tests/fixtures/itgmania-song-lua-micro/late-shared-runtime/gameplay-result.json`.
These checks validate the reported note visibility and affected primitives;
they do not establish whole-song pixel or interactive-minigame parity.
No full fixture suite was run.

To repeat the actual-chart check on Windows, run:

```text
cargo test --locked -p deadsync-shell --lib mawaru7_compiled_notefield_transitions -- --ignored --nocapture
```

The self-contained GPU regression needs no local Mawaru installation and
reproduces the same closed/open depth-mask contract:

```text
cargo test --locked -p deadsync-shell --lib song_lua_depth_mask_hides_art_then_opens -- --ignored --nocapture
```

It passes with 27,741 visible tap pixels while the covering art is masked,
then zero while the open door reveals that art. This small run takes 20.72
seconds including Lua compilation and GPU resource setup inside the test.

`DEADSYNC_MAWARU7_CAPTURE` optionally chooses the PNG output directory.
`DEADSYNC_MAWARU7_UNTIL_BEAT` bounds diagnostic compilation; leave it unset
for the full production preparation. The default Windows check always renders
and asserts visible notes, rather than making pixel validation optional.

# mawaru7 current result (0.5.1805)

The frozen project chart `mawaru7/mawaru7.sm` passes **1,454,202/1,454,202** existing
semantic and runtime-modifier checks. Pin hash `f7fdd8fafaee6188`,
dance-single Challenge, description `TaroNuke (converted by MrThatKid4)`,
and seed 1.

The original completion audit reports 874,663/874,665 against
the superseded reference. The current full capture reproduces two visibility failures and adds two
crop failures at idle boundaries. Sparse render tracks had closed a held
interval with the actor state already changed by its next command. This
created a ramp throughout the idle interval. Preserve the last captured
render value until the preceding update frame, then retain the command
state at its original dispatch frame before appending the next write. Replace
the two endpoint paths that used the changed state. This fixes hidden
grade sprites fading toward the next appearance and cropped doors
slowly opening while idle. The four native queue/update regressions pass.
Comparator code and tolerances are unchanged. The
reference records the current native float beat and countdown behavior;
retain every emitted observation in the current selected capture. Keep
the shared micro fixture: its unchanged standalone regression also passes
874,665/874,665 checks after the actual sparse-track fix. The targeted
idle-boundary regression passes 154/154 checks.
The capture preserves the previous full beat/time horizon and 60 Hz update
frequency.

Passing sections:

```text
  compile info         12/12 ok
  layer order          4/4 ok
  final render         1204/1204 ok
  judgment textures    8/8 ok
  sprite textures      566/566 ok
  render persistence   55466/55466 ok
  update values        163718/163718 ok
  player ranges        8/8 ok
  projected geometry   205778/205778 ok
  draw colors          395720/395720 ok
  draw crops           229532/229532 ok
  draw shadows         344298/344298 ok
  projected vibration  57383/57383 ok
  timeline             178/178 ok
  message commands     195/195 ok
  runtime modifiers    132/132 ok
```

Retain 17,086 update frames, ending at beat
816 and 284.7352863536562 seconds,
with zero Lua errors and zero dropped events. Raw capture SHA-256:
`c8b74a49a46838cf0085873289c81211c59f30eaa4cfb75ec645568ecd68d309`. Compression has an exact verified byte round trip.
The manifest records simfile, Lua, noteskin, harness and executable
provenance. No songs, supplied resources or frozen project data are edited.

The complete pinned corpus test passes
in MAIN's isolated checkout. This preserves the existing semantic and
modifier scope, not whole-game pixel parity. No downloads or visible
ITGmania instance. Both Cargo files bump exactly once from 0.5.1804
to 0.5.1805.
