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

## Known-length sequential output

The PCM16 harness now includes `wav/encode_known_i16`, using a Write-only slice
sink and upfront frame count. `validation/pcm16-known-output-repeat.{tsv,json}`
records the post-test run: median 0.321 ms for known-length output, 0.332 ms for
seekable finalization and 0.422 ms for Hound's bounded batches. The native timing
ranges overlap; no encoding-speed improvement is claimed from the new API.
Both native output paths request zero heap allocations. Input/output buffers,
resident-file scope and QueryThreadCycleTime accounting are unchanged. The raw
initial run is also retained as `pcm16-known-output.*`; it overlapped verification
work and is not the basis for a performance claim. The repeat ran after those
checks and mutation processes finished. Broader output formats/ADPCM throughput,
OS pipe/file costs and RSS remain outstanding.

## Forward-only native input

`validation/pcm16-forward-reader.{tsv,json}` adds the complete 4 MiB resident
PCM16 stream through a Read-only slice and ForwardReader, with the same 4096-sample
caller buffer and final form validation. Its median is 0.195 ms, versus 0.193 ms
for Reader and 13.336 ms for Hound in that run. The two native ranges overlap;
no speed advantage between these native reader APIs is claimed. These are logical
resident-audio rates, not disk/network throughput.

Forward opening plus full streaming requests one 16-byte format allocation,
versus Reader's two allocations / 112 bytes. No audio allocation or retained chunk
inventory is added; caller input/output, stack and process RSS are excluded.
Hound still allocates zero in this PCM16 case. Broader native formats, forward
ADPCM throughput, actual pipes/files and peak RSS remain unmeasured. The native
conversion code is now shared by both reader APIs; the repeated baseline rows
and sample-bit checks are retained in the same report.

## Physical sparse-file memory probe

[LARGE_FILES.md](LARGE_FILES.md) records 24 successful fresh-process checks against
real NTFS RF64/BW64 files, with native PCM and header-only IMA blocks. Files with
over 16 GiB logical length allocate 256 KiB on disk and support exact x64/i686
boundary seeks. Verify-process peak working sets stay at 3.41–3.42 MiB on x64 and
4.57–4.95 MiB on i686 across small/large probes. The raw report includes startup,
before/after snapshots and private commit. This is bounded whole-process memory
evidence for opening and random seeking, not isolated codec RSS, full-stream disk
throughput or a reference-library memory comparison. Comparative process memory,
representative sequential I/O and wider-format throughput remain unmeasured.

## Flat PCM regression after wavl support

`validation/wavl-pcm16-regression.{tsv,json}` repeats the unchanged resident 4 MiB
PCM16 workload after adding segmented audio and the explicit contiguous accessors.
All sample, encoded-output and direct-pointer checks pass. Median full read is
0.317 ms, Reader streaming 0.185 ms, and forward streaming 0.183 ms. The same-run
Hound full read/stream medians are 12.014/12.401 ms. Scoped allocation counts stay
at zero for borrowed/direct/full-copy operations, two calls/112 bytes for Reader
streaming and one call/16 bytes for forward streaming. Caller storage, stack and
process RSS are excluded as before.

Short construction costs have increased: borrowed open is 99.159 ns and direct
view including open is 141.495 ns, versus 82.596/96.616 ns in the retained
`pcm16-forward-reader` run. Same-run Hound open is 79.566 ns. These separate runs
have no affinity/frequency controls, but the increased short-operation time and
thread-cycle counts warrant profiling the added state and dispatch. No opening
speed improvement is claimed; this retained result is the baseline for that work.
Segmented throughput, allocation inventories and comparative memory still need
their own measurements.

## Compact PCM view state

The native view now retains validated sample/layout fields and the borrowed raw
format bytes, reconstructing the complete Format only on `Pcm::format()` calls.
On x64, Pcm decreases from 128 to 64 bytes. Wave (248), Reader (232) and
ForwardReader (256) retain their previous sizes. This is object/stack state,
not a reduction in heap allocation or measured process RSS. Native frame loops
use cached fields; constructor validation and public behavior are unchanged.

The PCM16 harness now reports these sizes and separates retained view creation
from full-format queries. Short-operation rows run one million iterations per
trial, with three trials per operation. `validation/pcm-layout-control-long.*`
and `pcm-layout-control-repeat.*` use the original Pcm/native_io sources from
`f729d7a64`; `pcm-compact-view{,-repeat}.*` use the retained compact state. All
four use the same instrumented benchmark, including sample/pointer/output checks.

| Median ns/op | Original 1 | Compact 1 | Original repeat | Compact repeat |
| --- | ---: | ---: | ---: | ---: |
| Borrowed open | 100.685 | 101.134 | 104.848 | 100.402 |
| Direct view including open | 150.367 | 135.609 | 145.255 | 125.211 |
| View from retained Wave | 50.867 | 36.051 | 47.977 | 34.190 |
| Full format query from retained Pcm | 10.397 | 25.006 | 10.132 | 22.532 |

Retained view creation takes about 29% less time in both pairs; direct access
including open takes 9.8–13.8% less time. A full-format query instead costs about
23–25 ns versus 10 ns. Creating a view and immediately querying that descriptor
therefore has roughly unchanged combined cost. Callers repeatedly inspecting
metadata can retain the returned Format. No opening, full-copy, streaming or
encoding improvement is claimed. Separate runs lack affinity/frequency control;
raw trial variation, CPU cycles and every other workload row remain available.
Requested allocation counts remain zero for borrowed/direct/caller-copy paths,
two calls/112 bytes for Reader streaming and one call/16 bytes for forward input.

The initial shorter 100000-iteration `pcm-layout-control.*` run was noisy and
is retained without supporting a speed claim. Its exact benchmark source is
`pcm-layout-control-bench.rs.txt`. A separate experiment sharing the top-level
and child chunk budget produced a 103.710 ns opening median against 100.685 ns
in the first long control. It was reverted: `pcm-shared-budget.*` and its exact
`pcm-shared-budget-container.rs.txt` snapshot preserve that rejected experiment.
The final parser keeps the earlier independent budget checks.

`validation/compact-native-replay.json` checks the immutable flat/wavl oracle
archives and prior independently expected hashes: 708 fixture gates, 4484
descriptor/read/range checks and 1912 exact encoded-file outputs pass. This is a
replay of established sample truth, not a fresh external-library comparison.
All 69 integration tests pass in x64 debug/release and i686 debug; denied-warning
Clippy passes (`compact-checks*`). Two million checked mutations, 2507 earlier
corpus inputs and 100000 fresh ASan executions also pass; see FUZZING.md.

## Native wavl output after container extraction

`benches/wavl.rs` checks native PCM16 output with caller-owned storage, 512 data
children for 4 MiB of audio, empty forms, sixteen ancillary chunks and a tiny
RF64 form with virtual silence above u32. It verifies sample/count truth before
measurement, reports QueryThreadCycleTime and requested heap counts, and runs
three warmed trials per row. Short rows use one million or 100000 iterations;
full resident-audio rows use 128. Input/output buffers, decoded verification,
stack and process RSS are excluded from scoped heap counts.

`validation/wavl-mux-control{,-repeat}.*` temporarily restores the native writer
from `1ffaa3190`; the exact bytes are preserved in
`wavl-mux-baseline-encode.rs.txt`. `wavl-mux-validated{,-repeat}.*` uses the retained
WavlMuxer wrapper and the same harness. The wrapper passes its descriptor already
validated by Spec::header to a private constructor; public raw-format entrypoints
still validate their input. Native object state remains 112 bytes on x64, and
every measured row requests zero heap allocations/bytes. A separate allocator
probe checks both public generic muxer constructors in all four containers.

| Median ns/op | Original 1 | Extracted 1 | Original repeat | Extracted repeat |
| --- | ---: | ---: | ---: | ---: |
| Open/finalize empty | 123.463 | 128.296 | 119.644 | 126.481 |
| Known-length empty | 107.762 | 111.161 | 92.749 | 112.802 |
| Open/finalize with 16 metadata chunks | 335.687 | 351.701 | 300.437 | 352.334 |
| Known-length with 16 metadata chunks | 315.913 | 342.402 | 288.928 | 335.973 |
| Segmented 4 MiB PCM16 output | 360085.938 | 371850.000 | 341236.719 | 361685.938 |
| Known segmented 4 MiB output | 356386.719 | 374156.250 | 331052.344 | 371630.469 |
| u64 silence/RF64 finalization | 199.851 | 206.347 | 177.224 | 204.287 |

The retained extraction is slower in these paired medians: about 4–6% for empty
seekable forms, 3–22% for known empty forms and 3–12% for full segmented output.
Metadata and virtual-silence rows also regress. CPU-cycle rows and all individual
trials are retained. Runs have no affinity/frequency control or confidence
intervals, so these do not prove a platform-wide cost; they do identify an
outstanding native-writer optimization issue. The codec-independent output API
is retained as required architecture work. No speed improvement or performance
parity is claimed for this extraction, and competitive broad-format performance
remains unfinished.

`wavl-mux-initial.*` records the initial version that parsed the native descriptor
twice; its exact two source files are preserved as
`wavl-mux-initial-{encode,mux}.rs.txt`. `wavl-mux-inline.*` records an additional
inlining experiment and its matching source snapshots. That experiment shows
mixed results, including a large short-row fluctuation, and was reverted rather
than supporting an improvement claim. All original attempts remain available.
The final constructors avoid duplicate parsing without those explicit hints.

All 83 integration tests pass in x64 debug/release and i686 debug, with Clippy.
The 708-case native replay retains 4484 descriptor/read/range hashes and 1912
complete writer outputs under the established BW64 dummy-byte rule. Two million
checked mutations, 1580 earlier corpus files and 100000 fresh ASan executions
also pass. WAVL.md records the raw descriptor/count API and the remaining
compressed-sequence decoding work; FUZZING.md records corpus and resource limits.

## Borrowed compressed wavl access

`benches/adpcm_wavl.rs` measures the new AdpcmWavl view on resident encoded input:
512 data children, four complete blocks per child and three held frames after
each child. Microsoft mono/stereo uses 256-byte blocks; stereo 4-bit IMA uses 256
and 3-bit IMA uses 248. The source is constant zero. The independent expected
reconstruction is zero for Microsoft/4-bit IMA and alternating 0/1 inside each
3-bit IMA block, with its final zero held during silence. Samples are verified
outside measurement. This is a narrow low-entropy workload, not a realistic
music/speech mix or an external-library comparison.

`validation/adpcm-wavl-bench-final.*` retains three warmed trials per operation,
thread cycles, CPU/compiler/source/binary fingerprints and unchanged-input checks.
Index/open rows run 10000 iterations, full decoded reads 32, and range rows
100000. Data ranges request 62 samples (whole frames); held seeks request one
frame from a rotating child. Medians on this Windows x64 machine:

| Case | Retained-Wave index us | Open + index us | Full decoded MiB/s | Data range us | Held-frame seek us |
| --- | ---: | ---: | ---: | ---: | ---: |
| Microsoft mono | 5.924 | 34.411 | 267.837 | 2.939 | 2.939 |
| Microsoft stereo | 5.945 | 34.537 | 265.765 | 2.880 | 2.891 |
| IMA 4-bit stereo | 5.924 | 33.837 | 256.409 | 3.123 | 3.035 |
| IMA 3-bit stereo | 5.923 | 36.443 | 243.034 | 4.096 | 4.101 |

These timings have no affinity/frequency control or confidence intervals and do
not establish comparative performance. Full-read throughput counts decoded i16
bytes, including held silence. The implementation decodes a final block again
when entering silence, even after the preceding range decoded it; caller scratch
does not retain a cross-call cache. The file-backed cache is now described below;
forward segmented reading remains future work. The earlier measured native wavl output regression also
remains unresolved; this new benchmark does not address it.

The dedicated allocator test separates allocation from timing. View construction
for eight inventoried children requests one allocation/128 bytes on x64/i686;
reads request zero. Each added bound is 16 bytes, with Vec minimum-capacity
rounding for small inventories. The underlying Wave inventory and caller
scratch/output already exist and are excluded. AdpcmWavl object state is 128
bytes on x64 and 80 on i686; the unchanged flat AdpcmData is 88/64 and still
constructs without allocation. These are requested heap/object counts, not codec
RSS. No additional unsafe production code or dependency is introduced.

The initial `adpcm-wavl-bench.*` run stopped at the 3-bit verification because
the harness incorrectly expected exact zero reconstruction. Its partial timing
rows, panic stderr and exact initial benchmark/runner snapshots are preserved;
they do not support a speed comparison with the corrected run. The final harness
uses the explicit 0/1 waveform and completes all four formats. All 87 tests,
Clippy, 216 independent fixture gates/1296 range checks, two million checked
mutations, 1581 prior corpus inputs, 100000 ASan executions and the 708-case native
sample/byte replay pass. WAVL.md documents the decoded-count/padding contract.

```sh
python crates/deadlib-audio/decode/wav/benches/run_adpcm_wavl.py --output target/wav/new-adpcm-wavl-bench
```

## File-backed wavl cache and flat-reader regression

`benches/adpcm_file.rs` uses resident Cursor input, caller output and the same
four block geometries/zero-source reconstructions as the borrowed benchmark.
It checks every decoded sample before timing. Flat input has 2048 blocks; wavl
has 512 four-block children, each followed by three held frames. `full_retained`
seeks an already opened reader to zero and decodes into a full caller buffer;
`stream127_retained` uses a reusable 127-frame buffer. Data range queries request
62 samples; rotating held queries request one frame, and `same_held_cached`
repeatedly seeks/reads the same held frame after warmup. Output throughput counts
decoded i16 bytes including silence. Heap/RSS are excluded from the timing harness.

`validation/adpcm-wavl-reader-bench.*` contains two isolated source copies and
matching profiles/harnesses. Baseline is commit `491b3c88d`; current is the new
file-backed segmented implementation. Both complete source trees, the exact
standalone manifest/benchmark and their hashes are archived. The explicit bench
profile matches workspace opt-level 3/LTO/debug=1, and both builds inherit the
workspace static MSVC-runtime config. Only this crate and std are compiled.
Separate source copies avoid mutating the active workspace during a control run.
Baseline/current flat runs alternate twice; a final current run also exercises
wavl. Three warmed trials are retained for every row, with QueryThreadCycleTime,
CPU/compiler/binary/config hashes and unchanged-input checks. There is no
affinity/frequency control or confidence interval; these are regression candidates
on this workload, not statistical or platform-wide performance claims.

Flat median ns/op (full/stream rows include roughly 2 MiB of decoded output):

| Row | Baseline 1 | Current 1 | Baseline 2 | Current 2 |
| --- | ---: | ---: | ---: | ---: |
| MS mono open | 646.590 | 659.390 | 730.490 | 671.110 |
| MS mono full read | 6206350.000 | 6406606.250 | 6964912.500 | 6412800.000 |
| MS mono stream 127 | 6505962.500 | 6756900.000 | 6794187.500 | 6742468.750 |
| MS mono data range | 3020.020 | 3176.243 | 3504.460 | 3183.580 |
| MS stereo open | 657.590 | 657.760 | 652.610 | 648.640 |
| MS stereo full read | 6067062.500 | 6250700.000 | 6063025.000 | 6243125.000 |
| MS stereo stream 127 | 6243693.750 | 6456618.750 | 6642456.250 | 6495656.250 |
| MS stereo data range | 2972.617 | 3085.360 | 3200.673 | 3091.717 |
| IMA 4-bit open | 638.000 | 643.200 | 663.500 | 632.500 |
| IMA 4-bit full read | 6571262.500 | 6625556.250 | 7055831.250 | 6593243.750 |
| IMA 4-bit stream 127 | 6695906.250 | 7638318.750 | 7552756.250 | 6756175.000 |
| IMA 4-bit data range | 3243.553 | 3761.710 | 3566.127 | 3220.837 |
| IMA 3-bit open | 653.070 | 770.650 | 629.130 | 664.610 |
| IMA 3-bit full read | 8356693.750 | 8452581.250 | 8474250.000 | 8231093.750 |
| IMA 3-bit stream 127 | 8441743.750 | 8739243.750 | 8528543.750 | 8613918.750 |
| IMA 3-bit data range | 4122.667 | 4129.153 | 4065.470 | 4130.027 |

Several rows reverse direction between runs. MS stereo full reads are about 3%
slower in both pairs; IMA 3-bit opening is 5.6–18% slower, streaming is 1.0–3.5%
slower and data ranges are 0.2–1.6% slower. These remain explicit optimization
targets. No overall flat speed improvement or parity is claimed. Neither this
comparison nor the new functionality resolves the earlier native-writer slowdown.

Current wavl medians, same narrow low-entropy input:

| Case | Open us | Full decoded MiB/s | Stream 127 MiB/s | Data range us | Rotating held seek us | Same cached held ns |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Microsoft mono | 35.118 | 297.733 | 271.626 | 3.278 | 3.290 | 129.041 |
| Microsoft stereo | 37.298 | 284.183 | 268.652 | 3.405 | 3.388 | 127.989 |
| IMA 4-bit stereo | 36.811 | 293.085 | 279.333 | 3.367 | 3.144 | 129.469 |
| IMA 3-bit stereo | 36.764 | 301.731 | 281.898 | 4.250 | 4.253 | 128.667 |

The owned reader reuses the container's existing timeline and caches blocks by
their physical offsets. It adds no second segment index and no extra held-frame
allocation. A preopened eight-child stereo probe requests two allocations/52 bytes
for block buffers and zero for later reads/seeks. AdpcmReader state remains 344
bytes on x64 (both standalone versions) and measures 248 on i686. Underlying
format/inventory and caller buffers are excluded, as is process RSS. The cache
I/O test proves repeated held seeks do not read audio again; the timing alone
would not prove that. Physical-file/RSS and external performance comparisons
remain unfinished.

All 89 tests pass in x64 debug/release and i686 debug with Clippy. The 216-case
independent campaign passes 2592 borrowed/file/range checks; 204 prior real-file
flat ranges and all 708 native gates/1912 writer outputs retain their hashes.
Two million checked mutations, 1827 previous corpus inputs and 100000 ASan
executions pass. WAVL.md records global trimming, error recovery, u64 virtual
offset scope and the remaining forward-only implementation.

```sh
python crates/deadlib-audio/decode/wav/benches/compare_adpcm_file.py --output target/wav/new-adpcm-file-bench
```
