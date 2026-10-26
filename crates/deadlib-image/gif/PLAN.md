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
- Milestone 2: 5369d9ffa; compositor and explicit policies; rectangle snapshots, background
  selection, external palettes, text rendering callback, scoped controls,
  extension/loop preservation. Nine integration tests pass in debug mode.
- Milestone 3: 279a21db4; pinned upstream corpus, gif-dispose full-canvas differential
  validation, 73 giflib oracle cases with 64 independent encoder rewrites,
  independently packed malformed/LZW and resource tests, every split/truncation,
  a 512-graphic independent compositor model, ASan/coverage fuzz smoke runs.
  Stage/pipeline benchmark baseline also recorded before optimization.
- Milestone 4: bulk canvas initialization and measured bulk LZW bit refill;
  rejected slower candidate retained as benchmark evidence; standards coverage,
  compatibility policies, benchmark results and limitations documented. Final
  audit also corrected unknown-graphic control scope per GIF89a section 12.

Final gates: 19 integration tests pass in both debug and release; the external
giflib gate passes all 73 cases (64 rewrites); Clippy, rustfmt and rustdoc with
warnings denied pass. Both coverage/ASan fuzz targets are rerun on final code.
All four milestones are complete; future performance experiments described in
BENCHMARKS.md are optional tuning opportunities, not unfinished codec features.

A clean four-milestone review history is available on the local branch
`codex/gif-codec-2026-10-26`; see AUDIT.md for the shared-index incident and
the isolated commit mapping. No shared history was rewritten.
