# Native component previews

Player Options now resolves the requested noteskin components through the same
compiled loader used for gameplay. An arrow preview builds Tap Note layers for
the style's columns without constructing receptors, holds, mines, or explosions.
Other components use their normal native helpers, including lift-to-tap and
active/inactive hold/roll fallbacks. Loader commands, rotation, model transforms,
animation metadata, texture paths, and texture decoding retain native behavior.

The preview runtime is a presentation snapshot with omitted fields empty. It
never enters the gameplay runtime cache. A full gameplay runtime already owned
by the session can be reused. Gameplay's existing entry points still construct
all components.

The session service records which components a snapshot contains. A new request
builds the union of existing and requested components in a worker, then publishes
the expanded snapshot. Existing components stay drawable throughout expansion;
an omitted component cannot be marked ready merely because it has no textures.
The existing 32-runtime limit, 256 MiB texture budget, two-job limit, focused
priority, adjacent prefetch, and warm screen re-entry behavior remain in effect.

This follows `rust-performance.md`'s M-HOTPATH and M-MEM-REUSE guidance: avoid
constructing unused runtime data and retain completed work across visits. It
does not reduce source texture resolution or eliminate cold image decoding.

## Validation

The installed HURG rendering test compares partial and full runtime output for
both Workshop families, all ten component types, and representative customized
variants. It compares rendered actors at five animation times and two preview
sizes, as well as ordinary decoded pixels and row/picker output. The full
baseline is loaded outside the weak gameplay cache so the test must exercise
selective construction.

The shell fixture checks that partial runtimes cannot contaminate the gameplay
cache, that previews reuse a resident full runtime, and that an already visible
arrow stays ready while a mine component is constructed and uploaded. It also
browses six pages of eight variants, exercises eviction and warm re-entry, and
selects a prefetched neighbor. Software and Vulkan/wgpu runs use hidden windows.

Windows/MSVC debug build with the existing optimized PNG codecs, installed HURG,
and no simultaneous Cargo build:

| Measurement | Software run | Vulkan/wgpu run |
| --- | ---: | ---: |
| Full runtime construction, mean of 144 loads | 25.575 ms | 25.535 ms |
| Arrow-only construction, same variants | 4.562 ms | 4.535 ms |
| Eight cold choices, range across six pages | 0.261–0.334 s | 0.306–0.333 s |
| First choice ready | 0.067–0.087 s | 0.067–0.086 s |
| Warm re-entry | 0.085 ms | 0.063 ms |
| Select a prefetched neighbor | 0.031 ms | 0.033 ms |
| Add a mine component while keeping the arrow visible | 25.367 ms | 26.737 ms |
| Largest service tick, including one upload | 9.108 ms | 6.402 ms |
| Peak accounted preview textures | 256 MiB | 256 MiB |

Runtime construction took about 82% less time. Mean visited slot references fell
from 240 to 72 for the arrow variants. Warm re-entry and selecting a prefetched
neighbor performed no runtime load or image decode. These are local loading
measurements, not total-process RSS or full-game frame-rate measurements. The
texture budget excludes other game assets and graphics-driver allocations.

Noteskin tests: 240 passed; asset tests: 127 passed; Player Options tests: 123
passed; shell tests: 329 passed. The installed HURG rendering test and both
preview-service fixture runs also passed.

Performance Clippy passed with `-D clippy::perf` and an allowance for the existing
`clippy::large_enum_variant` in `SimplyLoveRuntimeRequest`. Existing style
warnings remain. `git diff --check` passed.
`cargo build --bin deadsync --locked` rebuilt `target/debug/deadsync.exe`.

## Reproduction

```powershell
cargo test -p deadsync-noteskin --lib --locked
cargo test -p deadsync-assets --lib --locked
cargo test -p deadsync-theme-simply-love --lib --locked screens::player_options
cargo test -p deadsync-shell --lib --locked
$env:DEADSYNC_WORKSHOP_PACK = 'C:\GitHub\deadsync\target\debug\assets\noteskins\hurg'
cargo test -p deadsync-theme-simply-love --lib --locked workshop_components_animate_in_rows_and_picker -- --ignored --nocapture
$env:DEADSYNC_WORKSHOP_FIXTURE = $env:DEADSYNC_WORKSHOP_PACK
cargo test -p deadsync-shell --lib --locked workshop_preview_cache_benchmark -- --ignored --nocapture
$env:DEADSYNC_PREVIEW_WGPU = '1'
cargo test -p deadsync-shell --lib --locked workshop_preview_cache_benchmark -- --ignored --nocapture
```

The construction comparison warms both paths, alternates full/component load
order, and averages three passes over 48 arrow variants. Neither construction
path retains a full gameplay runtime between samples. The page measurements
start with an empty service/runtime/texture cache but warm filesystem/compiler
caches. They include native image decoding and upload. Slot-reference counts
include references to shared layer data and are not heap-byte measurements.
