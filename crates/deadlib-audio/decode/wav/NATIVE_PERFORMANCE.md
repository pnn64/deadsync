# Native format, Hound and physical-file measurements

2026-10-28, Windows x64, rustc 1.99.1, Hound 3.5.1. These are measurements on the
recorded machine, with three warmed trials and no affinity/frequency controls or
confidence intervals. Every result is tied to source/binary/compiler/CPU hashes.
They establish the stated workloads, not universal speed or memory superiority.

## Fixed inputs and output semantics

The independent Python packer creates 52 cases: all six native widths with
1/2/6 channels in RIFF, stereo RIFX/RF64/BW64, four reduced-valid-bit RIFF cases,
six 128-chunk metadata cases and six deliberately unaligned in-memory inputs.
Each contains about 1048576 interleaved samples, drawn from a repeated 4096-sample
period of signed extrema, IEEE edge words and seeded pseudorandom bit patterns.
Float words include signed zero, subnormals, infinities and NaN payloads. This
tests native bit handling; it is not a representative compressed music workload.

Integer comparison output is container-scale signed i32, including explicit U8
bias removal and I16/I24 expansion. Floating comparison output keeps its native
32/64-bit words. Every output is checked against independent truth before timing.
Wire and output MiB/s are separate columns: an expanded U8 buffer is four times
the wire size. Additional PCM16 rows compare native i16 output and Hound's
unchecked bulk writer in bounded 4096-sample blocks; the loop supplies exactly
the requested count. Native raw encoding and typed encoding are separate rows.

The 25 full-width RIFF cases supported by Hound must decode exactly. Unsupported
opens and nonexact/failed reduced-valid-bit reads are retained as capability
comments, without comparative decode rows. Hound rejects F64 and these non-RIFF
containers; its four reduced-valid-bit cases open but do not match the requested
container-scale output contract. No speed ratio is assigned to those paths.
Raw views preserve their exact input pointer in all 52 cases; typed direct views
are available only for aligned compatible native representations.

Opening work differs: Wave validates the complete form, including the footer;
Hound stops its initial scan at data. Full decode rows produce the same samples,
but Hound does not validate the trailing metadata as part of that read. Opening
latencies therefore describe the respective APIs, not identical validation work.

## Measured PCM32 bottleneck and change

`NativeIo::read_integers` previously expanded/reconstructed PCM32 in place just
like narrower packed samples. PCM32 already has the requested i32 representation.
It now delegates to the existing native i32 path: direct caller-byte reads on
matching byte order, followed by the existing byte swap when required. No new
unsafe code, allocation, conversion, normalization or production dependency is
introduced. Valid-bit padding and incomplete caller frames remain unchanged.

The full 52-case matrix runs before and after that three-line production change.
Both runs also pass 34 fresh-process memory probes and 22 real-file streaming
measurements. Initial stereo PCM32 medians, microseconds for 4 MiB of wire/output:

| Input | Seekable before | Seekable after | Forward before | Forward after |
| --- | ---: | ---: | ---: | ---: |
| RIFF aligned | 3585.250 | 185.213 | 3429.575 | 185.588 |
| RIFF unaligned | 3142.400 | 197.213 | 3075.988 | 195.388 |
| RIFX | 3330.650 | 732.975 | 3016.113 | 874.575 |
| RF64 | 3594.475 | 170.050 | 3580.163 | 200.175 |
| BW64 | 3104.538 | 189.513 | 3123.163 | 169.525 |

Two additional alternating paired repetitions use the immutable before/after
binaries on identical fixture bytes. Across those five cases, little-endian
seekable/forward medians improve 16.3–19.6 times; RIFX improves 4.1–4.5 times.
All outputs remain bit exact. The code change and the size of the effect support
retaining this optimization despite uncontrolled timing variance. Other rows
also fluctuate; no improvement is attributed to this change outside its PCM32
integer path. The source-difference manifest confirms only native_io.rs changes
between the two complete benchmark snapshots.

## Same-output Hound comparison

After-change stereo RIFF medians in milliseconds. These are explicit integer
expansion or native float outputs; the native PCM16 rows follow below.

| Type | Full caller buffer | Hound full | Seekable stream | Hound stream | Typed encode | Hound sample encode |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| U8 to i32 | 0.244 | 5.156 | 1.155 | 8.788 | 1.250 | 4.194 |
| I16 to i32 | 0.279 | 8.534 | 1.558 | 11.284 | 4.078 | 4.389 |
| I24 to i32 | 0.897 | 8.665 | 2.034 | 12.151 | 3.870 | 7.255 |
| I32 | 0.284 | 8.172 | 0.185 | 11.623 | 0.264 | 3.689 |
| F32 | 0.292 | 4.505 | 0.183 | 4.673 | 0.288 | 3.652 |
| F64 | 0.591 | unsupported | 0.409 | unsupported | 0.620 | unsupported |

Across all 25 supported same-output full-read cases, this crate's median time is
9.5–33.4 times lower than Hound's in this run. This is a measured comparison of
the recorded APIs/data, not a statement about every library or platform.
For stereo native i16, full copy including opening is 0.144 ms versus Hound's
5.473 ms. Native i16 encoding is 0.139 ms versus Hound's 0.189 ms bulk-block path.
The general i32-to-packed-I16 writer row remains much slower than native i16
input; callers should use the matching native method when they have that type.

Opening is not consistently faster. For the displayed stereo cases, borrowed
open is about 103–138 ns, versus Hound's 75–92 ns where supported. Full validation
and reference prefix-only opening differ as explained above. Borrowed opening,
raw/direct access, full caller copies and both native encoders request zero heap
allocations in all 52 cases. Reader/ForwardReader retain bounded format/inventory
storage; Hound's resident reader requests zero heap here. Requested allocation
and logical peak-live counters exclude caller storage, stack, process memory and
allocator-internal realloc transients. They are not physical allocator/RSS peaks.

## Real files and whole-process memory

Twelve RIFF files cover all six types at 8192 samples and 64 MiB of wire payload.
The latter files are 67108930–67108950 bytes long. Complete SHA-256 checks and
Windows FILE_STANDARD_INFO show 67112960 bytes allocated per large file, with no
sparse/compressed/directory flags. These are densely allocated ordinary files.
The benchmark's allocated_file_bytes field comes from GetCompressedFileSizeW,
which reports storage/file length; it is distinct from cluster-rounded
AllocationSize. The separate storage verifier records both exact length and
allocation. See Microsoft's [FILE_STANDARD_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_standard_info)
and [GetCompressedFileSizeW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getcompressedfilesizew).

Each memory probe is a fresh invocation of the same linked binary. Native/Hound
use a 64 KiB BufReader and roughly 4096 output samples; the common raw/truth/output
measurement buffers are allocated/touched before the snapshot. A raw-drain
baseline has the same common buffers. The input buffer is prefilled without
consuming it. Peak working set includes startup, executable/runtime pages, I/O
buffers and validation. Private commit and before/after snapshots are retained.
The RSS elapsed field includes per-sample truth checks and is not a throughput
comparison. OS file cache and other processes are outside process working set.

After-change large-file peak working sets, MiB:

| Type | Raw baseline | Native reader | Hound |
| --- | ---: | ---: | ---: |
| U8 | 3.578 | 3.613 | 3.594 |
| I16 | 3.586 | 3.613 | 3.598 |
| I24 | 3.582 | 3.617 | 3.602 |
| I32 | 3.582 | 3.613 | 3.602 |
| F32 | 3.586 | 3.625 | 3.598 |
| F64 | 3.617 | 3.656 | unsupported |

Native small/large peaks span 3.613–3.660 MiB. Both supported readers remain
bounded as file size increases. These data do not show lower native RSS than
Hound, and small differences should not be interpreted as isolated codec cost.
The test establishes bounded fresh-process streaming memory on these files.

Separate cached-file timings validate every sample in an untimed pass, then time
open/stream without per-sample validation. Source files are real, but warmed OS
cache means these are not cold-device bandwidth measurements. Large-file wire
MiB/s:

| Type | Native reader | Hound |
| --- | ---: | ---: |
| U8 | 606.877 | 68.632 |
| I16 | 834.602 | 123.219 |
| I24 | 930.074 | 185.494 |
| I32 | 2434.413 | 253.426 |
| F32 | 2300.359 | 385.805 |
| F64 | 2281.408 | unsupported |

The before-change PCM32 result is 749.793 MiB/s, with Hound 249.606; the current
Hound control is 253.426. The PCM32 improvement persists through cached file I/O.

## Artifacts and limits

`validation/native-widths-full.*` is the complete before run;
`native-widths-i32-native.*` is the complete after run. Each has raw operation
trials/capability comments, CPU cycles, requested heap fields and private binary
hashes. Source archives preserve exact bytes. Small content-addressed fixture
archives contain headers, wire/truth periods and tails; their manifests reconstruct
every exact full file without storing repeated hundreds of megabytes. Raw data,
errors, fixture/source archives, storage verification and alternating repetition
reports are retained. Both complete runs report unchanged inputs.

`native-widths-smoke.*` preserves the initial adapter failure: Hound finalization
leaves the cursor in its header, so its encoded extent must come from RIFF length.
`native-widths-smoke-fixed.*` records the successful six-format smoke before later
measurement refinements. Their exact source archives explain those differences.
Neither partial run supports a speed claim. No third-party source is embedded.

The 92-test suite passes x64 debug/release and i686 debug, with Clippy. The 708
retained native gates, 4484 descriptor/read/range checks and 1912 writer outputs
retain their expected hashes under the existing BW64 rule. Two million checked
mutations, 1839 prior corpus inputs and 100000 ASan executions pass after the
optimization; FUZZING.md records the new corpus. The later pinned investigation
and ADPCM geometry optimization are in PERFORMANCE.md. Wider Rust reference
comparisons and platform/profile audit remain open in AUDIT.md.

```sh
python crates/deadlib-audio/decode/wav/benches/run_native.py --output target/wav/new-native
python crates/deadlib-audio/decode/wav/benches/verify_native_files.py --report target/wav/new-native.json --fixtures target/wav/new-native-fixtures --output target/wav/new-native-storage.json
```
