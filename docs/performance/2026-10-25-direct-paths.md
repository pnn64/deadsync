# Direct PCM, Boolean validation, and muted panning - 2026-10-25

Three independent safe Rust changes preserve sample bits, decoded values, error positions, and voice progression. Both `deadlib-lua` and `deadlib-lua-legacy`, including concurrent working-tree changes, are excluded.

## Retained changes

1. **Mono PCM extraction:** read the first sample of each complete source frame directly into the output slice. This bypasses the general repeated-channel loop and its cycling iterator. A private, non-inlined slice loop keeps its stride traversal out of the stereo and repeated-channel kernels. An earlier inline stride iterator was slower and disturbed control code generation. No intermediate storage or new heap state is introduced; existing buffer resizing and partial-frame behavior remain intact.
2. **Boolean validation:** replace two scalar validation scans with one safe word-at-a-time scan. Mask each eight-byte word with `0xfefefefefefefefe`; any remaining bit marks an invalid Boolean. Little-endian loading plus trailing-zero counting identifies the earliest invalid byte. The scalar tail covers incomplete words. Arrays, owned/borrowed vectors, and reusable vectors retain their existing copy, limit, first-error, prefix, and reader-consumption rules. Existing unsafe conversion code is unchanged; validation adds no unsafe code. The implementation uses APIs compatible with the codec's Rust 1.86 minimum.
3. **Muted panned mixing:** share the direct unit-gain loop with zero gain, computing its scale once per buffer. This removes per-sample normalization and fused multiply-add work for muted voices. The addition remains necessary for signed zeros and NaN propagation. Keep the existing panned helper out of the centered mixer body with an inlining hint; no dispatch layer or new state is added.

The production diff is **37 added / 9 removed lines (28 net)** across five files. The simplification is in executed operations: direct first-channel loads, one bit-mask validation per eight bytes, and ordinary zero-product additions. No production unsafe, dependencies, public APIs, or heap structures are added.

## Measurement method

Windows x86_64, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1, repository static MSVC runtime flags. Main tables use the repository's **default release optimization level 3 and full LTO**. Supplemental TSVs use the same release settings with LTO disabled for faster development builds.

Production baselines were measured before the corresponding edits. The final confirmation builds use identical final benchmark sources with the five production files from `a7529feea`, followed by the optimized production files. Temporary source replacement was guarded by restoration in `finally`, including on build failure. The affected files are `deadlib-audio-core/src/mixer.rs`, `deadlib-audio-decode/src/resample.rs`, and `deadlib-bincode/src/{utils.rs,de/impls.rs,impls/alloc.rs}` under `crates/`. Both executable sets were archived before timing. No build launched by this task overlapped the final measurements.

Each version runs in four fresh processes, alternating before/after process order. Every process pins its worker to logical processor 2 and measures twelve batches, discarding the first. Windows `QueryThreadCycleTime` supplies thread-accounted CPU cycles; `Instant` supplies elapsed throughput. Tables use the median of the four process medians. The range is the minimum/maximum ratio of corresponding process runs, **not a confidence interval**. These counters are not retired-instruction PMU measurements.

Fixtures and capacity reservations are outside timing. Allocation probes run separately. Every one of the **1,392 steady-state probes** across both build configurations records zero allocations, reallocations, frees, and allocated/freed/peak churn bytes before and after. Existing input and output allocations remain live outside those probes.

A mapped operation writes one packet of the labeled frame count. Its input and output have a controlled relative address offset modulo 4096 (`phase=0` or `128`); this prevents allocation placement from masquerading as a `memcpy` regression. Both placements are retained. The function pointer and runtime channel counts are opaque to the optimizer. A Boolean operation decodes one vector into preallocated storage; `invalid=true` puts an invalid value at the final byte. A panning operation mixes one active stereo voice; cursor reset and clearing the output are included equally in both versions. The voice remains live after each operation. Signed integer encoding/decoding is an unchanged null control.

## Full-LTO CPU cycles and throughput

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired run range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `mapped/2to1/frames=64/phase=128` | 464.80 | 100.69 | 4.616x | 3.876-4.762x | 212.02 | 45.91 |
| `mapped/2to1/frames=4096/phase=128` | 26143.88 | 4460.17 | 5.862x | 5.777-6.082x | 11933.41 | 2034.63 |
| `mapped/6to1/frames=4096/phase=128` | 25920.57 | 4718.12 | 5.494x | 5.178-5.546x | 11825.41 | 2144.57 |
| `mapped/8to1/frames=4096/phase=128` | 26009.85 | 5090.34 | 5.110x | 4.584-5.289x | 11876.03 | 2319.26 |
| `bool/len=16/invalid=false` | 40.18 | 22.59 | 1.779x | 1.364-1.973x | 18.30 | 10.29 |
| `bool/len=256/invalid=false` | 282.52 | 74.19 | 3.808x | 3.792-4.540x | 128.63 | 33.72 |
| `bool/len=4096/invalid=false` | 3946.28 | 653.11 | 6.042x | 5.727-7.051x | 1797.13 | 296.31 |
| `bool/len=4096/invalid=true` | 3944.26 | 728.44 | 5.415x | 5.094-6.513x | 1797.54 | 330.53 |
| `bool/len=65536/invalid=false` | 69883.32 | 15334.27 | 4.557x | 4.476-4.682x | 31890.00 | 6979.50 |
| `pan/Left/gain=0/frames=256` | 1206.03 | 403.04 | 2.992x | 2.841-3.034x | 552.46 | 183.68 |
| `pan/Left/gain=0/frames=4096` | 18507.94 | 6865.90 | 2.696x | 2.429-2.992x | 8486.47 | 3130.53 |
| `pan/Right/gain=0/frames=4096` | 18314.53 | 7095.29 | 2.581x | 2.284-2.903x | 8330.74 | 3230.94 |

Across the twenty mono workloads, median CPU throughput improves **3.83-5.86x**. The larger Boolean cases (256-65,536 bytes, valid and invalid) improve **3.81-6.04x**. Muted left/right panning improves **2.58-2.99x**. These are operation-level gains; no whole-game frame-rate gain is inferred.

## Controls and limits

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired run range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `mapped/2to6/frames=4096/phase=128` | 65347.32 | 68985.33 | 0.947x | 0.879-0.972x | 29783.33 | 31461.11 |
| `mapped/2to8/frames=4096/phase=128` | 79351.05 | 77181.24 | 1.028x | 1.012-1.045x | 36181.97 | 35233.61 |
| `mapped/6to8/frames=4096/phase=128` | 79865.62 | 85241.57 | 0.937x | 0.911-0.958x | 36409.01 | 38868.03 |
| `mapped/8to3/frames=4096/phase=128` | 38766.82 | 39356.89 | 0.985x | 0.958-1.056x | 17679.94 | 17944.75 |
| `mapped/2to2/frames=4096/phase=128` | 609.47 | 611.73 | 0.996x | 0.953-1.030x | 274.59 | 275.62 |
| `mapped/1to2/frames=4096/phase=128` | 900.49 | 888.35 | 1.014x | 0.999-1.167x | 407.38 | 401.23 |
| `mapped/6to2/frames=4096/phase=128` | 5133.51 | 5099.33 | 1.007x | 0.999-1.023x | 2339.14 | 2324.18 |
| `bool/len=0/invalid=false` | 23.40 | 23.93 | 0.978x | 0.930-1.034x | 10.68 | 10.91 |
| `bool/len=1/invalid=false` | 24.69 | 25.69 | 0.961x | 0.948-1.269x | 11.27 | 11.72 |
| `bool/len=7/invalid=false` | 31.57 | 31.39 | 1.006x | 0.912-1.278x | 14.41 | 14.37 |
| `pan/Left/gain=1/frames=4096` | 6597.59 | 7057.74 | 0.935x | 0.834-1.040x | 3007.47 | 3215.07 |
| `pan/Right/gain=1/frames=4096` | 6768.29 | 7094.17 | 0.954x | 0.838-1.084x | 3100.31 | 3231.76 |
| `pan/Left/gain=0.375/frames=4096` | 18478.93 | 17306.99 | 1.068x | 0.954-1.208x | 8433.10 | 7890.99 |
| `pan/Center/gain=1/frames=4096` | 5427.23 | 5529.10 | 0.982x | 0.870-1.040x | 2471.21 | 2521.21 |
| `pan/Center/gain=0.375/frames=4096` | 30454.05 | 31927.36 | 0.954x | 0.827-1.161x | 13887.60 | 14565.47 |

Controls are not uniformly faster. In the full-LTO run, 6-to-8 mapping loses about 6% throughput (roughly 2.4 microseconds per 4096-frame packet); unit-gain panning loses about 5-7% for long buffers (about 0.15-0.21 microseconds), and the long centered fractional-gain case loses about 5% (about 0.67 microseconds). Small Boolean differences are around one CPU cycle. All workloads and process samples, including slower controls, are in the TSVs. The retained gains are scoped to mono output, Boolean collections, and muted panning.

The supplemental LTO-disabled builds also show the three targeted gains. Their controls have wider variation; they are recorded separately rather than combined with the default-release results. The signed codec TSV contains unchanged null controls, not the discarded branchless trial.

Two broader PCM rewrites were discarded: delegating all writes to the append routine, and copying repeated channel blocks with dynamic `memcpy` calls. They slowed stereo expansion despite shorter/generalized source. A signed zigzag bit-formula trial was also discarded: small-vector encoding changed 30,094.91 -> 29,807.37 cycles, while decoding changed 28,464.76 -> 29,924.43 cycles; no consistent useful benefit justified retaining it.

## Correctness and checks

- **189 release tests pass with default full LTO**, across `deadlib-audio-core`, `deadlib-audio-decode`, and `deadlib-bincode`; 12 performance tests remain ignored in ordinary runs. The LTO-disabled release suites also passed.
- Boolean checks cover every invalid byte (2-255) at each of 65 positions, a second later invalid byte, exact retained prefixes, exact reader consumption, empty/short arrays, word boundaries, truncation, and owned/borrowed decoding. Existing codec suites cover limits, non-peeking readers, and wire compatibility.
- PCM checks cover every i16 input value across zero through nine input/output channels, empty and partial frames, previous output contents, and allocation reuse.
- Existing and new panning checks compare sample bits over every i16 source value, both directions, signed zeros, subnormals, infinities, NaN payloads (including signaling NaNs in the existing suite), ordinary gains, cursor progress, and incomplete frames.
- The four new focused correctness tests also pass with debug assertions. Clippy completes with `-D clippy::incompatible_msrv`; remaining warnings concern existing code. New test files and the validation helper pass rustfmt checks. A direct Rust 1.86 invocation could not run because the installed toolchain reports a missing manifest; no minimum version is changed.

## Reproduction

Build the same three test targets for each production version and archive the reported executable paths. For baseline reproduction, keep these benchmark sources and use the five production files from the baseline revision listed above; restore current sources after the baseline build. Omit the LTO override for the main release results; add `--config profile.release.lto=false` for the supplemental run.

```powershell
cargo test --release --locked -p deadlib-audio-core --test panned_kernel -p deadlib-audio-decode --test mapped_output_kernel -p deadlib-bincode --test direct_kernels --no-run --message-format=json
cargo test --release --locked -p deadlib-audio-core -p deadlib-audio-decode -p deadlib-bincode --lib --tests
cargo test --locked -p deadlib-bincode --test bool_validation -p deadlib-audio-core --test panned_kernel -p deadlib-audio-decode --test mapped_output_kernel
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-audio-core -p deadlib-audio-decode -p deadlib-bincode --lib --tests -- -D clippy::incompatible_msrv

# Each archive directory contains mapped_output_kernel.exe, direct_kernels.exe,
# and panned_kernel.exe copied from Cargo's compiler-artifact executable fields.
python scripts/benchmark_direct_paths.py --archive before=target/direct-codec-media/before-lto --archive after=target/direct-codec-media/after-lto --target mapped --runs 4 --output target/reproduced-mapped.tsv
# Repeat with --target bool or pan. Use --target signed for the null control.
```

Raw records:

- Full LTO: [PCM](2026-10-25-direct-paths-lto-mapped.tsv), [Boolean](2026-10-25-direct-paths-lto-bool.tsv), [panning](2026-10-25-direct-paths-lto-pan.tsv).
- LTO disabled: [PCM](2026-10-25-direct-paths-mapped.tsv), [Boolean](2026-10-25-direct-paths-bool.tsv), [panning](2026-10-25-direct-paths-pan.tsv), [signed null controls](2026-10-25-direct-paths-signed.tsv).

Archived executable SHA-256 values:

| Build | Executable | Before | After |
| --- | --- | --- | --- |
| LTO disabled | `mapped_output_kernel.exe` | `693c584a25d2aa090209613296ad4e870d43919604e4981b0aa3df561e1ffb30` | `4ee473b07144fd2309662b06f7581c6a90d290317a354b023e75454ef51fe1e5` |
| LTO disabled | `direct_kernels.exe` | `5e0a9a1031975f2c7e4dbb1022ff0ef0330d8af351110b50ac5496bce3e56842` | `1633936a19672bb4dfcbbc5cc90cc8d6b0c007f4611061db67f261e2ea102ba1` |
| LTO disabled | `panned_kernel.exe` | `4c927b8ac8e2827e59df724598f14afcc4756ac50411522c269d3d3008c3b3d4` | `841dc0ef468d00f514912b5d8eca67b4ecf908dc3f30c6279ae98b64db8b1083` |
| full LTO | `mapped_output_kernel.exe` | `6bc0b745246967d6305badfe29f9bc6d006e5d7a36ddc482a68d359f3bb6955e` | `18929e2da52c9bb34cdf598d2df5c66bce3b14c125d918b79b07b6f6ff2400f6` |
| full LTO | `direct_kernels.exe` | `ee74e632458503bf01f527819b232f97ccf305fa5b26890171e18d9a3a1a096b` | `67fb4e5fed40254c2330b03ea3280eff61adcd155609b6a4645527d4d676aef5` |
| full LTO | `panned_kernel.exe` | `14ac2ea8a76597d32fc911e8accb58719ef3f544933b46202be07290c2aac452` | `7aecce11357e7da87bbe807aadb6b00d30ab724e0d8a3e8e623fc8d55f69c2f8` |
