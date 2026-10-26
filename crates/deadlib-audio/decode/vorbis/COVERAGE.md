# Vorbis coverage and numerical evidence

Status: **raw-packet synthesis and streaming Ogg Vorbis mapping implemented**. No full
Vorbis I conformance or competitive-performance claim is made. The complete
requested end state remains in the parent work plan.

## Implemented and tested (2026-10-26)

* Bitpacking/unpacking with widths 0..=32, unaligned 32-bit fields, zero-width
  reads including after end-of-packet, speculative lookahead and byte padding.
  Independent bit-index tests cover every width and starting bit alignment.
* Identification parameters, block-size ranges/order, signed bitrate hints,
  framing, speaker order for 1..=8 channels and unassigned larger layouts.
* Comments, duplicate/unknown fields, raw binary vectors, case-insensitive field
  lookup, resource limits and truncated/oversized metadata rejection.
* Ordered/unordered/sparse codebooks; 1..=32-bit Huffman codes assigned in entry
  order; over/underpopulation rejection, singleton 2015 erratum. Flat decode
  trees. Exhaustive-prefix and specification-vector tests independently check
  assignment; 32-bit-deep trees and singleton bit consumption are exercised.
* All three VQ lookup types, sequence vectors, custom float unpacking, exact
  integer roots and setup-memory/entry/value/node policy limits.
* Full setup-header grammar: both floor configurations, three residues,
  mapping-zero submaps/coupling, modes and framing. Floor-one X uniqueness,
  precomputed neighbors/order, cross-references and reserved fields validated.
  Synthetic fixtures cover both floors/all residues and every truncated prefix.
  Audio packet mode/window prefix parsing is available independently of Ogg.
* Floor-one integer prediction/line rendering and normative inverse-dB table;
  floor-zero LSP reconstruction with the corrected Bark equation, odd/even
  polynomial cases and 0..=63-bit amplitudes. Floor-one setup rejects >65 points.
* Residues 0/1/2, submap bundles, nonzero propagation and reverse-order inverse
  coupling. Flat reusable spectral/curve/classification storage.
* Native O(N log N) FFT-based inverse MDCT for every legal block size, Vorbis
  windows and hybrid slopes, exact raw overlap range and priming, planar f32
  output in Vorbis channel order. No clipping, resampling or hidden reordering.
* Reusable overlap/reset lifecycle; nominal floor/residue truncation behavior,
  prefix/non-audio discard, error preservation of previous overlap, explicit
  decode-memory policy and rejection of nonfinite spectral/PCM results.
* Page, incremental-byte and pull-reader Ogg mapping APIs; exact header page
  boundaries/granules, nonfatal comment truncation/framing errors, flat owned
  metadata, independent logical stream state, multiplexing, chains and unknown
  codec skipping. Ended streams release codec/metadata storage. Active-stream
  count and aggregate retained codec-element budgets are tested across chains
  and multiplexed input.
* Initial trim through the full first overlap, positive broadcast timestamps,
  final packet trim, zero/one-sample streams, single audio-page EOS and nil EOS.
  Retained positions, raw synthesis positions and original granules are distinct.
  Per-channel frame counts and timestamps are exact. Mapping failures are terminal
  until reset. Byte chunks down to one byte, interrupted/fragmented IO, truncation,
  invalid header boundaries and invalid granules are covered.

Windows x86-64, rustc 1.99.1 evidence:

* Nine primitive tests, seven synthesis unit tests, four lifecycle/malformed
  tests, zero-allocation test, generated PCM/golden/discrepancy tests and the
  generated-fixture header test; external/corpus
  tests are explicitly ignored unless invoked with their required inputs.
* Generated stereo tones (48 kHz) and eight-channel silence (44.1 kHz) accepted
  independently by Symphonia 0.6.1 and libvorbis 1.3.7.
* All **38 repository Ogg Vorbis assets** accepted independently by Symphonia's
  setup decoder. Native parser checked **1,611 codebooks and 119,030 audio packet
  prefixes**. Metadata parameters and channel counts matched.
* libvorbis 1.3.7 commit `0657aee69dec8508a0011f47f3b69d7538e9d262` with libogg
  1.3.6: identification parameters and complete comments matched on those assets.
  Every used codeword in **1,611 books (155,478 entries)** was encoded by
  libvorbis and decoded natively to the exact expected entry and consumed bit
  count. All active VQ vectors matched, with **maximum observed absolute error 0**.
  Generated fixtures add 70 books and 6,649 active entries, also with zero error.
* `cargo clippy -p deadlib-audio-decode-vorbis --all-targets -- -D warnings` passed.
  The normal dependency graph contains only this crate and native Ogg.
* Parent `scripts/verify_independent.ps1`: both copied crates passed their tests
  offline in a separate workspace; production trees contain no reference codecs.
* All 38 repository assets: **140,856,832 raw PCM samples** matched libvorbis
  and Symphonia with exact per-packet frame counts and positions. Maximum
  absolute error: **7.152557373e-7** vs libvorbis, **5.960464478e-7** vs Symphonia.
  Aggregate RMS error: 6.552946408e-8 / 4.440798834e-8. Per-file data is in
  [validation/2026-10-26-raw-pcm.tsv](validation/2026-10-26-raw-pcm.tsv).
* MDCT sizes 64..=8192 match an independent direct cosine equation and the
  libvorbis transform; maximum observed reference error 3.576278687e-7.
* Generated eight-channel distinct tones (including dependent coupling) match
  libvorbis with peak error 7.450580597e-8. The retained independent golden
  verifies this without a C toolchain. 8 kHz mono and reset replay also pass.
* Test-only counting allocator: **zero allocations during decode and reset**,
  including first decode, for stereo/8-channel tones/8 kHz mono across four
  repeated decode/reset cycles. Tables and scratch allocate during construction.
* Every byte prefix of the short stereo audio packets, plus 1,000 seeded
  malformed multichannel packets, completes without buffer escapes/nonfinite
  output; reset restores identical subsequent synthesis. This is not sustained
  coverage-guided fuzzing.
* Historical floor-zero fixture: 12,525,696 raw samples with exact libvorbis
  counts. Peak/RMS errors: 2.533197403e-5 / 1.510420136e-6 vs libvorbis and
  2.259016037e-5 / 1.581026734e-6 vs Symphonia. The oracle README pins its URL/hash.
* Full Ogg mapping on all 38 repository assets: **140,832,876 retained channel
  samples**, with exact final counts independently matching libvorbisfile 1.3.7
  and Symphonia 0.6.1. Peak/RMS errors: 7.152557373e-7 / 6.553499110e-8 versus
  libvorbisfile; 5.960464478e-7 / 4.441172991e-8 versus Symphonia. The Symphonia
  comparison also checks every retained packet timestamp and duration. See
  [validation/2026-10-26-ogg-pcm.tsv](validation/2026-10-26-ogg-pcm.tsv); source
  hashes are unchanged from `validation/2026-10-26-assets.sha256`.
* Generated mono/stereo/eight-channel full-file output, shifted/trimmed starts,
  shortened tails and mixed-rate chains match libvorbisfile counts and PCM.
  Empty streams match its reported zero total; its PCM seek API has no empty
  target, so the oracle emits no sample records in that case.
* Counting allocator: page-to-PCM mapping of in-page audio packets allocates
  zero times after setup, including final trimming/EOS. This excludes transport
  buffer growth, spanning-packet assembly, setup, metadata and new logical streams.

## Numerical criteria

Codebook vectors use f64 unpacking/evaluation and f32 storage. The external VQ
test allows `abs(native - reference) <= 2e-6 * max(1, abs(reference))` for different
floating arithmetic rounding; the actual tested vectors above were exact. This
is a codebook check, **not a PCM parity criterion**.

For raw and container-trimmed floor-one PCM, the gate is peak error <= `3e-6 * max(1, reference_peak)`
and RMS error <= `5e-7 * max(1, reference_peak)`. This accommodates different f32
FFT butterfly/rotation/window arithmetic while staying below one tenth of a
16-bit full-scale sample step at unit peak. Observed corpus errors are smaller.
Counts, positions and channel identity have zero tolerance. No frame shifting,
cross-correlation, resampling or gain fitting is applied to comparisons.

Floor-zero nonlinear LSP reconstruction gets a separate gate: peak <=
`5e-5 * max(1, reference_peak)`, RMS <= `3e-6 * max(1, reference_peak)`. Native
curves evaluate the specification's cosine/polynomial equations in f64 before
f32 storage; libvorbis stores intermediate cosines and polynomial products in
f32. Near LSP resonances these rounding differences are amplified. The separate
unit-peak bounds correspond to about 1.64 / 0.10 of a 16-bit sample step and do
not relax counts or channel layout. Odd/even products have independent closed
form DC checks. The historical file's observed maximum is below one 16-bit step.
Further high-order/dynamic-range conformance material remains necessary.

The Symphonia raw API is used with gapless priming enabled and untrimmed raw
packets (`trim_start/end == 0`), so initial priming and all subsequent raw frame
counts match without discarding comparison samples.

## Explicit reference/legacy discrepancies

* Symphonia 0.6.1 undoes coupling in forward order; Vorbis I section 4.3.5 requires
  reverse order. Independent coupling steps and stereo agree, but the eight-tone
  7.1 fixture exposes dependent-step disagreement (peak 0.2883, RMS 0.05769).
  The native decoder follows the specification and matches libvorbis. A named
  regression test keeps this discrepancy visible; reference tolerances are not
  widened for it. Symphonia is pinned to 0.6.1 in development dependencies.
* The historical floor-zero file assigns granules to pages without a completed
  packet. Native strict Ogg correctly rejects it. The raw codec test uses the
  independent development-only Ogg reader to extract packets, isolating codec
  validation. No production compatibility exception has been introduced yet.
* Symphonia 0.6.1 assigns negative timestamps for an edited negative initial
  offset but does not discard those samples (`trim_start == 0`). A named test
  documents the extra 37 samples on the edited fixture and compares the retained
  suffix separately. Ordinary parity tests require exact counts without shifting.
  Native strict counts and the initial-trimmed samples match libvorbisfile.
* Strict final trimming is limited to the last packet's overlap span. Synthetic
  larger trims make libvorbisfile's declared total disagree with its emitted PCM;
  they are rejected rather than silently treated as a compatibility policy.

## Remaining required work

* Sample-accurate seeking/pre-roll/repeated seeks, container seek/index APIs and
  broader malformed packet concealment differential validation.
* Official conformance material, unusual/legacy encoders, malformed/fuzz corpora,
  real ITGmania/StepMania song packs. Repository assets are only initial coverage.
  No nonstandard legacy compatibility policy has been added.
* Native encoding, independently decoded lossy validation, measured pipeline
  optimization and reproducible throughput/startup/memory/allocation/seek
  comparisons with libvorbis and Symphonia. No SIMD or performance claims yet.

Raw packet and full-container comparisons are separate: raw PCM tests assign
unknown granules and omit EOS trimming; `ogg_mapping` tests independently use
libvorbisfile and Symphonia's container demuxer with real timestamps and trim.
Neither establishes seeking correctness. External reference APIs remain in
separate test helpers.
