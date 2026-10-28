# WAV decoding completion audit, 2026-10-28

The current goal is complete: supported WAV decoding passes the test suite and
the applicable independent and external-decoder accuracy checks. The user
explicitly removed performance from the completion criteria. No further
performance optimization is required or scheduled by this goal.

The implementation is package `deadlib-audio-decode-wav`, with no production
dependencies. Only workspace registration is present; the existing decoder,
playback pipeline and audio backends are unchanged by this work.

## Verified decoding scope

- RIFF/RIFX and RF64/BW64 container mechanics, chunk bounds, odd padding,
  extensible formats, masks, valid-bit depths and 64-bit positions/counts.
- PCM8/16/24/32 and IEEE32/64, preserving integer values and original float
  words, including NaN payloads and signed zero. Conversion remains explicit.
- Microsoft ADPCM mono/stereo, including custom coefficient rows; 3/4-bit WAV
  IMA ADPCM and the documented extensible IMA mapping.
- Borrowed, seekable and forward decoding, flat and wavl/slnt audio, held
  samples, exact frame ranges/seeks, fact trimming and nonzero input bases.
- Metadata/unknown-chunk preservation, bounded parsing, malformed/truncated
  input rejection and documented I/O failure/recovery behavior.

## Final evidence

The final production source is committed in `718de57ab`. The machine audit
`validation/decoding-final-audit.json` verifies hashes for all 23 production
source files against the following reports. Its reproducible driver is retained
as `decoding-final-audit.py.txt`.

| Check | Result |
| --- | --- |
| Complete crate tests | 95/95 on x64 debug, x64 release and i686 debug |
| Clippy, all targets, warnings denied | Pass on x64 and i686 |
| Fresh native fixture/API/encoder checks | 460/460 |
| Fresh core PCM/float cases against all four external decoders | 24/24 |
| Fresh ADPCM arithmetic differential cases | 40/40 |
| Fresh encoded-file decoding/reference cases | 144/144 |
| Current native flat/wavl replay | 708/708; 4484 descriptor/read/range checks |
| Current compressed wavl checks | 216/216; 3024 borrowed/file/forward/range checks |
| Exact retained encoder outputs | 1912/1912 |
| Checked mutations | One million each on x64 and i686 |
| Prior fuzz corpus replay | 1846/1846 |
| x64 AddressSanitizer | 100000 executions; no failure artifact |

The fresh direct comparisons are `decoding-final-pcm.json`,
`decoding-final-adpcm.json` and `decoding-final-encoded.json`. Their parent driver
checks unchanged production inputs throughout. The native fixture ZIP has
verified per-file hashes. Current full tests, compressed checks and fuzz reports
are retained under `ms-narrow-*`; their names record the preceding milestone,
not a performance condition for this audit. The independent i128 Microsoft
oracle tests 2304 blocks, including both byte orders, extreme coefficients,
growing deltas, overflow errors and exact partial-output/sentinel behavior.
It exercises the bounded narrow branch on i686; x64 ASan exercises wide arithmetic.

## Meaning of reference accuracy

The pinned peers are dr_wav, libsndfile 1.2.2, Symphonia 0.6.1 and FFmpeg 7.1.
All four agree on the 24 declared native core cases. Wider native fixtures also
use independent bit truth; unsupported containers/layouts and external errors
are retained, as detailed in [PCM.md](PCM.md).

ADPCM peers disagree on rounding and padded output length. Microsoft encoded-file
decoding matches FFmpeg and Symphonia in all 72 cases; 4-bit IMA matches dr_wav
and libsndfile in all 72 cases, after explicitly limiting output to fact length.
These arithmetic choices are checked against the primary definitions and
independent vectors. We do not claim simultaneous bit parity with contradictory
rounding implementations. [ADPCM.md](ADPCM.md), [IMA_EXTENSIBLE.md](IMA_EXTENSIBLE.md)
and [WAVL.md](WAVL.md) retain every rejection, length difference and first mismatch.
The earlier 122-case 3-bit and 416-case extensible IMA banks also pass their
declared independent/arithmetic/layout gates; external combined-rounding checks
are identified explicitly rather than presented as exact spec-output matches.

## Published boundaries

Other encoded formats remain accessible as opaque data. Extensible Microsoft
ADPCM coefficient placement is not established and is rejected explicitly.
Forward decoding obeys the documented format/duration ordering and requires
finalization for complete container validation. Audio is decoded in its natural
data/wavl order; playlist execution and semantic ADM/XML profile interpretation
are not implemented. BW64 compatibility extensions are not broadcast-profile
certification. Windows x64/i686 are the tested hosts; RIFX byte order is tested
on those little-endian hosts.

These boundaries remain explicit API scope. The audit found no unresolved
decoding correctness defect. Historical performance observations in PLAN.md and
PERFORMANCE.md are outside this completed goal.
