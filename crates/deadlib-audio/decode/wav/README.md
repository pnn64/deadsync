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

Build and test by package name:

```sh
cargo test -p deadlib-audio-decode-wav
```

See [COVERAGE.md](COVERAGE.md) for evidence and limitations and [PLAN.md](PLAN.md)
for the full outstanding objective. The current implementation is under development.
