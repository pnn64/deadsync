# Song Lua mesh emission and actor predicates

This pass starts from `b86cc66df`, following the [simulation query pass](2026-10-19-song-lua-physics.md). It makes three changes under M-HOTPATH, M-INITIAL-CAPACITY, M-BOX-DST and M-THROUGHPUT in [rust-performance.md](../../rust-performance.md):

1. Read draw-mode names into the existing inline text converter and use a static default. Ordinary names and missing defaults avoid the former heap `String`; numeric coercions and conversion errors retain String's behavior.
2. Collect fixed-topology meshes directly into `Arc<[SongLuaOverlayMeshVertex]>` using standard trusted-length slice/range iterators. Triangle lists, quads, quad strips and fans retain the original vertex order and trailing-vertex handling while removing the temporary vector and its copy into Arc storage. Empty results allocate no output Arc. Line strips keep the original geometry routine and transfer its vector into Arc storage.
3. Specialize the ActorFrameTexture predicate through a checked stack conversion that validates and compares a Lua string without copying it. Other types retain checked String coercions/errors; other expected actor types keep the original path, including the Sprite/Banner alias. The wrapper is inlined so literal expected names can select their path at the caller.

The predicate borrows bytes only from a rooted, type-checked Lua string under mlua's VM lock. UTF-8 validation and comparison finish without Lua calls, and no pointer/slice escapes. Non-string conversions delegate to the original implementation. The existing actor getters and their order remain intact. Geometry construction uses safe Rust and standard-library Arc collection; source/range decoding, line geometry and per-vertex conversion remain unchanged.

## Measurements

The [raw results and validation receipt](2026-10-19-song-lua-mesh.json) retain six alternating-order rounds and 54 samples per variant/workload. Tables show medians before → after. Mesh results combine mode-text and output-storage changes; predicate results measure the actor-type helper separately.

| Case | CPU cycles/read | Allocations/read | Requested bytes/read | Reads/second |
| --- | ---: | ---: | ---: | ---: |
| Six vertices, default triangles | 2,227.80 → 1,722.56 | 3.00 → 1.00 | 466 → 232 | 981,877 → 1,264,354 |
| 4,096 vertices, triangles | 126,197.06 → 26,652.10 | 3.00 → 1.00 | 294,878 → 147,440 | 17,346 → 82,099 |
| 4,096 vertices, quads | 188,831.73 → 165,643.61 | 3.00 → 1.00 | 442,398 → 221,200 | 11,602 → 13,177 |
| 4,096 vertices, quad strip | 420,586.02 → 289,489.64 | 3.00 → 1.00 | 884,338 → 442,168 | 5,211 → 7,539 |
| 4,096 vertices, fan | 502,850.50 → 152,175.23 | 3.00 → 1.00 | 884,332 → 442,168 | 4,345 → 14,393 |
| 128 vertices, line strip | 40,000.44 → 39,536.58 | 3.00 → 2.00 | 54,898 → 54,880 | 54,792 → 55,432 |
| 128 raw-table vertices, triangles | 338,335.27 → 347,142.69 | 5.00 → 3.00 | 18,338 → 13,784 | 6,479 → 6,314 |

Fixed-topology output now has one exact Arc allocation instead of a temporary vector plus Arc allocation. Typical mode names remove a further string allocation. Raw-table inputs retain their original parsing costs; the common userdata-backed input holds an immutable source Arc. Line strips retain their geometry calculation. These cases use 6–4,096 source vertices; they do not establish whole-song or gameplay-FPS gains.

The four large fixed-topology cases run 1.14–4.73× as fast, use 12.3–78.9% fewer CPU cycles, and improve in every round. Their allocation calls fall from three to one and byte traffic is approximately halved. Small-mesh and line timings vary in sign across rounds. The raw-table case has a 2.5% lower median throughput despite fewer allocations; its per-round changes range from -14.9% to +4.2%.

| Case | CPU cycles/read | Allocations/read | Requested bytes/read | Reads/second |
| --- | ---: | ---: | ---: | ---: |
| AFT predicate, matching name | 306.91 → 292.09 | 0.00 → 0.00 | 0 → 0 | 7,142,985 → 7,501,313 |
| AFT predicate, Quad name | 319.76 → 279.99 | 0.00 → 0.00 | 0 → 0 | 6,860,240 → 7,827,023 |
| AFT predicate, 128-byte unknown name | 476.71 → 297.14 | 1.00 → 0.00 | 128 → 0 | 4,594,216 → 7,369,740 |
| Sprite predicate, Banner alias | 333.76 → 334.23 | 0.00 → 0.00 | 0 → 0 | 6,569,547 → 6,560,388 |

Short actor names already fit inline in the baseline; the specialized predicate avoids their copy and repeated validation. Long names also avoid the baseline spill allocation. Unknown/non-AFT expected types use the existing conversion path. Requested bytes describe allocation traffic rather than RSS. Reallocation counts and all individual samples are retained in the JSON.

Matching AFT names show a 5.0% median throughput increase with some round-to-round variation. The Quad-name miss improves 14.1% and the long-name miss improves 60.4%, consistently across rounds. Sprite/Banner alias timing remains near baseline (-0.1% median). All workloads retain zero reallocations.

## Behavior and validation

Four new release checks pass against frozen readers and fixed-topology builders from `b86cc66df`. They cover empty/incomplete primitives, every topology, mixed-case and unknown modes, ranged slices, raw-table and userdata inputs, exact signed-zero/nonfinite payload bits, mode/width coercions, malformed buffers, getter ordering, conversion errors, actor aliases, arbitrary UTF-8/binary names, long names, stack sentinels, coroutine calls and reentrant getters. An allocation check verifies the two-allocation reduction for a warmed triangle read.

The default-feature release library suite has 701 passes, 13 failures and six ignored performance benchmarks. Every failure matches the preceding commit's normalized diagnostic, whose retained log hash matches its committed receipt. All four new checks pass. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. The JSON records each failure and diagnostic hash, source/executable hashes and the build-receipt hash. Frozen methods and legacy builders were checked against `git show b86cc66df` after visibility adapters; shared helpers and embedded Lua chunks were checked unchanged. Native fixtures, acceptance criteria and tolerances are unchanged.

## Reproduction and scope

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit b86cc66df --test-filter lua_util::mesh_perf::benchmark_mesh --out docs/performance/2026-10-19-song-lua-mesh.json
```

Run the executable with `--test-threads=1` for the complete library suite. Each round performs one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Mesh samples perform 32–512 complete reads including returned-mesh destruction; predicate samples perform 20,000 reads. Fixture construction, Lua setup and Lua teardown are excluded. Both variants run in one executable with the same counted System allocator on Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled.

QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included in timing/cycles. No builds or other benchmarks overlap measurements. The frozen readers share unchanged Lua/range/line helpers and vertex types, and do not call the optimized predicate or Arc geometry builder.
