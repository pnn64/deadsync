# Hardware CRC folding, 2026-10-29

This pass starts at `3a4fe719d`, after the
[incremental-decoder changes](DECODER_BODY.md). Three commits accelerate the
existing direct Ogg polynomial without changing its seed, reflection or final
XOR rules. Public APIs, packet grammar, IO calls and encoded bytes remain the
same. The scalar implementation is retained and matches the baseline textually
apart from its private function name.

The gain justifies a narrowly scoped unsafe exception. The crate changes from
`forbid(unsafe_code)` to `deny(unsafe_code)`, with one explicit allowance on the
private `crc_hardware` module. Unsafe operations remain rejected elsewhere,
and `unsafe_op_in_unsafe_fn` is denied. The accelerated path is x86-64 only,
requires runtime PCLMULQDQ **and** SSSE3 checks, and starts at 128 input bytes.
Smaller inputs and other CPUs use the existing safe table implementation.
No production dependency, public field or per-object cache is added.

## Three measured changes

`fff5f20f2` introduces a single-state fold for public `crc32`. It processes
16-byte polynomial words with two carry-less multiplications, then uses the
existing table routine to reduce the final 16-byte state and process the tail.
Runtime feature detection precedes every entry to the target-feature function.
This first stage leaves container CRC updates on the scalar path.

`5d2fc6ebc` moves selection into the existing seeded `update` primitive.
Page parsing, readers, writers and owned encoding now share the fast kernel.
The public-only dispatch is removed. A nonzero incoming CRC is XORed into the
high 32 bits of the initial 128-bit message state, preserving split-update
semantics and the existing zero-checksum-field handling.

`da995922b` interleaves four independent states over 64-byte groups. The CPU
can overlap their carry-less multiplies instead of waiting for one state each
16 bytes. It combines the four states in original byte order, folds remaining
complete 16-byte words and uses the unchanged scalar final reduction/tail.
There is no unsafe pointer arithmetic or raw allocation: every load derives
from a complete fixed-size array or a checked 16-byte subslice.

Each additional 64-byte group replaces 64 scalar table lookups with eight
carry-less multiplies and register XORs. The public-only dispatch is removed
when seeded updates share the kernel. This is an execution reduction rather
than a source reduction: the source delta is 183 lines including native/Miri
tests, with the portable implementation retained unchanged.

## Polynomial and soundness argument

The polynomial is `P = x^32 + 0x04c11db7`, with no reflected input, initial XOR
or final XOR. Byte reversal in the SIMD register maps the first input byte to
the highest polynomial coefficients. PCLMUL forms a 128-bit polynomial product
from selected 64-bit halves; the immediate selectors 0x00 and 0x11 choose the
low/low and high/high halves. [Rust intrinsic documentation](https://doc.rust-lang.org/core/arch/x86_64/fn._mm_clmulepi64_si128.html).

Let `D = L + H*x^64`. Multiplying `L` by `x^128 mod P` and `H` by
`x^192 mod P`, then XORing the next message word, gives
`D*x^128 + next (mod P)`. Coefficients are generated at compile time by the
same polynomial transition, with no runtime power calculation. The seed enters
as `seed*x^96`; final scalar CRC of the folded word multiplies by `x^32`, so
that contribution is exactly `seed*x^128`, as in the previous implementation.

Four states use powers 512 and 576 instead, accounting for the three intervening
words. Combining them with three 128-bit folds gives
`D0*x^384 + D1*x^256 + D2*x^128 + D3 (mod P)`. Linearity preserves the complete
message and seed before the unchanged final reduction.

The only safe entry checks input length and both CPU features through
[`is_x86_feature_detected!`](https://doc.rust-lang.org/std/arch/macro.is_x86_feature_detected.html).
The private function is annotated for those features. SSE2 is part of the
x86-64 target baseline. Safe chunking supplies complete 64-byte arrays; indexed
loads are exactly 16 bytes at offsets 0, 16, 32 and 48. Remaining words are
`[u8; 16]` arrays. The sole store targets a fresh 16-byte local array.
[Unaligned 128-bit loads](https://doc.rust-lang.org/core/arch/x86_64/fn._mm_loadu_si128.html)
require no alignment, but still require all 16 bytes to be valid. These bounds
are supplied by the array/subslice types. No input pointer is written or
returned; no SIMD state escapes through a public ABI. All register values are
initialized, and all polynomial arithmetic is XOR or defined carry-less multiply.

Miri checked the hardware-kernel source, including intrinsic emulation, with
both features forced on over 21 boundary lengths, 16 alignments and two seeds.
Each borrowed slice ends
at its allocation boundary. The check passed in 167.23 seconds with no memory,
provenance or aliasing error. Its command, feature flags, source hashes, runner
hash and captured terminal tool result are in the
[Miri record](validation/2026-10-29-clmul-miri.json); this check has a captured
tool transcript rather than a redirected process log.

A separate Miri process with both features disabled asserts that the safe
entry declines hardware and that seeded updates match a bitwise reference.
Its complete log and minimal wrapper sources are recorded in the same JSON.
All five scalar CRC/transport unit tests also pass in a native i686 executable;
ARM64 compilation and direct rustc 1.96 library compilation pass. The package's
declared MSRV remains 1.97. Commands, source and artifact hashes are in the
[fallback/compatibility record](validation/2026-10-29-clmul-fallback-checks.json).

## Measurement boundaries

The full worker retains the previous 207 workloads and adds 39 raw CRC controls:
13 sizes around the 128-byte dispatch and word/group boundaries, with starting
offsets 0, 1 and 15. Four saved source versions run eight rotating trials, each
first twice, yielding **7,872 CPU/heap observations**. Every observation is the
median of eleven measured batches after warmup. Heap counters use a separate
untimed operation. The [record](validation/2026-10-29-clmul.json) pins compiler,
profiles, sources, binaries, ranges and median absolute deviations; all
[raw rows](validation/2026-10-29-clmul.tsv) remain available.

Windows QueryThreadCycleTime measures the benchmark thread pinned to logical
CPU 2 on a shared Intel Xeon E5-2696 v4 at 2.20 GHz. Frequency and other activity
are uncontrolled. Builds use rustc 1.99.1 / LLVM 23.1.1, opt-level 3, LTO,
benchmark debug symbols and static CRT. The baseline target has no globally
required PCLMUL/SSSE3 flags. Optional features are selected at runtime.

The [paired driver](validation/2026-10-29-clmul-paired.rs) links all four versions
with matching optimized LTO flags, alternates before/after across twelve
batches and discards warmup. Eight processes produce
[632 paired observations](validation/2026-10-29-clmul-paired.json) across 79 cases.
Raw CRC uses matching black-boxed function pointers and patterned input at
offset 3. Parse/reader inputs are framing-valid pages with bodies filled with
37; generation and encoding precede timing. Reader work includes allocation,
reads, CRC, EOF and cleanup. Writer operations emit a BOS header and eight
payload packets ending in EOS into a counting sink. Byte sizes and iteration
counts are explicit in the driver. No audio decoding or file loading is timed.
Paired throughput is the median of process-level cycle ratios; cycle columns
are separate medians.

## CPU and heap results

The public single-state CRC gains **2.29x / 2.33x** paired throughput at
4 KiB / 65,025 bytes. Four independent states add **3.56x / 3.63x** over the
seeded single-state kernel; every process wins in those cases. The whole-worker
ratio below measures the combined result directly. Small-input and first-call
costs are included rather than assuming every call improves.

| Adjacent paired case | Before cycles | After cycles | Paired CPU throughput | Faster processes |
| --- | ---: | ---: | ---: | ---: |
| `public/0` | 9.40 | 10.47 | 0.870x | 0/8 |
| `public/64` | 61.86 | 64.32 | 0.988x | 1/8 |
| `public/128` | 136.66 | 85.54 | 1.620x | 8/8 |
| `public/4096` | 4,096.76 | 1,771.00 | 2.288x | 8/8 |
| `public/65025` | 67,736.35 | 28,740.78 | 2.328x | 8/8 |
| `public/1048576` | 1,100,024.28 | 470,978.43 | 2.330x | 8/8 |
| `seeded-raw/4096` | 1,793.73 | 1,918.91 | 0.997x | 1/8 |
| `seeded-parse/17` | 56.85 | 58.39 | 0.996x | 2/8 |
| `seeded-parse/255` | 310.60 | 167.92 | 1.751x | 8/8 |
| `seeded-parse/4096` | 4,170.49 | 1,846.89 | 2.225x | 8/8 |
| `seeded-parse/65025` | 71,515.84 | 32,741.50 | 2.309x | 8/8 |
| `seeded-reader/4096` | 6,077.41 | 3,474.26 | 1.733x | 8/8 |
| `seeded-reader/65025` | 83,077.33 | 45,844.62 | 1.897x | 8/8 |
| `seeded-writer/4096` | 38,728.00 | 19,000.44 | 2.057x | 8/8 |
| `seeded-writer/65025` | 577,614.00 | 280,740.75 | 2.113x | 8/8 |
| `four/128` | 86.28 | 67.67 | 1.290x | 8/8 |
| `four/4096` | 1,773.58 | 514.45 | 3.559x | 8/8 |
| `four/65025` | 29,049.44 | 7,778.56 | 3.633x | 8/8 |
| `four/1048576` | 478,873.50 | 130,458.50 | 3.734x | 8/8 |
| `four-reader/4096` | 3,704.07 | 2,246.45 | 1.642x | 8/8 |
| `four-reader/65025` | 43,711.36 | 19,693.28 | 2.165x | 8/8 |
| `four-writer/4096` | 19,988.25 | 7,284.62 | 2.547x | 8/8 |
| `four-writer/65025` | 305,928.25 | 115,292.25 | 2.748x | 8/8 |

| Full-worker workload | Baseline cycles | Public CRC | Seeded updates | Four states | Combined CPU throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| `crc/64` | 70.90 | 73.53 | 73.31 | 73.31 | 0.967x |
| `crc/256` | 260.11 | 149.70 | 146.41 | 83.63 | 3.110x |
| `crc/4096` | 4,130.66 | 2,071.64 | 2,069.66 | 586.50 | 7.043x |
| `crc/65025` | 66,509.00 | 34,023.00 | 34,023.00 | 10,316.00 | 6.447x |
| `crc/1048576` | 1,051,405.00 | 523,946.50 | 523,617.00 | 145,419.00 | 7.230x |
| `crc-boundary/127/15` | 200.15 | 201.47 | 203.17 | 198.38 | 1.009x |
| `crc-boundary/128/15` | 146.82 | 119.34 | 122.70 | 98.11 | 1.497x |
| `crc-boundary/129/15` | 158.30 | 125.10 | 128.06 | 98.75 | 1.603x |
| `crc-boundary/4096/15` | 4,204.80 | 2,096.39 | 2,095.97 | 608.99 | 6.905x |
| `parse/65025` | 75,171.07 | 74,136.12 | 33,038.04 | 9,164.12 | 8.203x |
| `span-index/130050` | 8,137,523.50 | 8,286,892.50 | 6,913,481.50 | 6,950,248.50 | 1.171x |
| `writer/8192` | 78,344.36 | 77,668.71 | 34,403.18 | 10,546.30 | 7.429x |
| `writer/130050` | 1,259,470.42 | 1,225,965.83 | 573,690.70 | 195,142.36 | 6.454x |
| `reader/8192` | 23,950,194.00 | 24,123,817.00 | 12,789,058.50 | 5,510,217.50 | 4.347x |
| `demux/8192` | 25,100,924.50 | 25,117,824.50 | 13,287,543.50 | 5,995,751.00 | 4.186x |

All six heap counters are identical across every stage and all 246 workloads:
allocations, reallocations, frees, cumulative allocated/freed bytes and peak
live bytes. The 16 KiB scalar tables remain available; SIMD constants and
register/local-array state add no heap or per-object storage. Every measured
first-call operation below also has zero Rust allocation counters.

The largest slower controls remain below. In the paired raw-CRC control, an
empty input costs about one extra cycle; dispatch is deliberately avoided below
128 bytes, but the length test still has a cost. Compiler/layout effects and
shared-host activity are not separated by the full worker.

| Slower control | Baseline cycles | Final cycles | CPU cost increase |
| --- | ---: | ---: | ---: |
| `fragment-prefix/1/true` | 8.21 | 9.26 | 12.8% |
| `chained-index/0` | 506,331.62 | 561,590.62 | 10.9% |
| `decoder-header/255/27` | 426.16 | 464.24 | 8.9% |
| `stream/64/first` | 2.25 | 2.41 | 7.3% |
| `crc-boundary/63/0` | 92.73 | 99.44 | 7.2% |
| `fragment-prefix/2/false` | 10.98 | 11.78 | 7.2% |
| `paused-capture/1` | 235,509.75 | 251,821.31 | 6.9% |
| `span-demux/64` | 377,464.58 | 403,314.06 | 6.8% |
| `fragment-prefix/3/false` | 11.96 | 12.75 | 6.6% |
| `fragment-prefix/5/true` | 14.11 | 15.04 | 6.6% |
| `batched-capture/1` | 86,647.68 | 92,231.12 | 6.4% |
| `runtime-stats-demux/0` | 143,470.69 | 151,537.25 | 5.6% |

### First-call cost

The [first-call driver](validation/2026-10-29-clmul-cold.rs) runs one CRC in a
fresh process, then 64 warm CRCs. Eight processes per stage and size produce
[96 observations](validation/2026-10-29-clmul-cold.json). Input generation, process
startup and file IO are outside the timer. First-call cycles include the
allocation-counter scope and its overhead; they are not isolated CPU-detection
latency. This also captures first-use code/data and runtime feature detection.
All versions verify the result against the baseline after timing.

| Bytes | Baseline first cycles | Final first cycles | Baseline warm cycles | Final warm cycles |
| ---: | ---: | ---: | ---: | ---: |
| 128 | 47,192.50 | 66,289.00 | 156.05 | 89.17 |
| 4,096 | 48,838.50 | 66,399.00 | 4,691.81 | 613.92 |
| 65,025 | 109,640.00 | 74,300.50 | 75,387.97 | 8,855.45 |

First-call cost is higher for 128-byte and 4 KiB inputs. The feature check is
amortized across subsequent calls; the headline throughput results are warm
resident-input measurements, not a startup-latency claim.

### Repository Ogg assets

The [corpus driver](validation/2026-10-29-clmul-corpus.rs) loads three encoded
assets outside timing, then constructs a strict page reader/demuxer, consumes
all packets, finishes and cleans up. It uses two whole-file operations per
batch, twelve alternating batches and eight processes, yielding
[72 paired observations](validation/2026-10-29-clmul-corpus.json). File hashes,
byte lengths and matching packet/payload-byte totals are pinned. No decoding,
file loading or sample conversion is timed.

| Asset | Bytes | Packets | Baseline/public control | Seeded CPU throughput | Four-state CPU throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| `assets/music/SRPG10-GameOver.ogg` | 7,701,390 | 8,597 | 0.999x | 1.716x | 1.978x |
| `assets/music/in_two (loop).ogg` | 4,939,014 | 10,352 | 1.008x | 1.688x | 1.903x |
| `assets/sounds/start.ogg` | 6,744 | 20 | 1.000x | 1.443x | 1.404x |

The two music files gain 1.69 to 1.72x from seeded routing and a further
1.90 to 1.98x from four-state folding, winning all eight processes at both stages.
The public-only CRC change leaves this packet path nearly unchanged, confirming
why the seeded integration matters. Packet-content correctness is covered by
the reference tests and complete protocol traces below; this performance driver
checks packet counts and payload-byte totals across all four versions.


## Correctness and reproduction

Release validation passes **60 Ogg tests and 113 Vorbis tests (87 ignored)**,
Clippy with warnings denied, Cargo/rustfmt checks and Python compilation.
The new native hardware test compares hardware and retained scalar output with
an independent bitwise polynomial in **65,824 seed/alignment/tail/split cases**,
including 1 MiB buffers. Native test output confirms that the hardware path was
exercised. The [checks](validation/2026-10-29-clmul-checks.json) pin source/test
hashes, commands, complete native check logs and a clean libogg reference.

All four measured stages pass each of six
[complete protocol comparisons](validation/2026-10-29-clmul-protocol.json).
These compare bytes, errors, progress, callbacks, Debug output, positions,
finish/reset/reuse, partial IO and failures. Layout/probe diagnostics also match.

| Protocol | Identical bytes per stage |
| --- | ---: |
| Index and packet origins | 3,649,232 |
| Incremental decoder | 5,808,523 |
| Index lookup | 3,800,798 |
| Capture state | 78,020,534 |
| Packet writer | 46,897,193 |
| Public page encoding and IO | 26,967,237 |

Use the committed benchmark controls for all revisions:

```powershell
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-clmul-replay --capture before --source-revision 3a4fe719d
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-clmul-replay --capture public --source-revision fff5f20f2
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-clmul-replay --capture seeded --source-revision 5d2fc6ebc
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-clmul-replay --capture four --source-revision da995922b
python crates/deadlib-audio/decode/scripts/benchmark_ogg_kernels.py --root target/ogg-clmul-replay --stage before --stage public --stage seeded --stage four --runs 8 --output target/ogg-clmul-replay/results.tsv
```

Paired, corpus and first-call records retain exact rustc commands and linked
library hashes. Miri feature flags and wrapper sources are in its record. The
audit checks every retained commit against its measured source, the untouched
scalar implementation, the unsafe boundary, test execution and every summary.
These are crate-local measurements; this experimental crate is not integrated
into DeadSync's audio decoding pipeline.
