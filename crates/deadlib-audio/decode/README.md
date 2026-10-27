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
The requested goal remains open: broader conformance/legacy/fuzzing, index-free
codec seeking, comparative performance and competitive native encoding remain.

The independent [fuzz workspace](fuzz/README.md) supplies five coverage-guided
AddressSanitizer targets, seeded from normal and optional floor-zero fixtures,
generated multiplexing/chains and spanning packets. Its Windows runner retains
replayable corpora, hashes, logs and statistics without changing production
dependencies or the application workspace.

The initial [Vorbis performance baseline](vorbis/PERFORMANCE.md) measures full
decode, 4 KiB fragmented reads and warm-process startup against libvorbisfile and
Symphonia with exact decoded counts. It exposes a substantial native throughput
gap. Component profiling, memory/allocation/seek measurements, optimization and
competitive native encoding remain required.
The first measured transform optimization replaces a length-N complex FFT with
a paired-spectrum N/4 FFT. Its isolated kernel is 3.2–3.8x faster, transform
elements shrink 65%, and observed full-file times improve 1.35–1.67x on the two
longer tracks, with the same PCM/count/seek gates. Reference timing drift and
remaining throughput gaps are documented in the performance report.
The bounded Huffman prefix lookup adds another observed 1.08–1.19x full-file
speedup on the longer tracks in interleaved baseline/current trials, at about
6% higher stream-startup time. Tables fit the existing tree buffer and remaining
setup budget. Scalar, full PCM, seeking and sanitizer gates remain unchanged;
the short-clip tradeoff is explicit in the report.

The [resource and arbitrary-seek baseline](vorbis/RESOURCES.md) separates
requested-heap instrumentation, fresh-process memory and uninstrumented timing.
Five files pass 500 shared reference-PCM seek requests. Native's requested heap
is smaller than Symphonia's here, while eager two-pass indexing and page-sized
decoding leave clear seek performance gaps. Raw trials, counter rows and hashes
are retained; this milestone changes no production decoder behavior.
The following packet-read update adds generic resumable Ogg capture and
`OggSeekReader::read_packet`, sharing the existing page API's exact timing and
overlap logic. Paired trials improve seek batches 1.16-1.76x, with zero warmed
native seek allocations on all five inputs. Eager indexing and pre-roll still
leave performance work. Full differential, packet/mixed-read and sanitizer
evidence is recorded in the resource report.
The compact packet-mode index then selects the nearest overlap pre-roll packet
from two logical bits per packet. Paired longer-file seek batches improve another
1.99x/3.18x for 4,224 additional live requested bytes per track, beating both
references on those two seek workloads in this run. Short files, eager index
construction, and full-file throughput still have performance gaps. The
page-only pre-roll option preserves a lower-storage strategy with exact PCM.

The initial [native encoder](vorbis/ENCODING.md) now generates setup and audio
packets directly from PCM, with a native forward MDCT, fixed blocks, an adaptive
flat floor and one to four additive residue stages. Raw and Ogg APIs support
arbitrary input chunks and exact final trimming; normal raw encode/reset has
zero allocations after construction. Independent libvorbisfile and Symphonia
decoding validate the output, including large packets and 255-channel streams.
Source SNR and size measurements document its lossy precision tradeoffs. The
encoder still needs perceptual analysis, rate control, compression optimization
and comparative performance/real-music quality work.

## Normative sources

* [RFC 3533](https://www.rfc-editor.org/rfc/rfc3533.txt).
* [Xiph Ogg framing](https://www.xiph.org/ogg/doc/framing.html) and
  [stream/mapping requirements](https://www.xiph.org/ogg/doc/oggstream.html).
* [Vorbis I specification](https://www.xiph.org/vorbis/doc/Vorbis_I_spec.html),
  including Appendix A (Ogg mapping).

Implement from these specifications. Independent codecs are test/benchmark
oracles only, never production dependencies or delegated implementations.
