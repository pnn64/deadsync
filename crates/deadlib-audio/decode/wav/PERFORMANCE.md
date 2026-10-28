# Native PCM baseline and memory evidence

This is the first narrow baseline, not a completion claim for the performance
objective. Resident stereo PCM16, 1,048,576 frames (4 MiB audio), Intel Xeon
E5-2696 v4, Windows 11 x86_64, rustc 1.99.1. Workspace bench profile and flags are
fingerprinted in validation/pcm16-baseline-batched.json. No affinity/frequency
control; preserve the raw three trials, including variance. Disk I/O is excluded.

Reproduce with a new output stem:

```sh
python crates/deadlib-audio/decode/wav/benches/run_pcm.py --output target/wav/my-baseline
```

The driver checks build-input hashes before/after compilation, preserves compiler
output, binary hash, environment and raw TSV. The benchmark checks decoded/encoded
sample equality and pointer identity outside timing. Hound 3.5.1 is a dev-only
dependency. Both sides read/write caller-owned resident storage. Hound encoding
includes its specialized unchecked i16 writer with exact checked batch lengths,
both a full-file batch and 4096-sample batches that reuse its scratch allocation.

Median of the three trials in validation/pcm16-baseline-batched.tsv:

| Operation | This crate | Hound | Measured scope |
| --- | ---: | ---: | --- |
| Open | 75.9 ns | 84.2 ns | Borrowed bytes versus Cursor reader; tiny header |
| Direct view, including open | 89.7 ns | n/a | Borrowed slice, no sample traversal |
| Full read into i16 caller buffer | 0.367 ms | 12.877 ms | 4 MiB output, includes open |
| Stream into 4096-sample buffer | 0.184 ms | 13.036 ms | Full audio, includes open |
| Seek and read 256 frames | 0.327 us | 6.006 us | Warm reader, varying positions |
| Native i16 encoding | 0.309 ms | 0.412 ms | Hound bounded batches, includes finalize |
| Full-batch i16 encoding | 0.309 ms | 2.109 ms | Hound's full-file scratch allocation |

The full read has a 4 MiB caller output; streaming reuses an 8 KiB buffer that
fits in cache. Their bandwidth figures represent logical audio bytes, not physical
memory traffic. The direct-view timing is construction cost and is deliberately
not advertised as decoded throughput. Open validates all top-level headers here;
Hound may stop when reaching data. This fixture has no trailing metadata.

Scoped allocation probe (one warmed operation, caller storage excluded):

| Operation | This crate | Hound |
| --- | ---: | ---: |
| Borrowed open/full read/direct view | 0 calls, 0 bytes | Open/full read: 0 calls, 0 bytes |
| Reader open plus streaming | 2 calls, 112 peak requested bytes | 0 calls, 0 bytes |
| Warm seek/read | 0 calls, 0 bytes | 0 calls, 0 bytes |
| Encode, bounded Hound batches | 0 calls, 0 bytes | 1 call, 8192 peak requested bytes |
| Encode, full Hound batch | 0 calls, 0 bytes | 1 call, 4194304 peak requested bytes |

The resident input consumes 4,194,348 bytes. Full output consumes 4,194,304 bytes;
stream output consumes 8192 bytes plus reader state. Direct slices borrow input
storage and add no sample allocation. Requested heap bytes exclude allocator
overhead, stack and process RSS; the probe is not a peak-working-set measurement.

Windows CPU cycles are recorded using QueryThreadCycleTime (user + kernel thread
cycles), separately from wall-clock nanoseconds. They must not be converted to
time using an assumed CPU frequency. Other platforms currently report zero for
that unavailable counter. All raw trials and cycle values are retained.

## Retained native byte-copy specialization

Native-order borrowed I16/I32/F32/F64 caller-buffer reads now copy bytes in bulk.
This also accepts unaligned source bytes; direct typed slices still require
alignment. Opposite-order input retains endian conversion. Float payloads are
copied without arithmetic. No allocation is added.

The same harness measured full PCM16 reads at median 0.322 ms and 0.342 ms in
validation/pcm16-native-copy.tsv and pcm16-native-copy-repeat.tsv, versus 0.367 ms
before specialization (6.6–12.1% lower time in these runs). All candidate trials
were below all trials of the matching baseline for this operation. Small open and
seek costs varied between runs; no improvement is claimed for them. The first
candidate's full-read thread-cycle median fell from 801,460 to 704,768. Larger
corpora, other numeric formats and controlled interleaved runs remain necessary
before extrapolating the gain.

Verification: all 20 integration tests pass on Windows x86_64 (debug/release) and
i686 (debug), including integer extrema, float bit patterns, opposite byte order,
unaligned input and >4 GiB virtual seeking. Clippy with warnings denied passes;
the production library also checks for x86_64-unknown-linux-gnu. A cross-check is
not Linux runtime evidence. `cargo tree --edges normal -p deadlib-audio-decode-wav`
confirms no production dependencies.

Outstanding coverage: PCM8/24/32 and floats, ADPCM, large metadata inventories,
real-file cold/warm startup and streaming, nonseekable input, realistic and large
corpora, Symphonia and other Rust WAV libraries, dr_wav/libsndfile performance,
multiple architectures, peak RSS, fuzz/optimization parity, and broad confidence
intervals. These results do not establish superiority across those unmeasured cases.

## Resident cue/INFO metadata baseline

`benches/run_metadata.py` independently packs three leading-metadata RIFF fixtures
and computes a checksum over all six cue fields (including FourCC bytes) and
every INFO text byte. Both implementations must match that truth. Audio is skipped;
all input is resident and all metadata precedes data. Five warmed trials per
operation, same Windows/Xeon host, no affinity/frequency control. The reference is
the pinned dr_wav revision used by the differential campaigns, built with MSVC
/O2 /MT. Rust uses the fingerprinted workspace bench profile. Raw data, source/
binary hashes and fixture sizes are in `validation/metadata-baseline.{tsv,json}`.

```sh
python crates/deadlib-audio/decode/wav/benches/run_metadata.py --output target/wav/metadata-run
```

Median microseconds per operation:

| Fixture | Cue points / INFO entries | Open + walk, this crate | Open + walk, dr_wav | Retained walk, this crate | Retained walk, dr_wav |
| --- | ---: | ---: | ---: | ---: | ---: |
| Small, 960 metadata bytes | 32 / 4 | 0.379 | 1.297 | 0.205 | 0.112 |
| Medium, 103,064 metadata bytes | 4096 / 64 | 12.763 | 69.959 | 11.846 | 8.937 |
| Large, 529,432 metadata bytes | 16384 / 512 | 156.442 | 334.196 | 161.687 | 75.734 |

Fresh open+walk is 2.1–5.5x faster in these cases. Reusing retained borrowed views
is slower than walking dr_wav's already materialized metadata. That measured cost
remains an optimization target; these results do not imply every metadata query
is faster. Header-only opening is also recorded separately in the TSV and does
not interpret cue/INFO payload fields.

Rust operations request zero heap bytes. dr_wav's fresh structured open requests
one allocation of 1467/109751/582263 bytes respectively. Both retained-walk probes
allocate zero during the operation; dr_wav's retained object still owns the
allocation made before measurement, while the Rust views borrow the input.
The reference uses allocation callbacks with a 16-byte accounting header excluded
from requested bytes; timings include those callbacks. The Rust probe likewise
excludes allocator overhead. Caller input, stack, retained state and process RSS
are outside the scoped allocation counts. No native codec RSS claim is made.

This comparison covers little-endian leading cue/INFO metadata only. Broader
metadata families, trailing RF64/BW64 metadata, disks, other references/platforms,
and controlled confidence intervals remain outstanding.

## Retained metadata word scan

Component rows isolated INFO walking as the largest cost in the large fixture:
90.3 us for INFO versus 30.7 us for cues. `metadata::nul_terminated` now checks
one native machine word at a time using checked slices and zero-byte detection,
then locates the first zero in a matched word in physical byte order. It adds no
unsafe code, alignment assumption, overread, allocation or production dependency.
Empty/tail slices retain the scalar path.

The benchmark adds separate cue/INFO rows. Matching scalar baselines and word-scan
candidates were run twice with that same benchmark. Raw reports are
`validation/metadata-profile{,-repeat}.*` and
`validation/metadata-word-scan{,-repeat}.*`; source/binary fingerprints identify
each version. The scalar repeat temporarily restores the metadata module from
commit `02c7f7d76`, and the candidate is restored before its run. This is sequential
measurement without affinity/frequency control, not a controlled confidence interval.

| Large fixture, median us | Scalar run 1 | Word scan run 1 | Scalar repeat | Word scan repeat |
| --- | ---: | ---: | ---: | ---: |
| INFO only | 90.332 | 42.952 | 92.512 | 45.778 |
| Open + full metadata walk | 126.304 | 84.759 | 142.317 | 85.695 |
| Retained full metadata walk | 144.345 | 74.247 | 139.578 | 82.965 |

Large open+walk time decreased 33–40%, and retained walking decreased 41–49%.
INFO-only medians decreased in small/medium cases too; all raw rows and cycles
are retained. No header-opening or cue-only speed improvement is claimed.
Scoped heap counts remain zero. In the final repeat, dr_wav's retained large
walk is still faster (69.378 us versus 82.965 us); fresh open+walk remains faster
here (85.695 us versus 328.179 us). Materializing metadata once has an explicit
reuse/memory tradeoff, and this change does not erase it.

The matching scalar tests check every byte pair around word boundaries, every
short tail, unaligned long fields, termination absence and subtraction-borrow
cases. All 44 integration tests pass in x64 debug/release and i686 debug, including
the zero-allocation metadata probe; Clippy with denied warnings passes. The new
helper is also compared with a scalar search by the shared fuzz harness. See
FUZZING.md for additional mutation/sanitizer and retained-corpus replay evidence.
