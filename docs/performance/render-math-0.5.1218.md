# Rendering hot-path performance, 0.5.1218

Baseline: `d77a218a1` / 0.5.1217. Following the supplied guide's M-HOTPATH,
M-MEM-REUSE and M-THROUGHPUT recommendations, this pass measures repeated
note visibility and text-width work, reuses existing cached data, and reduces
CPU work without adding allocations or retained storage.

## Three optimizations

1. **Cache Hidden-only endpoint alpha.** Note samples below or above the Hidden
   fade interval now return the cached endpoint alpha, skipping intensity
   multiplication and both clamps. Finite-bound and nondegenerate checks move
   to cache construction. Interior arithmetic is unchanged; degenerate or
   nonfinite bounds keep the original fallback.
2. **Cache Sudden-only endpoint alpha.** Sudden uses its own endpoint results
   and degenerate-interval behavior. Its finite-bound checks also move to cache
   construction, and samples outside the fade interval skip repeated intensity
   and clamp work. Each single-effect cache excludes inactive modifiers,
   even if their values are negative, tiny or NaN.
3. **Measure ASCII widths through the existing glyph table.** ASCII text uses
   byte iteration and cached integer advances, avoiding UTF-8 decoding and
   general glyph lookup per character. Empty strings return zero immediately.
   Unicode text keeps the existing fallback-chain lookup. The ASCII table
   already resolves fallback/default glyphs; no additional advance table or
   invalidation mechanism is introduced.

Visibility runs for notes and hold-mesh samples, including the calls in
`crates/deadsync-notefield/src/holds.rs`. Logical text widths are used in the
Simply Love theme's gameplay HUD, song selection, options and other layouts.

Single and combined fades share the existing two endpoint-alpha fields,
renamed to reflect their expanded use. Cache size is unchanged. Combined-effect
arithmetic, fade boundaries, floating-point operation order inside the fade,
actor alpha and glow calculations are preserved. Font fields, dependencies,
production allocator and unsafe code are unchanged. These paths already had
zero heap traffic; this pass makes no allocation-count reduction claim.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), `x86_64-pc-windows-msvc`, release with full LTO.
No extra target-CPU flags. All five benchmark processes inherit affinity to
logical CPU 4. No build or other test suite runs alongside them.

The old visibility implementation and font lookup/measurement functions are
frozen in `crates/deadsync-notefield/tests/render_math/`. Their source and inline
attributes were checked against the baseline commit. The integration harness
includes the production transform source and calls the public font function,
compiling old and new code into one executable with identical settings.

Five independent invocations alternate old/new order (new first on runs 2 and
4). The existing `tests/support/perf.rs` harness performs three warmups, seven
timed batches and a separate allocation-counted operation. The table reports
medians of the five invocation medians. Windows `QueryThreadCycleTime` counts
the synchronous calling thread's CPU cycles. Throughput derives from wall time;
all timings include loop and benchmark barriers. Inputs and outputs are passed
through `black_box`.

A visibility operation evaluates 512 positions, with 1,024 operations per
batch. Positions follow `y0 + (i % 127) / 127 * span`. Hidden's fade fixture
starts at 120 with span 39.9; Sudden's starts at 160 with the same span. The
wide fixtures start at zero with span 320; outside fixtures start at 320 with
span 320. Combined and general fixtures start at 80 with span 160. Single and
combined fades use intensity 1. General uses Hidden 0.5, Sudden 0.75, Stealth
0.2, Blink 0.5 and RandomVanish 0.1. Identity and Stealth-only are controls.
Elapsed time is 1.25, Mini and offsets are zero. Cache creation occurs outside
visibility measurements and is measured separately: 32 complete constructions
per operation, 4,096 operations per batch.

A width operation measures the same string 32 times, with 2,048 operations per
batch. Fixtures include an empty string, one character, `Score 100.00%`, 64/1,024
ASCII characters, repeated Unicode text and 1,024 ASCII characters followed by
a CJK character. Three fonts split ASCII glyphs across a fallback chain, with
negative/positive advances, Unicode glyphs and a default glyph. Font loading,
ASCII table refresh and string construction occur outside measurement.

## Results

Times and cycles are per operation. Throughput is millions of visibility
queries, cache constructions or width queries per second, as appropriate.
Negative "fewer cycles" values mean more measured CPU work. The
[raw CSV](render-math-0.5.1218.csv) contains all 220 rows, including ranges and
allocation counters.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | M units/s old -> new |
|---|---:|---:|---:|---:|
| `cache/hidden-fade` | 1,295.2 -> 1,378.5 | 2,840.0 -> 3,020.7 | -6.4% | 24.706 -> 23.214 |
| `visibility/hidden-fade` | 3,394.1 -> 3,132.7 | 7,439.8 -> 6,871.2 | 7.6% | 150.848 -> 163.437 |
| `cache/sudden-fade` | 1,318.6 -> 1,319.0 | 2,873.4 -> 2,890.7 | -0.6% | 24.268 -> 24.261 |
| `visibility/sudden-fade` | 3,576.6 -> 2,914.0 | 7,844.1 -> 6,389.7 | 18.5% | 143.154 -> 175.706 |
| `visibility/hidden-wide` | 3,364.3 -> 1,697.9 | 7,376.6 -> 3,724.9 | 49.5% | 152.188 -> 301.558 |
| `visibility/sudden-wide` | 3,653.0 -> 1,574.5 | 8,004.2 -> 3,454.3 | 56.8% | 140.158 -> 325.180 |
| `cache/combined` | 1,319.1 -> 1,372.6 | 2,892.2 -> 3,010.1 | -4.1% | 24.259 -> 23.313 |
| `visibility/combined` | 3,869.4 -> 3,871.4 | 8,473.5 -> 8,489.8 | -0.2% | 132.319 -> 132.252 |
| `cache/general` | 1,791.7 -> 1,868.2 | 3,910.0 -> 4,077.8 | -4.3% | 17.860 -> 17.128 |
| `visibility/general` | 5,626.2 -> 5,352.7 | 12,317.3 -> 11,737.2 | 4.7% | 91.003 -> 95.652 |
| `cache/identity` | 299.0 -> 308.7 | 648.8 -> 677.1 | -4.4% | 107.033 -> 103.647 |
| `visibility/identity` | 672.3 -> 611.8 | 1,477.3 -> 1,341.4 | 9.2% | 761.604 -> 836.852 |
| `visibility/stealth` | 1,272.1 -> 1,232.3 | 2,793.9 -> 2,700.0 | 3.4% | 402.493 -> 415.475 |
| `visibility/hidden-outside` | 3,079.8 -> 1,261.5 | 6,758.4 -> 2,770.3 | 59.0% | 166.245 -> 405.858 |
| `visibility/sudden-outside` | 3,446.6 -> 1,317.6 | 7,560.7 -> 2,893.8 | 61.7% | 148.553 -> 388.592 |
| `width/empty` | 64.0 -> 52.9 | 141.4 -> 116.9 | 17.3% | 500.275 -> 604.576 |
| `width/single` | 250.5 -> 146.9 | 550.8 -> 323.2 | 41.3% | 127.726 -> 217.872 |
| `width/label` | 2,276.0 -> 702.5 | 4,990.4 -> 1,541.3 | 69.1% | 14.060 -> 45.549 |
| `width/ascii-64` | 11,381.7 -> 2,733.1 | 24,937.7 -> 5,985.2 | 76.0% | 2.812 -> 11.709 |
| `width/ascii-1024` | 179,064.4 -> 37,669.2 | 392,142.1 -> 82,544.9 | 79.0% | 0.179 -> 0.849 |
| `width/unicode` | 40,755.2 -> 41,168.5 | 89,252.4 -> 90,232.9 | -1.1% | 0.785 -> 0.777 |
| `width/mixed-long` | 181,252.0 -> 180,827.1 | 397,221.7 -> 396,258.5 | 0.2% | 0.177 -> 0.177 |

Hidden and Sudden across the wider position range use **49.5% and 56.8% fewer
cycles**, respectively. Fully outside their fade intervals, they use **59.0%
and 61.7% fewer**. Interior fade queries improve by **7.6% and 18.5%** after
moving interval-validity checks to cache construction. ASCII label widths use
**69.1% fewer cycles** and increase from **14.060 to 45.549 million queries/s**;
64- and 1,024-character ASCII widths use **76.0% and 79.0% fewer cycles**.

Cache construction measures **0.6-6.4% more cycles** depending on the fixture.
Hidden cache setup increases from 1,295.2 to 1,378.5 ns per 32 constructions,
about **2.6 ns per cache**. This is a setup-versus-repeated-query tradeoff;
cache creation without repeated visibility queries is not an improvement.
Combined visibility is effectively flat (**0.2% more cycles**). Unicode width
uses **1.1% more cycles**, while mixed-long text is essentially flat. Identity,
Stealth and general-query controls show small improvements, but their query
logic is unchanged and those differences are not attributed to an avoided
operation. The raw sample ranges retain the observed timing variability.

**All 220 old/new measurements report zero allocations, reallocations, frees,
requested bytes and freed bytes per operation.** The regression suite also
asserts zero churn for repeated alpha/glow and ASCII/Unicode width queries,
and equal old/new appearance-cache sizes. These measurements cover heap
traffic after fixture setup, not process RSS or font-loading allocations.

These are isolated CPU benchmarks, not end-to-end frame-rate measurements.
Visibility savings depend on the share of samples outside a single-effect
fade interval; ASCII savings depend on text content and length. Different
compilers, CPUs and workload mixes can change the result.

## Validation and reproduction

Seven new regression tests compare old and new behavior:

- All 32 combinations of Hidden, Sudden, Stealth, Blink and RandomVanish;
  Mini and offset changes, exact fade boundaries and adjacent floats.
- Nonfinite and subnormal values, signed zero, degenerate intervals, plus
  2,000 deterministic cases generated from arbitrary f32 bit patterns.
- Single-effect endpoint values with fractional, excessive and infinite
  intensity, and negative/tiny/nonfinite inactive modifiers.
- Every ASCII code, control characters, Unicode, missing glyphs, fallback
  mutations, default changes and an explicitly overridden ASCII cache entry.
- 500 deterministic mixed-Unicode strings with signed glyph advances.
- Integer-width overflow: matching debug panics and release wrapping results.
- Allocation-free repeated queries and unchanged appearance-cache size.

Non-NaN floating-point outputs, including signed zero, are compared bit for
bit; NaNs are compared by classification. Alpha, actor alpha and glow all
match. The seven tests pass in debug and release. Broader validation passes
1,812 rendering/notefield/theme unit tests (163 + 370 + 1,279); five existing
manual tests are ignored. The locked application check, performance-focused
Clippy and scoped rustfmt check pass. Theme tests emit the existing unused
`song_lua_overlay_camera_state` warning; Clippy reports no warnings.

```powershell
cargo test -p deadlib-present -p deadsync-notefield -p deadsync-theme-simply-love --lib --locked -- --test-threads=1
cargo test -p deadsync-notefield --test render_math --locked -- --test-threads=1
cargo test -p deadsync-notefield --release --test render_math --locked -- --test-threads=1
cargo check -p deadsync --locked
cargo clippy -p deadlib-present -p deadsync-notefield --lib --tests --locked --no-deps -- -A clippy::all -W clippy::perf
```

After building, invoke the release executable five times. The following uses
the measurement host's logical CPU 4 (choose an allowed CPU on other hosts):

```powershell
$benchmark = Get-Item target/release/deps/render_math-*.exe
$process = Get-Process -Id $PID
$originalAffinity = $process.ProcessorAffinity
try {
    $process.ProcessorAffinity = [IntPtr]16
    foreach ($run in 1..5) {
        if ($run % 2 -eq 0) { $env:DEADSYNC_PERF_REVERSE = '1' }
        else { Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue }
        & $benchmark.FullName --ignored --exact benchmark_render_math --nocapture --test-threads=1
    }
} finally {
    $process.ProcessorAffinity = $originalAffinity
    Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
}
```
