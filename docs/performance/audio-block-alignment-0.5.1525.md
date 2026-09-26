# Skip frame alignment for full music blocks

Baseline: `eaba07c81`.

`MusicBlockWriter::try_push` caps each transfer at 256 device frames. Previously,
it divided the capped sample count by the channel count and multiplied it back
on every call. A full block is already frame-aligned. It now uses that capacity
directly for full blocks and retains the original rounding for partial blocks.

This removes a runtime integer division from full-block transfers in the decoder
worker. The sample copy, queues, recycling, and timing values are unchanged.

## Behavior checks

The differential test compares the production implementation with a frozen copy
of the original `try_push`. It covers 37,458 pushes: every input length from zero
through two blocks plus one frame, for channel settings 0, 1, 2, 3, 4, 6, 8, 16,
and 32. Zero channels retains the constructor's normalization to one channel.
It checks accepted lengths, queued samples, timing bits, queue presence, and
outstanding-block counts. Timing inputs include arbitrary floating-point bit
patterns. All comparisons pass.

The comparison also passed before changing production code. Full release and
debug crate suites pass: 102 tests each, including existing saturation,
concurrent-transfer, callback-boundary, and generation-transition checks.
Clippy passes with performance lints denied; the existing `too_many_arguments`
warning in `telemetry.rs` remains.

```powershell
cargo test --locked --release -p deadlib-audio-core
cargo test --locked -p deadlib-audio-core
cargo clippy --locked -p deadlib-audio-core --lib -- -D clippy::perf
```

## Before/after measurements

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Three paired runs pinned to logical CPU 6, reversing
variant order in run 2. No builds or other tests from this pass ran during the
measurements. Both variants run in the same executable through a black-boxed
function pointer, retaining a runtime channel count for both implementations.

Each single-transfer operation includes push, sample copy, pop, and recycling.
Its seven timing batches contain 131,072 operations each. Full blocks contain
256 frames; larger inputs contain four blocks plus one sample; partial inputs
contain 137 frames. The partial-tail control adds `channels - 1` samples, which
must be left unconsumed. Empty inputs also exercise the empty pop.

Mixed-packet operations exercise the caller's chunking loop and timestamp
advancement. Decoder-sized packets contain 137, 256, 512, 1,152, and 4,096 frames
(6,153 frames total). Small packets contain 1, 7, 31, 137, 255, and 256 frames
(687 total). Each operation transfers every packet, recycling each block.
Seven timing batches contain 65,536 operations each. Initial shorter runs had
noisy small-packet results, so these longer runs determine the comparison.

Times below are medians of run medians. Cycle reductions and throughput gains
are medians of paired run ratios, so they need not equal ratios of the displayed
times. Positive cycle reductions mean less CPU work.

| Fixture | Before ns/op | After ns/op | Cycle reduction | Paired range |
| --- | ---: | ---: | ---: | ---: |
| Full block, mono | 47.1 | 47.0 | 0.1% | -0.5% to 4.9% |
| Full block, stereo | 64.0 | 60.2 | 5.8% | 5.5% to 6.8% |
| Full block, 6 channels | 207.0 | 204.6 | 1.2% | -1.3% to 2.9% |
| Full block, 8 channels | 264.4 | 263.5 | -0.5% | -1.4% to 3.6% |
| Partial block, stereo | 53.3 | 48.6 | 2.5% | 0.7% to 8.7% |
| Partial block with tail, stereo | 47.8 | 47.2 | 1.1% | -3.8% to 2.0% |
| Empty input, stereo | 12.0 | 12.4 | -1.9% | -5.4% to -1.4% |
| Decoder packets, mono | 1,348.3 | 1,271.4 | 3.6% | 3.4% to 5.9% |
| Decoder packets, stereo | 1,885.8 | 1,817.2 | 4.2% | 3.3% to 4.4% |
| Decoder packets, 6 channels | 5,775.0 | 5,717.3 | 0.8% | 0.7% to 1.0% |
| Small packets, mono | 279.7 | 277.8 | 1.4% | 0.5% to 3.9% |
| Small packets, stereo | 309.8 | 301.6 | -0.7% | -1.2% to 2.6% |
| Small packets, 6 channels | 584.1 | 584.3 | 1.0% | -0.8% to 3.8% |

Stereo full-block throughput rises 6.2%; decoder-packet throughput rises 3.8%
for mono and 4.4% for stereo. Gains become small relative to copying for wider
formats. No reliable gain is claimed for partial or small-packet controls.
Empty calls show a small added cost in several channel settings; the real
decoder caller only enters its push loop when input remains.

All cases retain zero allocations, reallocations, and frees per operation.
Accounting runs separately from timing, after transport and fixture setup.

These are transport microbenchmarks on one CPU/compiler, with both endpoints
on one thread. They do not measure decoding, a running output device, total
game CPU use, or audio latency. The absolute saving per full stereo transfer
is only a few nanoseconds. Cycle counts use `QueryThreadCycleTime`; clock and
harness overhead are included equally, not subtracted. The
[raw CSV](audio-block-alignment-0.5.1525.csv) contains all 186 final measurements,
including less favorable results, timing ranges, and allocation counts.
Throughput units are transfers for the single-transfer benchmark and samples
for mixed packets.

## Reproduce

The integration test includes the actual transport source and the frozen
baseline method. Build once, then run the test executable directly so no build
work overlaps measurements:

```powershell
cargo test --locked --release -p deadlib-audio-core --test block_alignment --no-run
(Get-Process -Id $PID).ProcessorAffinity = 64
# Use the executable path printed by Cargo:
# <executable> benchmark_block_alignment --ignored --nocapture --test-threads=1
# <executable> benchmark_packet_alignment --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Capture stderr directly to a file. Local raw logs, including preliminary runs,
remain in the ignored `target/block-alignment-pass` directory. The final CSV
uses `final1.txt` through `final3.txt` and `longpacket1.txt` through
`longpacket3.txt`.
