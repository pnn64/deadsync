# Full song Lua fixtures

Run a complete fixture from the repository root:

```powershell
cargo test --test full_song_lua ff8751bfe369bcb22108866e6a10ec4183c6dcd410982e3a89d43836db39a2c9.tar.zst
```

No environment variables or extra test flags are needed. Archive filenames,
SHA-256 prefixes, and unambiguous song titles work. A missing or ambiguous
selector fails instead of silently running no tests. Historical filenames
remain aliases in `index.json` when recompression changes the archive hash.

```powershell
cargo test --test full_song_lua -- --list
cargo test --test full_song_lua -- --all
```

Without a selector, the target prints help and exits successfully. This keeps
an ordinary `cargo test` from unexpectedly running the expensive full corpus.

The runner verifies archive and member hashes, compiles the complete Lua
runtime, composes every frame for the entire captured duration, and compares
native semantics, geometry, textures, commands, modifiers, and recorded player
transforms. Progress is printed every two seconds, including elapsed time,
the current stage, composition frames and percentage, or completed comparison
and failure counts. It prints each comparator's tally and a final result with
fixture counts, passed/failed comparison counts, and elapsed time. A parity
mismatch or an extraction/compile/composition error returns a nonzero exit code.
The comparison total is discovered as the comparators run; it is not an estimate
based on the number of raw events in the native trace.

Full-song references must use native song timing, have complete update-frame
coverage, and report no native runtime errors or dropped events. Captures
with positive hibernation calls require harness `0.1.6` or later; non-default
`SetUpdateRate` calls require `0.1.7` or later. The runner rejects obsolete
references before comparing gameplay; regenerate them with
the sibling harness. Songs without Lua references have empty compiled layers
and still receive root, command, player, and modifier checks. All 18 recovered
songs with empty Lua closures were recaptured with `0.1.6` and pass on DeadSync
`0.5.1835`; the historical capture audit below predates this verification.

All retained full-song references live directly in this folder:

- `<sha256>.tar.zst`: self-contained simfile, Lua dependencies, assets, manifest,
  and native semantic/render trace. `index.json` lists primary captures; the
  runner also discovers and lists retained archive versions in this folder.
- `<sha256>.json.zst`: preserved native reference versions used by focused
  regression tests. Identical JSON captures share a file. Names hash the
  decompressed JSON, preserving reference identities across compression changes.
- `<sha256>.manifest.json`: small original provenance manifests.
- `references.json`: historical reference paths mapped to the consolidated files.

Earlier result notes use paths from before consolidation. `references.json`
maps those paths to the current files, preserving the recorded native versions.

Both archives and reference traces use zstd's highest standard level, 22.
Reference data is unchanged: recompression verifies the SHA-256 of the
decompressed bytes before removing the original. The small indices and
provenance manifests remain readable JSON. Focused synthetic micro fixtures
and layer-discovery fixtures keep their existing test folders.

Validate archive integrity without replaying the Lua runtime:

```powershell
cargo test --test song_lua_itgmania_semantic_parity whole_song_archive_index_and_streamed_members_are_valid
```

`capture-report.json` and `remaining-charts.md` retain the native capture audit
and explain uncaptured songs. Fixtures do not imply that DeadSync currently
passes every native comparison; the runner reports remaining parity gaps.

After intentionally regenerating references with `itgmania-harness-rs`, use
`scripts/normalize_song_lua_fixtures.py` (Python and the zstd CLI) to normalize
compression and update content addresses. Native baseline updates must retain
their source revision, dependency hashes, and capture provenance.

## Harness recovery audit (2026-10-07)

The sibling `../itgmania-harness` repair added 71 verified complete
archives, bringing the primary index to 483 of 501 simfiles. 18 remain
pending; see `remaining-charts.md` and `capture-report.json` for reasons and
capture provenance. Eighteen recovered simfiles have no Lua references and
therefore contain empty actor captures.

Loop and Space Creation Theory use their Edit charts; Fiji uses Hard. The
Challenge contexts for those songs remain unresolved. Megalovania generated
data only in an isolated capture copy. Every new archive uses level-22 zstd
compression, with decompressed contents and member hashes verified. Existing
fixtures were preserved. DeadSync fixture tests have not been run for this
capture update.

Each primary entry in `index.json` records `harness_version`, read from its
archive manifest. New harness captures write the same version in both files.
The runner checks the values match and reports the version before comparison.
Retained archives report their own manifest version. Versions describe the
generating harness; recompression does not change that provenance.

## Local movie archives

Archives over 100,000,000 bytes are kept locally and explicitly ignored by
Git. Their index entries retain hashes, aliases, harness version, and
`local_only: true`. When present, they receive every normal comparison.
Fresh checkouts report unavailable local-only archives and omit them from
bulk runs; explicitly selecting an absent archive fails with an explanation.
Missing tracked fixtures still fail validation. Recreate the two Episode 16
captures from the original corpus using harness `0.1.4` and level-22 zstd.

## Model diagnostic revalidation (2026-10-09)

The original KABOOM case 18 was recaptured privately with harness `0.1.39`
at `1ceab4b`, using unchanged song and Cyber noteskin inputs. The capture
reaches the native chart endpoint and records all eight Model tracks at
all 11,969 updates, for 95,752 samples. Archive member hashes were verified.

DeadSync `6ef81312c` passes 9,450,796 of 9,450,796 Model comparisons,
including transformed UVs with native per-vertex texture flags and material
update history. `model-coverage-audit.json` records the exact source pins,
executable, archive, trace and test-log hashes, coverage and remaining gaps.

This is a Model diagnostic result. World/view coordinates, texture identity,
lighting, additional render states and timing controls still need verification.
The whole-song Model acceptance guard remains closed. The private capture
does not replace the canonical archive or add an index entry; the full corpus
is still incomplete.

The expanded comparator at `807df3fe4` additionally verifies every recorded
Model world/view coordinate against production matrices. It passes
12,217,036 of 12,217,036 checks on the same private capture; the audit retains
this separate executable and log provenance. That run predates the subsequent
same-name mesh loader repair and uses the harness `0.1.39` reference. Harness
`0.1.40` corrects the hardware mesh path; refresh the original reference before
claiming complete Model parity. Texture identity, lighting, additional render
states, timing controls and framebuffer validation remain unresolved.

The fresh original capture from harness `0.1.41` at `b57bf3c` now includes
both the corrected hardware mesh path and native Model update ordering.
DeadSync `d5e0b6704` passes 12,217,036 of 12,217,036 comparisons
on that capture, retaining the original endpoint, all eight Model tracks and
every update. The audit records this separate reference and comparator pin.

An independent native actor scene also verifies queued `setstate`, own and
parent hibernation, and parent update rate through the production compiler
and both Model builders: 1,688 draws and 10,128 transformed UV coordinates
at 0.000001 tolerance. The harness checks suppressed and visible draws against
the same native scene; the playback test checks material history on native
draws after wakeup. Texture identity, secondary material animation, lighting,
additional render states, empty meshes, late creation and framebuffer
validation remain pending. The whole-song Model guard stays closed and the
fresh diagnostic archive stays private.

Harness `0.1.42` at `a80f15b` now observes registered native texture identity
and sampler state for every Model draw. An independent 61-update scene
retains all 183 diffuse, additive and glow draws with four distinct images.
DeadSync `ad222a896` fixes the secondary material's independent update
clock and queued state selection, replacing its elapsed-time selector.
Both production builders match 122 submitted secondary rectangles and
translations (488 coordinates at 0.000001 tolerance); 193 playback tests,
the material asset control and gameplay shell check pass. The audit retains
this separate control and its before/after evidence.

The earlier 12,217,036-check result remains pinned to its original harness
0.1.41 and DeadSync sources. It does not compare the newly observed texture
bindings and is not a current full-song validation. Separate additive
rendering, forced secondary filtering, per-image atlas wrapping, physical
image identity, secondary per-vertex scaling and the other recorded Model
gaps remain open. The whole-song Model acceptance guard stays closed.

DeadSync `e92b253f7` now submits separate diffuse, additive and
glow passes through both production Model builders. The same retained
native material scene verifies 366 production draws, 2,196 transformed
UV coordinates and 4,392 unlit color components at 0.000001 tolerance.
A second independent native scene checks 36 per-material sphere flags
and geometry cache separation. Geometry is precomputed before gameplay;
the prewarm regressions, all 194 playback tests and the shell check pass.

The audit appends this source pin and its before/after evidence without
changing historical results. UV comparisons explicitly map individual
native images into the current atlas. Forced secondary filtering,
per-image wrapping, physical texture identity, sphere GPU coordinates,
lighting, other render states and framebuffer validation remain open.
No original archive was replaced or accepted by this control repair.
The whole-song Model guard stays closed and full corpus work continues.

The original KABOOM case 18 was recaptured privately with harness 0.1.42
at `a80f15b`, using unchanged song and Cyber inputs. The archive reaches
the original endpoint and retains all eight Model tracks at all 11,969
updates (95,752 samples). Every one of its 2,712 draws now records a
bound source image, filtering, wrapping and sphere state.

The current comparator at `1c083768b` requires that metadata and compares
actual submitted image identities, resolving noteskin paths only through
the verified native inventory. It passes 12,565,528 of 12,568,240 checks
on this fresh capture. The 2,712 failed observations all report the
missing production per-draw sampler representation, with eight distinct
pass messages. Recorded geometry, transforms, UVs, unlit colors, image
identities and sphere flags produced no other failures. This original
scene binds one Cyber image; it does not close multi-image atlas gaps.

Three metadata controls and the archive guard pass. Root debug symbols
were disabled only by the test command after the ordinary Windows bin
link hit a PDB limit; repository build settings were preserved. The audit
retains the exact source, executable, archive, trace and log hashes.
The failed diagnostic archive remains private, the canonical index is
unchanged and the whole-song Model acceptance guard remains closed.


The sampler gap from that capture is closed for its recorded draws by
`455663e3f`. Model filtering/wrapping now travels through the render IR,
batching, clipping and all five backends. Immutable GPU sampler bindings
are prepared at texture creation. Nearest overrides sample the base image;
secondary materials force linear filtering; diffuse/glow retain Actor
sampling; Model wrapping starts enabled as in the native constructor.

The current frozen comparator passes all 12,568,240 recorded comparisons
on the unchanged harness 0.1.42 original KABOOM capture, including its
2,712 sampler comparisons. The native image control also checks 732
sampler booleans across 366 draws through both production builders.
Renderer controls, playback, application checks, DX12/Vulkan WGPU readback
and Metal cross-type-checks pass. Fractional UNORM readback permits one
byte of quantization; nearest and endpoint colors remain exact.

These observations do not establish complete Model framebuffer parity.
Per-image physical bindings and atlas wrapping, mip/trilinear behavior,
sphere GPU coordinates, lighting, other render states, empty draws and
late Model creation still require independent native verification.
Native Metal, GL and Vulkan backend framebuffer checks also remain open.
The private archive is retained, the canonical index is unchanged, and
the complete Model acceptance guard remains closed. All 501 simfiles,
492 archive contexts, aliases, variants and unindexed sources remain in
the goal scope.


Per-state image selection is repaired in `e7252ef75`. Model animations now
retain their individual source images and full UV domains, including unequal
image dimensions and repeated image states. Replay records both material
state indices; gameplay and preview preparation load every image before use.
The generated Model atlas path is removed.

The independent harness 0.1.42 image control verifies 366 actual image
bindings and 2,196 native UV coordinates through both production builders,
without mapping native UVs into an atlas. All 859 asset/noteskin/note-field
checks, 194 playback checks, the preview residency control and shell build
pass. The comparator frozen from this committed source also passes all
12,568,240 comparisons on the unchanged original KABOOM capture. Metadata
controls and the complete-archive guard pass.

This proves selected image identity and recorded draw parameters for these
controls. Native framebuffer output, wrapping pixels, image preprocessing,
mip/trilinear sampling, sphere coordinates and the remaining Model states
still require independent verification. The private diagnostic archive and
canonical index are retained; complete Model acceptance remains closed.
The full 501-source/492-context scope is unchanged.


Harness 0.1.43 (`32b73981`) repairs a separate reference-adapter error:
Model's explicit stretch request was ignored. The 5x9 native request
control now reports an 8x16 image and allocation, and retains requested
stretch/mipmap/hot-pink flags in all three material passes. All 202
harness tests pass, with five existing corpus tests ignored.

The portable `model-texture-request` micro fixture pins this clean native
capture. A runtime probe of the committed DeadSync decoder confirms a
5x9 opaque-pink image with no Model stretch or mip defaults. That is an
open production gap. The native capture proves requests and headless
dimensions; preprocessing pixels, effective mip state, mip storage and
framebuffer output still need independent verification. Complete Model
archive acceptance remains closed, and the canonical index is unchanged.


Harness 0.1.44 (`f812935e`) links the pinned native surface utilities for
CPU pixel controls and removes the no-op Zoom stub. Twelve controls cover
RGB/RGBA/palette hot-pink selection, native resizing and hidden-alpha
cleanup. The local reference sources match the pinned native sources.
All 204 harness tests pass; five existing corpus tests remain ignored.

These controls exposed a production decoder error: fully transparent
images retained hidden RGB values, while native FixHiddenAlpha clears
them to black. DeadSync `3b1ef26f7` removes that early-return path. Three
actual PNG decode controls now match all 40 native RGBA bytes exactly,
covering fully transparent, uniform-edge and mixed-edge images. All 220
asset tests pass. The new portable `texture-surface` micro fixture retains
the clean native capture and binary pixel goldens; JSON line endings are
fixed so its source hashes survive checkout.

This closes the verified hidden-RGB cleanup gap for these decoder cases.
Model default stretch/color-key preparation, distinct Sprite/Model resource
identities, effective mip state, physical mip storage and native framebuffer
output remain open. Complete Model acceptance stays closed; the canonical
index and the full 501-source/492-context scope remain unchanged.


DeadSync `bd18c8663` keeps native Model images separately resident from
Sprite images and applies the native CPU color-key/stretch preparation.
Harness 0.1.45 (`2eb6b85d4`) supplies bounded native
surface controls for the Model 2048 size limit. The seven local reference
files match the pinned ITGmania source.

All 21 production PNG controls match 69,776 compiled native RGBA bytes
exactly. These cover palette duplicates, packed indices and alpha, 16-bit
high-byte stripping, edge-key selection, iterative rounding, odd sizes and
mixed-axis resizing. The shared-image control retains the Sprite's original
5x9 opaque image while preparing an independent 8x16 transparent Model view.
The portable `model-texture-preparation` fixture retains the independent
native output, source PNGs and pixel goldens.

Asset, noteskin, note-field, playback and preview checks pass, and the shell
compiles. The original KABOOM capture is revalidated with the committed
comparator; source chart and cyber noteskin bytes remain unchanged. The
12,568,240 recorded material/geometry comparisons pass exactly; they do
not prove framebuffer pixels.

Native file-loader execution, indexed GIF/BMP behavior, full bitmap
format/settings/device-size policy, effective mip state, physical mip
storage and native framebuffer output remain open. Complete Model acceptance
stays closed and the full 501-source/492-context scope remains unchanged.


DeadSync `4e68a4a48` extends native PNG high-byte stripping to
ordinary texture views. Four actual file-loader controls from committed harness
0.1.46 (`17e23b4a9`) match all 560 decoded RGBA bytes exactly,
covering RGB16, RGBA16, grayscale16 and grayscale-alpha16. The previous generic
path produced red 24 for a sample that native decoding produces as 23.
All 224 asset tests and the existing 8-bit decode integration pass.

The file oracle executes pinned PNG/GIF/BMP/JPEG loaders and RageBitmapTexture
preparation with explicit settings. Its 24 controls distinguish adjusted IDs
from final upload format and mipmap requests. The 14 local source files match
the pinned reference after line-ending normalization. The portable
`bitmap-file-decoding` micro fixture retains raw native output, source files,
byte goldens and source/codec/executable hashes.

Harness 0.1.47 (`40790ab22`) also applies game texture defaults
extracted from native PrefsManager initializers. A 2051x3 Model now uses a
2048x8 image/allocation matching actual RageBitmapTexture with game settings;
the constructor-only capture previously capped it to 1024x8. All 208 harness
tests pass, with five existing corpus tests ignored and dependency pins
preserved. Explicit file-oracle profiles retain identical native results.

Sprite upload pixels remain excluded because unused native POT padding is not
fully initialized. Production indexed GIF/BMP handling, default tiny Sprite
stretching, complete bitmap policy, effective mips, physical mip storage and
native framebuffer output remain open. Complete Model acceptance stays closed;
the canonical archive index and full 501-source/492-context scope are unchanged.


DeadSync `d0d3746ce` and `59f62fd3b`
close the indexed BMP/GIF decoder controls using actual native file-loader and
CPU upload output from committed harness 0.1.48 (`2ea017def`).
All 35 original inputs are covered: 33 exact pixel comparisons (8,704 RGBA
bytes, zero tolerance) and two matching decoder/source-probe rejections.
The baseline had 22 failing pixel controls. Windows/OS2 BMP1/4/8, low-first
BMP4 packing and odd rows, palette identity/keying, GIF local/global tables,
transparency, offsets, interlacing and first-frame selection are retained in
the portable `indexed-bitmap-files` fixture. The 15 local reference files
match the pinned sources after line-ending normalization.

Harness 0.1.49 (`e5ee5826a`) replaces handwritten regular
texture and archive metadata probes with actual native LoadFile. A valid GIF
Model previously rejected by the PNG/JPEG-only adapter now matches all three
native source/image/allocation observations. The 28 prior bitmap controls are
unchanged. All 210 harness tests pass (five existing skips); DeadSync passes
1,434 current unit/integration tests, including compiled Lua getters and Model
registration. Existing dependency pins are preserved.

The native header oracle also loads all 21 original-corpus GIF files. Their
first-frame sizes match their canvas sizes; this inventory does not establish
full-song rendering parity. Sprite upload pixels are now captured only when
their power-of-two inputs fill the allocation; other undefined padding remains
excluded. Tiny Sprite preparation/metadata/UV behavior, true-color BMP and JPEG
precision, other generic image consumers, complete bitmap policy, Lua error
behavior, physical mips and native framebuffer output remain open. Complete
Model acceptance remains closed and the original 501-source/492-context scope
and canonical archive index are unchanged.

## Fresh native revalidation, pass 101

Warp Zone was recaptured from the unchanged original song with harness
`0.1.49`, using the pinned native ITGmania implementation. The current
DeadSync comparator passed 212,220 checks, including complete composition
over 156.56 seconds and the recorded player transforms. The native capture
contains 9,395 update frames, no runtime errors, and no dropped events.
The refreshed archive uses zstd level 22; recompression preserves all
decompressed native bytes. The previous archive and historical aliases
remain available. `native-revalidation-pass101.json` records the source,
CLI, trace, archive, and verification identities.

This is the first chart in the renewed corpus pass. The original scope
remains 501 simfiles and 492 contexts; full corpus and native framebuffer
parity are still incomplete.

Position 2, Let Me Hear That, also passed 205,071 comparisons against
a fresh `0.1.49` native capture: 145.09 seconds, 8,707 update
frames, no native runtime errors or dropped events. Its original archive
and historical aliases remain available. Full corpus and framebuffer
parity are still incomplete.

Position 3, Waltz Capriccio, also passed 260,300 comparisons against
a fresh `0.1.49` native capture: 141.01 seconds, 8,462 update
frames, no native runtime errors or dropped events. Its original archive
and historical aliases remain available. Full corpus and framebuffer
parity are still incomplete.

## Fresh native revalidation, pass 102

Complete movie archives over 100 MB retain the native level-12 encoding
and stay ignored locally. The small index and proof records are committed;
the local archive remains usable with the full-song runner.

Position 4, `100 Bad Days/100 Bad Days.sm`, passed 175,181 comparisons
against fresh native `0.1.49` capture over 131.98 seconds and 7,920
update frames, with no native runtime errors or dropped events. Original
sources and historical aliases are retained. This checks movie geometry
and blend/command behavior; movie pixels and native framebuffer parity
remain unproven. `native-revalidation-pass102.json` records the identities.

Position 5, `100 Bad Days/100 Bad Days.ssc`, passed 175,181 comparisons
against fresh native `0.1.49` capture over 131.98 seconds and 7,920
update frames, with no native runtime errors or dropped events. Original
sources and historical aliases are retained. This checks movie geometry
and blend/command behavior; movie pixels and native framebuffer parity
remain unproven. `native-revalidation-pass102.json` records the identities.

Position 6, 10:35, passed 248,156 comparisons
over 166.62 seconds and 9,999 native update frames.
The native capture has no runtime errors or dropped events; original
sources, prior captures, and historical aliases are retained.

The first six refreshed archives passed 1,276,109 comparisons on
DeadSync `55212b44e`, after the native texture image getter fix. Ten
native bitmap controls and ten assertions through the native Lua texture
binding establish getter values; 1,206 domain tests pass. See
`../itgmania-song-lua-micro/texture-image-getters` for native inputs and
goldens. Physical tiny Sprite uploads and framebuffer parity remain open.
Full corpus and native framebuffer parity are still incomplete.

Position 7, 188|HS|Holdswitch[lv.08] the shadow, passed 345,742 comparisons
over 130.00 seconds and 7,801 native update frames.
The native capture has no runtime errors or dropped events; original
sources, prior captures, and historical aliases are retained.
Full corpus and native framebuffer parity are still incomplete.

## Fresh native revalidation, pass 103

Position 8, ChikuTaku, passed 328,107 comparisons over
221.78 seconds and 13,308 native update frames. The original
simfile is unchanged, the native capture has no runtime errors or dropped
events, and decompressed archive bytes are preserved by lossless level-22
recompression. Prior captures and historical aliases remain available.
`native-revalidation-pass103.json` records the source and verifier identities.
The original 501-source/492-context scope is unchanged. Full corpus,
physical tiny Sprite uploads, and native framebuffer parity remain open.

The native tiny Sprite upload oracle is repaired in harness `0.1.50`.
Ten varied-color and alpha controls retain native metadata and nine
fully initialized prepared images (68,608 RGBA bytes), with byte-identical
repeat captures. See `../itgmania-song-lua-micro/texture-sprite-preparation`.
A 7x9 control excludes unused allocation padding. DeadSync physical
tiny uploads, logical source binding, image-coordinate offsets and UV
mapping still need repair against these native controls. Framebuffer
and complete corpus parity remain open.

## Fresh native revalidation, pass 104

Position 9, Spooky, passed 157,678 comparisons over
118.01 seconds and 7,082 native update frames. The original
song files are unchanged. Native harness `0.1.50` reports no runtime errors
or dropped events. Lossless level-22 recompression preserves decompressed
native bytes; prior captures and aliases remain available.

The first nine refreshed archives passed 2,107,636 comparisons
on committed DeadSync `20e14e048`, after its Sprite preparation repair.
Ordinary Sprite uploads now apply native minimum-eight and maximum-2048
sizing while retaining logical source dimensions. The default high-resolution
controls compare eight complete prepared images (68,352 native RGBA bytes),
with an additional padded NPOT metadata control. Software upload readback,
startup jobs, replacement handling and tiny image offsets are checked;
1,437 domain tests pass. See `native-revalidation-pass104.json` and
`../itgmania-song-lua-micro/texture-sprite-preparation`. Earlier pass
sections describe the source at that time. Alternate low-resolution
profiles, complete NPOT coordinate/sampling behavior, native GPU/mip
and framebuffer output remain open. The original 501-source/492-context
scope is unchanged; full corpus parity is incomplete.

## Ordered fixture validation, pass 105

DeadSync `def9a12f1` restores Technique's startup Model texture bindings.
Seven Technique checks pass, including actual OpenGL arrow pixels and
advancing rotation. The existing archives at positions 10–18 pass, as
does the fresh Riddle capture: 2,064,514 comparisons across ten archives
covering nine canonical positions. Completed logs removed from the local
build directory were recovered by repeating only the affected checks.

The first remaining case is position 19, KABOOOOOM. Harness `0.1.51`
captures 11,969 updates over 199.46 seconds with no runtime errors or
dropped events. Its complete 42,039,171-byte native archive is retained,
along with the earlier version. Its production Model comparison passes
12,568,240 checks. DeadSync `fb9d04ed4` removes the blanket Model
rejection that prevented these comparisons from running in the archive
suite. Complete per-actor/update observations remain required; missing
Model tracks and incomplete columns still fail. The complete archive
passes 14,842,953 comparisons, with no failures.

The two source revisions above validate 16,907,467 comparisons across
eleven archives covering positions 10–19 and the retained Riddle
version. See `native-revalidation-pass105.json`. Position 20 is next;
the original 501-source/492-context scope remains active. These checks
establish the captured semantics and represented render states; native
GPU framebuffer parity and unsupported rendering profiles remain open.

## Ordered fixture validation, pass 106

Positions 20 through 24 pass 15,929,057 comparisons
on committed DeadSync `fb9d04ed4`. Position 20
initially stopped because its old harness 0.1.31 capture omitted
Actor base rotation. Its original chart and Lua files are unchanged;
a complete harness 0.1.51 native capture supplies the missing observations.
The previous archive and historical aliases remain available.

Position 20 records 11,969 updates over
199.46 seconds, with no native runtime errors or
dropped events; 14,842,953 comparisons pass.

See `native-revalidation-pass106.json`. Position 25 is next.
The original 501-source/492-context scope remains active. Full corpus,
unsupported rendering profiles, and native GPU framebuffer parity
remain incomplete.

## Ordered fixture validation, pass 108

Position 25, 321STARS, passes all 51,262,467 comparisons
against DeadSync `e645dfdda`. The old harness 0.1.31
capture omitted Model base rotation; a complete 0.1.51 capture retains
the original chart and Lua bytes and records 4,794 updates with no runtime
errors or dropped events. The earlier archive and aliases remain available.

The fresh capture initially failed 264,768 texture comparisons because
the comparator used music time for material history indexed by elapsed
time. An independent native actor control confirms elapsed deltas. The
positive-offset regression passes 561,139 checks after the correction.
No production behavior or comparison tolerance was changed.

See `native-revalidation-pass108.json`. Position 26 is next. The original
501-source/492-context scope remains active. The record retains additional
explicit Model culling and multitap boundary observations; full corpus
and native framebuffer parity remain incomplete.

## Ordered fixture validation, pass 109

Positions 26 through 30 pass 3,812,806 comparisons
against DeadSync `e645dfdda`. Each complete archive
was compiled and compared in order; execution stops at the next failure.

See `native-revalidation-pass109.json`. Position 31 is next.
The original 501-source/492-context scope and additional pending
observations remain active. Full corpus and framebuffer parity are open.

## Ordered fixture validation, pass 110

Position 31, And Drugs, passes 182,791 comparisons against committed
DeadSync `e645dfdda`. Its older continuous-bpm clock was rejected. The
complete harness 0.1.51 capture supplies native timing and records 7,634
updates over 127.20 seconds, with no native errors or dropped events.
Original chart and Lua bytes, the previous archive, and aliases are retained.

See `native-revalidation-pass110.json`. Position 32 is next. The original
501-source/492-context scope and additional pending observations remain
active. Full corpus and framebuffer parity are still open.

## Ordered fixture validation, pass 111

Position 32, [09] Delightful Day, passes 282,140 comparisons
against committed DeadSync `e1557ce33`. The complete
harness 0.1.51 native capture records 12,740 updates
over 212.31 seconds, with no native runtime errors
or dropped events. Original chart and Lua bytes, the prior archive, and
historical aliases are retained.

See `native-revalidation-pass111.json`. Position 33 is next. The original
501-source/492-context scope and additional pending observations remain
active. Full corpus and framebuffer parity are still open.

## Ordered fixture validation, pass 112

Position 33, [3878] [09] LALA (Hard), passes 203,003 comparisons
against committed DeadSync `e1557ce33`. The complete
harness 0.1.51 native capture records 8,309 updates
over 138.46 seconds, with no native runtime errors
or dropped events. Original chart and Lua bytes, the prior archive, and
historical aliases are retained.

See `native-revalidation-pass112.json`. Position 34 is next. The original
501-source/492-context scope and additional pending observations remain
active. Full corpus and framebuffer parity are still open.

## Ordered fixture validation, pass 113

Position 34, [3959] [09] Spectrum Sequence (Hard), passes 252,881 comparisons
against committed DeadSync `e1557ce33`. The complete
harness 0.1.51 native capture records 8,178 updates
over 136.28 seconds, with no native runtime errors
or dropped events. Original chart and Lua bytes, the prior archive, and
historical aliases are retained.

See `native-revalidation-pass113.json`. Position 35 is next. The original
501-source/492-context scope and additional pending observations remain
active. Full corpus and framebuffer parity are still open.

## Ordered fixture validation, pass 114

Position 35, [3752] [09] Who the Hell Is Edgar?, passes 228,297,347 comparisons
against committed DeadSync `d7cff0db6`. The complete
harness 0.1.51 native capture records 9,197 updates
over 153.26 seconds, with no native runtime errors
or dropped events. Original chart and Lua bytes, the prior archive, and
historical aliases are retained.

The callback fix retains hit commands when native material clocks
require chronological replay. The 75 native explosion comparisons, all
40 player/lane/grade command checks, and 29 texture samples pass. The
material control uses its native 120 Hz deltas, including partial updates;
all original expected values and tolerances are retained.

Native TimingData uses separate float multiplication and addition for
scroll prefixes and displayed-beat queries. DeadSync now does the same;
its prior fused arithmetic shifted arrow offsets by 1/512px, magnified
by Model projection. Six exact native golden offsets cover ordinary
and cached queries, and all 45 timing tests pass.

Lossless recompression keeps the complete native tar stream identical
while bringing this archive below 100 MB. The separate authored-onset
test still requires correction of its instantaneous-update assumption;
its onset, position and countdown assertions remain active.

See `native-revalidation-pass114.json`. Position 36 is next. The original
501-source/492-context scope and additional pending observations remain
active. Full corpus and framebuffer parity are still open.

## Ordered fixture validation, pass 115

Position 36, [5604] [10] flip69, passes 216,773,579 comparisons
against committed DeadSync `e7fe82d02`. Its complete native
harness 0.1.51 capture contains 7,569 updates over
126.13 seconds, with no runtime errors or dropped events.
Lossless compression preserves every tar byte and reduces the archive
from 121,696,985 to 99,480,363 bytes, below 100 MB. Original
chart, Lua, earlier archive and aliases are retained.

ArrowEffects now reads the shared music clock rather than inverting a
fixed beat. Thirty native stop/delay timestamps pass 540 exact checks
through both player states and six music rates. Edgar also passes all
228,297,347 complete-archive comparisons after this fix. Its native
frame onset/countdown checks are fixed and passing; eight independently
measured off-grid positions remain open at the original tolerance.

See `native-revalidation-pass115.json`. Position 37 is next. Song/Steps
timing getter placeholders, explicit Model culling, the original
501-source/492-context corpus and native framebuffer parity remain open.

## Ordered fixture validation, pass 116

Position 37, [5811] [10] Lake of Lost Nostalgia, passes all 323,368 comparisons against
committed DeadSync `207e17110`. The complete harness 0.1.52
native capture records 7,801 updates, with no native runtime errors or
dropped events. Original chart, Lua, prior archive and aliases remain.

Simply Love uses an ActorFrame as the Judgment root, with an initially
hidden Sprite child. Draw capture now retains player HUD containers as
external sources. All 7,801 custom-draw frames match the seven native
requests; four focused draw tests and the archive integrity check pass.
No requests were filtered and no tolerances changed. See
`native-revalidation-pass116-draw.json` for source evidence and
`native-revalidation-pass116.json` for the complete archive result.

Position 38 is next. Independent Edgar off-grid positions, timing
getter placeholders, explicit Model culling, remaining original sources
and contexts, and native framebuffer parity remain open.

## Ordered fixture validation, pass 117

Position 38, [4252] [10] media offline (Medium), passes all
582,201 comparisons against committed DeadSync
`207e17110`. The complete native harness 0.1.52 archive
contains 8,070 updates over 134.48 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass117.json`. Position 39 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 118

Position 39, [4914] [10] Riddle, passes all
101,059 comparisons against committed DeadSync
`207e17110`. The complete native harness 0.1.52 archive
contains 8,453 updates over 140.86 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass118.json`. Position 40 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 119

Position 40, [10] Riddle, passes all
201,819 comparisons against committed DeadSync
`207e17110`. The complete native harness 0.1.52 archive
contains 8,453 updates over 140.86 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass119.json`. Position 41 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 120

Position 41, [6005] [11] CO5M1C R4ILR0AD (Hard), passes all
199,543 comparisons against committed DeadSync
`207e17110`. The complete native harness 0.1.52 archive
contains 8,084 updates over 134.71 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass120.json`. Position 42 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 121

Position 42, [11] 時計の国のジェミニ, passes all
193,408 comparisons against committed DeadSync
`207e17110`. The complete native harness 0.1.52 archive
contains 8,421 updates over 140.33 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass121.json`. Position 43 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 122

Position 43, [5800] [11] KENPO SAITO, passes all
102,078 comparisons against committed DeadSync
`32216ad9e`. The complete native harness 0.1.52 archive
contains 9,165 updates over 152.73 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass122.json`. Position 44 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 123

Position 44, [5916] [11] Let Me Hear That, passes all
205,071 comparisons against committed DeadSync
`32216ad9e`. The complete native harness 0.1.52 archive
contains 8,707 updates over 145.09 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass123.json`. Position 45 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 124

Position 45, [5813] [11] 西新宿清掃曲, passes all
167,463 comparisons against committed DeadSync
`ba32031b5`. The complete native harness 0.1.52 archive
contains 6,147 updates over 102.43 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass124.json`. Position 46 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 125

Position 46, [5408] [11] Palette Lab (Hard), passes all
183,127 comparisons against committed DeadSync
`ba32031b5`. The complete native harness 0.1.52 archive
contains 8,198 updates over 136.62 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass125.json`. Position 47 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 126

Position 47, [6210] [11] Slamurai, passes all
541,079 comparisons against committed DeadSync
`ba32031b5`. The complete native harness 0.1.52 archive
contains 7,311 updates over 121.82 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass126.json`. Position 48 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 127

Position 48, [12] And Drugs↑↑, passes all
182,791 comparisons against committed DeadSync
`ba32031b5`. The complete native harness 0.1.52 archive
contains 7,634 updates over 127.20 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass127.json`. Position 49 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 128

Position 49, [7086] [12] Blacksphere, passes all
158,515 comparisons against committed DeadSync
`ba32031b5`. The complete native harness 0.1.52 archive
contains 7,178 updates over 119.61 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass128.json`. Position 50 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 129

Position 50, [12] Bunny House, passes all
348,857 comparisons against committed DeadSync
`ba32031b5`. The complete native harness 0.1.52 archive
contains 9,158 updates over 152.61 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass129.json`. Position 51 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 130

Position 51, [6804] [12] Godspeed, passes all
601,973 comparisons against committed DeadSync
`ba32031b5`. The complete native harness 0.1.52 archive
contains 8,250 updates over 137.48 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass130.json`. Position 52 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 131

Position 52, [7287] [12] Myths You Forgot, passes all
560,613 comparisons against committed DeadSync
`ba99d38a7`. The complete native harness 0.1.53 archive
contains 7,527 updates over 125.42 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass131.json`. Position 53 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 132

Position 53, [7124] [12] Picture in My Mind, passes all
495,069 comparisons against committed DeadSync
`ba99d38a7`. The complete native harness 0.1.53 archive
contains 6,865 updates over 114.40 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass132.json`. Position 54 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 133

Position 54, [12] Tacos, passes all
88,204 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 8,015 updates over 133.57 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass133.json`. Position 55 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 134

Position 55, Flying Castle, passes all
174,687 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 7,367 updates over 122.76 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass134.json`. Position 56 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 135

Position 56, [14] [CRYSTAL_ACCESS], passes all
194,374 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 8,641 updates over 144.00 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass135.json`. Position 57 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 136

Position 57, [14] I'm For You, passes all
343,432 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 13,326 updates over 222.07 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass136.json`. Position 58 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 137

Position 58, [CRYSTAL_ACCESS], passes all
194,374 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 8,641 updates over 144.00 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass137.json`. Position 59 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 138

Position 59, [CRYSTAL_ACCESS], passes all
194,374 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 8,641 updates over 144.00 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass138.json`. Position 60 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 139

Position 60, [F]FS+BR(lv.9) Bunny House, passes all
421,837 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 9,158 updates over 152.61 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass139.json`. Position 61 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 140

Position 61, [F]FS+BR(lv.9) Bunny House, passes all
421,837 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 9,158 updates over 152.61 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass140.json`. Position 62 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 141

Position 62, [FULL SONG] ChikuTaku, passes all
328,107 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 13,308 updates over 221.78 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass141.json`. Position 63 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 142

Position 63, [FULL SONG] 花月ノ夢, passes all
324,861 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 13,358 updates over 222.61 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass142.json`. Position 64 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 143

Position 64, [FULL SONG] 粛聖!! ロリ神レクイエム☆, passes all
394,677 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 16,125 updates over 268.73 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass143.json`. Position 65 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 144

Position 65, [FULL SONG] Stuck in the Abyss, passes all
285,701 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 11,905 updates over 198.40 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass144.json`. Position 66 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 145

Position 66, [N]TECH SOUP(MASTER) Flying Castle, passes all
172,269 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 7,284 updates over 121.38 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass145.json`. Position 67 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 146

Position 67, [N]TECH SOUP(MASTER) Flying Castle, passes all
174,687 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 7,367 updates over 122.76 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass146.json`. Position 68 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 147

Position 68, happy century, passes all
184,083 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 8,367 updates over 139.43 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass147.json`. Position 69 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 148

Position 69, [T04] Palette Lab (Hard), passes all
183,127 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 8,198 updates over 136.62 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass148.json`. Position 70 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 149

Position 70, [T08] CO5M1C R4ILR0AD (No CMOD), passes all
199,543 comparisons against committed DeadSync
`6217b2455`. The complete native harness 0.1.54 archive
contains 8,084 updates over 134.71 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass149.json`. Position 71 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 150

Position 71, [T09] Picture in My Mind, passes all
495,069 comparisons against committed DeadSync
`63b08812a`. The complete native harness 0.1.54 archive
contains 6,865 updates over 114.40 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass150.json`. Position 72 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 151

Position 72, Palette Lab, passes all
183,127 comparisons against committed DeadSync
`63b08812a`. The complete native harness 0.1.54 archive
contains 8,198 updates over 136.62 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass151.json`. Position 73 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 152

Position 73, LALA, passes all
203,003 comparisons against committed DeadSync
`63b08812a`. The complete native harness 0.1.54 archive
contains 8,309 updates over 138.46 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass152.json`. Position 74 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 153

Position 74, I'm For You, passes all
300,259 comparisons against committed DeadSync
`63b08812a`. The complete native harness 0.1.54 archive
contains 13,326 updates over 222.07 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass153.json`. Position 75 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 154

Position 75, Think of a happy place, passes all
247,161 comparisons against committed DeadSync
`63b08812a`. The complete native harness 0.1.54 archive
contains 11,233 updates over 187.20 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass154.json`. Position 76 is next. The
original 501-source/492-context scope and independent timing, culling
and framebuffer observations remain open.

## Ordered fixture validation, pass 155

Position 76, A Dramatic Irony, passes all
760,235 comparisons against committed DeadSync
`415cb4223`. The complete native harness 0.1.56 archive
contains 6,643 updates over 110.69 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass155.json`. Position 77 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.

## Ordered fixture validation, pass 156

Position 77, A Op.01, passes all
159,089 comparisons against committed DeadSync
`415cb4223`. The complete native harness 0.1.56 archive
contains 7,231 updates over 120.50 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass156.json`. Position 78 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.

## Ordered fixture validation, pass 157

Position 78, Abraham's OP, passes all
238,881 comparisons against committed DeadSync
`466815487`. The complete native harness 0.1.58 archive
contains 10,857 updates over 180.93 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass157.json`. Position 79 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.

The standalone compiled ITGmania Current/Song control also passes. Its
normal Lua capture has 241 frames with no errors or dropped events.
DeadSync still fails that control at the distinct-level identity assertion;
this chart result does not close general Current/Song option parity.
The pending control is recorded in the pass JSON.

## Ordered fixture validation, pass 158

Position 79, Accelerator, passes all
445,655 comparisons against committed DeadSync
`cc6bc40b7`. The complete native harness 0.1.58 archive
contains 6,156 updates over 102.58 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass158.json`. Position 80 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.

## Ordered fixture validation, pass 159

Position 80, Accelerator, passes all
445,655 comparisons against committed DeadSync
`cc6bc40b7`. The complete native harness 0.1.58 archive
contains 6,156 updates over 102.58 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass159.json`. Position 81 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.

## Ordered fixture validation, pass 160

Position 81, Accendio, passes all
492,633 comparisons against committed DeadSync
`cc6bc40b7`. The complete native harness 0.1.58 archive
contains 6,829 updates over 113.79 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass160.json`. Position 82 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.

## Ordered fixture validation, pass 161

Position 82, Accendio, passes all
492,633 comparisons against committed DeadSync
`f9e210bf0`. The complete native harness 0.1.58 archive
contains 6,829 updates over 113.79 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass161.json`. Position 83 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.

## Ordered fixture validation, pass 162

Position 83, Actin' Up, passes all
442,107 comparisons against committed DeadSync
`73cbeaba1`. The complete native harness 0.1.58 archive
contains 5,950 updates over 99.14 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass162.json`. Position 84 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.

## Ordered fixture validation, pass 163

Position 84, Actin' Up, passes all
442,107 comparisons against committed DeadSync
`73cbeaba1`. The complete native harness 0.1.58 archive
contains 5,950 updates over 99.14 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass163.json`. Position 85 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.

## Ordered fixture validation, pass 164

Position 85, Aegleseeker, passes all
196,634 comparisons against committed DeadSync
`73cbeaba1`. The complete native harness 0.1.58 archive
contains 8,802 updates over 146.67 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass164.json`. Position 86 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.

## Ordered fixture validation, pass 165

Position 86, After Dark, passes all
491,861 comparisons against committed DeadSync
`1121d63d1`. The complete native harness 0.1.58 archive
contains 16,666 updates over 277.75 seconds, with no
runtime errors or dropped events. Its integrity check passes. Original
chart, Lua, prior archive, aliases, and all tolerances are retained.

See `native-revalidation-pass165.json`. Position 87 is next. The
original 501-source/492-context scope and independent timing and
framebuffer observations remain open. Explicit Model culling is verified
in `native-revalidation-model-culling.json`.
