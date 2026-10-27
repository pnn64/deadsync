# Synthesis component diagnostic

Real-packet profiling identifies residue as the next song-throughput target:
**55.2% / 42.2%** of recorded component time, versus **23.1% / 30.7%** for inverse
MDCT plus finite checks. These are diagnostic shares, not uninstrumented CPU
samples or a cross-library ranking. No production codec source is changed.

## Reproduce and validate the copy

```powershell
python crates/deadlib-audio/decode/scripts/profile_synthesis.py --repeats 7
python crates/deadlib-audio/decode/scripts/profile_synthesis.py --allow-zero-span-granules --repeats 7 target/deadlib-audio-reference/corpus/floor0/vorbis_floor_type_0.ogg
```

The Python runner creates a fresh ignored standalone workspace and inserts
eight timer/counter groups into a copy of the current `decoder.rs`. Every edit
requires a unique source anchor; a decoder refactor must update the probe if
an anchor changes. The small mode-header helper is extracted unchanged from
`setup.rs`. Floor/residue modules use current source; codebooks, bitpacking,
transforms and public setup types come from the actual production crate.
There is no production feature, hook or API for profiling.

Before timing, the driver compares every raw PCM value's bits, frame/channel
count, raw position, block size, truncation flag and discarded-packet result
between the production decoder and instrumented copy. Six inputs compare
**32,343,680 channel samples** exactly. All their audio packets have complete
mode/floor/residue payloads; none is discarded or nominally truncated.

Seven trials shuffle production/profiled order with a recorded per-file seed.
Both decoders reuse warmed contexts and resident packet vectors, reset between
whole traversals, and discard borrowed output. Reset/loop/output consumption are
included in whole-traversal timers. Setup, input loading, container IO, trimming
and seek work are excluded. Calibration targets 150 ms, capped at 10,000
iterations; long files use one traversal per trial. Cargo release settings
merge the repository release and bench profiles: opt-level 3, LTO, bench debug
information, the normal Windows CRT flags and no custom target CPU.

## Recorded Windows results, 2026-10-27

Same shared, unlocked Xeon/Windows machine as the other performance reports.
Each component column is its median nanoseconds divided by the sum of component
medians. The whole traversal is measured separately, so those shares exclude
timer/counter and harness overhead outside the recorded intervals.

| Input | Production raw ms | Profiled raw ms | Clear + floor | Residue | IMDCT + finite |
| --- | ---: | ---: | ---: | ---: | ---: |
| Mono fixture | 0.020 | 0.024 | 14.3% | 24.9% | 42.6% |
| Stereo fixture | 0.219 | 0.227 | 13.5% | 22.2% | 45.0% |
| Eight-channel fixture | 0.655 | 0.683 | 15.4% | 18.4% | 49.6% |
| `in_two` | 372.903 | 380.290 | 10.5% | 55.2% | 23.1% |
| `Goin' Under` | 345.397 | 361.270 | 12.6% | 42.2% | 30.7% |
| Historical floor-zero | 798.973 | 817.524 | 67.4% | 13.7% | 13.8% |

Instrumentation adds about 2.0% / 4.6% to the song medians and about 20.7% for the
tiny mono file. Added `Instant` calls, counter fields, module boundaries and
compiler layout can all affect the result. Small header/coupling/window shares
are especially sensitive to timer overhead. The raw times also reflect current
machine activity and exclude the container work of full-file pipeline reports.

Both songs use residue type two; floor-zero uses type zero and has a different
dominant cost. Type two's logical channel interleave is specified in
[Vorbis I section 8.5](https://xiph.org/vorbis/doc/Vorbis_I_spec.html).
The next candidate is the type-two vector scatter, whose current inner loop
divides and takes a remainder by the dynamic bundle size for every coefficient.
Any optimization must preserve vector order, nonzero bundles and complete
vectors on end-of-packet, then improve an uninstrumented paired pipeline run.
This profile does not prove that division alone accounts for the residue cost.

The historical file has one nonconforming zero granule on an unfinished page.
The separate run explicitly enables the existing named container exception and
records exactly one event. A direct strict-mode check rejects the same file.
No repair or compatibility policy is silently enabled by the default command.

## Evidence and limits

[Main manifest](validation/2026-10-27-synthesis-main-manifest.json),
[summary](validation/2026-10-27-synthesis-main-summary.tsv),
[70 trials](validation/2026-10-27-synthesis-main-trials.tsv),
[floor-zero manifest](validation/2026-10-27-synthesis-floor0-manifest.json),
[summary](validation/2026-10-27-synthesis-floor0-summary.tsv),
[14 trials](validation/2026-10-27-synthesis-floor0-trials.tsv), and
[checks](validation/2026-10-27-synthesis-checks.json) retain hashes, counts,
generated-source provenance, worker logs, compiler profiles and strict rejection.
Generated source, executables and external media stay under ignored `target/`.

Python syntax, driver formatting and the generated workspace's release Clippy
pass. The instrumented decoder reconstructs exactly from the checked current
source. Production Ogg/Vorbis and the prior validation sources match `c7d23c320`;
this tooling milestone does not claim a new production sanitizer campaign.
Broader conformance, synthesis performance, index-free codec seeking and encoder
quality/rate control remain open; the crates remain independent of DeadSync.
