# deadlib-audio-decode-wav

Independent pure-Rust WAV implementation. No production dependencies, playback
integration, or changes to the existing DeadSync decoder. Registered only as a
Cargo workspace member. License follows the repository (GPL-3.0).

`Wave::parse(&bytes)` validates a complete form while borrowing every payload.
`wave.chunks()` exposes unknown chunks and original padding without copying;
`wave.as_bytes()` retains the exact form. Bytes after the RIFF boundary are excluded.
Native LIST/wavl audio also supports multiple data chunks and slnt held-frame
silence. `wave.wavl_chunks()` exposes those children; `wave.pcm()` reads across
them without materializing the whole sequence. See [WAVL.md](WAVL.md) for exact
silence, metadata, fact and seeking behavior. Keep the Wave binding alive while
using a Pcm view, which can borrow its segment index.
`wave.pcm()` provides interleaved U8/I16/I24/I32/F32/F64 access. Native typed slices
are available when byte order and alignment permit. Caller-buffer reads work for
unaligned or swapped input and return whole frames, counted in interleaved samples.
Integer values keep their container scale (valid bits are left justified); U8 to
i32 explicitly removes the 128 bias. Float reads preserve bit patterns including
NaN payloads, signed zero, and subnormals. No implicit float normalization occurs.
`Wave::data()`/`data_offset()`, reader `data_chunk()` and `Pcm::as_bytes()` return
Some only for contiguous data; segmented audio returns None. `Pcm::read_frames`
copies raw whole frames, and `Pcm::samples()` returns the logical count as u128.
The view caches its native layout in compact state. `Pcm::format()` reconstructs
the complete descriptor from validated borrowed bytes; retain that result when
querying format metadata repeatedly. Frame reads do not repeat that parsing.

`Reader<R: Read + Seek>` scans chunk headers, retains format and metadata offsets,
and seeks over audio. `seek_frame` addresses native PCM/float frames exactly with
64-bit offsets. `read_frames` returns raw whole frames; typed methods return
interleaved sample counts. `read_integers` explicitly expands packed U8/I16/I24/I32
inside the caller's output storage. `read_metadata` preserves the audio position.
After an I/O error a successful audio seek is required before reading again.

`Stream<R: Read>` is a forward-only chunk demuxer. `next_chunk` drains unread
payload and padding; `read_chunk` fills caller-owned bytes. Call `finish` to
validate through the form boundary. RF64/BW64 ds64 is consumed during construction
and exposed separately. `Reader::chunks` describes subsequent chunks. Demuxing
does not require any audio decoder or format chunk ordering.

`Writer<W: Write + Seek>` writes native PCM/float frames incrementally, carries
arbitrary ancillary chunks, and finalizes sizes with `finish`. Default output
reserves JUNK space for automatic RIFF-to-RF64 promotion. RIFX, explicit RF64 and
BW64 mechanics are also available. `Spec` describes sample width, valid bits and
channel mask; extensible output is selected when needed. Raw frame writes preserve
all bits. Typed writes require their native format; `write_integers` explicitly
packs integer containers after checking range. Failed writes poison the writer.
Dropping a writer does not finalize a file. Sinks are used from their current
position and are not truncated; callers replacing longer files must truncate them.

`WavlWriter` emits native data/slnt/ancillary child chunks incrementally, with
seekable finalization or declared byte/frame lengths on a Write-only sink.
It splits long silence counts without allocating PCM. See WAVL.md for count
accounting, size limits and RF64/BW64 LIST mechanics.
`WavlMuxer` supplies the same container operations for a raw format descriptor
and preencoded data children. Producers supply decoded frame counts; block
validation stays with the codec. WavlWriter now uses this common muxer.
BW64 dummy fields are preserved raw on input and zeroed on output. Its float,
compressed and wavl paths are compatibility extensions; see [BW64.md](BW64.md)
for count limits, the RF64 distinction and remaining profile requirements.

`Adpcm` provides independent Microsoft/IMA block decoding into i16 caller buffers.
`AdpcmData` adds fact-trimmed borrowed audio and exact range access using reusable
block scratch. See [ADPCM.md](ADPCM.md) for coverage and reference disagreements.
`AdpcmWavl::new(&wave)` adds borrowed compressed data/slnt sequences with exact
range reads and held-frame seeks. It retains one compact index and uses caller
scratch without expanding virtual silence. Flat AdpcmData remains allocation-free.
See [WAVL.md](WAVL.md) for global fact trimming and segment-padding requirements.

`Adpcm::encode_block` writes a full lossy block into caller-owned encoded storage;
the last real frame is repeated as padding. `AdpcmSpec` emits standard descriptors.
`AdpcmWriter` accepts arbitrary batches of whole i16 frames, retains one pending
block, and records the unpadded frame count on finish. Both Microsoft and 4-bit
IMA encoding are native Rust; `AdpcmKind::Ima3` adds standard 3-bit WAV IMA.
Encoding does not promise lossless source recovery.
IMA also supports the extensible GUID/samples-per-block mapping and caller-buffer
descriptor serialization with channel masks. AdpcmWriter honors force_extensible
for IMA; requesting the unresolved Microsoft mapping returns Unsupported before
I/O. See [IMA_EXTENSIBLE.md](IMA_EXTENSIBLE.md) for precise scope and evidence.

`AdpcmReader<R: Read + Seek>` scans through the shared container reader and caches
one decoded block. It reads interleaved i16 into caller storage and supports exact
frame seeks, including RF64 positions above 4 GiB. Its memory depends on block
geometry and metadata descriptors. Metadata access preserves the logical audio
position. I/O or decode errors require a successful seek before reading again.
AdpcmReader also handles compressed wavl children and held-frame seeks, reusing
the container's inventory and the same one-block cache. Its wavl_chunks method
exposes child descriptors for metadata reads. AdpcmForwardReader handles both
flat and segmented compressed data using two reusable block buffers and a known
duration; metadata callbacks can inspect children while decoding or finishing.

`Muxer<W: Write + Seek>` accepts a raw format descriptor, encoded bytes and
decoded frame counts. It preserves the descriptor without codec dispatch and
handles fact/ds64/RIFF finalization. Producers own encoded block validation; native
PCM/float counts are checked against block alignment. The native `Writer` uses
this same muxer. RF64 promotion also accounts for decoded counts above 32 bits,
even when compressed data is small.

Known-length output also works with Write-only sinks: `Muxer::with_lengths`,
`Writer::with_frames` and `AdpcmWriter::with_frames` emit final sizes before audio
and finalize with `finish_stream`. See [STREAMING.md](STREAMING.md) for count
validation, RF64 selection and sequential I/O semantics.

`ForwardReader<R: Read>` provides native frame/typed reads and raw encoded-data
access; `AdpcmForwardReader` adds bounded-block compressed decoding. Metadata
callbacks can inspect leading/trailing ancillary payloads. See STREAMING.md for
final validation and duration/order requirements on a forward-only source.

`metadata` provides borrowed LIST/INFO/adtl, cue, sampler, broadcast extension,
RF64 marker and BW64 CHNA/XML envelope views. Matching helpers serialize into
caller buffers. `Chunk::metadata` performs explicit bounded parsing; raw payloads
and unknown chunks remain available unchanged. See [METADATA.md](METADATA.md) for
the validation evidence and the boundary between container data and ADM/XML
interpretation.

Build and test by package name:

```sh
cargo test -p deadlib-audio-decode-wav
```

See [COVERAGE.md](COVERAGE.md) for evidence and limitations and [PLAN.md](PLAN.md)
for the full outstanding objective. The current implementation is under development.
Bounded mutation, coverage-guided fuzzing and sanitizer evidence is recorded in
[FUZZING.md](FUZZING.md).
Performance history and remaining targets are in [PERFORMANCE.md](PERFORMANCE.md).
[NATIVE_PERFORMANCE.md](NATIVE_PERFORMANCE.md) records all six native widths,
same-output Hound comparisons, dense real-file streaming, process memory and the
measured PCM32 integer-path optimization. [AUDIT.md](AUDIT.md) tracks the full goal.
Native PCM/float sample and encoder comparisons against four references are in
[PCM.md](PCM.md). Real NTFS files over 16 GiB, 32/64-bit seek checks and scoped
whole-process memory measurements are in [LARGE_FILES.md](LARGE_FILES.md).
