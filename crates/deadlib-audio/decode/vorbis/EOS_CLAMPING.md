# EOS clamping and declared versus emitted counts

Follow-up: [reference seeking](REFERENCE_SEEKING.md) now matches the recorded
172 seek differences through an explicit reference mode. The inventory below
continues to guard the default retained sequential PCM timeline.

The target remains exact behavior, counts and positions with the original PCM
tolerances. This milestone closes reproduced sequential EOS clipping/count
differences. It also retains a failing-parity inventory for libvorbisfile seek
positioning; complete parity has not been established.

`OggOptions::eos_behavior` and `SeekOptions::eos_behavior` select
`EosBehavior::Vorbis` (default) or `EosBehavior::Libvorbis`. The default requires
the final granule to fit the final overlap. The explicit reference policy follows
the pinned reference's bounded last-packet padding. It can discard at most the
currently available overlap, cannot retract PCM already emitted, and adds no
samples when the declared endpoint exceeds available PCM.

An empty EOS page supplies no synthesis block to clip. A discarded final packet
also supplies no block. A discarded intermediate packet breaks the synthesis
sequence counter, while preserving the existing overlap samples. The next
accepted block restarts the reference granule/sample clock, which affects EOS
clipping. Packet header block-size queries used for declared initial offsets are
separate from synthesis prefix acceptance: libvorbisfile does not require window
flags for that query.

The reference converts positive excess samples to C `long` before clamping.
The explicit policy preserves that host ABI behavior. Direct counter injection
after real overlap priming tests 2^31 and 2^32 boundaries on Windows, without
generating multi-hour audio. A further 32 states cover signed 64-bit counter
wrap near the largest granule; 14 of those exposed an incorrect count before
the reference clocks were changed to explicit wrapping arithmetic. Public
native sample positions retain checked arithmetic. The independent C callback
remains unmodified.
An actual container witness also reproduces the wrap: the captured implementation
emits 586 frames, while corrected wrapping and libvorbis emit 1,600. Both compiled
native prefixes satisfy the original PCM bounds; the discrepancy is an exact
count error.
The portable offline counter golden retains values independent of `long` width;
the external boundary matrix checks the host ABI explicitly.

`StreamEnd` now distinguishes:

| Field | Meaning |
| --- | --- |
| `frames` | Declared duration, final granule minus declared initial offset. |
| `emitted_frames` | Actual frames from a complete sequential decode. |
| `start_granule` | Declared initial offset. |
| `emitted_start_granule` | Initial timestamp used for returned native PCM. |
| `end_granule` | Original encoded EOS granule. |

Strict and indexed endpoint-repair policies retain equal declared/emitted frame
counts. The reference policy permits a difference. The existing indexed
`trim_to_final_granule` policy repairs the entire retained interval; combining it
with bounded reference EOS returns an explicit conflict error.

For the stereo witness, raw overlap ends at 4,672 and the last overlap starts at
3,648. With one audio packet per page, declared EOS 1 emits 3,648 frames in
libvorbisfile. The previous native default emitted those 3,648 provisional frames
and then rejected the stream. Explicit reference EOS now succeeds and reports
declared 1/emitted 3,648. The compiled before/after witness preserves identical
native PCM, and both prefixes meet the original numerical bounds against C.

The real Mecha-Tribe Assault file declares 4,630,500 frames and emits 4,762,304.
Reference EOS matches both values and all emitted PCM; endpoint repair remains a
separate policy. The pinned FFmpeg `1sec.ogg` declares/emits 44,608 despite encoded
EOS 44,619. Its declared initial offset is 11; the sequential PCM starts at zero.

Validation covers 1,260 generated cases across mono, stereo and two eight-channel
fixtures; zero/one/two/all audio packets; one/two/all packets per page; accepted,
empty, non-audio, nil-page and sequence-gap tails; and final positions around the
last overlap and beyond available output. Direct synthesis checks 3,330 accepted
and 720 discarded packet decisions, 7,645,056 finite PCM values, plus 64 injected
counter states. Sequential libvorbisfile decoding independently checks all 1,260
streams. Native pull, one-byte/17-byte/whole-input push, and reset replays compare
complete PCM bits and metadata. Offline goldens retain 45 timed sequences,
30 portable counter states, 25 full stream/metadata cases and repeated seeks.

The finite peak/RMS bounds remain 3e-6 / 5e-7 times max(1, reference peak).
Counts, packet decisions, block sizes and clock states use exact comparisons.
No performance measurement or application integration is included.

## Remaining seek parity differences

The EOS index addresses the retained sequential PCM and accepts seeks through
the declared duration. Both native pre-roll strategies return identical suffix
bits. The full audit performs 4,476 C requests and 8,952 native seeks, checking
all declared durations and recording 172 remaining C/native differences. Every
C seek returns success; 144 have a position difference, 144 have a returned-length
difference, with overlap between these groups. The other requests also compare
PCM within the original bounds. The retained inventory is
[`tests/fixtures/eos-seek-differences-v1.tsv`](tests/fixtures/eos-seek-differences-v1.tsv):
case, requested frame, C status, C position, C frames, native position, native
frames. Its exact contents are asserted, so improvements require an explicit
inventory update and fresh C evidence.

For example, a discarded packet immediately before the second accepted audio
packet, with EOS 1, emits 576 frames sequentially. C seeking to frame 1 reports
position 1 and EOF, while the native retained-PCM seek returns the 575-frame
suffix. These are positioning decisions, not floating-point error. They remain
open, alongside the previously reproduced positive broadcast-offset seek cases.
Selecting `EosBehavior::Libvorbis` does not claim those vorbisfile quirks.

Reference EOS still requires known, nonnegative signed-64-bit final granules and
the existing header, page sequence, CRC and initial-offset framing checks. Other
libvorbisfile recovery and negative/unknown granule behavior remain outside this
bounded policy and part of the wider parity work.

The new test-only `oggstream` mode uses unseekable vorbisfile callbacks, while
`oggseekstates` records real seek status/tell/read output. Neither uses the older
helper's zero-total shortcut, which hid nonempty PCM in declared-zero files.
Old modes remain available unchanged for earlier retained regressions.

The final source passes 200 release tests, 137 isolated offline tests, default
and all-feature Clippy, and fuzz harness tests/lints. Three AddressSanitizer
targets run for 300 seconds each, completing 320,398 executions with no failure artifacts
or slow inputs. Fresh C recaptures preserve all 18,737 earlier procedural inputs
and 40 older PCM responses. Full logs, both compiled count witnesses, source and
corpus hashes, seed selection and the open seek inventory are retained in
[`validation/2026-10-27-eos-clamping-checks.json`](validation/2026-10-27-eos-clamping-checks.json).
