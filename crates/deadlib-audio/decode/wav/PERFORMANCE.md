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

Next measured opportunity: borrowed native reads currently loop over endian
loads, while writer and file-backed reads can use a bulk copy. Measure a native
byte-copy specialization with unaligned/swapped/float-bit parity before keeping it.

Outstanding coverage: PCM8/24/32 and floats, ADPCM, large metadata inventories,
real-file cold/warm startup and streaming, nonseekable input, realistic and large
corpora, Symphonia and other Rust WAV libraries, dr_wav/libsndfile performance,
multiple architectures, peak RSS, fuzz/optimization parity, and broad confidence
intervals. These results do not establish superiority across those unmeasured cases.
