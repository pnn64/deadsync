# WAV ADPCM block codecs and differential evidence

`Adpcm::from_format` validates block geometry independently of the WAV container.
`decode_block` fills caller-owned interleaved i16 storage and returns frames.
Microsoft ADPCM supports mono/stereo, all seven standard predictors, additional
coefficient pairs, high-nibble-first interleaving and block padding. IMA supports
3-bit and 4-bit WAV word groups with arbitrary channel counts within block-field limits.
Each block restarts from its header. Neither block decoder allocates.

`AdpcmData::new(&wave)` borrows encoded data and uses fact/RF64 sample counts to
trim padding. `read_frames(start, output, scratch)` gives sample-accurate random
access with caller-owned block scratch, returning interleaved samples. The
container retains unrecognized encoded formats without invoking either codec.
`AdpcmWavl::new(&wave)` handles compressed LIST/wavl data/slnt children with a
compact frame index and caller-owned block scratch. It implements exact borrowed
range reads, including direct seeks into held silence. `Adpcm::data_frames`
reports a payload's decoded capacity without reading its block headers. Per-child
padding, the global fact limit and evidence are documented in [WAVL.md](WAVL.md).
AdpcmReader also supports segmented data and held-frame seeks with its one-block
cache. AdpcmForwardReader handles both layouts without an index, using a known
duration and bounded metadata callbacks. Its finish operation checks all child
geometry/counts while leaving intentionally unread block headers undecoded.
Extensible IMA now uses its GUID and samples-per-block union through every block
and file API. `AdpcmSpec::format_extensible` serializes speaker masks, and IMA
writers honor force_extensible. [IMA_EXTENSIBLE.md](IMA_EXTENSIBLE.md) records the
independent truth, reader limitations and unresolved Microsoft coefficient layout.
BW64 dummy words never supply a duration; its ADPCM compatibility path requires
fact and supports literal counts through u32::MAX. Writers reject larger BW64
fact counts. RF64 retains its 64-bit count extension. See [BW64.md](BW64.md).

Primary algorithm references:

- [Microsoft Multimedia Standards Update, 1994](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/RIFFNEW.pdf), Microsoft ADPCM and DVI ADPCM sections.
- [IMA Recommended Practices revision 3, 1992](https://www.cs.columbia.edu/~hgs/audio/dvi/IMA_ADPCM.pdf), ADPCM reference algorithm.

The implementation uses independent integer shifts for IMA reconstruction and
division toward zero for Microsoft prediction. The independent golden vectors
include a negative fractional prediction that distinguishes division from shift.

## Native encoding milestone

`Adpcm::encode_block` now writes caller-owned block storage without allocating.
It supports the same standard/custom Microsoft descriptors and 4-bit IMA word
layouts as decoding. Partial final blocks repeat the last frame; fact records
the original length. `AdpcmSpec` emits standard fmt payloads. `AdpcmWriter`
buffers incomplete input, bypasses that copy for full blocks, and reuses two
fallibly allocated buffers. Memory depends on block geometry, not file duration.
Native PCM/float encoding remains bit preserving; ADPCM is explicitly lossy.

Selection is deliberately bounded: IMA estimates the starting index from up to
32 differences; Microsoft scores coefficient predictors on up to 32 real frames.
The quantizer tracks reconstructed samples, rather than original sample history.
This is a fast basic encoder, not a trellis or perceptually optimized encoder.
Impulse quality is a known weakness; SNR/RMSE and maximum errors are retained in
the report rather than inferred from successful decoding.

Encoder validation:

```sh
python crates/deadlib-audio/decode/wav/tests/reference_adpcm_encode.py --output target/wav/my-encoding-validation.json
```

The Symphonia 0.6.1 reference lives in a separate test workspace with its own
lockfile, so its dependencies do not affect the application workspace. The script
builds this reference, dr_wav, and crate examples; generates PCM independently;
encodes using this crate; and compares decoded bits through all four references.
2026-10-28: 144 fixture sets, 75,096 original samples, all decoded lengths exact.
Tests vary mono/stereo, three block sizes, lengths 1/17/1025, and constant/sine/
noise/impulse signals. All 36 constant cases preserve the original samples.

| Reference | Exact encoded-file decoding after fact trimming | Details |
| --- | ---: | --- |
| FFmpeg 7.1 | 108/144 | All 72 MS; 36/72 IMA (known rounding difference) |
| Symphonia 0.6.1 | 108/144 | All 72 MS; 36/72 IMA (same rounding distinction) |
| dr_wav pinned commit above | 132/144 | All 72 IMA; 60/72 MS |
| libsndfile 1.2.2 | 113/144 | All 72 IMA; 41/72 MS |

Four new encoder tests verify exact constant/header-only vectors, full/partial
block batch invariance in all four containers, custom predictors, up to 4000 IMA
channels, exact fact trimming and invalid-input atomicity. Total crate tests: 27.

## File-backed decoding and exact seeks

Four added integration tests cover sequential/random ranges in all container
spellings, mono/stereo, unaligned caller capacity, nonzero source bases, metadata
between reads, short reads, cache reuse, I/O recovery, malformed headers, data
before fmt with trailing fact, a short final IMA block, and RF64 seeks above
4 GiB/2^32 frames. The virtual large-file test runs with and without fact, proving
ds64 fallback while reading less than 512 actual source bytes.

```sh
python crates/deadlib-audio/decode/wav/tests/reference_adpcm_file.py --output target/wav/my-file-validation.json
```

Forward-only compressed reading is implemented; see STREAMING.md. Remaining:
extensible compressed mappings, RIFX external validation, exhaustive Symphonia decode
comparison and longer fuzz campaigns. Negative Microsoft header deltas are
rejected. Unbounded delta growth errors safely rather than overflowing. The
initial implementation prioritizes correctness; no ADPCM throughput claim yet.

## Three-bit WAV IMA

`AdpcmKind::Ima3` selects standard 3-bit WAV IMA encoding. Existing `Ima` remains
4-bit. `Adpcm::bits_per_sample()` exposes the validated width. The borrowed,
seekable, forward-only and known-length output APIs share the same block codec.
No production dependency or whole-file buffer is introduced.

The format and packing follow
[Microsoft RIFFNEW, pages 30–37](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/RIFFNEW.pdf):
block bytes are `(4 + 12*N)*channels`, with `1 + 32*N` frames. The header retains
the initial sample and step index. Each channel's 12 encoded bytes span three
32-bit words; physical words alternate channels. Codes can cross a word boundary
and must skip other channels' words. Short final blocks must contain complete
three-word channel groups. Bad step indices, partial groups, oversized blocks and
insufficient caller storage fail safely. Header-only blocks are supported.

Reconstruction uses independently rounded step, half-step and quarter-step terms
from the primary algorithm. The pinned
[FFmpeg 7.1 decoder](https://github.com/FFmpeg/FFmpeg/blob/n7.1/libavcodec/adpcm.c#L459)
combines these terms before rounding. The independent Python oracle records both
arithmetic variants so packing agreement can be checked without changing the
production arithmetic or concealing sample-bit differences.

```sh
python crates/deadlib-audio/decode/wav/tests/reference_ima3.py --output target/wav/new-ima3.json
```

| Pinned reference | Successful decodes / 122 | Exact spec-arithmetic prefixes | Exact combined-arithmetic prefixes |
| --- | ---: | ---: | ---: |
| FFmpeg 7.1 | 56 | 30 | 56 |
| dr_wav a466dbe88fe2576d46c43877d602f3d66b8561a7 | 52 | 14 | 14 |
| libsndfile 1.2.2 | 0 | 0 | 0 |
| Symphonia 0.6.1 | 0 | 0 | 0 |

The predeclared 44 mono/stereo RIFF packing/combined-arithmetic gates all pass.
The other 12 successful FFmpeg cases are mono/stereo RF64/BW64. FFmpeg rejects
54 wider-channel cases and 12 RIFX cases in this driver. Symphonia and libsndfile
explicitly reject the 3-bit width. dr_wav's matching prefixes are header-only or
single-frame coincidences; its remaining successful outputs disagree. This is
not evidence of general 3-bit support in dr_wav. The report records every exit,
full output length/hash, fact-limited prefix, first mismatch and stderr. Prefix
trimming is explicit because references may emit the encoded block padding.
No external reader in this run validates 3-bit RIFX or more than two channels;
those paths are supported by primary-spec fixtures and the independent oracle.

Encoding remains a bounded greedy lossy quantizer. Recorded constant-signal
errors are at most one i16 unit; large discontinuities/noise can be much worse
(maximum errors 60512/46865 in the ramp/noise cases). Successful encoded-file
decoding does not imply high perceptual quality, and no compressed-throughput
superiority is claimed.

## Cached file-reader parameters

AdpcmReader validates its owned immutable descriptor during construction and
caches compact block parameters. Loading later blocks borrows custom Microsoft
coefficient bytes directly from that descriptor without parsing it again.
Per-block header/sample checks, short tails, fact trimming, I/O poisoning and
metadata restoration keep their previous contracts. Channels, frames/block and
coefficient count retain the u16 limits of their validated on-wire fields.
No self-referential storage or new allocation is needed. The public block codec
still validates caller-supplied descriptors through Adpcm::from_format.
