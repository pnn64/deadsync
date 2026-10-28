# deadlib-audio-decode-wav

Independent pure-Rust WAV implementation. No production dependencies, playback
integration, or changes to the existing DeadSync decoder. Registered only as a
Cargo workspace member. License follows the repository (GPL-3.0).

`Wave::parse(&bytes)` validates a complete form while borrowing every payload.
`wave.chunks()` exposes unknown chunks and original padding without copying;
`wave.as_bytes()` retains the exact form. Bytes after the RIFF boundary are excluded.
`wave.pcm()` provides interleaved U8/I16/I24/I32/F32/F64 access. Native typed slices
are available when byte order and alignment permit. Caller-buffer reads work for
unaligned or swapped input and return whole frames, counted in interleaved samples.
Integer values keep their container scale (valid bits are left justified); U8 to
i32 explicitly removes the 128 bias. Float reads preserve bit patterns including
NaN payloads, signed zero, and subnormals. No implicit float normalization occurs.

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

`Adpcm` provides independent Microsoft/IMA block decoding into i16 caller buffers.
`AdpcmData` adds fact-trimmed borrowed audio and exact range access using reusable
block scratch. See [ADPCM.md](ADPCM.md) for coverage and reference disagreements.

`Adpcm::encode_block` writes a full lossy block into caller-owned encoded storage;
the last real frame is repeated as padding. `AdpcmSpec` emits standard descriptors.
`AdpcmWriter` accepts arbitrary batches of whole i16 frames, retains one pending
block, and records the unpadded frame count on finish. Both Microsoft and 4-bit
IMA encoding are native Rust; encoding does not promise lossless source recovery.

`AdpcmReader<R: Read + Seek>` scans through the shared container reader and caches
one decoded block. It reads interleaved i16 into caller storage and supports exact
frame seeks, including RF64 positions above 4 GiB. Its memory depends on block
geometry and metadata descriptors. Metadata access preserves the logical audio
position. I/O or decode errors require a successful seek before reading again.

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
The first resident PCM16 comparison and allocation/cycle evidence is recorded in
[PERFORMANCE.md](PERFORMANCE.md); wider formats and platforms remain unmeasured.
