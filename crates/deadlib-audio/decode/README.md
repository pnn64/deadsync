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
Symphonia; strict behavior remains separate from legacy compatibility. Native
Ogg now provides a validated page/packet index, index-free granule bisection and
strict packet capture after seeks, including continuation and multiplexing.
Vorbis now provides prefix-based indexing and sample-accurate retained-frame
seeking with overlap pre-roll, initial/final trimming and repeated stream selection.
All 1,900 corpus seeks match libvorbisfile positions/counts and PCM criteria.
The pinned upstream coded test grid adds 528 rate/channel/quality combinations,
with exact counts, reference PCM parity, 17-byte streaming and 2,640 native seeks.
External historical/song media is hash-pinned and downloaded only to ignored
scratch storage. Strict decoding exposes malformed granules in one shipped
ITGmania song and a historical setup page. Two explicit, disabled-by-default
policies handle these cases; the indexed endpoint policy can trim across prior
pages using the final length known in advance. Four historical/shipped files
and 36 native seeks validate those policies; see [vorbis/LEGACY.md](vorbis/LEGACY.md).
**This is not the completed goal:** encoding, broader conformance/legacy/fuzzing,
index-free codec seeking and comparative performance work remain. The next work
is longer fuzz campaigns and reproducible pipeline benchmarks,
followed by measured optimization and independent native encoding validation.

The independent [fuzz workspace](fuzz/README.md) supplies four coverage-guided
AddressSanitizer targets, seeded from normal and optional floor-zero fixtures,
generated multiplexing/chains and spanning packets. Its Windows runner retains
replayable corpora, hashes, logs and statistics without changing production
dependencies or the application workspace.

## Normative sources

* [RFC 3533](https://www.rfc-editor.org/rfc/rfc3533.txt).
* [Xiph Ogg framing](https://www.xiph.org/ogg/doc/framing.html) and
  [stream/mapping requirements](https://www.xiph.org/ogg/doc/oggstream.html).
* [Vorbis I specification](https://www.xiph.org/vorbis/doc/Vorbis_I_spec.html),
  including Appendix A (Ogg mapping).

Implement from these specifications. Independent codecs are test/benchmark
oracles only, never production dependencies or delegated implementations.
