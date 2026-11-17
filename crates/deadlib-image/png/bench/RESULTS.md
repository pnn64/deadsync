# Decoder results, 2026-11-17

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.100.0, release/LTO, logical CPU 0. Runtime detection confirms BMI2, PCLMULQDQ, SSE2, SSSE3 and SSE4.1. References: png 0.18.1 and image 0.25.10.

Two independent processes each measured five batches of at least 80ms per image/codec after builds, tests and fuzzing finished. Wall-time and thread-cycle medians are selected independently. All native sample and RGBA16 comparisons pass.

Ratios below 1 favor native decoding. Each table value is the geometric mean of both runs' per-image thread-cycle ratios. `png` includes sub-byte unpacking/U16 conversion; `png-packed` retains the packed-byte API. Image expansion remains part of the image API.

| Corpus | Images | Native / png | Native / image | Native / png-packed |
| --- | ---: | ---: | ---: | ---: |
| all | 89 | 0.799 | 0.704 | 0.982 |
| formats | 60 | 0.753 | 0.636 | 0.972 |
| photos | 24 | 0.926 | 0.924 | 0.930 |
| assets | 5 | 0.803 | 0.654 | 1.435 |

The overall native/png ratios in the individual runs are 0.7980, 0.8001.

| Allocator requests across 89 decodes | Calls | Cumulative requested bytes |
| --- | ---: | ---: | ---: |
| native | 284 | 44,093,707 |
| png | 1,057 | 66,057,037 |
| image | 1,122 | 65,708,107 |
| png-packed | 1,016 | 62,132,357 |

Native uses fewer allocation calls and cumulative requested bytes on all 89 cases than normalized png and image. Both metadata-rich icons now use 16 calls each, versus png's 23 and image's 18. Packed png requests fewer bytes on nine sub-byte cases with a different output representation.

**The buffered-decoder benchmark target is met on this Windows x86-64 host.** Native uses fewer cycles than normalized png and image in every corpus group in both independent runs, as well as fewer total measured cycles across the corpus. It also averages fewer cycles than the packed-byte API overall. These are corpus averages: some individual files remain slower, and packed sub-byte output performs less expansion work. Streaming/APNG performance and other architectures were not timed.

This iteration pipelines the next primary Huffman lookup ahead of the bit-buffer refill, using already valid prefetched bits so the memory reads can overlap. The cached lookup consumes no bits, and the boundary/canonical paths retain exact prefix semantics. It also includes runtime-dispatched SSE4.1 Average and Paeth reconstruction for three-, four-, six- and eight-byte strides. Average corrects the byte instruction for PNG rounding; Paeth evaluates all pixel lanes together with exact tie handling. Loads and stores use checked slices, including partial pixels. The portable kernels remain available. Inflation, checksums, metadata validation and resource budgets remain in force; no production dependency was added.

Production denies unsafe code outside the CPU dispatch modules. Runtime checks establish BMI2/PCLMUL/SSSE3/SSE4.1 instruction support; byte reads and writes use checked slices and fixed-size arrays. The portable backend remains available.

Validation: 161 enabled tests pass (21 external-oracle tests ignored); 20 tests pass on 32-bit Windows; ARM library compilation succeeds; strict Clippy passes for production and the benchmark workspace. Expanded independent fixtures cover overlapping copies, full-token boundaries, output slack, prefetched end markers, suffix consumption and fragmented IDATs on both inflater backends. A new independent regression uses 12?15-bit canonical codes, every bit offset, output slack, suffixes and fragmented IDATs to verify primary lookahead when only 8?15 bits remain accounted. The filter tests cover every Paeth triplet, every Average input pair with wrapping residuals, independent byte oracles at all supported strides, short rows, alignment offsets and untouched surrounding bytes. Existing Adler checks cover starting states, splits and modulus boundaries. The final source completed 200,000 native-feedback ASan executions with source hashes and instrumentation isolation verified.

[Raw results, logs and hashes](results/windows-x86_64-20261117-pipelined/manifest.json) include both timing runs, allocations, portable checks and compressed ASan manifests/logs. [Reproduction instructions](README.md) describe measurement definitions. Earlier [hardware](results/windows-x86_64-20261117-hardware/manifest.json) and [zero-copy](results/windows-x86_64-20261117/manifest.json) measurements remain separate.
