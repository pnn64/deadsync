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

## Reproduce validation

```sh
python -m pip install --target target/wav/python --no-deps soundfile==0.13.1
python crates/deadlib-audio/decode/wav/tests/reference_adpcm.py --output crates/deadlib-audio/decode/wav/validation/adpcm-references.json
cargo test -p deadlib-audio-decode-wav --target-dir target/wav
```

Python requires numpy and cffi; native references require a C compiler and FFmpeg.
The script downloads checksum-pinned dr_wav only under target/wav/oracle, builds
the small reference driver, constructs independent synthetic blocks, generates
additional encoded files from sine/noise/impulse PCM via FFmpeg, and records hashes
and versions. Windows uses the installed MSVC environment. No reference library
is linked into production code. The SoundFile wheel includes libsndfile.

2026-10-28 Windows x86_64 results: 40 fixture sets, 390,630 decoded samples.
Synthetic IMA sets cover every initial index, nibble, and zero/both signed extrema;
Microsoft sets cover every standard predictor, signed initial sample combinations,
and nibble patterns. Additional files vary codec, channel count, signal and length.

| Reference | Bit-exact sets after limiting output to fact length | Observed disagreements |
| --- | ---: | --- |
| FFmpeg 7.1 | 20/40 | All 20 IMA sets use different reconstruction rounding |
| dr_wav commit a466dbe88fe2576d46c43877d602f3d66b8561a7 | 38/40 | Two synthetic Microsoft sets round negative predictions down |
| libsndfile 1.2.2 (SoundFile 0.13.1) | 38/40 | Two synthetic Microsoft sets differ on extreme/prediction cases |

All IMA sets match both dr_wav and libsndfile. All Microsoft sets match FFmpeg.
These fixed expectations follow the primary arithmetic definitions and golden
vectors; there is no claim of simultaneous parity with mutually disagreeing
references. JSON retains every reference's full length/hash and first mismatch.
The initial naive full-output FFmpeg comparison is also retained as
validation/adpcm-ffmpeg.json. FFmpeg emits full padded blocks for generated files;
our range API trims those to the fact length. Prefix comparisons make that length
difference explicit without ignoring arithmetic mismatches.

Twenty integration tests now pass. ADPCM-specific tests also cover custom
coefficients, malformed header indices, negative deltas, stereo ordering, IMA
1/2/6/32-channel layouts, short blocks, arbitrary cross-block ranges and fact bounds.

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

Microsoft output matches FFmpeg and Symphonia in every case; IMA matches both
dr_wav and libsndfile in every case. Libsndfile rejects 24 MS fixtures because
it requires at least seven samples per block; the fixtures contain six samples
in valid nine-byte-per-channel blocks. Its log and all other disagreements are
retained in `validation/adpcm-encoding-references.json`. No full parity with
mutually disagreeing references is claimed.

Four new encoder tests verify exact constant/header-only vectors, full/partial
block batch invariance in all four containers, custom predictors, up to 4000 IMA
channels, exact fact trimming and invalid-input atomicity. Total crate tests: 27.

## File-backed decoding and exact seeks

`AdpcmReader` consumes the shared `Reader` scan, allocates an encoded and decoded
block once, and fills caller-owned interleaved i16 output. It caches decoded PCM
for repeated reads or seeks within a block. `seek_frame` uses fixed block
addressing within data and binary search for wavl children, decoding a block
prefix or held final frame when necessary. File opening skips audio;
fact or ds64 counts trim final padding. Metadata access keeps the logical audio
position intact. Errors poison reads until a successful seek. This reader does
not allocate PCM for the entire file.
For wavl it reuses the consumed Reader's existing timeline, with no second index
allocation. Empty data/unknown children preserve the held frame, and metadata
child descriptors remain accessible through wavl_chunks. Global fact trimming and
interior padding follow the borrowed policy documented in WAVL.md. Two new tests
and the expanded independent matrix exercise cache identity across children,
metadata restore failure, exact seeks, u64 silence and virtual offsets above
4 GiB. The narrow resident timings and observed flat regression candidates are
recorded in PERFORMANCE.md; broad comparative performance remains unproven.

Four added integration tests cover sequential/random ranges in all container
spellings, mono/stereo, unaligned caller capacity, nonzero source bases, metadata
between reads, short reads, cache reuse, I/O recovery, malformed headers, data
before fmt with trailing fact, a short final IMA block, and RF64 seeks above
4 GiB/2^32 frames. The virtual large-file test runs with and without fact, proving
ds64 fallback while reading less than 512 actual source bytes.

```sh
python crates/deadlib-audio/decode/wav/tests/reference_adpcm_file.py --output target/wav/my-file-validation.json
```

Run the encoder campaign first to regenerate its hash-checked corpus. The file
gate verifies 144 complete real-file outputs against retained reference results,
plus 60 exact ranges around beginnings, block boundaries and EOF. All 204 pass;
evidence is in validation/adpcm-file-ranges.json. Total crate tests: 31, passing
Windows x86_64 debug/release and i686 debug; Clippy with warnings denied passes.
This establishes correctness and bounded I/O, not competitive compressed
throughput. At that milestone real files seek explicitly for each uncached block;
the later buffered-source cursor optimization below removes redundant sequential
seeks.

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

`validation/ima3-references.{json,build.log,corpus.zip}` retains 122 passing cases:
48 container/channel/block geometries, two exhaustive initial-index/codeword/
signed-extrema sets, and 72 native encoder cases. RIFF/RIFX/RF64/BW64, 1/2/6/33
channels, header-only blocks, interleaved word crossings, short final blocks and
fact trimming are covered. All 732 borrowed, file, forward short-read/interruption
and exact range checks match independent spec arithmetic. Four Rust tests add a
fixed packed-byte/sample gold vector, every initial table entry/codeword/extreme,
malformed geometry, output sentinels and both writer finalization modes.

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

All 60 crate tests pass on Windows x64 debug/release and i686 debug; Clippy with
denied warnings passes. All 204 retained Microsoft/4-bit IMA real-file/range
checks still match their earlier reference hashes (`ima3-prior-adpcm.json`). Two
one-million checked mutation campaigns, all 1474 earlier corpus files and 100000
fresh libFuzzer/ASan executions pass. FUZZING.md records sources, bounds, raw logs
and the complete archived coverage-generated corpus.


## Cached file-reader parameters

AdpcmReader validates its owned immutable descriptor during construction and
caches compact block parameters. Loading later blocks borrows custom Microsoft
coefficient bytes directly from that descriptor without parsing it again.
Per-block header/sample checks, short tails, fact trimming, I/O poisoning and
metadata restoration keep their previous contracts. Channels, frames/block and
coefficient count retain the u16 limits of their validated on-wire fields.
No self-referential storage or new allocation is needed. The public block codec
still validates caller-supplied descriptors through Adpcm::from_format.

The extra-predictor file test checks independently calculated samples using the
eighth coefficient row, both endian orders, RIFF/RIFX/RF64/BW64, flat/wavl,
held frames and reverse seeks. All 93 tests pass on x64 debug/release and i686
debug with Clippy. The repeated 216-fixture campaign passes 3024 three-API checks;
all 1869 preceding fuzz inputs, two million checked mutations and 100000 fresh
ASan executions pass. PERFORMANCE.md records source-pinned before/after results:
Microsoft flat reads improve 5-9%, while x64 reader state decreases eight bytes.
This is an internal optimization; no new compressed-library speed claim is made.


## Buffered seekable sources

AdpcmReader preserves read-ahead supplied by a BufReader: successful encoded
reads establish the next known byte position, so adjacent blocks do not seek
again. A successful explicit seek positions the next uncached block directly.
Cached PCM reads do not advance the physical cursor. Metadata success restores
that cursor; metadata errors and physical read/seek failures require a
successful explicit audio seek before decoding resumes. Codec errors retain the
same logical poisoning contract. Nonzero source bases and wavl child boundaries
retain their existing meaning. Generic container byte-span checks remain active
on every read. No allocation is added, and block-buffer size remains independent
of file length. PERFORMANCE.md records the physical-file counter and timing
results, the eight-byte inline-state tradeoff and the new 94-test validation.

## Bounded 32-bit Microsoft arithmetic

On 32-bit targets the Microsoft decoder uses i32 only while coefficient and
delta bounds prove all intermediate arithmetic fits. Extreme custom coefficient
pairs and growing deltas use the original checked i64 loop. Signed prediction
division, overflow errors and partially written output retain their contracts.
64-bit targets retain wide arithmetic. [MS_ARITHMETIC.md](MS_ARITHMETIC.md)
contains the proof, independent i128 oracle and source-pinned measurements.
