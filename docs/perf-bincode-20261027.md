# Bincode primitive performance pass

Baseline: `bfceaf7ea391` (the starting checkout). Scope: `deadlib-bincode`.
Performance guidance: `rust-performance.md` from the other DeadSync checkout,
`C:/Github/rework/deadsync`; that ignored file was absent from this checkout.

## Retained changes

1. Replace four signed-encoding sign branches with unsigned left shift XOR
   arithmetic sign shift. This preserves ZigZag bytes, including signed minima,
   without overflow in debug builds. The `isize` forwarding path also benefits.
2. Allow the small `u16` and `u32` varint decoder functions to inline. Their
   bodies, reader calls, error values, and limit accounting are unchanged.
3. Validate Boolean bytes in 256-byte blocks, retaining 64-byte blocks and the
   ordered eight-byte/tail scan. Successful large inputs need one block branch
   per 256 bytes instead of per 64 bytes. Errors still report the first invalid
   byte and retain exactly the same output prefix and reader position.

Production diff: **20 added, 49 removed lines (29 fewer)**. No new unsafe code,
dependencies, API, wire formats, or runtime CPU-feature requirements.

## Measurements

Windows 11, Intel Core i7-1250U, `rustc 1.97.1`, x86_64 MSVC, release `opt-level=3`,
LTO enabled, repository static-CRT flags. Both revisions are compiled into one
binary. The benchmark pins its thread to logical CPU 2 and alternates variant
order. `QueryThreadCycleTime` measures thread CPU cycles; wall time is secondary.
Each run reports the median of eleven batches after one warmup batch; three
fixed runs were collected after our validation/build jobs finished. Other host
activity was left running. Initial exploratory results were excluded.

The table gives medians of per-run cycle counts and medians of the paired
throughput ratios. Thus a ratio can differ from dividing the two displayed
medians. Integer vectors have 4,096 elements. `small` signed i32 values span
-120..120; `mixed` values span -30,000..30,000. Unsigned small values span 0..240;
wide inputs use deterministic pseudorandom bits.

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| i32 small encode | 5,836.52 | 4,864.97 | 1.200x | 1.114–1.200x |
| i32 mixed encode | 8,349.18 | 7,471.10 | 1.118x | 1.117–1.125x |
| u16 small decode | 12,166.75 | 7,281.46 | 1.671x | 1.669–1.722x |
| u16 wide decode | 12,304.14 | 7,737.65 | 1.590x | 1.586–1.590x |
| i32 mixed decode | 18,985.36 | 17,400.72 | 1.088x | 1.086–1.092x |
| 4,096 valid Booleans | 194.60 | 173.63 | 1.121x | 1.119–1.127x |
| 65,536 valid Booleans | 4,889.06 | 4,424.58 | 1.107x | 1.105–1.111x |
| 4,096 Booleans, first invalid | 13.80 | 17.89 | 0.771x | 0.767–0.771x |
| 65,536 Booleans, first invalid | 14.12 | 19.57 | 0.779x | 0.722–0.784x |

The Boolean tradeoff is deliberate: early errors in large buffers cost roughly
4–6 additional cycles, while valid large vectors and late errors improve. Short
late-error controls also regress: 64 bytes, 20.93 -> 25.37 cycles (0.825x paired);
128 bytes, 26.82 -> 28.64 cycles (0.906x paired). Short valid 8/64-byte controls
are at parity. Endian controls are at parity. No whole-game speedup is claimed.

All 55 workloads assert **zero allocations, reallocations, frees, and allocated
bytes** for both implementations after warmup. This pass improves CPU work;
it does not claim an allocation reduction. All 165 paired observations, including
slower/unchanged controls and source SHA-256 digests, are in
[`perf-bincode-20261027-results.json`](perf-bincode-20261027-results.json).

## Reproduction and correctness

```powershell
py scripts/benchmark_bincode.py --before bfceaf7ea391 --runs 3 --output target/bincode-perf/results.json
```

The script archives the selected codec revision under `target`, links the current
source directly, and uses the same derive implementation for both primitive
benchmarks. It leaves the game workspace and its lockfile unchanged. Allocation
fixtures and input generation are outside timing; output checks use real codec
APIs. The runner is Windows-specific because the shared cycle counter uses the
Windows thread API.

Validation used a temporary standalone manifest under `target/bincode-perf` to
avoid resolving unrelated game dependencies. It points directly to the production
`src/lib.rs`, the unchanged derive crate, and **all six integration test files**;
it does not copy the implementation. Both debug and release passed **98 tests**
each (78 unit/integration and 20 documentation tests), with three benchmarks
ignored. Clippy passed with `--all-targets -- -D warnings`; changed Rust files
passed rustfmt, and `git diff --check` passed.

```powershell
cargo test --manifest-path target/bincode-perf/Cargo.toml --offline --release
cargo test --manifest-path target/bincode-perf/Cargo.toml --offline
cargo clippy --manifest-path target/bincode-perf/Cargo.toml --offline --all-targets -- -D warnings
```

The existing tests cover golden bincode 2 bytes, both endiannesses, fixed/variable
integer configurations, limits, truncation, non-peeking readers, output reuse,
and error prefixes. Added tests exhaust every i16 value against the independent
arithmetic wire definition, cover deterministic wide signed values, validate
Boolean arrays around 256/512-byte boundaries, and inject competing Boolean
errors at every position across three large blocks while checking reader state.

## Discarded trials

- Endian decoding through safe `Vec::extend`: short-vector throughput fell to
  0.26–0.53x and large vectors did not improve.
- Endian decoding into spare capacity: short u32 throughput fell to about 0.61x
  and large vectors stayed at parity.
- Branchless signed decoding: CPU results stayed at parity; retain the original.
- Inlining larger varint decoders: the broader width sweep caught significant
  u64/u128 regressions. Only the small u16/u32 hints remain.
- Inlining unsigned encoding: no repeatable benefit beyond signed arithmetic;
  those hints were removed.
- 128-byte Boolean blocks without the 64-byte tail path: valid 64-byte vectors
  regressed to 0.69x; the retained layout keeps that tail path.

Each candidate was compared with the original before further changes. The
retained optimizations affect separate encoding, decoding, and Boolean scan
paths, and the final combined benchmark rechecks all three plus controls.
