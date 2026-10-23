# Song Lua live draw extraction and actor formatting

Baseline: `ade130230`. Workspace patch: `0.5.1875` to `0.5.1876`.

Three changes remove intermediate ownership and copying:

1. **Borrow draw labels during conversion.** The converter takes its frame
   vector and a borrowed label map directly. Live extraction no longer clones
   the map and every label string or constructs an otherwise empty `Capture`.
   Conversion invokes no Lua callbacks. Capture state is restored after both
   successful conversion and returned errors; diagnostic labels remain available
   until conversion completes.
2. **Extract immediate frames in place.** `Vec::extract_if` moves only immediate
   submissions into the output work list, preserving the normal frames in their
   existing vector and preserving both orders. This removes rebuilding the normal
   vector, its growth, and copies of retained frame headers. The retained vector
   can keep capacity from an earlier peak until the scene is released.
3. **Format actor identity once.** The actor's `__tostring` callback writes type,
   delimiters, and pointer directly into its final string. It removes the temporary
   identity string and its copy. Type aliases, String coercions/errors, Windows
   uppercase sixteen-digit identity formatting, and other platforms' pointer
   formatting remain unchanged.

These changes follow `M-HOTPATH`, `M-MEM-REUSE`, and `M-THROUGHPUT` in
[rust-performance.md](../../rust-performance.md). No production helper, cache,
dependency, or unsafe code is added. Existing raw-VM/string conversion helpers
remain unchanged; these ownership changes use safe Rust.

## Measurements

Three fresh confirmation processes give 27 samples per implementation/workload.
The table reports pooled medians per operation and median matched-pair throughput;
ratios of pooled medians can differ. Text units are individual callbacks. Draw
units are complete extractions. Reallocation bytes are included in requested bytes.

The one-normal-frame control removes one allocation and 224 requested bytes,
with paired throughput of 1.20x to 1.42x across runs. Label borrowing removes
129 allocations and 11,812 bytes in the one-normal/three-immediate/128-label
case. Actor formatting removes one allocation and one reallocation per callback.
Small controls with unchanged allocation traffic have mixed timings; no speedup
is claimed for every draw shape. The 128-normal-frame case is a stress control.

| Effect / workload | Cycles before | After | Paired throughput | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| labels / draw_quiet | 487 | 418 | 1.16x | 0.000 -> 0.000 | 0 -> 0 | 0.000 -> 0.000 |
| labels / draw_one | 4,253 | 3,519 | 1.18x | 8.000 -> 6.000 | 1 -> 1 | 2,573.000 -> 2,400.000 |
| labels / draw_normal_one | 5,439 | 4,568 | 1.16x | 9.000 -> 7.000 | 1 -> 1 | 2,797.000 -> 2,624.000 |
| labels / draw_normal_one_multi | 33,309 | 6,057 | 5.37x | 142.000 -> 13.000 | 1 -> 1 | 17,268.000 -> 5,456.000 |
| labels / draw_labels128 | 35,559 | 11,880 | 2.92x | 135.000 -> 6.000 | 1 -> 1 | 14,212.000 -> 2,400.000 |
| labels / draw_long_labels128 | 618,880 | 284,637 | 2.07x | 135.000 -> 6.000 | 1 -> 1 | 536,452.000 -> 2,400.000 |
| labels / draw_mixed | 34,276 | 6,455 | 5.21x | 142.000 -> 13.000 | 1 -> 1 | 17,268.000 -> 5,456.000 |
| labels / draw_retained128 | 16,586 | 14,466 | 1.14x | 9.000 -> 7.000 | 6 -> 6 | 16,685.000 -> 16,512.000 |
| partition / draw_quiet | 425 | 412 | 1.02x | 0.000 -> 0.000 | 0 -> 0 | 0.000 -> 0.000 |
| partition / draw_one | 3,539 | 3,539 | 1.01x | 6.000 -> 6.000 | 1 -> 1 | 2,400.000 -> 2,400.000 |
| partition / draw_normal_one | 4,349 | 3,389 | 1.23x | 7.000 -> 6.000 | 1 -> 1 | 2,624.000 -> 2,400.000 |
| partition / draw_normal_one_multi | 5,680 | 5,309 | 1.07x | 13.000 -> 12.000 | 1 -> 1 | 5,456.000 -> 5,232.000 |
| partition / draw_labels128 | 12,683 | 10,817 | 1.06x | 6.000 -> 6.000 | 1 -> 1 | 2,400.000 -> 2,400.000 |
| partition / draw_long_labels128 | 301,367 | 289,431 | 1.06x | 6.000 -> 6.000 | 1 -> 1 | 2,400.000 -> 2,400.000 |
| partition / draw_mixed | 6,455 | 5,906 | 1.09x | 13.000 -> 12.000 | 1 -> 1 | 5,456.000 -> 5,232.000 |
| partition / draw_retained128 | 14,761 | 5,117 | 3.01x | 7.000 -> 6.000 | 6 -> 1 | 16,512.000 -> 2,400.000 |
| text / tostring_Actor | 2,823 | 2,494 | 1.19x | 4.016 -> 3.016 | 3 -> 2 | 131.258 -> 86.258 |
| text / tostring_Quad | 2,892 | 2,406 | 1.21x | 4.016 -> 3.016 | 3 -> 2 | 134.266 -> 86.266 |
| text / tostring_WrapperState | 2,792 | 2,390 | 1.08x | 4.016 -> 3.016 | 3 -> 2 | 136.297 -> 112.297 |
| text / tostring_TopScreen | 2,951 | 2,456 | 1.21x | 4.016 -> 3.016 | 3 -> 2 | 165.328 -> 141.328 |
| text / tostring_Sprite | 3,154 | 2,576 | 1.19x | 4.016 -> 3.016 | 3 -> 2 | 136.266 -> 88.266 |
| text / tostring_ | 2,738 | 2,258 | 1.20x | 3.016 -> 2.016 | 3 -> 2 | 106.219 -> 82.219 |

[Full JSON evidence](2026-10-23-song-lua-live-extract.json) retains the initial
unchanged comparisons, pilot runs, expanded common-case confirmations, source and
binary hashes, stage verification, and all raw samples. The two common one-normal
controls were added after checking the live caller's normal-history reset; the
original pilot results remain in the evidence.

The unchanged baseline ran before any production edit. Frozen original functions
match `ade130230` after identical formatting. A second frozen function matches the
intermediate source after label borrowing and before vector retention. The final
harness isolates the effects in one optimized binary:

- `labels`: original extraction/conversion to label-only extraction/conversion.
- `partition`: label-only extraction to the complete production extraction.
- `text`: original native callback to the production metatable's callback.
- Default draw mode compares original extraction to both production draw changes.

Each process alternates original/current order for ten rounds and discards the
first. VM construction, fixtures, labels, source bindings, and callback installation
are outside timing. Draw batches use thirty-two independent VMs, one extraction
each. Text batches use sixteen VMs and 128 callback calls per VM; text results
are normalized per callback. Output destruction is timed; VM destruction is not.
Lua GC is stopped. The quiet control has no immediate frames and should allocate
nothing.

The worker is pinned to logical CPU 2 on an Intel Xeon E5-2696 v4 Windows x86_64
host. Rust 1.99.1 uses release optimization level three, sixteen code-generation
units, disabled LTO, static MSVC runtime, and a 32 MiB worker stack. Incremental
compilation is enabled only for song Lua, with the same release settings for both
variants. Builds and measurements are sequential. Scheduled worker cycles come
from `QueryThreadCycleTime`, including kernel execution; they are not retired
instruction PMU counts. Throughput uses elapsed time. Rust `System` allocation
counters cover the current thread, including mlua's Rust allocator traffic, and
include requested reallocation bytes. They exclude prepared input allocations,
native malloc, retained capacities, and RSS.

The unchanged controls vary by several percent, reaching roughly nine percent
for a small draw case. Desktop activity and scheduling are uncontrolled. These
are scoped measurements of extraction and formatting, with no whole-game FPS
claim. Synthetic capture inputs exercise independently sized diagnostic maps,
normal/immediate mixtures, and retained frame buffers. They do not include actor
updates, draw callbacks, mesh decoding, audio, or rendering.

## Correctness and reproduction

The new extraction comparison checks eighty-four combinations across the original,
label-only, and final functions. It compares every output and the capture state
left behind, covering quiet captures, mixed frame order, duplicate owners, pending
calls/targets, missing sources, conversion errors, and early returns. Formatter
comparisons cover type aliases, empty/long/Unicode/NUL text, pointer identity,
numeric coercion, missing values, invalid UTF-8, and conversion errors.

The unchanged full unit baseline has **824 passing tests, six failures, and forty
ignored benchmarks**. The failures are four older draw-reference comparisons,
`live_native_vertex_colors_mutate_destinations_without_allocation`, and
`compile_song_lua_accepts_vertex_diffuse_style_shims`. The latter two fail on
vertex-corner ordering assertions. These failures precede the optimizations; final validation
introduces no additional failures. The test-only compile-finalization reference
also needed the current third `bake_startup_tweens` argument before it could build;
it now uses the same actor-frame input as the production caller.

The final library suite has the same **824 passes, six failures, and forty ignored
benchmarks**, with no added failures. Both new reference tests pass, including the
eighty-four extraction/state combinations. All eleven version tests pass. The
playback integration wrapper also needed its missing production quaternion-module
import before the all-target check could compile.

Default Clippy is blocked by the unchanged `clippy::while_immutable_condition`
deny in `src/cmd.rs:349`. The all-target Clippy check completes with that single
lint allowed on the command line, retaining existing warnings. No diagnostics
reference either new performance-test file. New files pass rustfmt, and the diff
passes whitespace checks.

```powershell
cargo clippy -p deadsync-song-lua --release --locked --offline -j 2 --config profile.release.lto=false --config profile.release.package.deadsync-song-lua.incremental=true --config profile.release.package.deadsync-song-lua.codegen-units=16 --no-deps --all-targets -- -A clippy::while_immutable_condition
cargo test -p deadsync-version --release --locked --offline -j 2 --config profile.release.lto=false -- --test-threads=1
```

```powershell
$env:RUST_MIN_STACK = '33554432'
cargo test -p deadsync-song-lua --release --locked --offline -j 2 --config profile.release.lto=false --config profile.release.package.deadsync-song-lua.incremental=true --config profile.release.package.deadsync-song-lua.codegen-units=16 --lib -- --test-threads=1
```

For each isolated benchmark, set `DEADSYNC_LIVE_EXTRACT_BENCH` to `draw` or `text`
and set `DEADSYNC_LIVE_EXTRACT_VARIANT` to `labels`, `partition`, or an empty string
for the combined production comparison. Run the same Cargo command with
`--lib benchmark_live_extract -- --ignored --nocapture --test-threads=1`.
Each workload emits `LIVE_EXTRACT_METRIC` JSON. Local logs, binary snapshots,
reference checks, and build receipts are under `target/song-lua-live-extract`.
