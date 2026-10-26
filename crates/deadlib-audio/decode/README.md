# Independent audio codecs

This directory is being developed independently of DeadSync's audio pipeline.
Only Cargo workspace membership is permitted until integration is separately
requested. The existing `deadlib-audio-decode`, Symphonia path, playback and
backends must remain unchanged.

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
6. Reproducible full-file, streaming, startup, allocation/memory and arbitrary
   seek benchmarks against libvorbis and Symphonia. Optimize measured bottlenecks
   in bit IO, codebooks, MDCT, setup and seeking; use SIMD only with evidence.
7. Independently build/test/benchmark both packages by package name and extracted
   manifests, with no production codec dependencies. Maintain coverage, numeric
   tolerance, benchmark and remaining-limitations documents.

Each tested milestone is committed separately. Per-crate coverage documents
record current evidence and outstanding work; this checklist preserves the full
scope while implementation progresses.

## Current checkpoint

Ogg framing/streaming, Vorbis raw-packet PCM synthesis and the Ogg Vorbis mapping are implemented, with
libogg, libvorbis and Symphonia differential evidence. Both packages are Cargo
workspace members. Raw synthesis includes both floors, all residues, inverse
coupling, native FFT MDCT, windows and overlap, with zero packet/reset allocations.
The mapping adds metadata/header lifecycle, exact initial/final trimming,
timestamps, arbitrary byte streaming, independent multiplexed streams and chains.
All 38 repository assets have exact retained counts against libvorbisfile and
Symphonia; strict behavior remains separate from legacy compatibility.
**This is not the completed goal:** codec/container seeking, encoding, broader
conformance/fuzzing and comparative performance work remain. The next milestone
is native container indexing and sample-accurate Vorbis seeking with pre-roll.

## Normative sources

* [RFC 3533](https://www.rfc-editor.org/rfc/rfc3533.txt).
* [Xiph Ogg framing](https://www.xiph.org/ogg/doc/framing.html) and
  [stream/mapping requirements](https://www.xiph.org/ogg/doc/oggstream.html).
* [Vorbis I specification](https://www.xiph.org/vorbis/doc/Vorbis_I_spec.html),
  including Appendix A (Ogg mapping).

Implement from these specifications. Independent codecs are test/benchmark
oracles only, never production dependencies or delegated implementations.
