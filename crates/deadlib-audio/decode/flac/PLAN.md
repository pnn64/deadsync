# FLAC implementation and evidence ledger

Scope: an independent RFC 9639 codec, registered only as a workspace member.
Do not change the existing decoder, Symphonia integration, backends or playback.
Production codec dependencies are prohibited; the sibling Ogg container is allowed.
Reference implementations are permitted only in validation and benchmarking.

Milestones (commit each when verified):

1. Foundation (implemented; 5 integration tests pass): checked MSB bit I/O, CRC-8/16, native MD5, complete metadata
   representations and bounded parsing, package registration.
2. Native decoding (implemented; evidence in COVERAGE.md): independent frame API, all subframes/predictors/Rice modes,
   4–32 bits, 1–8 channels, all block sizes/rates, exact 33-bit side channels,
   fixed/variable numbering, streaming, integrity and sample-accurate seeking.
3. Native encoding: constant/verbatim/fixed/LPC, partitioned Rice and escape,
   stereo choices, compression levels, metadata/MD5/seek table output, reusable
   streaming encoder and exact PCM roundtrips.
4. Ogg FLAC: mapping lifecycle, native Ogg crate, streaming and seeking,
   encoding; retain a container-independent codec API.
5. Validation: libFLAC and Symphonia differential tests, official conformance
   corpus, malformed/truncated inputs, streaming splits, metadata, checksums,
   seek positions, fuzz targets and reproducible runs.
6. Performance: reproducible whole-file/stream/startup/seek/encode benchmarks,
   ratios, allocations and memory versus references; optimize measured hot paths
   and document evidence and limitations without asserting unmeasured parity.

Completion requires current evidence for every item, not just passing roundtrips.
Record commands, versions, corpus provenance, coverage gaps and measurements in
the crate documentation as each milestone is reached.

Outstanding decoder improvements: file-backed Read+Seek adapter, SEEKTABLE-assisted
startup/seek acceleration, measured buffer/allocation improvements. Current exact
seeking uses borrowed input and a lazy validated frame index. Default resource
budgets deliberately require configuration for extreme metadata/frame payloads.
