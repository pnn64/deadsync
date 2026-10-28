# Sequential WAV input and output

`Stream<R: Read>` is the codec-independent forward-only chunk demuxer. It checks
form/chunk extents, RF64/BW64 size tables, limits and odd padding without buffering
audio. Its `read_chunk` never crosses the current payload; `next_chunk` drains
any remainder and padding. A caller can supply its own codec and metadata policy.
Native/ADPCM forward-only frame convenience readers are the next implementation
step; seekable Reader and AdpcmReader already provide frame APIs and exact seeks.

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
