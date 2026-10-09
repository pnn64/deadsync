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
