# JPEG-1 implementation plan

The requested end state is a complete, independently reusable, native Rust
JPEG-1 decoder and encoder. Register only this package in the workspace;
do not integrate it into DeadSync or change the existing image codecs/loaders.
Production code may depend on shared `deadlib-image` primitives, never another
JPEG implementation. Preserve unrelated work and commit every milestone.

1. Bounded marker/segment parsing, native JPEG model, table validation,
   entropy bit handling, sequential Huffman coefficient reconstruction,
   8/12-bit integer IDCT, component planes, metadata and color conversion.
2. Progressive Huffman decoding including successive approximation,
   arbitrary legal scan/component ordering, restarts and exact coefficient
   differential validation. Streaming input/output and resource boundaries.
3. Native DCT encoder: quality, precision, subsampling, restart intervals,
   optimized Huffman tables, sequential/progressive scans, metadata retention.
4. Predictive lossless, arithmetic entropy coding, and hierarchical JPEG-1
   processes in both directions. Exact sample validation for lossless modes.
5. Independent, pinned conformance fixtures and libjpeg-turbo/libjpeg/mozjpeg
   oracles: coefficient equality, justified pixel tolerances, malformed input,
   every streaming boundary, metadata/ICC, unusual subsampling and fuzzing.
6. Reproducible stage/pipeline benchmarks: initialization, decoding, streaming,
   encoding, cycles, allocations and peak memory. Compare throughput, size and
   visual quality to the references. Optimize measured bottlenecks and recheck
   correctness. SIMD requires measured benefit.

These are implementation stages, not reductions in scope. The goal remains
open until all requested modes, validation and performance work are verified.
See COVERAGE.md for current evidence and gaps. Never infer conformance from
internal roundtrips alone.

Normative sources:
- ITU-T T.81 / ISO/IEC 10918-1, especially Annexes A–H, J and K:
  https://www.w3.org/Graphics/JPEG/itu-t81.pdf
- JFIF 1.02: https://www.w3.org/Graphics/JPEG/jfif3.pdf
- ITU-T T.871 / ISO/IEC 10918-5:
  https://www.itu.int/rec/T-REC-T.871/en

Completion evidence must identify each supported process, precision, entropy
mode and API, external oracle versions/commands, fixture provenance, numerical
error bounds and actual benchmark/fuzz runs. Unsupported features must return
explicit errors until implemented.

Verified milestones:
- `cdc8e854d`: package registration, complete SOF process model, native borrowed
  marker/entropy segment reader, marker/precision/truncation tests.
- Sequential decoder milestone: native Huffman, sequential 8/12-bit coefficients,
  bounded allocations, integer IDCT, component planes and initial color pipeline.
  All 25 pinned libjpeg-turbo cases have exact coefficients/tables and native
  IDCT error <=1. Twelve Rust tests pass in debug, release and isolated builds;
  rustfmt and Clippy with warnings denied pass. See tests/oracle/README.md.
  Progressive, encoding, rare modes and remaining gates are still open.
