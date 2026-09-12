# Noteskin preparation — 0.5.1164

Baseline: `7754dc3f7` / 0.5.1163. This pass applies `rust-performance.md`'s
M-HOTPATH, M-MEM-REUSE and M-THROUGHPUT guidance to three active noteskin loading
paths. These are transition/loading costs, not steady-state frame benchmarks.

## Changes and active callers

1. **Avoid unnecessary filesystem work in noteskin discovery.**
   `find_child_dir_case_insensitive` checks the entry name before constructing a
   full path or checking its type. Both it and `find_file_with_prefix` use the
   enumerated ordinary file type; symlinks and file-type errors retain the old
   path-metadata fallback. Prefix lookup constructs a path only for a new best
   candidate. Case handling, PNG filtering, lexicographic selection, duplicate
   warning counts and cache lifetime are unchanged. These helpers serve
   `resolve_skin_dir`, `NoteskinData::resolve_path` and texture prefix resolution.
2. **Build sprite plans without disposable copies.** The two animation path
   adapters choose animation versus atlas/frame fallback before moving their
   texture key into the result. Uniform animation uses a stack delay description
   instead of allocating a frame-sized temporary Vec that normalization copied.
   The final owned duration buffer remains. Active asset adapters are
   `itg_slot_from_path_animated` and `itg_slot_from_path_all_frames` in
   `deadsync-assets/src/noteskin/texture.rs`.
3. **Move resolved tap layers into their shared storage.**
   `emit_tap_note_column`, called by `itg_runtime_columns_compiled` for every
   column, consumes the sorted Vec into `Arc<[T]>`. Previously it cloned each
   slot then dropped the originals. Real `SpriteSlot::clone` creates a fresh
   atomic identity and clones shared resource handles. Moving avoids those
   increments/decrements and identity generation. It still allocates the final
   Arc; this is not a zero-allocation column builder. Owned layers retain their
   original unique identities; quantized note clones still receive fresh ones.
   Layer values/order, shared quantization layers and lift fallback sharing are
   preserved. The public helper that also returns the original Vec is unchanged.

## Method

- Windows x86_64 MSVC, Intel Xeon E5-2696 v4 @ 2.20 GHz; Rust 1.98.0
  (`88d9e12ae`, LLVM 22.1.8). Release opt-level 3, full LTO.
- Three complete runs in one release test executable; old/new, new/old,
  old/new order. Each result is the median of seven timing batches after three
  warmups; tables take the median of the three run medians. Allocation counting
  is a separate operation through the existing thread-local counted allocator.
  Outputs are black-boxed and dropped inside each measured operation.
- 8 iterations per cold filesystem batch, 4,096 for warm lookups, 2,048 for
  sprite plans and tap columns. One throughput unit is one lookup, plan or
  column construction. Values are not notes/second or rendered frames/second.
- Cold lookup cases include identical eviction of that fixture's cache entries
  on both sides. Files exist before measurement and OS filesystem caches are
  warm. This measures a cold application lookup cache, not cold disk access.
  The directory target sorts after the distractor filenames. Fixture setup and
  cleanup are excluded. Warm cases retain the application cache.
- Layer fixtures model SpriteSlot's fresh atomic ID, four Arc resources and
  inline state. They are a surrogate, not the asset crate's concrete SpriteSlot
  or an entire loaded skin. Both implementations include the same input clone
  to supply an owned Vec each operation, plus final output destruction. This
  shared fixture cost dilutes the benefit of avoiding the discarded clones.
- Frozen old function bodies are committed beside the tests. Their bodies were
  audited against the parent commit allowing only imports, visibility and
  formatting; unchanged callees are shared. Source hashes were recorded before
  and checked after the benchmark. All Cargo builds/checks finished first.
- Cycles are calling-thread `QueryThreadCycleTime` values, not retired hardware
  instructions or whole-process CPU. Requested/freed bytes measure allocator
  traffic, not peak live memory/RSS, GPU memory or filesystem cache memory.
  Results are from this host; tiny differences can be measurement/code-layout
  effects. No full-game load-time or frame-rate improvement is claimed.

## CPU, time and throughput

Negative cycle changes are improvements. All 21 pairs are shown, including
unchanged paths and regressions. Times are microseconds per operation.

| Case | Old µs | New µs | Old thread cycles | New thread cycles | Cycle change | Old ops/s | New ops/s |
|---|---:|---:|---:|---:|---:|---:|---:|
| lookup_dir_small | 500.8625 | 96.6500 | 1097719.5 | 211927.2 | -80.69% | 1,997 | 10,347 |
| lookup_dir_128 | 10564.7875 | 190.9125 | 23119742.6 | 418449.2 | -98.19% | 95 | 5,238 |
| lookup_dir_1024 | 84525.8375 | 910.4125 | 185039542.2 | 1995803.5 | -98.92% | 12 | 1,098 |
| lookup_prefix_128 | 451.5125 | 195.6500 | 989423.6 | 428711.0 | -56.67% | 2,215 | 5,111 |
| lookup_prefix_many | 10262.7375 | 217.7500 | 22478473.8 | 477302.8 | -97.88% | 97 | 4,592 |
| lookup_prefix_miss | 196.0250 | 203.5125 | 429588.9 | 445969.1 | +3.81% | 5,101 | 4,914 |
| lookup_dir_warm | 0.3194 | 0.3453 | 700.6 | 752.9 | +7.47% | 3,131,259 | 2,895,928 |
| lookup_prefix_warm | 0.3462 | 0.3357 | 759.4 | 736.4 | -3.03% | 2,888,576 | 2,978,909 |
| layers_empty | 0.1132 | 0.1145 | 249.0 | 251.8 | +1.12% | 8,835,203 | 8,737,201 |
| layers_single | 1.0927 | 1.0388 | 2396.9 | 2278.7 | -4.93% | 915,144 | 962,677 |
| layers_three | 1.4183 | 1.2336 | 3109.9 | 2706.8 | -12.96% | 705,064 | 810,608 |
| layers_eight | 2.1691 | 1.6886 | 4758.6 | 3698.4 | -22.28% | 461,012 | 592,216 |
| layers_32 | 7.0178 | 5.0797 | 15382.4 | 11137.1 | -27.60% | 142,495 | 196,861 |
| layers_zero_quantizations | 1.3116 | 0.8087 | 2876.5 | 1774.2 | -38.32% | 762,444 | 1,236,491 |
| sprite_atlas | 0.1198 | 0.0689 | 256.9 | 151.9 | -40.87% | 8,348,960 | 14,514,529 |
| sprite_uniform_8 | 0.5093 | 0.4107 | 1118.5 | 900.8 | -19.46% | 1,963,378 | 2,434,617 |
| sprite_uniform_64 | 0.2983 | 0.1936 | 653.9 | 425.5 | -34.93% | 3,352,431 | 5,166,498 |
| sprite_uniform_4096 | 4.6506 | 3.3500 | 10196.1 | 7346.2 | -27.95% | 215,024 | 298,512 |
| sprite_no_delays | 0.1376 | 0.0813 | 302.7 | 179.2 | -40.80% | 7,264,988 | 12,292,917 |
| sprite_explicit_64 | 0.3416 | 0.3335 | 746.1 | 730.6 | -2.08% | 2,927,387 | 2,998,097 |
| sprite_explicit_fallback | 0.1294 | 0.0904 | 284.7 | 199.1 | -30.07% | 7,725,387 | 11,064,290 |

## Allocation churn

Each cell is old → new per operation. A/R/F means allocation/reallocation/free
calls. Requested and freed bytes include reallocations when present.

| Case | A/R/F | Requested bytes | Freed bytes |
|---|---|---:|---:|
| lookup_dir_small | 25/6/25 → 15/2/15 | 2,887 → 1,170 | 2,887 → 1,170 |
| lookup_dir_128 | 409/134/409 → 143/2/143 | 52,039 → 3,090 | 52,039 → 3,090 |
| lookup_dir_1024 | 3099/1032/3099 → 1037/2/1037 | 396,454 → 16,289 | 396,454 → 16,289 |
| lookup_prefix_128 | 151/5/151 → 144/3/144 | 4,154 → 3,162 | 4,154 → 3,162 |
| lookup_prefix_many | 526/130/526 → 144/3/144 | 52,083 → 3,147 | 52,083 → 3,147 |
| lookup_prefix_miss | 141/2/141 → 141/2/141 | 2,849 → 2,849 | 2,849 → 2,849 |
| lookup_dir_warm | 1/0/1 → 1/0/1 | 80 → 80 | 80 → 80 |
| lookup_prefix_warm | 1/0/1 → 1/0/1 | 87 → 87 | 87 → 87 |
| layers_empty | 2/0/2 → 2/0/2 | 2,592 → 2,592 | 2,592 → 2,592 |
| layers_single | 4/0/4 → 4/0/4 | 3,152 → 3,152 | 3,152 → 3,152 |
| layers_three | 4/0/4 → 4/0/4 | 4,240 → 4,240 | 4,240 → 4,240 |
| layers_eight | 4/0/4 → 4/0/4 | 6,960 → 6,960 | 6,960 → 6,960 |
| layers_32 | 5/0/5 → 5/0/5 | 33,072 → 33,072 | 33,072 → 33,072 |
| layers_zero_quantizations | 2/0/2 → 2/0/2 | 4,368 → 4,368 | 4,368 → 4,368 |
| sprite_atlas | 2/0/2 → 1/0/1 | 94 → 47 | 94 → 47 |
| sprite_uniform_8 | 4/0/4 → 2/0/2 | 158 → 79 | 158 → 79 |
| sprite_uniform_64 | 4/0/4 → 2/0/2 | 606 → 303 | 606 → 303 |
| sprite_uniform_4096 | 4/0/4 → 2/0/2 | 32,862 → 16,431 | 32,862 → 16,431 |
| sprite_no_delays | 2/0/2 → 1/0/1 | 94 → 47 | 94 → 47 |
| sprite_explicit_64 | 4/0/4 → 3/0/3 | 862 → 815 | 862 → 815 |
| sprite_explicit_fallback | 2/0/2 → 1/0/1 | 94 → 47 | 94 → 47 |

## Tradeoffs and limits

- `lookup_prefix_miss`: +3.81% thread cycles; 196.0250 → 203.5125 µs/op.
- `lookup_dir_warm`: +7.47% thread cycles; 0.3194 → 0.3453 µs/op.
- `layers_empty`: +1.12% thread cycles; 0.1132 → 0.1145 µs/op.

The layer builder's requested bytes and allocator calls are expected to remain
unchanged for Arc-backed slots: the benefit is avoiding clone/drop CPU work.
Its generic ownership regression test additionally proves that heap-owned
payloads can move without a payload allocation. That test is not presented as
an allocation reduction for the real SpriteSlot type.

Filesystem results assume entries remain stable during a lookup. As with the
existing cache and directory enumeration, these functions do not provide an
atomic snapshot if files change concurrently. Live/broken Windows directory
junctions were tested. File symlink creation on this host returned privilege
error 1314, so that conditional test branch was skipped; Unix branches are
included but were not executed here. Forced file-type I/O errors were not
injected; the original metadata fallback remains for that branch.

## Behavior and validation

- Noteskin tests: **238 passed, 5 ignored**, in both debug and release. Ten new behavior/allocation tests; three new ignored benchmarks.
- Downstream debug tests: **126 assets** (0 ignored) and **370 notefield** (0 ignored) passed.
- Downstream asset tests include loading bundled dance/pump skins and real
  SpriteSlot animation/model behavior. Notefield tests cover cached geometry,
  animation and visual output.
- Full plan comparisons cover zero/single/many frames, zero grid components,
  explicit indices and missing-delay fallback, NaN/infinite/negative/zero
  delays, both animation clocks, callback order and early returns. Playback
  frames and duration float bits are compared against old output.
- Lookup comparisons cover case, Unicode, empty prefixes, many matches, wrong
  entry types, PNG versus general matching, absent/invalid parents and cache
  hit/miss persistence. Cache persistence tests use the existing shared test
  lock so other tests cannot invalidate their state mid-check.
- Layer comparisons cover empty/single/multiple layers, stable sort ties,
  zero/one/many quantizations, complete values and shared resource pointers,
  preserved input identity, distinct note-clone identities, lift fallback
  sharing and preservation of existing outputs for empty input.
- `cargo clippy -p deadsync-noteskin --lib --locked -- -D clippy::perf`
  and `cargo check -p deadsync --all-targets --locked` passed. The strict
  all-target Clippy run found two pre-existing `cloned_ref_to_slice_refs`
  errors in unchanged `tests/packs.rs:242,245`; rerunning all targets with
  `-D clippy::perf -A clippy::cloned_ref_to_slice_refs` passed. Existing style
  warnings and the frozen baseline signature warning remain. Modified function
  fragments/new test files pass rustfmt; unchanged adjacent source was audited.
- Cargo.toml advances exactly 0.5.1163 → 0.5.1164; Cargo.lock updates only the
  three workspace-version packages. Both are included in the pass commit.

Reproduce the tests/benchmarks:

```powershell
cargo test -p deadsync-noteskin --lib --locked
cargo test -p deadsync-noteskin --lib --release --locked
cargo test -p deadsync-noteskin --lib --release --locked preparation_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-noteskin --lib --release --locked preparation_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

Optional Windows junction coverage uses `DEADSYNC_NOTESKIN_LINK_DIR`, pointing
to a directory containing `linked` (a live directory junction) and `broken`
(a dangling directory junction). The local fixture, validation logs, three raw
benchmark runs, medians and source hashes are under the ignored
`target/noteskin-preparation-perf/` directory.
