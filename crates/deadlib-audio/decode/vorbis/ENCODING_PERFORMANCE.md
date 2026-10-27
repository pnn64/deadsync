# Native encoding baseline and real-audio validation

This baseline measures the initial native fixed-block encoder from `0e5ecb23d`
against pinned libvorbis 1.3.7. It adds test/benchmark tooling and changes no
production codec code. The native encoder has a substantial compression gap;
its low CPU time does not establish competitive perceptual quality or bitrate.

## Inputs and independent gates

Six sources cover mono at 8 kHz, stereo at 48 kHz, eight-channel silence at
44.1 kHz, the repository's mono `boom.ogg` effect, and two stereo 44.1 kHz songs.
The songs are `assets/music/in_two (loop).ogg` and the pinned ITGmania
`goin-under.ogg`. Each song supplies exactly 220,500 frames starting at frame
661,500: five seconds beginning at 15 seconds. Short files use their full retained
length. These are decoded, previously compressed media excerpts, not original
studio PCM. Input is never clipped, normalized, aligned, resampled or gain fitted.

The native indexed reader prepares one little-endian interleaved f32 input per
source. Before encoding, independent libvorbisfile `ov_pcm_seek` extraction must
match that exact interval, rate and channel count. The existing floor-one PCM
gates apply. This prevents an extraction/seek discrepancy from being attributed
to encoder quality. The input PCM, original media and reference extraction
hashes are retained in the [manifest](validation/2026-10-27-encoding-baseline.json).

Four native precision stages (1..4) and three Xiph VBR quality settings
(0, 0.4, 0.9) produce **42 encoded streams**. Every stream is decoded independently
by native, libvorbisfile and Symphonia. All agree on exact retained lengths,
rates and channel layouts across **6,401,696 channel samples per decoder**.
Native/Symphonia comparison follows declared speaker identities. Each C decode
starts with `ov_raw_seek(0)`, preserving the already documented spanning-packet
PCM-seek-zero discrepancy as a separate test rather than shifting these outputs.

Decoder PCM must meet peak `3e-6 * max(1, reference_peak)` and RMS
`5e-7 * max(1, reference_peak)` without adjustment. Observed maximum native/C
peak error is 6.557e-7, and the largest case RMS is 1.019e-7. Native/Symphonia
maxima are 5.961e-7 / 6.601e-8. Source reconstruction error is measured separately
against the common input. It includes the additional lossy encoding generation.

The [quality/size report](validation/2026-10-27-encoding-baseline-quality.tsv)
records all configurations, encoded and decoded-reference hashes, page/packet
counts, header/audio packet bytes, total/audio bitrate, source SNR/peak/RMS and
both independent decoder differences. Silence has no finite source SNR (an
empty TSV field); it must still reconstruct exact zero amplitude.

## Timing method

Windows x86-64, rustc 1.99.1, the repository's release/bench profile and MSVC
Release reference build. Reference sources are clean and pinned to:

* libogg: `be05b13e98b048f0b5a0f5fa8ce514d56db5f822` (1.3.6).
* libvorbis: `0657aee69dec8508a0011f47f3b69d7538e9d262` (1.3.7).

Both workers load the exact same resident interleaved PCM before timing. Each
iteration constructs a fresh encoder, writes identification/comments/setup and
Ogg pages, accepts 4,096-frame input chunks, finishes the stream and releases
codec/mux state. Output goes to an observable discard sink, excluding file IO
and output retention. Native uses `PacketEncoder` plus its normal `PacketWriter`
path; Xiph uses `vorbis_encode_init_vbr`, analysis/bitrate packet APIs and libogg.
Default mux page policies and codec configurations differ and are recorded.
Input deinterleaving into libvorbis's analysis buffer is inside the timed API
path, as is native's copy into its analysis ring.

Each worker performs an untimed warm encoding before its measured iterations.
This is warm-process latency, not executable launch or cold filesystem latency.
Per-configuration pilots select enough iterations for approximately 0.15 seconds
of full encoding. Seven trials shuffle the seven configurations using seed
4187, yielding **294 timing rows**. Reported values are per-iteration medians.
Min/max and median absolute deviation are retained in the
[summary](validation/2026-10-27-encoding-baseline-summary.tsv) and
[raw trials](validation/2026-10-27-encoding-baseline-trials.tsv).
The largest relative full-time MAD across these cases is 6.43%; this is a desktop
measurement, not a claim of a controlled or exhaustive hardware study.

Three timers share the same run:

* **Open:** through analysis initialization, metadata/setup generation and header
  page flushing, before PCM input.
* **First packet:** through encoding and mux submission of the first audio
  packet with positive granule. It includes open time and configured batching.
  The packet need not yet have been flushed as a page. Supplied input frames and
  packet granule are reported separately. Supplied frames identify the caller's
  chunk boundary, not the minimum intrinsic lookahead or playback latency.
* **Full:** all input, final granule/EOS, page flushing and codec/mux cleanup.

Timing uses uninstrumented workers. It does not measure encoding heap/process
memory; the earlier [decoder resource report](RESOURCES.md) has separate scope.
The raw encoder's existing zero-allocation push/reset tests also have separate
scope from constructor, Ogg mux or arbitrary writer allocations.

## Song results

Total bitrate includes headers, Ogg framing and final padding. SNR is unweighted
sample error; it is not a perceptual listening score. Native stages and Xiph
quality numbers are distinct controls. These rows are configuration comparisons,
with no assertion that equal numbers mean equal bitrate or quality.

| Five-second excerpt | Encoder setting | Total kb/s | Source SNR dB | Full time ms |
| --- | --- | ---: | ---: | ---: |
| in_two | Native stage 1 | 75.0 | 13.30 | 10.96 |
| in_two | Native stage 2 | 347.0 | 31.75 | 12.70 |
| in_two | Native stage 3 | 729.4 | 54.04 | 16.31 |
| in_two | Native stage 4 | 1168.8 | 77.32 | 19.12 |
| in_two | Xiph q0 | 69.4 | 13.83 | 118.72 |
| in_two | Xiph q0.4 | 132.8 | 22.61 | 94.34 |
| in_two | Xiph q0.9 | 285.3 | 34.93 | 99.37 |
| Goin' Under | Native stage 1 | 56.3 | 13.13 | 10.14 |
| Goin' Under | Native stage 2 | 412.7 | 29.32 | 13.31 |
| Goin' Under | Native stage 3 | 908.1 | 51.61 | 17.34 |
| Goin' Under | Native stage 4 | 1257.9 | 75.48 | 19.66 |
| Goin' Under | Xiph q0 | 75.8 | 17.10 | 102.89 |
| Goin' Under | Xiph q0.4 | 135.6 | 23.03 | 82.68 |
| Goin' Under | Xiph q0.9 | 323.2 | 35.97 | 89.32 |

For these two excerpts Xiph q0.9 has both lower bitrate and higher unweighted
SNR than native stage 2. Native high-precision stages spend much more data to
reduce sample error; the fourth stage approaches the 1,411.2 kb/s rate of
uncompressed 16-bit stereo at 44.1 kHz. Low CPU cost reflects a much simpler coding policy and
different rate-distortion points. It must not be presented as a matched-quality
encoder speedup. Perceptual tests, quality/rate control and wider real-music
evaluation remain necessary.

Warm startup examples:

| Excerpt / setting | Open ms | First-packet ms | Supplied frames | First positive granule |
| --- | ---: | ---: | ---: | ---: |
| in_two / Native stage 3 | 0.039 | 0.124 | 4096 | 512 |
| in_two / Xiph q0.4 | 4.264 | 4.876 | 4096 | 128 |
| Goin' Under / Native stage 3 | 0.041 | 0.124 | 4096 | 512 |
| Goin' Under / Xiph q0.4 | 4.377 | 4.895 | 4096 | 128 |

## Measured compression opportunity

`profile_encoding.py` parses the actual native packets after the independent
round-trip gates. It verifies the initial encoder's complete flat-floor setup,
fixed eight-bit VQ trees and residue grammar, then consumes every audio packet
through zero padding. Unsupported configurations are rejected. Profiling does
not approximate the input MDCT or recompute quantization.

The leading radix-16 digit of a small scalar is usually seven or eight. The four
corresponding VQ pairs occupy **89.44% / 95.20%** of first-stage symbols in the
two-stage song cases. Their measured first-stage alphabet entropies are
**2.689 / 2.237 bits per pair**, while the current book always writes eight.
The [symbol report](validation/2026-10-27-encoding-baseline-symbols.tsv) covers
all 24 native outputs; the [analysis manifest](validation/2026-10-27-encoding-baseline-symbols.json)
retains the two song histograms and source/report hashes.

One candidate complete prefix-length multiset assigns three bits to these four
central pairs, eight to four corner pairs and nine to the other 248 entries:
`4/2^3 + 4/2^8 + 248/2^9 = 1`. On the measured symbols alone it would save
57,100 / 73,648 payload bytes for the two-stage songs without changing VQ values
or reconstructed PCM.
At baseline capture this was a projection. The subsequent
[compact-book update](ENCODING_ENTROPY.md) implements and measures it with
bit-identical PCM and the documented header costs. The projection excludes
about 159 extra setup bytes, packet rounding, Ogg pagination and
encoder/decoder CPU costs. A production change must measure those costs and
retain exact decoded PCM, numerical parity, allocation and sanitizer gates.

Adaptive floors, transient blocks, channel coupling, more effective entropy/VQ
coding, masking and explicit rate/quality control remain separate encoder work.
Sparse codec seeking and broader decoder performance/conformance also remain
part of the original goal.

## Reproduce and evidence

With the pinned references already fetched and corpus hashes verified:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_encoding.py --run-root target/deadlib-audio-reference/encoding-new --seconds 5 --repeats 7 --min-seconds 0.15
python crates/deadlib-audio/decode/scripts/profile_encoding.py target/deadlib-audio-reference/encoding-new
```

`--run-root` must be new. Optional positional media paths replace the default
sources. If the pinned external song is absent it is omitted; the manifest
records actual inputs. The script builds the native Cargo `encoding` benchmark
and a separate `deadlib_vorbis_encoding` C++ executable against clean references.
The normal production dependency graph remains unchanged.

The manifest records source/input/output/worker hashes, compiler versions,
release flags, registry artifacts, profiles, exact selected intervals and
policy definitions. Sources, inputs and binaries are rechecked after timing.
Generated PCM/media remain in ignored scratch storage; no external song is
redistributed. The [validation checks](validation/2026-10-27-encoding-baseline-checks.json)
record formatting/Clippy/Python checks and the extracted offline test/benchmark
build. No production decoder behavior, playback registration or audio backend
is changed by this milestone.
