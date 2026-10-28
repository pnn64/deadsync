# Native and compressed comparison with Symphonia

This campaign extends the Hound/native results in NATIVE_PERFORMANCE.md to a
second Rust implementation and Microsoft/IMA ADPCM. It does not claim every
codec or platform is faster. It identifies a required buffered-file improvement.

## Inputs and comparison contract

`benches/run_symphonia.py` builds an independent source copy of this crate and
`tests/oracle/performance`, with Symphonia 0.6.1 features wav/pcm/adpcm/opt-simd.
Both use opt-level 3, LTO, debug information level 1 and panic-abort release,
with the repository's static MSVC runtime configuration. Reference dependencies
stay in the isolated test workspace. The library has no production dependencies.
Source/profile/lock snapshots, binary hashes and compiler/CPU details are retained.

Native inputs reuse the 52 independently packed native cases: six widths,
1/2/6 channels, four containers, valid-bit layouts, metadata and unaligned bases.
Another 37 cases cover Microsoft mono/stereo, random 3/4-bit IMA, sign-only 4-bit
IMA, four containers and selected six-channel/extensible variants. Sixteen
independent compressed blocks form each repeated period. Microsoft blocks use
all seven predictors and independent random target quantization; scalar decode
of the packed bytes determines truth. Random IMA uses random headers/indices and
codes. Sign-only 4-bit IMA uses codes 0/8, for which separate-shift and combined
rounding agree. It is an explicitly limited comparison subset; random cases are
also retained and their mismatch is not hidden.

The common output is interleaved i32 left-aligned to bit 31 for integer PCM,
native-width IEEE words for floats, and i16 for ADPCM. The i32 alignment is an
explicit benchmark adapter after native container-scale reads; production APIs
keep their existing scale. Symphonia uses its slice/interleaving API, including
conversion to i16 for compressed output. Float comparisons use bit words,
including signaling/quiet NaNs, infinities, subnormals and signed zero.

Every native case verifies borrowed complete reads, owned full/4096-sample
streaming and 128 exact 32-frame seek ranges against independent period truth.
Symphonia full, streaming and seek operations have separate exact gates. Timings
exist only for matching operations. Its accurate seek adapter resets the decoder,
decodes the containing packet and discards the prefix as required by the
[FormatReader contract](https://docs.rs/symphonia-core/0.6.1/symphonia_core/formats/trait.FormatReader.html).
Native opening scans and retains the full container inventory, including the
footer; Symphonia's opening/demuxing work stops at its data boundary. Unknown
metadata retention and validation work are therefore different. Open timings
are API costs, not identical parser-work measurements.

The benchmark thread is pinned: processor bit 43 on x64 and bit 31 on i686.
Frequency and other processes are uncontrolled. Each row has three trials and
an untimed warmup of 10% of iterations, bounded to 1..10000. CPU cycles are from
QueryThreadCycleTime. Heap counters bracket a complete operation after warmup;
caller input/output/truth buffers and stack are excluded. Requested logical
allocation peaks do not include a realloc implementation's transient physical
peak. Retained-seek rows reuse the already opened reader and decoder buffers.
No confidence interval or cross-architecture speed causation is claimed.

## Gates and measurements

`validation/symphonia-widths-full.*` passes all 89 native complete/stream gates
and all 11392 native seek ranges. Symphonia matches 38 full/stream gates: all
34 RIFF native cases and four RIFF Microsoft/sign-only IMA cases. The remaining
51 cases retain errors or first mismatching words and have no same-output timing
ratio. All 34 native Symphonia seek gates pass; its four compressed seek gates
fail at frame 7936. In the published reader, the seek byte calculation multiplies
the aligned frame timestamp by block size without dividing by frames/block.
We infer that this calculation causes the observed ADPCM EOF failures; this
combines source inspection with a driver observation, not a library-wide seek
certification.
The [reference source](https://github.com/pdeljanov/Symphonia/blob/ee35874b571a35a9a6e15d3bc9a3aaf8f11fbeee/symphonia-format-riff/src/wave/mod.rs)
is linked for inspection; published package hashes are authoritative for the run.

Random IMA4 differs from the separate-shift oracle starting at sample 2 in the
stereo RIFF case. Symphonia's
[combined delta expression](https://github.com/pdeljanov/Symphonia/blob/ee35874b571a35a9a6e15d3bc9a3aaf8f11fbeee/symphonia-codec-adpcm/src/common_ima.rs)
explains the recorded rounding difference. Three-bit IMA rejects explicitly.
RIFX/RF64/BW64 probe rejections and six-channel compressed limits remain visible.
No reference failure is used as evidence that an input violates its specification.

Representative RIFF stereo medians, milliseconds for roughly one million output
samples (not one million frames):

| Type | Native full | Symphonia full | Native stream | Symphonia stream |
| --- | ---: | ---: | ---: | ---: |
| U8 | 1.668 | 2.957 | 1.548 | 2.936 |
| I16 | 2.209 | 3.142 | 2.057 | 3.095 |
| I24 | 2.856 | 3.940 | 2.579 | 3.954 |
| I32 | 0.328 | 3.331 | 0.233 | 3.278 |
| F32 | 0.341 | 2.961 | 0.204 | 2.689 |
| F64 | 0.658 | 3.594 | 0.470 | 3.556 |
| Microsoft | 6.055 | 5.830 | 6.111 | 5.688 |
| IMA4 sign-only | 6.525 | 5.333 | 6.796 | 5.400 |
| IMA4 random | 16.092 | nonmatching | 16.719 | nonmatching |
| IMA3 random | 14.212 | rejected | 13.988 | rejected |

Across all 34 matching native cases, owned full reads have 1.30-11.34 times
lower median time, streams 1.41-18.29, and the exact 128-range operation
17.20-93.70. Opening has 2.28-18.73 times lower median time with the unequal work
contract above. Native borrowed full operations request zero allocations in all
89 cases. Native owned native-format full reads request 2-8 allocations with
112-6162 peak live bytes, versus Symphonia's 168-921 calls and 68672-177104 bytes.
Compressed buffers depend on geometry; these figures do not include callers.
Microsoft and sign-only IMA resident reads remain slower in the shown cases.

## Physical files, memory and I/O amplification

The full campaign passes 48 fresh-process RSS probes and 32 separate cached-file
stream runs. There are small and at-least-64-MiB wire files for all six native
widths, Microsoft stereo and sign-only IMA stereo. Each file is dense, nonsparse
and uncompressed; FILE_STANDARD_INFO, attributes and full SHA-256 are checked.
All 105 distinct resident/physical fixture files pass storage/hash verification.
The small content-addressed archive reconstructs every full file exactly.

RSS probes allocate and touch common output/truth/raw buffers, open and prefill
a common 64 KiB BufReader, then snapshot process working set/private commit.
The after snapshot keeps the decoder alive. Baseline drains raw bytes. Every
codec sample is checked; elapsed RSS time includes that checking and is not a
throughput result. Whole-process peak working sets are 3.75-3.81 MiB baseline,
3.80-3.85 MiB native, and 3.92-4.01 MiB Symphonia, across small/large files.
These are process observations including code/runtime pages, not isolated codec
RSS. They substantiate bounded memory for these cases.

Separate file timings validate in an untimed pass, then time open/stream with
sample checking disabled. File buffers/decoder allocations occur inside this
measurement. OS cache is warm; this is not cold-device bandwidth. Large-file
wire MiB/s:

| Type | Native | Symphonia |
| --- | ---: | ---: |
| U8 | 525.85 | 305.21 |
| I16 | 699.89 | 519.69 |
| I24 | 791.82 | 527.29 |
| I32 | 2039.21 | 757.08 |
| F32 | 2086.14 | 840.15 |
| F64 | 2066.60 | 1034.75 |
| Microsoft | 33.18 | 85.92 |
| IMA4 sign-only | 31.18 | 93.91 |

`examples/adpcm_io_probe.rs` counts actual File calls below a 64 KiB buffer and
checks every sample against the independent period. For both large compressed
files it records 131072 reads, 131072 seeks and 8585774834 read bytes for
67108864 encoded bytes: **127.94-times read amplification**. Small 8192-byte
payloads amplify 8.53 times. The reader seeks before each uncached 512-byte
block, discarding buffered read-ahead. This verifies a substantial buffered-file
bottleneck, despite bounded memory and competitive resident decoding. Removing
those unnecessary sequential seeks is required optimization work.

## Profile checks and artifacts

The i686 smoke passes ten resident cases, 24 fresh-process memory probes and
16 cached-file runs, with all six native widths bit-exact. Native full times for
those widths are 0.32-3.05 ms versus 3.45-5.12 ms. Microsoft is 18.42 ms versus
6.28 ms, exposing an additional 32-bit arithmetic cost; sign-only IMA is 7.00 ms
versus 6.18 ms. This narrow i686 subset has no large-file RSS run. Its peak process
ranges are 4.88-4.93 MiB baseline, 4.92-4.97 native, and 5.06-5.15 Symphonia.
Different affinity masks and uncontrolled frequency prevent treating the x64/i686
ratios as an architecture-only experiment. Linux, ARM and big-endian hosts are
not measured here; RIFX byte order is exercised on these little-endian hosts.

`symphonia-widths-smoke.*` is the initial successful x64 subset;
`symphonia-widths-full.*` is the complete x64 matrix;
`symphonia-widths-i686.*` is the 32-bit subset. A subsequent verified x64 smoke
checks all 496 published files of 21 registry dependencies against their
lock-pinned .crate archives before and after the run. The earlier complete runs
have a retained post-run package verification, with that weaker timing scope
explicit. Both the reference and native source ZIPs and raw stdout/stderr ZIPs
retain their entry hashes. All campaigns report unchanged benchmark inputs.
No third-party source is embedded. No production source changes in this
milestone: its bytes still match the preceding 93-test x64 debug/release and
i686 debug, Clippy, two-million-mutation and ASan evidence. New example/oracle
Clippy checks pass on x64 and i686. The checksum, fixture reconstruction and
storage checks are separate from speed claims.

```sh
python crates/deadlib-audio/decode/wav/benches/run_symphonia.py --output target/wav/new-reference
python crates/deadlib-audio/decode/wav/benches/run_symphonia.py --smoke --target i686-pc-windows-msvc --output target/wav/new-reference-i686
python crates/deadlib-audio/decode/wav/benches/verify_native_files.py --report target/wav/new-reference.json --fixtures target/wav/new-reference-fixtures --output target/wav/new-reference-storage.json
```
