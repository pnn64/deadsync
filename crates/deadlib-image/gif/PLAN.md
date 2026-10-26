# Implementation milestones

Scope: independent pure-Rust GIF87a/GIF89a crate; only workspace registration,
no application integration or modifications to existing image APIs/loaders.
Unrelated working-tree changes must remain untouched.

1. Native indexed model, resource-bounded incremental parser, native LZW,
   lossless indexed encoding, shared image conversion, basic differential tests.
2. Compositing, all disposal methods, extension preservation and scope,
   timing/loop policies, external palettes and plain-text rendering interface.
3. Broad independent fixtures, Rust gif and giflib oracles, established corpus,
   malformed/streaming/resource tests and fuzz targets with executed smoke runs.
4. Reproducible stage and pipeline benchmarks, measured optimization, parity
   revalidation, standards/compatibility/performance/limitations documentation.

Each milestone is committed independently. Completion requires evidence for
every requested capability, not only successful internal roundtrips.

Completed:
- Milestone 1: b7bfb9df8; native model/parser/LZW/encoder, all palette sizes,
  both Rust gif differential directions, interlacing and fragmented input.
- Milestone 2: compositor and explicit policies; rectangle snapshots, background
  selection, external palettes, text rendering callback, scoped controls,
  extension/loop preservation. Nine integration tests pass in debug mode.
