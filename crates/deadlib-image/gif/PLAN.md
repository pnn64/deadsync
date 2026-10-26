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
