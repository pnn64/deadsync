# Sequential WAV input and output

`Stream<R: Read>` is the codec-independent forward-only chunk demuxer. It checks
form/chunk extents, RF64/BW64 size tables, limits and odd padding without buffering
audio. Its `read_chunk` never crosses the current payload; `next_chunk` drains
any remainder and padding. A caller can supply its own codec and metadata policy.
`ForwardReader<R: Read>` adds native frame reads and raw encoded-data access;
`AdpcmForwardReader<R: Read>` adds Microsoft/4-bit IMA frame decoding. Seekable
Reader and AdpcmReader provide arbitrary exact seeks.

## Forward-only frame reading

Opening a ForwardReader consumes only headers through the first data header. It
retains bounded format bytes and no per-chunk inventory or audio allocation.
Native calls fill complete interleaved frames in caller buffers: raw bytes,
native i16/i32/f32/f64, or explicit integer expansion via read_integers. The
conversion implementation is shared with Reader, with integer container scale,
valid-bit padding and exact float payload bits unchanged. A raw read_data call
also supports opaque encoded formats; subsequent native calls require the current
byte position to be frame aligned.

```rust
use deadlib_audio_decode_wav::{Error, ForwardReader};
use std::io::Read;

fn drain_native_i16(input: impl Read) -> Result<u64, Error> {
    let mut reader = ForwardReader::new(input)?;
    let channels = usize::from(reader.format().channels);
    let mut samples = vec![0i16; channels * 128];
    let mut frames = 0;
    loop {
        let n = reader.read_i16(&mut samples)?;
        if n == 0 { break; }
        frames += (n / channels) as u64;
    }
    reader.finish()?;
    Ok(frames)
}
```

The convenience reader requires fmt before data, matching the ordering requirement
in the [1991 IBM/Microsoft WAVE specification, page 3-22](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/riffmci.pdf).
Data-before-fmt files use the seekable Reader compatibility path. Finish drains
unread data and validates the remaining form, including duplicate reserved chunks,
chunk limits, physical truncation and odd padding. Reading zero samples at the
data boundary does not validate the footer. Bytes following the RIFF form stay
unconsumed. I/O errors poison the stream; failed reads may have modified output.

with_metadata visits leading ancillary chunks, and finish_with_metadata visits
trailing ones. The callback receives ChunkInfo and a bounded ChunkPayload that
implements Read. It can consume any payload prefix; the rest is drained before
the next header. It cannot read padding/headers or advance the demuxer. fmt,
fact and ds64 are handled internally. Raw outer padding is skipped; use borrowed
Chunk/Wave views when exact original pad bytes must be retained. The callback
chooses whether to copy, parse or discard payloads, and its errors are propagated.

AdpcmForwardReader retains one encoded and one decoded block, shared by arbitrary
caller read sizes. It knows the exact decoded length before emitting samples from
a leading fact, ds64 sample count, or an explicit external frame count supplied
to from_reader_with_frames. Finish checks the final encountered fact/ds64 against
that count and rejects a missing required duration. Trailing fact therefore works
with ds64 or an externally known count. A plain RIFF stream whose count is only
available after data needs that explicit count; this API does not buffer the
whole encoded stream or emit untrimmed speculative samples. Its initial and final
duration checks do not decode intentionally unread compressed blocks.

## Known-length Write-only output

`Muxer::with_lengths` accepts `DataLength { bytes, frames }`, a raw format
descriptor, container options and ancillary chunks. It needs only `Write`. It
emits the final form/data/fact/ds64 sizes up front; no seek, header rewrite or
audio buffering is needed. Format and metadata validation, checked size arithmetic
and native byte/frame consistency checks happen before any output.

`Writer::with_frames` derives native byte length from the validated sample layout.
`AdpcmWriter::with_frames` derives encoded length by rounding up to complete codec
blocks. The final block repeats the last real frame as needed; fact/ds64 count
only the declared input frames. The normal block algorithms and two reusable
ADPCM buffers are shared with the seekable writer.

```rust
use deadlib_audio_decode_wav::{SampleFormat, Spec, WriteOptions, Writer};

fn wav() -> Result<Vec<u8>, deadlib_audio_decode_wav::Error> {
    // Vec<u8> implements Write and does not implement Seek.
    let mut writer = Writer::with_frames(
        Vec::new(), Spec::new(SampleFormat::I24, 2, 48000),
        WriteOptions::default(), &[], 2,
    )?;
    writer.write_integers(&[-8388608, 8388607, -1, 0])?;
    writer.finish_stream()
}
```

Call `finish_stream` on a Write-only sink. A seekable sink may use `finish` for
either mode; a known-length writer takes the sequential finalization path without
seeking. An unknown-length writer requires `finish` and Seek. Dropping or calling
`into_inner` does not finalize or validate the declared length.

An oversized write is rejected before changing the output, counters or ADPCM
pending samples. Underfill is rejected at finalization before adding container
padding or flushing; ADPCM also checks the total before encoding pending samples.
The caller must finalize and handle that error. Earlier successfully written
bytes cannot be retracted from a nonseekable sink. A partial I/O failure poisons
the writer; subsequent calls fail without continuing output. Flush errors are
reported. Short writes and Interrupted are handled by write_all.

Known-length RIFF chooses RF64 up front when enabled and required by form size,
data length or a fact count using the 32-bit sentinel. It does not reserve an
unused JUNK chunk. RIFX and RIFF with promotion disabled reject overflowing
declared sizes before output. Explicit RF64/BW64 always use ds64. Embedded sink
prefixes do not affect the form-relative sizes. Native sample bits, unused valid
bit padding, float NaNs, metadata payloads and odd-byte alignment retain the same
semantics as the seekable writer. Encoded producers remain responsible for codec
block validity and decoded counts; the generic muxer does not guess them.

## Evidence

Five new integration tests bring the total to 49. The independent Write-only
test sink produces short writes and recurring Interrupted errors and records
flushes/failures. Seventy-two native combinations check all containers, native
widths, 1/3/33 channels, masks, valid bits, float bit patterns, odd metadata and
embedded prefixes against independently packed bits and exact seekable output.
One hundred ADPCM combinations compare identical container/encoded bytes across
Microsoft/IMA layouts, channel counts and empty/partial/full/cross-block durations.
Additional checks cover overfill/underfill atomicity, poisoned I/O, flush errors,
u64 arithmetic, RIFF sentinel boundaries and decoded-count-only RF64 promotion.
Large-header tests use small physical buffers; they do not claim physical >4 GiB
output evidence.

The mutation harness also compares known-length native output with seekable
output and validates known-length ADPCM against the same encoded block bytes.
Performance and scoped heap evidence for the Write-only native path is recorded
in PERFORMANCE.md. Broader codec throughput and physical-file/RSS evidence remain.
All 49 tests pass in x64 release and i686 debug, and Clippy passes. Two checked
one-million-mutation campaigns (x64/i686, distinct seeds) pass with the new native
and ADPCM known-length properties. Reports are retained in
`validation/fuzz-known-output-{x64,i686}-1m.json`.

Seven forward-reading tests bring the total to 56, passing x64 debug/release and
i686 debug with Clippy. Tests cover all native widths/containers, 1/3/4097
channels, NaN payloads, caller-buffer and physical-read splits, bounded metadata
before/after format/data, complete truncation sweeps, duplicate footers, raw/frame
alignment, short ADPCM blocks, trailing duration checks, I/O/codec poisoning and
u64 durations from tiny physical sources. These large declarations do not prove
successful physical multi-gigabyte streaming.

The retained 144 four-reference ADPCM fixtures pass all 432 Read-only real-file
decodes with frame buffers 1/127/4097 and physical read limits 1/17/4096, including
recurring Interrupted. Exact fixture/output hashes and reference provenance are
in `validation/adpcm-forward-references.json`. The reference arithmetic scope and
disagreements remain those documented in ADPCM.md.

Two million checked x64/i686 mutations pass with forward native/ADPCM state
properties; all 1474 earlier archived corpus files replay. A fresh 100000-execution
ASan campaign also passes and its complete 1108-file corpus is archived. See
FUZZING.md for budgets, source hashes, commands and raw evidence. These checks
remain bounded evidence rather than exhaustive conformance or malformed-input proof.
