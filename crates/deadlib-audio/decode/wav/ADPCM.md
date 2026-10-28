# WAV ADPCM block codecs and differential evidence

`Adpcm::from_format` validates block geometry independently of the WAV container.
`decode_block` fills caller-owned interleaved i16 storage and returns frames.
Microsoft ADPCM supports mono/stereo, all seven standard predictors, additional
coefficient pairs, high-nibble-first interleaving and block padding. IMA supports
4-bit WAV word groups with arbitrary channel counts within block-field limits.
Each block restarts from its header. Neither block decoder allocates.

`AdpcmData::new(&wave)` borrows encoded data and uses fact/ds64 sample counts to
trim padding. `read_frames(start, output, scratch)` gives sample-accurate random
access with caller-owned block scratch, returning interleaved samples. The
container retains unrecognized encoded formats without invoking either codec.

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
addressing and decodes the block prefix when necessary. File opening skips audio;
fact or ds64 counts trim final padding. Metadata access keeps the logical audio
position intact. Errors poison reads until a successful seek. This reader does
not allocate PCM for the entire file.

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
throughput. Real files currently seek explicitly when loading each uncached block;
avoiding redundant sequential seeks is a remaining performance target.

Remaining: forward-only compressed audio convenience, 3-bit DVI, extensible
compressed mappings, RIFX external validation, exhaustive Symphonia decode
comparison and longer fuzz campaigns. Negative Microsoft header deltas are
rejected. Unbounded delta growth errors safely rather than overflowing. The
initial implementation prioritizes correctness; no ADPCM throughput claim yet.
