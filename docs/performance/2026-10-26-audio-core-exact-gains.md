# Audio core: exact gain arithmetic and borrowed mode parsing

Three optimizations are retained in `deadlib-audio-core`: fold steady music normalization into its gain, extend exact SFX products beyond powers of two, and parse output modes without allocating a lowercase string. Production remains under `#![forbid(unsafe_code)]`; public APIs, sample order, gain progression, voice state, and atomic ordering are unchanged.

Measurements use Windows `QueryThreadCycleTime`, with the benchmark thread pinned to logical CPU 2. Each executable takes one warmup and eleven measured batches per workload; the tables use medians of batch medians across rotating process runs. CPU throughput is before cycles divided by after cycles. Allocation counters cover only the operation, excluding fixture construction. Builds use the repository's release optimization, LTO, and static MSVC runtime settings with rustc 1.99.1 / LLVM 23.1.1. No native-CPU flags were added.

The script archives actual Cargo test executables and source snapshots under `target/audio-core-kernels`. It checks that sources did not change during compilation and records executable/source SHA-256 values, compiler identity, build command, and host metadata in the adjacent manifests. Final build manifests also cover Cargo configuration and the lockfile. The measurements use production functions, not a copied candidate implementation.

| Final workload | Before cycles/op | After cycles/op | CPU throughput | Allocations/op |
| --- | ---: | ---: | ---: | ---: |
| Music, steady gain, 4,096 samples | 2,234.22 | 1,673.89 | 1.335x | 0 -> 0 |
| Centered SFX, gain 0.375, 4,096 frames | 29,663.09 | 5,309.83 | 5.586x | 0 -> 0 |
| Left SFX, gain 0.375, 4,096 frames | 16,593.84 | 6,430.72 | 2.580x | 0 -> 0 |
| Right SFX, gain 0.375, 4,096 frames | 16,736.20 | 6,475.92 | 2.584x | 0 -> 0 |
| Parse `Auto` | 143.51 | 17.96 | 7.988x | 1 -> 0 |

Final music data is in [final-music.tsv](2026-10-26-audio-core-final-music.tsv). The SFX rows select `pan-before` and the retained `final-panned` build from the six-run [three-stage confirmation](2026-10-26-audio-core-masked-sfx.tsv); its third, rejected mask-selection candidate is also recorded. Parsing data is in [final-mode.tsv](2026-10-26-audio-core-final-mode.tsv). [summary.json](2026-10-26-audio-core-summary.json) records the selections and allocation counts.

Each change was also measured before proceeding to the next production change. The immediate music comparison was 2,307.22 -> 1,641.31 cycles for 4,096 samples ([data](2026-10-26-audio-core-folded-scale.tsv)). Centered SFX at gain 0.375 was 35,517.40 -> 6,537.10 cycles for 4,096 frames ([data](2026-10-26-audio-core-dyadic-sfx.tsv)). Parsing `Auto` was 163.84 -> 19.41 cycles ([data](2026-10-26-audio-core-borrowed-mode.tsv)). Snapshot comparisons confirm that these pairs changed only `render.rs`, `mixer.rs`, and `output.rs`, respectively, among production source files.

These microbenchmarks establish benefits for the listed workload classes, rather than an overall game speedup. Control measurements varied across runs. The first combined SFX comparison showed 5–17% slowdowns in several existing fast paths ([data](2026-10-26-audio-core-final-sfx.tsv)); those slowdowns did not reproduce in the subsequent three-stage comparison. Both results are retained. The final music comparison includes small slowdowns in some zero/unit-gain controls, alongside its steady-gain and settling-ramp improvements.

Steady music previously converted every sample as `(i16 / 32768) * scale`, with separate zero- and unit-gain loops. For finite gains whose magnitude is at least `f32::MIN_POSITIVE * 32768`, dividing the gain by 32768 is exact. Converting as `i16 * (scale / 32768)` therefore produces the same rounded product. This removes one multiply per ordinary sample and deletes the separate unit-gain loop. Signed zero retains multiplication by each sample. Tiny gains and non-finite values retain the original arithmetic order. An initial broader predicate failed at the underflow boundary and was discarded before performance acceptance.

SFX already used an unfused loop when multiplication was exact. A normal f32 gain with its lowest 15 fraction bits clear has at most nine significant bits. Its integer significand is at most 511, and `32768 * 511 < 2^24`, so multiplying any i16 sample is exactly representable. Above the same normalization bound, this permits the existing addition loop instead of a fused multiply-add per sample. Powers of two retain their existing eligibility below that bound. Other gains retain the fused path. The two existing loops are reused; no SIMD dependency, unsafe code, allocation, or dispatch layer is introduced.

Output-mode parsing now trims the borrowed input, rejects incompatible lengths, and uses ASCII case-insensitive comparison. This removes the owned lowercase string, its allocation, its deallocation, and its full-input lowercase pass. The same Unicode whitespace trimming and ASCII-only case semantics remain. All seven nonempty parser workloads go from one allocation and one free to zero; the empty-input control stays allocation-free. This is a configuration-path improvement, not callback work.

Correctness checks cover every i16 value at all f32 gain exponents for steady music, including zero, signed zero, tiny gains, infinities, and NaN bits. SFX differential tests cover every i16 sample, every nine-bit gain fraction at exponent 127, all normal powers of two, representative exponent boundaries, gains just outside eligibility, both pan directions, centered output, incomplete frames, cursor progress, signed zeros, infinities, and NaN payloads. Parser tests cover all ASCII case combinations of the three names, Unicode whitespace, embedded NULs, and invalid Unicode spellings.

The final release suite passed 104 tests with eight benchmark tests ignored; benchmarks were run explicitly. The debug library suite passed 56 tests. Clippy passed with warnings denied and only the two pre-existing lint categories allowed: `chunks_exact_to_as_chunks` in the unchanged fallback and `too_many_arguments` in telemetry. New test/parser formatting, Python compilation, and scoped `git diff --check` passed. Commands, full output, and final source hashes are recorded in [validation.json](../testing/2026-10-26-audio-core-validation.json).

Rejected experiments remain documented in their TSVs and manifests: [signed gain steps](2026-10-26-audio-core-signed-step.tsv), [separate multiply/add steps](2026-10-26-audio-core-separate-step.tsv), [float clamping](2026-10-26-audio-core-clamped-pcm.tsv), [integer clamping](2026-10-26-audio-core-integer-clamp.tsv), [fixed pan channels](2026-10-26-audio-core-constant-pan.tsv), [unrolled panning](2026-10-26-audio-core-unrolled-pan.tsv), [PCM batches](2026-10-26-audio-core-pcm-batch.tsv), and the mask-selection candidate in the three-stage confirmation. None showed a consistent benefit sufficient to retain.

To build and measure the current production source:

```powershell
python scripts/benchmark_audio_core.py --build current
python scripts/benchmark_audio_core.py --build current-pan --target panned_kernel
python scripts/benchmark_audio_core.py --compare baseline current --benchmark render::kernels::benchmark_callback_kernels --runs 4 --output target/audio-core-kernels/music.tsv
python scripts/benchmark_audio_core.py --compare pan-before current-pan --runs 6 --output target/audio-core-kernels/sfx.tsv
python scripts/benchmark_audio_core.py --compare parser-before current --benchmark mode_parser::benchmark_callback_kernels_mode_parser --runs 4 --output target/audio-core-kernels/mode.tsv
```

The comparison commands use the archived baseline executables from this run. For a fresh baseline, use an isolated checkout of this change's parent commit, copy the current benchmark harness/script into it without changing production source, and build the corresponding baseline stages there. The source and binary hashes in each manifest identify the measured builds independently of unrelated workspace changes.
