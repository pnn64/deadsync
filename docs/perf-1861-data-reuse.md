# Performance pass: shared buffers, audio padding, ReplayGain encoding

Branch: `perf/1861-data-reuse-20261008`

Base: `0530013f08d5b481d29533127de2dc41033f63be` (committed main at pass start)

Version: **0.5.1860 → 0.5.1861**, including the three workspace-version entries in Cargo.lock.

## Changes

1. Construct shared audio samples and graph vertices directly with `Arc::from(Vec<T>)`.
   Remove the intermediate `into_boxed_slice()` at 23 production call sites in audio,
   line meshes, shell commands, density/QR graphs, evaluation, gameplay and practice.
   A vector with spare capacity no longer asks the allocator to shrink it immediately
   before copying its values into the final Arc allocation. The resulting Arc contains
   the same values and exact length. Exact-capacity vectors also use the simpler path.
2. Remove the music converter's retained `padded` planar allocation and the SFX
   loader's separate `resample_in` allocation. Append EOF zeros to the existing
   planar input and process it at its existing offset. Music input lengths are
   restored before error propagation and consumption, so temporary padding never
   counts as source frames or changes timestamps. SFX reuses the same input vectors
   for the final silent flush. No decoder or resampling algorithm changes.
3. Encode ReplayGain headers and entries directly into one reserved vector.
   A maximum wire-size reservation replaces the sizing traversal and zero-filled
   output buffer. Fixed-size byte arrays preserve the existing 12-byte header,
   and entries retain the standard bincode encoding and cache version.

No Song Lua source changes. No new dependencies. Existing unmerged perf branches
were checked before selection: render/chart hot paths, presentation data paths,
environment mapping/WAV buffering, resource fast paths, audio gain/planar input,
score state, resource loading, profiles, noteskins, input maps, Linux syscalls and
font formats. These changes do not repeat their pending optimizations. In particular,
the decoder's PlanarAccum and conversion loops are untouched.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.1 / LLVM 22.1.8.
Release opt-level 3 with `--locked --config profile.release.lto=false` for both
implementations. LTO-enabled release performance was not measured. Frozen originals
come from the base commit; the provenance audit permits only formatting and test
visibility changes. Originals and current implementations run in the same binary.

Each row reports the median of nine measured rounds after one warmup, alternating
which implementation runs first. Consumed Vec inputs are prepared outside the timed
region; conversion, output destruction and consumed-input destruction are timed.
Cache inputs are reused; cache output allocation and destruction are timed. SFX
benchmarks include opening the same generated PCM WAV, decoding, resampling and
destroying the returned samples; the file is warm in the OS cache. No builds or
compatibility runs overlap the final timed run. Times are wall-clock measurements,
not CPU cycle counters, and near-unity differences do not establish a CPU speedup.

| Operation | Original ns/op | Current ns/op | Throughput ratio |
| --- | ---: | ---: | ---: |
| Arc PCM 4096 spare=0 | 2,517.30 | 2,438.70 | 1.03x |
| Arc PCM 4096 spare=4096 | 4,789.30 | 4,291.00 | 1.12x |
| Arc PCM 96000 spare=32000 | 114,975.00 | 118,215.00 | 0.97x |
| Arc PCM 1000000 spare=1000000 | 1,095,937.50 | 1,080,493.75 | 1.01x |
| converter setup 2ch | 2,667,373.44 | 2,586,656.25 | 1.03x |
| converter setup 8ch | 2,568,845.31 | 2,536,234.38 | 1.01x |
| SFX load 731 | 2,860,887.50 | 2,861,746.88 | 1.00x |
| SFX load 44100 | 10,339,512.50 | 10,291,709.38 | 1.00x |
| cache 8 sparse=false | 213.64 | 200.74 | 1.06x |
| cache 10000 sparse=false | 177,781.25 | 152,209.38 | 1.17x |
| cache 10000 sparse=true | 190,803.12 | 130,243.75 | 1.46x |

Allocation accounting uses a scoped thread-local System allocator wrapper. Counts
are allocator calls/requested bytes, not process RSS or physical memory. Construction
figures below exclude destruction of the returned object:

| Allocation result | Original | Current | Improvement |
| --- | ---: | ---: | ---: |
| Vec → Arc, nonempty spare-capacity input | 1 allocation + 1 reallocation | 1 allocation, 0 reallocations | Removes every shrink call |
| Stereo music converter allocations | 274 | 271 | 3 fewer |
| Stereo converter retained heap requests | 417,278 B | 295,766 B | 121,512 B / 29.1% less |
| Eight-channel music converter allocations | 298 | 289 | 9 fewer |
| Eight-channel converter retained heap requests | 1,263,380 B | 777,332 B | 486,048 B / 38.5% less |
| 731-frame stereo SFX allocation/reallocation calls | 282 / 2 | 279 / 1 | 4 fewer calls |
| 44,100-frame stereo SFX allocation/reallocation calls | 282 / 4 | 279 / 3 | 4 fewer calls |
| 44,100-frame SFX total heap bytes requested | 1,643,122 B | 1,447,162 B | 195,960 B / 11.9% less |

SFX rows combine the Arc and padding changes; the isolated Arc test and converter
construction measurements establish each change's own allocation/memory benefit.
All tested warmed music tails perform zero allocations, reallocations or frees.

Capacity tradeoffs: the cache reserves at most `21 + 35 * entries` bytes without
zero-initializing them. Normal 10,000-entry data requested 350,021 B versus 350,007 B;
the deliberately small-integer synthetic case requested 350,021 B versus 110,015 B.
This is transient encoding capacity, and the serialized file length is unchanged.
Direct Arc construction retains the source Vec's spare capacity until the copy is
finished, instead of shrinking it first; final retained Arc memory is unchanged.
The Arc change is an allocation-churn improvement, not a claim of lower peak memory.

## Regression and compatibility checks

All pre-existing test outcomes and failure diagnostics match the baseline. New
checks compare complete PCM output and timestamp bits across 1/2/6/8 channels,
rate conversion, pitch preservation, empty/short/partial streams and repeated
resets; compare SFX samples across source/output channel and sample-rate mappings;
verify zero allocator traffic for warmed EOF drains; and compare cache bytes across
integer-width boundaries, float bit patterns and entry counts. The original cache
wire-format, v1 migration, stale-file and content-refresh tests still pass.

| Suite | Passed | Existing failures | Ignored | Identical failure diagnostics |
| --- | ---: | ---: | ---: | ---: |
| semantic | 112 | 61 | 74 | 61 |
| actor | 30 | 1 | 0 | 1 |
| playback | 1 | 0 | 0 | 0 |
| deadlib_audio-tests | 40 | 0 | 2 | 0 |
| deadsync_audio_analysis-tests | 20 | 0 | 1 | 0 |
| deadlib_present-tests | 177 | 0 | 0 | 0 |
| deadsync_theme_simply_love-tests | 1136 | 1 | 2 | 1 |
| deadsync_shell-tests | 343 | 17 | 6 | 17 |

The three ignored paired benchmarks were run separately and passed. Existing
package failures are the theme Cyber model-height fixture guard and 17 shell
failures (including subsequent poisoned session-lock failures). They also occur
on untouched committed main and were not masked or changed.

The Song Lua harness compares against the checked-in native ITGmania semantic
and actor fixtures. All 278 native outcomes and 62 failure diagnostics are
unchanged. Both Epidermis archives and Sharkmode were attempted before and after;
all three stop before behavioral comparisons at the same existing fixture guard:

`noteskin dependency changed: common/common/Fallback Receptor.lua`

- Actual: `52ffa6df0701b426d6af887f9c91cd61895d8b32f349f0cd7ddfa48bae41e74d`
- Expected: `96623726284f5ae0c5b12e05e0ae841eae40e100341a20042db68c9b80b52c74`

Those full-song checks are blocked, not successful compatibility passes. No native
fixtures, expected hashes or compatibility code were edited. Complete logs and the
machine-checked comparison are retained under this worktree's ignored `target/`.

## Reproduce

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio -p deadsync-audio-analysis --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio -p deadsync-audio-analysis --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadlib-audio --test playback -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadlib-present -p deadsync-theme-simply-love -p deadsync-shell --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance -- --test-threads=1
```

The final two commands reproduce the existing failures described above. Formatting
checks cover only changed Rust files, and `git diff --check` passes. This pass is
committed only on its perf branch and is not merged. The original checkout and
its uncommitted work were not modified. The commit excludes `deadsync-song.json.gz`,
`rust-performance.md`, `optimize.sh` and `optimize.ps1`.
