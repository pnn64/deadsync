# Vorbis coverage and numerical evidence

Status: **header/codebook implementation; no audio synthesis yet**. No full
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

Windows x86-64, rustc 1.99.1 evidence:

* Nine primitive tests plus one generated-fixture header test; external/corpus
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

## Numerical criteria

Codebook vectors use f64 unpacking/evaluation and f32 storage. The external VQ
test allows `abs(native - reference) <= 2e-6 * max(1, abs(reference))` for different
floating arithmetic rounding; the actual tested vectors above were exact. This
is a codebook check, **not a PCM parity criterion**. PCM tolerances must be derived
and documented when the transform/synthesis implementation exists; exact sample
counts, layout, granule interpretation and seek positions will have zero tolerance.

## Remaining required work

* Floor-zero/one audio curve decode, all residue audio decode, inverse coupling,
  inverse MDCT, windowing, overlap-add and PCM output.
* Raw decoder lifecycle/overlap/reset and nominal truncated audio packet behavior;
  non-audio packets and nonfatal comment handling at decoder level.
* Ogg Vorbis mapping, exact initial/final trimming and sample count, timestamps,
  multiplexing/chains, sample-accurate seeking/pre-roll/repeated seeks.
* Official conformance material, unusual/legacy encoders, malformed/fuzz corpora,
  real ITGmania/StepMania song packs. Repository assets are only initial coverage.
  No nonstandard legacy compatibility policy has been added.
* Native encoding, independently decoded lossy validation, measured pipeline
  optimization and reproducible throughput/startup/memory/allocation/seek
  comparisons with libvorbis and Symphonia. No SIMD or performance claims yet.

Header syntax alone does not prove audio conformance. The synthetic floor-zero
header check does not prove floor-zero audio compatibility. External codebook
tests use private libvorbis APIs only inside the separate test helper process.
