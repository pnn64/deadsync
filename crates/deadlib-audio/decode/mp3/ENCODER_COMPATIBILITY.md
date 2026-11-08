# Independent encoder compatibility

The 2026-10-28 gate adds 30 files generated from the same deterministic,
non-frame-aligned three-second WAV inputs as the seek corpus:

| Encoder | Tested configurations |
| --- | --- |
| FFmpeg libmp3lame | All nine MPEG-1/2/2.5 rates, mono/stereo: 18 files |
| Shine through FFmpeg libshine | 32/44.1/48 kHz, mono/stereo: six files |
| Windows Media Foundation through FFmpeg mp3_mf | 32/44.1/48 kHz, mono/stereo: six elementary streams |

All 30 files pass exact raw frame/rate/channel/sample counts and unchanged
unclipped PCM limits (`2e-6` RMS, `2e-5` maximum) against libmad, mpg123 and
Symphonia 0.6.1. All also pass exact file sample counts, retained PCM and native
bit-identical seeks, plus mpg123 sample-coordinate/PCM seek comparisons.
`tests/reference_file.py --encoder-corpus` independently walks every CBR physical
boundary and interprets trim fields to establish the expected file count.
libmp3lame's retained count additionally must equal the source WAV count.

This gate found and corrected a file-mapping bug: real FFmpeg/Shine tags contain
zero encoder delay and zero padding. Applying the 529-sample decoder offset and
then automatically draining DSP history emitted 529 samples beyond both
reference file decoders. Default tagged gapless output now stops at physical
EOF. Caller-supplied `SampleMode::Trim` retains explicit DSP-tail reconstruction.
Regression tests exercise both behaviors, all MPEG versions, stereo and seeks.
Shine's retained length need not equal the source WAV length: this is a decoder
parity gate, not a claim that Shine preserves arbitrary source lengths.

## Generation limitations are explicit

FFmpeg's Media Foundation wrapper emits packets containing multiple physical
MP3 frames. The FFmpeg MP3 muxer counts those packets when writing Xing, producing
a false frame count (34 advertised versus 117 physical audio frames in the
44.1-kHz stereo probe). Reference file decoders disagree on that malformed count.
The generated comparison therefore uses `-write_xing 0` for Media Foundation;
native tagged mode rejects the inconsistent count rather than inventing a
duration. This does not hide a raw PCM decoder disagreement.

The twelve Media Foundation attempts below 32 kHz exit successfully but produce
no independently decodable audio on this machine. They are recorded as unusable
generation attempts and excluded from the 30 passing files. The generator checks
FFprobe's decoded-frame count, rate and channels, not just encoder exit status.
FFmpeg's libshine wrapper advertises only the three MPEG-1 rates; no lower-rate
Shine claim is made.

Run `tests/generate_encoder_corpus.py --sources SEEK_CORPUS --output NEW_DIR`
where FFmpeg/FFprobe provide the stated encoders. Run `reference_pcm.py` and
`reference_file.py --encoder-corpus` with the validation CLIs and C wrappers.
The exact FFmpeg version, commands, input/source hashes, diagnostics, binary
hashes and results are retained in `tests/validation/2026-10-28-encoders`.

This adds two independent encoder implementations to the prior LAME/legacy
vector evidence. Historical encoders, broader real music and sanitizer/fuzz
campaigns remain part of the decoder completion audit.
