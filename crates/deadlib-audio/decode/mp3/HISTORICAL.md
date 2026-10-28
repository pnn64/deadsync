# Historical music, strict errors and recovery

`tests/data/historical.json` pins five files from FFmpeg's public sample archive
by URL, encoded length and SHA-256. Media remains in ignored scratch storage.
The archive contains valid music, damaged Layer III input and a mislabeled
Layer II file; these roles are recorded separately rather than counting every
download as passing MP3 coverage. No unknown encoder identity is inferred.

## Complete music files

`Silent_Light.mp3` and `01 - Charity Case.mp3` pass full raw PCM comparisons
against libmad, mpg123 and Symphonia 0.6.1. They additionally pass file-level
gapless PCM, exact sample counts and independently cropped libmad output:

| File | Retained samples/channel | Native sample seeks | Maximum reference PCM error |
| --- | ---: | ---: | ---: |
| Silent_Light | 11,619,072 | 30,341 | 2.295e-6 |
| Charity Case | 8,486,016 | 22,185 | 2.385e-6 |

Every native seek is bit-identical to native sequential PCM. The audit includes
every frame boundary and neighboring samples, fixed DSP boundaries, random
positions, repeated/backward seeks and EOF. Sixty independent mpg123 seeks
check exact requested coordinates/counts and compare each returned sample to
both reference and native sequential PCM. All comparisons retain the original
`2e-6` RMS / `2e-5` maximum limits with no gain or alignment fit.

The historical harness compares file PCM incrementally so the different output
chunk sizes of the reference decoders cannot change counts or truncate checks.
The full valid-file raw gates compare 40,216,320 interleaved sample values per
decoder, including raw metadata/delay samples where present.

## Damaged and unsupported inputs

* `ascii.mp3` has an invalid `big_values` field at byte zero. Native and libmad
  both reject there and emit no PCM.
* `Ed_Rush_-_Sabotage.mp3` has invalid SCFSI reuse after a short block. Native
  and libmad both first reject byte 3,678,041. Their complete 8,800-frame prefix
  agrees within the unchanged PCM limits. Other references continue decoding;
  that does not turn the illegal field into a conforming bitstream.
* `Die Jodelschule.mp3` is MPEG Layer II, independently identified by FFprobe
  and its first header's layer bits. It is excluded from this Layer III codec's
  positive corpus, rather than mislabeled as an unsupported MP3 encoder.

The Ed Rush file also exercises explicit `Recovery::Conceal`. An independent
header walk counts all 16,835 physical frames / 19,393,920 samples per channel.
The native reader reports three concealed frames, preserves the exact timeline,
and emits bit-identical PCM with 1-, 17- and 4,096-byte reads and indexed decoding.
All 50,588 indexed seek checks reproduce that same recovered sequential output.
No cross-decoder PCM equivalence is claimed for concealed damaged frames.

## Reproduction and evidence

Fetch with `tests/fetch_historical.py --output NEW_DIR` (or `--music-only`).
Run `reference_pcm.py --case Silent` and `--case 01` for the full raw gates.
`reference_historical.py` adds exact file/count/seek checks through the native
`file`, Symphonia `file_reference`, public mpg123 `file.c` and libmad `decode.c`
wrappers. It accepts `--wsl-distro Debian` from Windows with local NumPy.

For recovery, first run `file INPUT OUTPUT raw audit recover discard` on the
Ed Rush file. Supply that output to `reference_rejections.py --indexed-recovery`
to check strict prefixes and the three complete streaming read schedules.
The original input is unchanged; rejection and concealment are explicit policies.

Reports, source/binary/input/PCM hashes and command traces are archived in
`tests/validation/2026-10-28-historical`. This is functional decoder evidence;
it does not certify the completeness or original ISO distribution identity of
the separately mirrored conformance vectors.
