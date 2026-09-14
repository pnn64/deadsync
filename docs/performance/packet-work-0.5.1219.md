# Audio packet performance, 0.5.1219

Baseline: `c5fb50443` / 0.5.1218. This pass follows the supplied guide's
M-HOTPATH, M-MEM-REUSE and M-THROUGHPUT guidance: measure packet processing,
keep reusable output buffers, and remove repeated indexing, redundant stores
and redundant numeric operations.

## Three optimizations

1. **Direct mono resampler output.** A one-channel destination now converts
   the first source channel directly into the retained output vector. It
   avoids per-sample channel wrapping and generic interleaved indexing, and
   avoids initializing newly exposed output elements before overwriting them.
   The shortest input channel still limits the frame count, including unused
   input channels. The append loop is kept in a separate function so its
   buffer-growth code stays outside the existing stereo conversion body.
   Existing stereo fast paths and other channel mappings remain.
2. **Process fade envelopes by complete frame slices.** Channel iteration uses
   each complete mutable frame instead of reconstructing and checking every
   sample index. Float-to-i16 casts supply the final saturation, removing the
   explicit post-round clamp. Fully silent packets clear all complete frames
   together; incomplete trailing samples remain untouched. Fade interpolation,
   rounding, full-volume bypass and timestamp saturation retain their order.
3. **Write converted floating-point WAV samples directly.** Float32 and Float64
   packets now append initialized samples into the existing vector instead of
   resizing with zeroes and then replacing them. The conversion also relies on
   Rust's saturating cast instead of an explicit post-round clamp. Nonfinite
   source samples still produce zero; finite values whose scaled product
   overflows still saturate. PCM paths, little-endian interpretation, malformed
   packet errors and output mutation on failure remain unchanged.

The callers are the music decoder worker (`MusicStages::pull`), the sound-effect
resampling path in `deadlib-audio/src/stream.rs`, packet fade handling in that
worker, and the WAV reader shared by streaming and sound-effect loading. Mono
conversion savings apply to mono destinations. Fade work applies while a fade
is active. WAV gains apply to floating-point WAV input, not compressed codecs.

There are no new buffers, cached fields, dependencies, production allocator
changes or unsafe operations. Retained packet processing was already free of
heap churn, and remains so. The memory improvement is reduced redundant
writes, not a claim of fewer allocations or lower process memory.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0 (`88d9e12ae`,
LLVM 22.1.8), `x86_64-pc-windows-msvc`, release with full LTO. No additional
CPU-feature flags. Benchmark processes inherit affinity to logical CPU 4.
No compilation or other test suite ran alongside the final measurements.

The frozen resample source and WAV decoding helpers in
`crates/deadlib-audio-decode/tests/packet_work/` were checked against the
baseline commit, including inline attributes. The harness is a test module of
the production WAV reader and calls the real private decoder and public
resample functions. Old and new code run in the same executable through opaque
function pointers with input/output barriers.

Five independent invocations alternate variant order: new first on runs 2 and
4. The shared `tests/support/perf.rs` harness runs three warmups, seven timing
batches and a separate allocation-counted operation. Results below are medians
of the five invocation medians. Windows `QueryThreadCycleTime` reports the
synchronous calling thread's CPU cycles; throughput derives from wall time.
Loop, dispatch and barrier overhead is included.

- Resampler operations convert one packet of 16, 256 or 4,096 frames. Each
  timing batch has 4,096 operations. Inputs are deterministic planar floats,
  with distinct per-channel data; output storage is preallocated and retained.
- Fade operations reset a retained buffer from deterministic i16 input and
  process 512 complete frames with 1, 2 or 6 channels. Each batch has 2,048
  operations. The reset copy is included. Fades span frames 0 to 4,096, in both
  directions. Silent and unchanged full-volume packets are explicit controls.
- WAV operations decode 8,192 samples with 512 operations per batch. `1`, `4`
  and `5` in the workload names mean PCM16, Float32 and Float64. `warm` retains
  the full output length; `regrow` truncates to one sample before each call
  without dropping capacity; `cold` creates and drops a new output vector for
  every call. Source-byte generation and behavior comparisons are untimed.

## Results

Times and cycles are per packet operation. Throughput is millions of output
samples per second. Negative "fewer cycles" values mean more measured CPU
work. The [raw CSV](packet-work-0.5.1219.csv) retains all 230 rows with timing
ranges, allocation/reallocation/free counts, and allocated/freed bytes.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | M units/s old -> new |
|---|---:|---:|---:|---:|
| `resample/1to1-16` | 165.7 -> 66.4 | 364.5 -> 146.2 | 59.9% | 96.547 -> 240.941 |
| `resample/1to1-256` | 2,232.1 -> 909.2 | 4,892.6 -> 1,993.3 | 59.3% | 114.691 -> 281.557 |
| `resample/1to1-4096` | 36,545.6 -> 13,883.6 | 80,028.2 -> 30,424.5 | 62.0% | 112.079 -> 295.024 |
| `resample/2to1-256` | 2,183.2 -> 888.9 | 4,787.5 -> 1,949.4 | 59.3% | 117.262 -> 287.983 |
| `resample/6to1-256` | 2,274.1 -> 926.2 | 4,986.4 -> 2,031.2 | 59.3% | 112.570 -> 276.407 |
| `resample/1to2-256` | 893.3 -> 889.8 | 1,959.6 -> 1,951.4 | 0.4% | 573.133 -> 575.429 |
| `resample/2to2-256` | 1,684.8 -> 1,653.1 | 3,676.2 -> 3,625.8 | 1.4% | 303.895 -> 309.712 |
| `resample/6to6-256` | 12,588.4 -> 12,601.4 | 27,580.6 -> 27,562.0 | 0.1% | 122.018 -> 121.891 |
| `fade/stereo-fade` | 7,627.6 -> 6,523.0 | 16,691.3 -> 14,285.6 | 14.4% | 134.249 -> 156.982 |
| `fade/mono-fade` | 5,160.8 -> 3,966.7 | 11,310.7 -> 8,701.0 | 23.1% | 99.209 -> 129.076 |
| `fade/surround-fade` | 15,662.5 -> 14,207.9 | 34,246.2 -> 31,089.8 | 9.2% | 196.138 -> 216.218 |
| `fade/silent` | 6,793.8 -> 78.8 | 14,901.1 -> 173.6 | 98.8% | 150.725 -> 13001.562 |
| `fade/unity` | 43.0 -> 53.3 | 95.0 -> 117.8 | -24.0% | 23831.273 -> 19204.689 |
| `fade/fade-in` | 7,521.3 -> 6,409.3 | 16,463.7 -> 14,053.7 | 14.6% | 136.147 -> 159.768 |
| `wav/1-warm` | 504.1 -> 582.0 | 1,109.5 -> 1,281.0 | -15.5% | 16250.694 -> 14074.846 |
| `wav/1-regrow` | 504.3 -> 519.5 | 1,110.4 -> 1,131.4 | -1.9% | 16244.400 -> 15768.060 |
| `wav/1-cold` | 906.4 -> 800.8 | 1,993.1 -> 1,760.3 | 11.7% | 9037.501 -> 10230.010 |
| `wav/4-warm` | 34,837.3 -> 28,646.9 | 76,370.1 -> 62,657.4 | 18.0% | 235.150 -> 285.965 |
| `wav/4-regrow` | 32,783.8 -> 29,716.4 | 71,724.2 -> 65,143.5 | 9.2% | 249.880 -> 275.673 |
| `wav/4-cold` | 35,068.4 -> 29,012.5 | 76,879.0 -> 63,594.1 | 17.3% | 233.601 -> 282.361 |
| `wav/5-warm` | 29,263.3 -> 26,804.5 | 64,097.4 -> 58,755.3 | 8.3% | 279.941 -> 305.620 |
| `wav/5-regrow` | 31,792.8 -> 26,357.8 | 69,679.2 -> 57,782.1 | 17.1% | 257.669 -> 310.800 |
| `wav/5-cold` | 30,303.5 -> 26,659.4 | 66,244.4 -> 58,450.9 | 11.8% | 270.332 -> 307.284 |

Mono output uses **59.3% fewer cycles** for a 256-frame packet, increasing
throughput from **114.691 to 281.557 million samples/s**. Across the mono
fixtures the reduction is 59.3-62.0%. The stereo and surround conversion controls
are effectively flat (0.1-1.4% fewer cycles); their algorithms are unchanged.

Active stereo fades use **14.4% fewer cycles**, with mono and surround fades
using **23.1% and 9.2% fewer**. Fade-in saves **14.6%**. Completely silent packets
save **98.8%**, since they need only the reset copy and bulk zero-fill. The
full-volume bypass control measures **24.0% more cycles**, an increase from
43.0 to 53.3 ns per packet, including its reset copy. That very short control
does not demonstrate a speedup.

Warm Float32 WAV decoding uses **18.0% fewer cycles**; Float64 uses **8.3%
fewer**. Regrowth saves **9.2%/17.1%**, and cold decoding including allocation
and destruction saves **17.3%/11.8%** for Float32/Float64 respectively. The
unchanged PCM16 warm/regrow controls measure **15.5%/1.9% more cycles** (77.9 ns
and 15.2 ns more wall time per 8,192-sample packet). PCM16 cold creation measures
11.7% fewer cycles. These small controls varied between runs;
no PCM decoding improvement is claimed. All per-run ranges are retained in
the CSV rather than treating every input as a win.

Every retained-buffer measurement reports **zero allocations, reallocations,
frees, requested bytes and freed bytes**. Every cold WAV measurement reports
**one allocation, one free, zero reallocations and 16,384 allocated/freed
bytes**, identically for old and new. Buffer reuse tests also assert unchanged
capacity and allocation address.

For an 8,192-sample floating-point WAV packet, the old cold path initialized
16,384 output bytes before replacing them; regrowth from one sample initialized
16,382 bytes. Those redundant stores are removed. These byte counts are
calculated from the removed zero-fill, not measured hardware memory traffic.
The warm WAV path had no such zero-fill; its arithmetic change is measured
separately by the full-length warm fixture. Heap counters exclude fixture
setup and do not measure process RSS.

These synthetic packet benchmarks do not establish whole-game FPS gains,
end-to-end decoder speedups or callback deadline improvements. Results depend
on channel layout, sample format, packet size, compiler and CPU. The fully
silent fade fixture is a specific shortcut, not the expected gain for normal
active fades.

## Validation and reproduction

Seven new regression tests compare exact integer output against the baseline:

- Mono/stereo/surround mapping, empty channels, truncated/unequal source
  lengths, requested frame limits, output shrink/growth and nonfinite floats.
- 300 deterministic resampler cases generated from arbitrary f32 bit patterns.
- Fade boundaries, incomplete trailing frames, zero channels, negative and
  extreme i64 fade endpoints, saturating u64 positions and every i16 value.
- 2,000 randomized fade packets and near-unity bypass thresholds.
- All six WAV encodings, arbitrary sample bytes, Float32/Float64 nonfinite and
  extreme values, malformed partial samples, and unchanged output on error.
- All i16 halfway boundaries and adjacent f64 inputs, also converted to Float32,
  confirming identical rounding and saturation.
- Allocation-free repeated resampling, fading and WAV regrowth, with stable
  output capacity and storage address.

The seven comparisons pass in debug and release. The full audio/decoder/stream
suite passes **78 tests**, including the playback integration test; five manual
benchmarks/captures are ignored. The locked application check, scoped rustfmt
check and performance-focused Clippy pass. Audio tests report an existing
unused `assert_reduced_churn` helper in shared test support.

```powershell
cargo test -p deadlib-audio-decode -p deadlib-audio -p deadsync-audio-stream --locked -- --test-threads=1
cargo test -p deadlib-audio-decode --release --lib --locked packet_work -- --test-threads=1
cargo check -p deadsync --locked
cargo clippy -p deadlib-audio-decode --lib --tests --locked --no-deps -- -A clippy::all -W clippy::perf
```

After building, repeat the benchmark executable five times. This example uses
logical CPU 4; choose an allowed CPU on other hosts:

```powershell
$benchmark = Get-Item target/release/deps/deadlib_audio_decode-*.exe
$process = Get-Process -Id $PID
$originalAffinity = $process.ProcessorAffinity
try {
    $process.ProcessorAffinity = [IntPtr]16
    foreach ($run in 1..5) {
        if ($run % 2 -eq 0) { $env:DEADSYNC_PERF_REVERSE = '1' }
        else { Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue }
        & $benchmark.FullName --ignored --exact wav::packet_work::benchmark_packet_work --nocapture --test-threads=1
    }
} finally {
    $process.ProcessorAffinity = $originalAffinity
    Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
}
```
