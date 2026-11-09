Graph-line construction now writes vertices directly into one exactly sized `Arc` allocation. It partitions the output into fixed-size segment and cap arrays, initializes every vertex, and publishes the immutable slice. This removes the flattened iterator chain's per-vertex state checks and temporary copies while keeping segment/cap geometry, order, input rejection and the allocation count. No inline assembly, SIMD intrinsics or CPU feature dispatch are added.

The production caller builds a cached evaluation-screen life graph with **100 points** (`screens/evaluation/graph_build.rs`, `GRAPH_LIFE_SAMPLE_COUNT`). Evaluation setup constructs the mesh and subsequent frames reuse it. This is a screen-construction optimization, outside the software renderer and Lua, not a per-gameplay-frame saving.

Archived Cargo release binaries measure **2.336x** faster construction at 100 points, about **57.2% fewer CPU cycles**, pooling both six-pair runs below. Wall-clock medians are approximately **10.98 us to 4.66 us** per complete build and free. The [integrated raw data](2026-11-09-graph-line-mesh-raw.csv) retains all 2880 observations with run identifiers; the [summary](2026-11-09-graph-line-mesh-summary.csv) includes every workload and observed process-pair range.

| Workload | Before cycles/build | After cycles/build | Median speedup | Twelve-pair range |
| --- | ---: | ---: | ---: | ---: |
| graph-0 | 201.9 | 200.9 | 1.017x | 0.945-1.111x |
| graph-1 | 198.9 | 202.0 | 1.001x | 0.908-1.073x |
| graph-2 | 546.1 | 343.5 | 1.605x | 1.491-2.044x |
| graph-4 | 932.1 | 503.9 | 1.854x | 1.760-2.131x |
| graph-32 | 7,090.8 | 3,059.3 | 2.324x | 2.157-3.014x |
| graph-100 | 24,033.2 | 10,221.9 | 2.336x | 0.294-2.769x |
| graph-256 | 62,096.2 | 25,215.1 | 2.495x | 0.983-3.036x |
| graph-2048 | 506,187.6 | 219,541.2 | 2.333x | 1.217-2.747x |
| graph-16384 | 8,372,566.9 | 6,501,768.6 | 1.295x | 1.252-1.453x |
| coincident-256 | 201.5 | 198.1 | 1.027x | 0.517-2.209x |
| zero-thickness-256 | 199.8 | 196.3 | 1.013x | 0.600-1.134x |
| nan-thickness-256 | 197.2 | 196.3 | 1.004x | 0.921-1.211x |

The benchmark includes the complete returned `Arc`, allocation, all output writes, black-box observation and final destruction/free. Fixture generation is outside timing. It uses synthetic smooth points with the real evaluation graph's point count, not a recorded whole-screen workload. Other sizes and rejected-input controls are retained to show the practical limits. Ranges are observed process extrema, not confidence intervals or guarantees on other hardware; allocation and system state can materially change absolute cost and ratios. Compiler-presence observations from the measurement window are described below.

The [first integrated run](2026-11-09-graph-line-mesh-initial-summary.csv) measured 2.331x at 100 points, but one process pair regressed to 0.294x (the other five were 2.312x-2.769x). This triggered one repeat using the same unchanged binaries. The [repeat](2026-11-09-graph-line-mesh-repeat-summary.csv) measured 2.367x, with all six ratios between 2.275x and 2.533x. The table and raw data pool both runs, including the slow pair; none is discarded. Other controls also have outliers. Unrelated compilers were active; no specific cause for the first run's slow pair was established. These results support a median improvement in the measured workload, not a guarantee that every execution is faster.

The preceding extracted prototype measured **3.128x** at 100 points (range 3.005x-3.241x). It used panic abort, one codegen unit and both variants in one executable; the integrated Cargo test binaries use panic unwinding and separate before/after processes. The [prototype raw data](2026-11-09-graph-line-mesh-prototype-raw.csv) and [summary](2026-11-09-graph-line-mesh-prototype-summary.csv) remain separate. The prototype's empty-input case was 0.940x, approximately 5 ns slower. Its largest case improved 1.291x, with allocation/output memory traffic still present.

Earlier extracted measurements were more variable: 256 points measured 1.178x [1.144,3.197] and 2048 points 1.697x [1.554,3.114]. Those observations are retained in the [early raw data](2026-11-09-graph-line-mesh-early-prototype-raw.csv) and [early summary](2026-11-09-graph-line-mesh-early-prototype-summary.csv), rather than selecting only the best run. They used the same slice-chunk candidate before the 100-point case was added to the workload list.

The audited extracted assembly reserves 1240 stack bytes in the baseline versus 168 in the candidate, excluding saved general registers. The baseline materializes temporary segment/cap arrays and branches through iterator state for every copied vertex. The candidate emits direct stores of six/twelve vertices per loop iteration. Its cap loop also keeps the known-finite half-width visible to LLVM: two `fmaf(half, 0, coordinate)` calls per point become zero additions. Those additions preserve non-NaN signed-zero semantics. Segment validation still calls `fmaf`, and normalization still calls `hypotf`. Static `fmaf` call sites decrease from three to one; static sites are not dynamic calls per complete graph. Assembly and source snapshots remain in the local audit directory.

The production function matches the measured prototype after comments/formatting and the Clippy-recommended replacement of constant-sized slice chunks with typed array chunks. The geometry closures and rejection logic remain unchanged. The generic reference freezes the complete original graph builder from `057aae6f4`, including its original iterator collection. The complete benchmark/test module, its wrapper and fixtures are identical in the archived before/after library binaries.

The crate still denies unsafe code by default. A local allowance covers the final `Arc::assume_init` conversion. The proof is documented next to it: after the early guard, n >= 2; checked size arithmetic reserves exactly `(n-1)*6 + n*(CAP_SIDES*3)` vertices. The two output regions cover the allocation, each contains an exact number of typed array chunks, and the matching input loops write every slot exactly once. All access before finalization uses safe `MaybeUninit::write`; nothing reads uninitialized storage. A panic before finalization drops uninitialized storage without reading its elements. The allocation-count regression confirms that this initialization strategy adds no allocation or reallocation.

**NaN payload/sign bits may differ.** This is the qualification approved with the prototype and documented beside the production code. The regression requires exact bits for every non-NaN coordinate/color, including signed zeros and infinities, and requires NaN classification to match while tracking payload differences. The prototype observed 121 NaN-bit differences; the integrated release and unoptimized regression runs observed zero. Zero in these builds does not establish a cross-platform guarantee about unspecified NaN payloads.

Validation:

- **198 baseline and 198 updated library release tests pass**, with eleven opt-in benchmarks ignored. All twenty-four separate archived-binary benchmark invocations pass.
- The new differential compares **13,355 graphs / 3,680,970 vertices** against the frozen original. It checks output lengths, vertex order, every position/color component and initial strong/weak ownership counts. Cases cover empty/small/large inputs, the real 100-point size, finite/random coordinates, signed zeros, subnormals, NaNs, infinities, invalid/extreme thickness, duplicate points at different positions, and distances around the degenerate-segment threshold. It includes 10,000 deterministic random-IEEE fixtures and 1000 additional finite curves. All non-NaN bits match, with zero NaN-bit differences in the integrated run.
- The new allocation integration test compares **27 size/thickness cases** against the original using the repository's scoped allocation counter. Both builders allocate once, never reallocate, and free once, with identical requested/freed/peak bytes. For 100 valid points, both emit 1794 vertices and request **43,072 bytes** on this x86_64 machine. Invalid input retains the same 16-byte empty-Arc allocation. The counting allocator is in a separate integration binary and is not linked into timing.
- **Eight line tests pass at opt-level 0 with debug assertions and overflow checks**, including the new differential and zero NaN-bit differences. This directly compiles the production presentation crate with cached release dependencies and fat LTO; it is not a full Cargo debug build.
- `cargo check --release` and Clippy for all crate targets complete. Existing warnings remain, including the unchanged many-argument line-emission helper and duplicate imports of shared benchmark support. The new production loops use typed chunks as suggested by Clippy. The frozen original locally allows its intentional `map(...).flatten()` shape. Changed source and new test/reference files pass rustfmt and whitespace checks.
- Full application runtime/GPU and cross-architecture runtime tests were not performed.

Measurements used an Intel Xeon E5-2696 v4, Windows x86_64 MSVC, rustc 1.99.1 / LLVM 23.1.1, release opt-level 3, fat LTO, static CRT and generic CPU settings. The committed ignored benchmark calls the public production builder through a non-inlined wrapper, with points, thickness, color and returned output black-boxed. The benchmark thread is pinned to logical CPU 2; `QueryThreadCycleTime` measures cycles and `Instant` measures wall time.

Each process measures ten rounds of `max(32768 / max(point_count, 1), 8)` builds per case, discarding round zero. Each of two runs uses six fresh before/after process pairs, alternating binary order and rotating cases by 0, 5, 10, 15, 20 and 25. Pooled speedups are medians of all twelve paired process-median ratios; absolute columns are independent medians, so their ratio need not equal the reported speedup. Raw process IDs 0-5 identify run 0 and IDs 6-11 identify run 1. No own builds or correctness checks overlapped timing. Unrelated rustc activity was observed at the measurement boundaries; the retained presence records identify the processes. Alternating variants does not eliminate contention.

SHA-256:

- Before executable: `0508932fd1468128b1c96cde659be62c214fac3d36f47dc310f4e56d52e31b77`.
- After executable: `3b1939d05c8c27fc8bbd1b68e1229a3e05eea7acc5dc7d00b40ed08e87db7dc8`.
- Timing function body: `fc34850f1dc2e274a1e7ad1608297c0d6aa3881ce730262898d791bd724ff208`.

Reproduction from the repository root:

```powershell
cargo test --release --locked -j 2 -p deadlib-present --lib --test graph_line -- --test-threads=1
$env:GRAPH_LINE_BENCH_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-present --lib line::graph_performance::benchmark_graph_line_mesh -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:GRAPH_LINE_BENCH_ROTATION
cargo check --release --locked -j 2 -p deadlib-present
cargo clippy --release --locked -j 2 -p deadlib-present --all-targets --no-deps
```

For before/after comparison, archive the library test executable reported by `cargo test --release --locked -j 2 -p deadlib-present --lib --no-run --message-format=json` on each side of the production change, retaining identical benchmark source and fixtures. Run the exact ignored benchmark directly from the archived executables with all six rotations and alternating process order. Binaries, manifests, runners, source snapshots, compiler-presence records, verification, assembly and validation logs remain in `target/line-cap-audit-2026-11-09/`.
