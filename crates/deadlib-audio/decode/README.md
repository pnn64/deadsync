# Independent audio codecs

This directory is being developed independently of DeadSync's audio pipeline.
Only Cargo workspace membership is permitted until integration is separately
requested. The existing `deadlib-audio-decode`, Symphonia path, playback and
backends must remain unchanged.

The current priority is **complete behavioral parity**, not performance.
Counts, positions and behavior require exact agreement; the original justified
PCM tolerances remain in force. See [the parity ledger](vorbis/PARITY.md) for
corrected differences, independent evidence and remaining gaps.

## Architecture and complete work plan

* `ogg/`: `deadlib-audio-decode-ogg`, codec-independent container, no production
  dependencies. Pages, CRC, packet assembly, validation, logical/physical streams,
  multiplexing, chains, incremental IO, recovery, granule landmarks and seeking.
* `vorbis/`: `deadlib-audio-decode-vorbis`, native Vorbis I codec depending only
  on the Ogg crate. Raw packet decoding must remain independently usable.

The requested end state includes **all** of the following. An implemented
primitive or passing unit test does not establish completion of its parent item.

1. Complete Ogg container implementation and mapping-independent APIs; compare
   packet boundaries, flags, CRC and granules against libogg.
2. Vorbis bitpacking, identification/comments/setup, codebooks and Huffman/VQ,
   both floors, all residues, mappings, channel coupling, inverse MDCT,
   windowing, overlap-add and PCM reconstruction.
3. Ogg Vorbis mapping, exact sample counts and channel order, timestamps,
   initial/final granule trimming, pre-roll, repeated sample-accurate seeks,
   streaming, decoder reset, multiplexing and chaining.
4. Independent differential validation against libogg/libvorbis and Symphonia;
   official conformance material, malformed/fuzz inputs, arbitrary IO boundaries,
   unusual channel counts, legacy encoders and real ITGmania/StepMania songs.
   Document any legacy policy separately from the normative decoder.
5. Native Vorbis encoding validated through independent decoders, with lossy
   reconstruction criteria. Container writing alone is not codec encoding.
6. Retain existing reproducible measurements as diagnostics. Further performance
   work is deferred while parity gaps remain; speed is not a completion criterion.
7. Independently build/test both packages by package name and extracted
   manifests, with no production codec dependencies. Keep benchmark targets
   buildable and preserve existing measurements; new performance work is outside
   the current request. Maintain coverage, numeric criteria and limitations.

Each tested milestone is committed separately. Per-crate coverage documents
record current evidence and outstanding work; this checklist preserves the full
scope and its completion evidence.

## Current checkpoint

**The Vorbis decoding goal is complete.** The
[completion audit](vorbis/DECODING_COMPLETION.md) maps all seven work-plan items
to current independent evidence and final source-hashed checks. Counts,
positions, timestamps and channel identity are exact under each documented
policy; the original PCM bounds are unchanged.

The full release gate enables all external reference/corpus tests, excluding
only manual fixture generators. Extracted offline builds/tests, lint, fuzz
harness checks and benchmark-target compilation also pass. Independent
comparisons cover libogg, libvorbis/libvorbisfile and Symphonia, all 38 repository
assets, 528 upstream coded vectors, historical/legacy media, two shipped ITGmania
songs and 32 additional chart-linked StepMania songs. Component and full-packet
matrices cover malformed/recovery behavior and unusual legal configurations.

The native mapping supports exact trim/timing, arbitrary byte streaming, reset,
multiplexing/chains and indexed sample-accurate seeking. Explicit
[reference seeking](vorbis/REFERENCE_SEEKING.md) reproduces C file scheduling,
including retained block PCM after failed preroll. The default addresses exact
sequential PCM suffixes. [Untimed empty EOS pages](vorbis/UNTIMED_EOS.md) now
decode successfully, with declared and emitted durations kept separate when
the reference reports different values. Named strict/reference/legacy policies
and other-decoder defects remain documented and tested.

The [native encoder](vorbis/ENCODING.md) remains independently validated,
including the 144-case decoder matrix. Competitive encoding quality/rate
control, sparse/index-free codec seek strategies, additional platforms and
application integration are future tasks outside the current decoding request.
The indexed API is complete for its documented contract.

The independent [fuzz workspace](fuzz/README.md) preserves bounded sanitizer
campaigns and replayable evidence. Additional media and longer campaigns remain
useful maintenance; these tests are not exhaustive certification of every input.

Historical measurements and rejected experiments remain available in the
[Vorbis performance report](vorbis/PERFORMANCE.md),
[container report](ogg/PERFORMANCE.md), [resource/seek report](vorbis/RESOURCES.md),
[synthesis profile](vorbis/SYNTHESIS_PROFILE.md),
[residue report](vorbis/RESIDUE_PERFORMANCE.md),
[encoding comparison](vorbis/ENCODING_PERFORMANCE.md),
[entropy update](vorbis/ENCODING_ENTROPY.md),
[scalar-book evaluation](vorbis/SCALAR_BOOK_EVALUATION.md) and
[prefix-path report](vorbis/PREFIX_PATHS.md). Performance is not a completion
criterion for this request.

## Normative sources

* [RFC 3533](https://www.rfc-editor.org/rfc/rfc3533.txt).
* [Xiph Ogg framing](https://www.xiph.org/ogg/doc/framing.html) and
  [stream/mapping requirements](https://www.xiph.org/ogg/doc/oggstream.html).
* [Vorbis I specification](https://www.xiph.org/vorbis/doc/Vorbis_I_spec.html),
  including Appendix A (Ogg mapping).

Implement from these specifications. Independent codecs are test/benchmark
oracles only, never production dependencies or delegated implementations.
