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

Remaining: ADPCM encoding, file-backed compressed streaming/seek convenience,
3-bit DVI, extensible compressed mappings, RIFX external validation, Symphonia
comparison and longer fuzz campaigns. Negative Microsoft header deltas are
rejected. Unbounded delta growth errors safely rather than overflowing. The
initial implementation prioritizes correctness; no ADPCM throughput claim yet.
