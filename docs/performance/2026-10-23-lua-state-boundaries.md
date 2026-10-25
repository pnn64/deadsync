# Lua captured-state and table boundaries, 2026-10-23

Baseline: `a85295a5e`. This pass adds safe closure restoration and fixed array reads to `deadlib-lua-legacy`, batches RGB extraction in Song Lua, and removes repeated key borrowing from noteskin alias normalization. The release patch moves from `0.5.2350` to `0.5.2351`.

## Changes and ownership

1. `Function::set_upvalue` converts a value and writes the existing Lua closure cell under one lock and stack guard. Song capture restoration uses it in place of an allocating protected `exec_raw` call for each cell. Other closures keep sharing the same cell. Native C closures are excluded so Rust callback storage remains private. Generated snapshot indices are checked one-based `u8` values. This removes the game-side unsafe restoration block.
2. `Table::raw_get_array` reads fixed slots in order under one lock and table root, converting each before the next read. RGB extraction uses a three-element batch; optional alpha keeps its original separate conversion and default. Nil, coercion, checked panic payloads and early errors keep their original behavior. The array uses stack storage. Generic access lives in deadlib; RGB/alpha semantics stay in Song Lua.
3. Noteskin alias extraction uses `for_each` and consumes each string handle into an owning `BorrowedStr`. UTF-8 is validated once, and matching reads the retained immutable string view. This avoids iterator cursor cloning and repeated borrowed-string handles. Raw traversal order, duplicate-case selection, false values, original value identity, and subsequent `__index`/`__newindex` calls are preserved. Alias names and normalization policy stay in noteskins.

The new unsafe blocks are limited to deadlib's existing lock/root/stack primitives. `lua_setupvalue` updates an existing cell and performs its GC barrier without allocating or invoking authored code. Fixed reads reserve three stack slots, retain the table, perform checked conversions, and restore the caller's stack on error or unwinding. Native C callback cells cannot be inspected or changed through these APIs.

## Measurement

[Full measurements, hashes, build receipts, check output and exact compiled snapshots](2026-10-23-lua-state-boundaries.json).

Rust 1.99.1, Windows MSVC static CRT, release, `--locked -j 2 --config profile.release.lto=false`. Workers are pinned to logical CPU 2. Both variants use the same counted System allocator. Counts include Rust allocations and mlua's Rust-backed Lua allocator; native malloc is excluded. Bytes are requested allocation/reallocation bytes, not resident or retained memory. Samples run sequentially; unrelated machine activity is not controlled.

Operation medians use 27 samples per variant from three fresh processes, alternating order, after a discarded warmup pair. Restoration measures saved-row decoding plus cell writes; restoring table contents is covered by complete compilation. The color baseline retains its original inline attribute. The frozen alias helper matches the baseline after only name and formatting normalization.

| Operation | Cycles before -> after | Operations/s before -> after | Allocations before -> after | Requested bytes before -> after | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| restore_1_cells | 1,750 -> 1,096.21 | 1,256,441.72 -> 2,007,056.06 | 2 -> 0 | 80 -> 0 | +59.74% |
| restore_8_cells | 12,499.07 -> 7,701.79 | 175,318.45 -> 282,998.01 | 16 -> 0 | 640 -> 0 | +61.42% |
| restore_32_cells | 48,612.39 -> 30,041.49 | 44,961.58 -> 73,022.89 | 64 -> 0 | 2,560 -> 0 | +62.41% |
| restore_60_cells | 92,396.21 -> 57,136.88 | 23,663.83 -> 38,306.44 | 120 -> 0 | 4,800 -> 0 | +61.88% |
| color_table | 1,161.23 -> 918.37 | 1,886,601.26 -> 2,388,965.03 | 0 -> 0 | 0 -> 0 | +26.63% |
| noteskin_aliases_0 | 1,966.93 -> 2,779.77 | 1,076,987.8 -> 778,588.81 | 0 -> 2.02 | 0 -> 80.77 | -27.71% |
| noteskin_aliases_8 | 7,246.07 -> 5,117.09 | 301,389.22 -> 427,664.55 | 8 -> 2.02 | 128 -> 80.77 | +41.90% |
| noteskin_aliases_64 | 45,208.43 -> 23,528.51 | 48,494.95 -> 93,128.16 | 65 -> 3.02 | 9,088 -> 11,728.77 | +92.04% |
| noteskin_aliases_512 | 378,416.28 -> 180,477.87 | 5,789.82 -> 12,140.24 | 513 -> 3 | 80,768 -> 104,912 | +109.68% |

Empty alias tables pay for the protected visitor and are slower by about 0.4 microseconds. Larger retained string-view records increase temporary requested bytes when inline storage spills. For 64 keys, allocation count falls from 65 to about 3 while requested bytes rise from 9,088 to 11,729. These costs are retained in the measurements.

## Complete compilation

Song uses 15 samples per variant per fixture, across three fresh process pairs. Noteskins use 54 samples per variant across six pairs, including a reversed-order confirmation. Input/context preparation, output formatting/serialization and final output destruction are excluded equally from both variants. Source reads, Lua creation/execution, probes, extraction and returned DTO preparation are included.

| Workload | Cycles before -> after | Compilations/s before -> after | Allocations before -> after | Requested bytes before -> after | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| offline_state_boundaries | 955,679,519 -> 942,115,289 | 2.29 -> 2.32 | 293,318 -> 293,304 | 43,656,583 -> 43,656,023 | +1.47% |
| live_state_boundaries | 835,680,832 -> 831,006,368 | 2.62 -> 2.63 | 271,485 -> 271,485 | 42,918,872 -> 42,918,817 | +0.55% |
| offline_capture_inputs | 1,262,036,546 -> 1,265,272,860 | 1.73 -> 1.73 | 455,735 -> 455,713 | 62,176,701 -> 62,175,821 | -0.23% |
| noteskin_complete_aliases | 8,158,047 -> 8,328,488.5 | 268.04 -> 263 | 11,628 -> 11,562 | 594,859 -> 601,579 | -1.88% |

The noteskin initial result was -2.01% throughput; the reversed confirmation was +0.80%. The pooled result is -1.88%, with 66 fewer allocations and 6,720 more requested bytes. Whole-compiler timing is mixed and does not establish a general CPU improvement. The retained improvements are demonstrated in the targeted operations; the full workloads show their practical limits.

## Validation and remaining Lua

- Focused native checks: 11 passed on Lua 5.1 and 11 on Lua 5.4. Coverage includes shared cell updates, alias identity, GC rooting, signed-zero/NaN bits, invalid and native closure indices, conversion errors, nil holes, numeric coercion, raw access, reentrant conversion order, and stack restoration after errors and caught Rust panics.
- Two focused noteskin checks passed: alias identity/order with invalid UTF-8 keys and metamethod side effects; complete serialized bundle equality against the existing frozen compiler.
- All 228 complete measured/warmup outputs matched exactly: 108 Song Debug DTO outputs and 120 full noteskin bincode bundles. The new Song fixture mutates a shared scalar cell, rebinds a captured table and edits an alias across action probes, and uses RGB/alpha coercions. Each operation process also compares restored values/identities and color bits with the original operations.
- Targeted formatting, Python syntax and diff checks pass. No full Song or noteskin suite was run. The six historical Song failures were already absent in the previous pass's baseline; this pass neither fixes nor suppresses them. Miri was not run on Lua FFI.

A generic checked-array replacement for the existing permissive vector reader was rejected: malformed caught-panic userdata would gain different behavior. The original vector reader is retained.

Input dispatch still operates through the Lua-owned live scene and mutable actor tables. This pass supplies reusable safe restoration and reads; identifying captured-cell identity and lowering input rules into explicit runtime state remain future work. No new interaction executes without a VM, and no gameplay retained-memory reduction is claimed. Loading, compilation probes and live input/state/timer fallback continue to use Lua.

No theme or rendering policy changed. Pixel comparisons, whole-app FPS and a representative chart corpus were not measured. Song Debug output covers returned DTOs and the retained scene's Debug fields; it does not serialize every internal VM cell. Native tests independently exercise shared cell restoration.

## Reproduction

```powershell
cargo test -p deadlib-lua-legacy --no-default-features --features lua51,vendored,send --test raw_array --test upvalues --release --locked -j 2 --config profile.release.lto=false
# Repeat the focused native targets with lua54 in place of lua51.
cargo build -p deadsync-song-lua --example state_boundaries --example compile_pipeline --release --locked -j 2 --config profile.release.lto=false
cargo build -p deadsync-noteskin --example compile_boundaries --release --locked -j 2 --config profile.release.lto=false
cargo test -p deadsync-noteskin --lib --no-run --release --locked -j 2 --config profile.release.lto=false
# Run only compiler::aliases_perf::aliases_preserve_identity_raw_order_and_metamethod_effects
# and compiler::callbacks_perf::complete_compiled_noteskin_bytes_match_baseline from that executable.
python scripts/benchmark_lua_state.py --song <state_boundaries.exe> --noteskin-before <before.exe> --noteskin-after <after.exe> --noteskin-tests <noteskin-tests.exe> --out <operations> --rounds 3
python scripts/benchmark_song_lua_pipeline.py --suite state_boundaries --before <before.exe> --after <after.exe> --out <pairs> --pairs 3 --samples 5
python scripts/benchmark_song_lua_pipeline.py --suite capture_inputs --only offline_capture_inputs --before <before.exe> --after <after.exe> --out <capture-pairs> --pairs 3 --samples 5
```

For the noteskin confirmation, run `compile_boundaries <fixture-directory> <report.json> 9` in after/before, before/after, after/before order, using the alias-normalization fixture and the preserved executables. Compare every report snapshot before pooling the samples.

Preserve baseline/candidate executables before rebuilding. All production Rust files match their recorded measurement hashes. The workspace/lockfile version bump follows measurement and changes release metadata only.
