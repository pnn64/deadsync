# Player Options preview memory

The reported 0.5.1164 panic is `wgpu error: Out of Memory` from
`prewarm_option_previews -> preload_texture_keys -> create_texture`. Player
Options prepared every selectable component runtime and uploaded their native
textures before presenting the screen. Leaving options dropped the screen's
owning Arcs and cleared the weak runtime/path caches, while uploaded textures
remained in the asset store.

The installed pinned HURG Workshop contains 1,049 component choices. All of its
PNG headers sum to 8,426.1 MiB of RGBA pixels, including the two preview atlases.
That is a source-corpus size, not a measurement of the game's RSS or the exact
subset previously uploaded by Player Options.

## Changes

- Rendering records the components drawn in rows and the eight visible picker
  results. The focused selection takes priority; immediately adjacent component
  choices are prefetched afterward. Opening Player Options constructs no
  noteskin runtimes and uploads no catalog textures synchronously.
- At most two background jobs prepare runtimes/native textures. Runtime loads
  remain serial because variants may share a compiler-cache file; texture
  decoding overlaps runtime loading and other decoding. The application thread
  polls both slots without waiting and uploads at most one texture per frame.
  Obsolete results are discarded. Leaving the screen drains pending decoded
  images without uploading them.
- The shell retains up to 32 runtimes across visits. A ready component's Arc and
  resident native textures are reused on re-entry. Source catalog or column-count
  changes invalidate the cache, including in-flight results.
- Preview-owned textures have a 256 MiB budget, conservatively charging twice
  the RGBA size when mipmaps are requested. Eviction removes the oldest hidden
  source; visible sources and pre-existing game textures are protected. Nearby
  prefetches can be evicted to make room for visible choices. The
  budget excludes existing game assets, driver overhead, and retired in-flight
  resources. It is not a total process-memory limit.
- A decoded preview is limited to 64 MiB (two outstanding images at most);
  decoder scratch and conversion allocations are additional. Source dimensions
  are checked before using the ordinary gameplay decoder. Runtime loading,
  texture keys, filename hints, sampling, native pixels, and animation data use
  the normal noteskin path. A cold icon stays empty until its native component
  is ready; it never displays a differently oriented raw atlas cell first.
- Readiness is tracked per component. A ready arrow stays visible while another
  component of its runtime loads. Texture discovery follows the quarter-note
  layers displayed by the previews, including the normal fallback layers.
- Unused installed atlases are no longer decoded/uploaded at startup or pack
  activation. HURG's two atlases accounted for 32 MiB of RGBA texture pixels.
  They remain on disk for compatibility with the installed pack format.
- Development builds optimize the `png` and `fdeflate` codec crates. Profiling
  found native texture decoding dominated cold preview latency in unoptimized
  builds. Application debug checks and the release profile remain unchanged.
- Installation decodes each unique thumbnail source once across both families:
  895 decodes instead of 1,049. Two workers send only small thumbnails through a
  bounded channel. Atlas placement remains deterministic. Cancellation/error
  releases blocked senders before joining workers.
- Extraction buffers archive reads and output writes, and creates each parent
  directory once. Preparation progress advances through extraction and both
  families without resetting. Existing valid installations continue to bypass
  extraction and compilation. Stage timings are logged.

This applies `rust-performance.md`'s M-HOTPATH and M-MEM-REUSE guidance to the
actual loading boundary and cache lifetime, and M-THROUGHPUT to bounded thumbnail
work. Steady visible-demand collection reuses its buffers and shared names.

## Validation and measured results

Windows/MSVC, Rust 1.98.0. The release atlas comparison used the pinned installed
Workshop, with filesystem caches warm and no simultaneous Cargo build. One
old/new pair measured **10.605 s -> 6.792 s** (36.0% less wall time), with identical
RGBA output for both complete atlases. This measures thumbnail decoding and
atlas assembly, excluding archive extraction, PNG encoding, and manifest
validation. It is one local measurement, not a statistical cross-machine claim.

The final preview-service validation used a debug executable with optimized PNG
codecs and no simultaneous Cargo build. Six pages of eight native variants
passed with Software and Vulkan/wgpu, including eviction after exceeding the
32-runtime cache. Both stayed at or below **256 MiB** of accounted preview-owned
textures. The first serial implementation needed 3.655–4.852 s per Software page;
two jobs with unoptimized codecs needed 2.074–2.551 s. The final results were:

| Measurement | Software | Vulkan/wgpu |
| --- | ---: | ---: |
| Eight cold choices, range across six pages | 0.426–0.558 s | 0.447–0.590 s |
| First choice ready, including first page | 0.093–0.214 s | 0.094–0.227 s |
| Warm re-entry preparation | 0.065 ms | 0.066 ms |
| Select a prefetched neighbor | 0.035 ms | 0.038 ms |
| Largest service tick, including one upload | 10.877 ms | 7.170 ms |

Warm re-entry and selecting the prepared neighbor performed no runtime load or
image decode and reused the same runtime Arc. A separate check adds an uncached
mine component and verifies the cached arrow remains ready. These are local
loading measurements, excluding the rest of the frame and screen initialization.
Earlier serial measurements overlapped builds, so the comparison is diagnostic,
not a statistical or cross-machine benchmark.

- Noteskin release unit tests: **240 passed**, 5 existing benchmarks ignored;
  the real-atlas benchmark ran separately and passed.
- Player Options tests: **123 passed**; the installed HURG row/picker animation
  test ran separately and passed, comparing native pixels and rendered layers.
- Shell unit tests: **329 passed**; its installed-Workshop service fixture ran
  separately and passed on both backends.
- Asset unit tests: **126 passed** after switching to native-only preview loads.
- Updater unit tests: **189 passed**, live download test ignored.
- `cargo build --bin deadsync --locked` rebuilt `target/debug/deadsync.exe`.

The first combined debug build exhausted C: space and produced PDB linker
failures. Builds completed after free space became available. Strict performance
Clippy found a pre-existing `large_enum_variant` in the unchanged
`SimplyLoveRuntimeRequest` enum (`effects.rs:925`); the follow-up check allows
that lint while retaining `-D clippy::perf` for the remaining checks.
The follow-up check passed with existing style warnings. `git diff --check`
also passed.

## Reproduction

The ignored fixture tests read the installed Workshop. They do not download or
modify its source files. The shell test writes compiler caches under
`target/options-preview-fixture` and creates a hidden window.

```powershell
$env:DEADSYNC_WORKSHOP_FIXTURE = 'C:\GitHub\deadsync\target\debug\assets\noteskins\hurg'
cargo test -p deadsync-noteskin --lib --locked
cargo test -p deadsync-updater --lib --locked
cargo test -p deadsync-theme-simply-love --lib --locked screens::player_options
cargo test -p deadsync-assets --lib --locked
cargo test -p deadsync-shell --lib --locked option_previews
cargo test -p deadsync-noteskin --lib --release --locked workshop_atlas_benchmark -- --ignored --nocapture
cargo test -p deadsync-shell --lib --locked workshop_preview_cache_benchmark -- --ignored --nocapture
$env:DEADSYNC_WORKSHOP_PACK = $env:DEADSYNC_WORKSHOP_FIXTURE
cargo test -p deadsync-theme-simply-love --lib --locked workshop_components_animate_in_rows_and_picker -- --ignored --nocapture
$env:DEADSYNC_PREVIEW_WGPU = '1'
cargo test -p deadsync-shell --lib --locked workshop_preview_cache_benchmark -- --ignored --nocapture
```

The atlas comparison checks every output pixel against the prior sequential
placement/decoding path. The preview service fixture browses six pages of eight
native arrow variants, exceeding the runtime cache capacity; checks memory
accounting throughout; verifies warm re-entry and selecting a prefetched neighbor
perform no load/decode; verifies per-component readiness; and compares a
software-uploaded native texture against the ordinary gameplay decoder. The
optional Vulkan/wgpu run also submits frames to exercise GPU allocation and
retirement. These are loading tests, not full-game FPS or RSS benchmarks.
