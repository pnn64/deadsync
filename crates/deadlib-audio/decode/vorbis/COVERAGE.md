# Vorbis coverage and numerical evidence

Status: **raw synthesis, streaming mapping, indexed sample-accurate seeking and initial native encoding implemented**. No full
Vorbis I conformance or competitive-performance claim is made. The complete
requested end state remains in the parent work plan.

The active priority is complete behavioral parity with exact behavior/counts
and the original PCM tolerances. The [parity ledger](PARITY.md) records two
corrected discard/truncation differences, 7,014 independent cases and remaining
work. Performance measurements do not establish completion.

The [bit-mutation follow-up](BIT_MUTATIONS.md) adds 55,845 finite-reference
cases and corrects floor-zero selector/rounding plus VQ sequence offsets.
The [degenerate-book follow-up](DEGENERATE_BOOKS.md) matches all eleven nonfinite
cases under explicit IEEE propagation, adds empty-book consumer behavior and
accepts nonempty zero-dimensional scalar books. Default finite-output rejection
remains a separately tested policy.

The [configuration matrix](CONFIGURATION_PARITY.md) adds every mode-table size,
both mode block orders, surplus residue classwords and stage-free submaps.
It corrects packet discard/bit-consumption differences and widens transform
internals to keep cancellation cases within the original PCM bounds.

The [geometry matrix](RESIDUE_GEOMETRY.md) extends mixed-book, submap and channel
coverage and exposes a specification/reference disagreement for unaligned
type-two residue ranges. `Residue2Layout` offers the normative default and an
explicit libvorbis layout through raw, Ogg and indexed APIs.

The [codebook extent follow-up](CODEBOOK_LIMITS.md) adds a configurable 24-bit
reference declaration guard before lengths/allocation, 475 exact declaration and
whole-initialization checks, plus a full-wire-field override under other limits.

The [floor-zero parameter follow-up](FLOOR0_PARAMETERS.md) separates unsigned
amplitudes from the pinned reference's signed reader, sticky EOF for widths above
32, Bark-grid rounding and incomplete LSP concealment. Explicit IEEE synthesis
also matches whole-block NaN propagation from nonfinite spectra.

## Implemented and tested (2026-10-26)

* Bitpacking/unpacking with widths 0..=32, unaligned 32-bit fields, zero-width
  reads including after end-of-packet, speculative lookahead and byte padding.
  Independent bit-index tests cover every width and starting bit alignment.
  Another 2,592 mixed-width streams cover cache carries, lookahead history and
  every short packet tail. The header fuzz target now checks direct wire-bit
  values, exact reader state, invalid widths and sticky exhaustion even for
  bytes that cannot parse as headers. A fresh 300-second ASAN campaign completes
  142,674 executions without failure. Two paired refill alternatives were
  rejected; production bitpacking is unchanged. See
  [REFILL_EVALUATION.md](REFILL_EVALUATION.md) for the measurements and scope.
* Identification parameters, block-size ranges/order, signed bitrate hints,
  framing, speaker order for 1..=8 channels and unassigned larger layouts.
* Comments, duplicate/unknown fields, raw binary vectors, case-insensitive field
  lookup, resource limits and truncated/oversized metadata rejection.
* Ordered/unordered/sparse codebooks; 1..=32-bit Huffman codes assigned in entry
  order; over/underpopulation rejection, singleton 2015 erratum. Flat decode
  trees. Exhaustive-prefix and specification-vector tests independently check
  assignment; 32-bit-deep trees and singleton bit consumption are exercised.
  The [bounded-prefix call policy](PREFIX_PATHS.md) preserves those operations
  while inlining prefix/VQ wrappers and outlining tree/refill helpers. Eight
  repeated raw traces preserve 42,738,816 PCM sample bit patterns and every
  symbol/counter table; resource counters match the paired baseline. The
  initial streaming sanitizer timeout and successful baseline/current replays
  are retained with the validation evidence.
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
  decode-memory policy and default rejection of nonfinite spectral/PCM results;
  explicit libvorbis IEEE propagation is available.
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
* Full container/prefix Vorbis index, cached metadata/setup packets and per-page
  granule/raw-position/pre-roll landmarks. Exact retained-frame seeks select one
  preceding valid audio packet, prime overlap, then decode/discard to the target.
  Raw packet `reset_at` supports caller-managed positions with the same pre-roll.
  Logical streams can be selected independently through multiplexed input and
  mixed-rate/layout chains. Index and selected codec storage have separate limits.
* Repeated forward/backward/end seeks, one-sample/empty files, nonfatal/spanning
  metadata, ignored packets, complete initial-overlap discard, negative start
  edits, positive offsets above 2^54, tail trim and nil EOS. Injected read/seek
  failures are terminal until a successful seek; failed stream switches cannot
  pair the previous stream with the new codec's setup.

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
* Release tests exhaust **8,242 retained-frame targets** in the short mono,
  stereo and eight-channel fixtures. Every returned block on the first output
  page is bit-identical to sequential native synthesis; selected complete
  suffixes also match. Debug tests sample every 97th target to bound runtime.
* libvorbisfile 1.3.7: **1,900 repeated sample-accurate seeks across all 38 assets**,
  **12,032,724 returned channel samples**, exact seek positions/counts and peak/RMS
  error **5.960464478e-7 / 5.852150475e-8**. Boundary, end, backward and seeded
  random targets are included. Results:
  [validation/2026-10-26-seeks.tsv](validation/2026-10-26-seeks.tsv), with the
  same source hashes as `validation/2026-10-26-assets.sha256`.
* Generated libvorbisfile seek comparisons include stereo, 8 kHz mono, dependent
  eight-channel coupling, initial/tail edits, one sample, nil EOS and mixed-rate
  chains. Symphonia 0.6.1 agrees on generated mono/stereo/edited/broadcast seeks
  when its packet-level seek is followed by codec pre-roll and timestamp discard.
* Historical floor-zero packets repacketized unchanged into strict Ogg: 50 seeks,
  393,218 channel samples, peak/RMS error 1.764297485e-5 / 1.352849024e-6 versus
  libvorbisfile, within the existing floor-zero bounds. This isolates codec
  seeking and introduces no exception for the source's invalid Ogg granules.
* Counting allocator: repeated same-stream seeks plus complete suffix decoding
  allocate zero times after warming transport capacity for mono/stereo/surround.
  Raw positioned reset also allocates zero times. Index construction, codec
  switching and growth of transport/spanning-packet buffers are excluded.
* Pinned Xiph libvorbis upstream coded test grid: **528 files**, 1..=8 channels,
  six rates from 16 to 96 kHz, eleven quality settings, **4,866,048 channel
  samples**. Native strict PCM/counts match libvorbisfile with peak/RMS error
  **4.172325134e-7 / 5.142492184e-8**. All 2,640 native seeks and 17-byte streaming
  runs match sequential native output exactly. Upstream's own encode/decode
  checks pass. The reproduction script regenerated all 528 files byte-identically
  on this Windows build. See `validation/2026-10-26-upstream.tsv` and
  `validation/2026-10-26-upstream.sha256`. This is upstream coded test material,
  not exhaustive conformance certification.
* Four hash-pinned historical/ITGmania files: **68,398,796 retained channel
  samples** compared with libvorbisfile, plus **36 repeated native seeks**
  returning 229,384 channel samples. Floor-zero peak/RMS error is
  2.533197403e-5 / 1.510427612e-6; floor-one peaks are <=7.152557373e-7.
  The two malformed files require separate explicit options; both are rejected
  by strict defaults. The other two decode strictly. Mecha's reference-emitted
  count differs from its declared count, recorded separately rather than
  accepted as ordinary count parity. See [LEGACY.md](LEGACY.md) and
  [validation/2026-10-26-legacy.tsv](validation/2026-10-26-legacy.tsv).
* Independent libFuzzer workspace with Windows AddressSanitizer, debug assertions
  and overflow checks. Initial concurrent 300-second campaigns completed
  **127,539 header**, **3,777 raw packet** and **3,914 streaming/seek** executions
  without crash/assertion/timeout/RSS-limit failure. Seeds include floor-zero,
  dependent eight-channel coupling, chains/multiplexing and spanning packets.
  Oracles check reset determinism, finite PCM, pull/push agreement and repeated
  seek error recovery. See [the fuzz documentation](../fuzz/README.md), including
  source/binary/seed hashes, caps and feedback counter limitations. Longer
  campaigns, additional grammar/malformed inputs and independent differential
  fuzz oracles remain required.
* Initial five-file comparative performance baseline: 225 trial rows across
  native/libvorbisfile/Symphonia, resident full decode, capped 4 KiB reads and
  warm-process stream startup. All complete decode counts agree. Native full
  decode is 3.2–3.5x slower than Symphonia and 2.0–2.7x slower than libvorbisfile
  on the two longer stereo tracks. See [PERFORMANCE.md](PERFORMANCE.md) for
  settings, raw evidence, variability, startup results and remaining metrics.
* Measured MDCT optimization: paired-spectrum N/4 FFT, derived in
  [MDCT.md](MDCT.md), reduces transform elements 65% and measured kernel time
  3.2–3.8x. Two longer tracks improve 1.35–1.67x in observed full-file runs;
  shared-machine reference drift and all trial data are in PERFORMANCE.md.
  Direct cosine, 38-file raw/full PCM, 1,900 seeks, 528 upstream vectors, four
  historical/shipped files, reset/allocation, exact memory-budget, release/Clippy
  and offline independent builds pass without tolerance changes. New peaks:
  raw/full libvorbis 7.152557373e-7; upstream 3.576278687e-7; corpus seeks
  6.854534149e-7. Resumed sanitizer campaigns add 16,816 clean executions.
* Bounded two-to-eight-bit Huffman prefix lookup shares the tree buffer and uses
  only the setup budget left after full grammar validation. Exact node limits,
  zero-spare-budget fallback, every byte prefix/alignment, packet tails and
  32-bit-deep paths preserve entries, consumed bits and sticky EOP. All scalar/VQ,
  PCM, seek, upstream/historical, release/Clippy, allocation and offline gates
  above pass without tolerance changes. Three resumed sanitizer targets add
  141,962 clean executions. Interleaved native baseline/current builds show
  1.51–1.77x scalar speedup and 1.08–1.19x full decode speedup on the two longer
  songs, with about 6% higher startup time. Short clips regress 2–7% overall;
  [PERFORMANCE.md](PERFORMANCE.md) records this tradeoff and all trial ranges.

## Numerical criteria

Codebook vectors use f64 unpacking/evaluation and f32 storage. The external VQ
test allows `abs(native - reference) <= 2e-6 * max(1, abs(reference))` for different
floating arithmetic rounding; the actual tested vectors above were exact. This
is a codebook check, **not a PCM parity criterion**.

For raw and container-trimmed floor-one PCM, the gate is peak error <= `3e-6 * max(1, reference_peak)`
and RMS error <= `5e-7 * max(1, reference_peak)`. This accommodates different
transform/window arithmetic while staying below one tenth of a
16-bit full-scale sample step at unit peak. Observed corpus errors are smaller.
Counts, positions and channel identity have zero tolerance. No frame shifting,
cross-correlation, resampling or gain fitting is applied to comparisons.

Floor-zero nonlinear LSP reconstruction gets a separate gate: peak <=
`5e-5 * max(1, reference_peak)`, RMS <= `3e-6 * max(1, reference_peak)`. Native
curves retain libvorbis's f32 cosine/polynomial rounding boundaries, with a
wider fallback when those intermediate products overflow. Near LSP resonances,
rounding differences can be amplified. The separate
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
* The historical floor-zero file assigns zero to one unfinished setup-header
  page. Strict Ogg rejects it. The explicit `allow_zero_span_granules` option
  ignores that value without changing the timeline. Streaming/pull/indexed
  decoding then matches reference counts and the floor-zero numerical bounds.
  The original raw tests still isolate synthesis from container handling.
* Symphonia 0.6.1 assigns negative timestamps for an edited negative initial
  offset but does not discard those samples (`trim_start == 0`). A named test
  documents the extra 37 samples on the edited fixture and compares the retained
  suffix separately. Ordinary parity tests require exact counts without shifting.
  Native strict counts and the initial-trimmed samples match libvorbisfile.
* Strict final trimming is limited to the last packet's overlap span. Synthetic
  larger trims make libvorbisfile's declared total disagree with its emitted PCM;
  strict decoding rejects them. Indexed `trim_to_final_granule` can clip across
  packets/pages using the endpoint known in advance. Synthetic tests check both
  layouts, repeated seeks and invalid endpoints. Streaming stays strict.
* libvorbisfile 1.3.7 reports PCM position 900 after requesting retained frame 1
  in the generated stream with broadcast offset +900. A named external regression
  records this discrepancy. Native seeking preserves frame 1 / granule 901 and
  matches sequential PCM; the Symphonia pre-roll comparison also covers this case.
* The pinned ITGmania shipped song `Mecha-Tribe Assault.ogg` (libVorbis 20040629)
  has final granule 4,630,500 after a preceding page at 4,692,672. Strict decoding
  rejects this decreasing granule. Explicit indexed endpoint clipping emits
  4,630,500 frames per channel; libvorbisfile reports that length but emits
  4,762,304. Native PCM matches the reference prefix inside the declared interval,
  including endpoint seeks. `Goin' Under.ogg` (libVorbis 20070622) passes
  strict PCM/count comparison against libvorbisfile and Symphonia: 10,886,400
  channel samples, peak errors 7.152557373e-7 / 4.768371582e-7. Sources/hashes and
  paired SSC charts are pinned in `validation/corpus.json`. Policy details and
  reproduction commands are in [LEGACY.md](LEGACY.md).
* FFmpeg's `Lumme-Badloop.ogg` (libVorbis 20020713) passes strict container/mapping
  decoding with 17,862,912 frames per channel. Its name alone is not treated as
  evidence of a required compatibility exception.
* Two additional pinned historical files pass strict decoding: the floor-zero
  `coyote.ogg` (Xiphophorus 20001031) and six-channel `test6.ogg` (aoTuV b2).
  Their 10,393,344 channel samples per decoder match libvorbisfile/Symphonia
  counts, timestamps and channel layouts within the existing numerical bounds.
  Streaming resets and 512 repeated seeks match native sequential PCM bits.
  Five other pinned files preserve endpoint, sequence or CRC rejection through
  40 API/policy paths; `1sec.ogg` has a named encoded-endpoint/reference-count
  discrepancy. See [HISTORICAL.md](HISTORICAL.md) for evidence and limitations.

## Remaining required work

Initial native encoding now generates fixed-block setup/audio packets from PCM
and supplies raw and Ogg APIs. All legal transform sizes, four additive precision
stages, 1..255 channels, exact input lengths, reset/chunk determinism, allocation
checks and independent decoder parity have dedicated tests. The 144-case
libvorbisfile matrix compares 2,375,544 channel samples. A named reference test
records a lost first block after libvorbisfile PCM-seek zero for a spanning
priming packet; raw-start decoding returns the full correct output. See
[ENCODING.md](ENCODING.md) for the precise interval checks, measured lossy source
error, coding subset, resource policies and outstanding encoder work.

* Index-free/sparse codec seeking and measured startup/seek optimizations. The
  indexed reader eagerly scans the full container and all audio mode prefixes;
  it does not decode every audio payload during construction. Broader malformed
  packet concealment differential validation remains necessary.
  `read_packet` now avoids decoding unused page tails, with exact suffix/mixed
  read checks, 1,900 corpus and 2,640 upstream packet-read seeks, and paired
  1.16-1.76x seek-batch gains. The subsequent compact packet-mode index avoids
  page-interval pre-roll synthesis, adding paired 1.99x/3.18x longer-track gains
  for 4,224 additional live requested bytes per track. Both packet and page-only
  pre-roll strategies are tested. Eager construction and sparse seeking remain.
* Broader conformance material, unusual/legacy encoders, malformed/fuzz corpora,
  and wider real ITGmania/StepMania song-pack coverage beyond the pinned shipped
  songs. The two documented compatibility policies do not conceal packet holes
  or repair arbitrary malformed containers.
* Competitive native encoding quality/bitrate control, broader real-music lossy
  evaluation and encoding performance, measured decoding pipeline
  optimization and broader throughput/startup plus memory/allocation/seek
  comparisons with libvorbis and Symphonia. The five-file
  [resource/seek baseline](RESOURCES.md) now covers requested heap, process memory
  and 500 shared PCM-window requests, exposing substantial seek performance
  gaps. This remains an initial sample rather than a complete benchmark suite.
  No SIMD or competitive performance claims yet.
  The initial [encoding baseline](ENCODING_PERFORMANCE.md) now measures six
  shared PCM inputs, 42 independently decoded streams and 294 timing trials.
  It exposes a compression gap on two song excerpts and profiles actual packet
  symbols for an entropy improvement. The subsequent
  [compact-book update](ENCODING_ENTROPY.md) reduces paired two-stage song files
  26–29% with identical decoded PCM through three decoders. Header costs increase
  tiny-file sizes. These comparisons do not establish matched perceptual quality
  against another encoder or complete the broader performance/conformance goal.

Raw packet and full-container comparisons are separate: raw PCM tests assign
unknown granules and omit EOS trimming; `ogg_mapping` tests independently use
libvorbisfile and Symphonia's container demuxer with real timestamps and trim.
The separate `seeking` suite checks pre-roll, exact requested positions, retained
suffixes and repeated seeks. These fixtures/corpus checks do not establish full
Vorbis conformance or competitive performance. External reference APIs remain in
separate test helpers.
