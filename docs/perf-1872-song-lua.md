# Song Lua compilation and fixture performance

Song preparation compiles Lua into Rust playback data before gameplay. The
performance tool calls that production preparation entry point and verifies
that it returns no retained Lua VM. Full-song fixtures continue to use their
diagnostic scene and existing native replay comparisons.

The changes reduce repeated work in both paths:

1. Live capture resets use keys interned for that VM. Compiler captures retain
   fresh block tables so earlier snapshots remain valid.
2. Batched actor-state reads retain their scalar keys instead of hashing and
   interning the same strings for every actor on every frame.
3. Environment reads resolve ordinary globals in one protected VM operation,
   avoiding repeated intermediate Rust references. The original reader handles
   metamethods, rebinding and invalid targets, preserving its error behavior.
4. Diagnostic traces deliver JSON values directly to the replay collector,
   avoiding serialization followed immediately by parsing. Text logging remains
   available when no structured consumer is installed.
5. Command comparison consumes the recorded events and transfers event arrays
   into the report, avoiding repeated deep copies of complete traces.
6. Path normalization allocates a replacement only when a backslash is present.
7. Independent fixtures can replay in separate worker processes with `--jobs N`.

The worker option accepts multiple selectors or `--all`.
Each worker has an independent process, VM and asset services. Each chart's Lua
callbacks and frames retain their chronological order. Cargo already builds
independent crates concurrently; the measured builds use eight Cargo jobs.
Within one chart, compilation remains chronological because callbacks share and
mutate Lua state.

These changes follow the hot-path profiling, allocation reuse and throughput
guidance in [rust-performance.md](../rust-performance.md). CPU profiles showed
repeated Lua key creation and protected environment lookups, rather than song
file parsing, dominating the selected workloads. The results measure the seven
changes together; they do not assign separate speedups to individual changes.

## Measured results

Measurements compare runtime commit `591cb2db0` with optimization commit
`7579c6775`, using the same measurement instrumentation on both revisions.
The [raw results and build receipts](perf-1872-song-lua-results.json) contain
every sample, complete output fingerprints, source/input hashes and allocation
traffic. Values below show before → after; G cycles means billions of cycles
and M allocations means millions of successful allocator calls, excluding
reallocations.

| Workload | Wall seconds | User CPU cycles (G) | Allocations (M) | Throughput gain |
| --- | ---: | ---: | ---: | ---: |
| 7th Gear production compilation, null renderer | 28.63 → 27.01 | 94.09 → 88.10 | 51.15 → 44.05 | 6.0% |
| 7th Gear production compilation, OpenGL branch | 74.13 → 65.58 | 233.03 → 211.56 | 94.30 → 86.59 | 13.0% |
| VTubers Never Die, full 67,455-frame replay | 95.26 → 89.97 | 301.61 → 286.04 | 60.681 → 60.678 | 5.9% |
| Mountain King, full 56,068-frame replay | 245.72 → 220.00 | 773.71 → 693.12 | 146.48 → 146.28 | 11.7% |
| Mawaru7, diagnostic 21,438-frame prefix | 617.76 → 526.14 | 1737.98 → 1546.19 | 504.42 → 497.98 | 17.4% |

The null-renderer compiler and VTubers rows use medians of three runs per
revision. The more expensive OpenGL branch, Mountain King and Mawaru7 each use one
before/after pair. The OpenGL case selects the production compilation branch
through its renderer argument; all measurements run headlessly without a game
window or GPU rendering. These are measurements on the selected workloads,
not a corpus-wide speedup claim.

The machine is a 40-vCPU Xeon E5-2697 v2 KVM guest with 64 GB RAM, running Linux
6.8.0-142 and rustc 1.98.1. Comparisons use CPU 2; the worker comparison uses
CPUs 2 and 3. Builds use release optimization with LTO disabled and eight Cargo
jobs. No build or other benchmark runs concurrently with a timed case.

The worker check replays GREAT GREAT and freg. using the optimized runner and
the same two accepted archives in both modes. Both modes pass all 484 recorded
comparisons. This is one serial/two-worker pair:

| Workers | Wall seconds | User CPU cycles (G), all processes | Allocations (M), all processes | Fixtures/second |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 43.30 | 135.11 | 29.324 | 0.0462 |
| 2 | 31.21 | 135.52 | 29.358 | 0.0641 |

Two workers increase throughput by 38.8%, using 0.3% more aggregate CPU cycles
and 0.12% more allocations. Allocation totals include the parent and both child
processes. The default remains one worker; `--jobs N` opts into concurrent
fixtures.

## Correctness

The optimized and frozen baseline executables replay the same retained captures.
Their complete report fingerprints match after the process-specific
normalizations described below. The native comparison rules, tolerances and
fixture acceptance criteria are unchanged.

The compiler suite has 309 passing tests and seven existing failures on both
revisions, with matching failure diagnostics. All 47 native replay unit tests
pass on both revisions. Three focused scalar/reset/environment tests and four
benchmark fingerprint tests also pass. The recorded result JSON names the seven
existing failures and includes source, executable and log hashes.

Mawaru7's measured prefix is a diagnostic workload through music second 30; it
does not accept that pending chart. Mawaru9 is excluded.

## Reproduction

Build the two headless tools with the same profile for both revisions:

```sh
cargo test --release --jobs 8 --config profile.release.lto=false --test full_song_lua --no-run
cargo build --release --jobs 8 --config profile.release.lto=false -p deadsync-shell --features song-lua-performance --bin song_lua_compile_performance
```

The compiler binary uses production dependencies without `test-support`;
`performance` enables only read-only inspection of the compiled layers. The
similarly named root test target also enables diagnostic recording through
Cargo's test dependencies and is not the production loading benchmark.

Save the executable paths from Cargo's JSON output. Both tools support opt-in
allocation accounting; the compiler tool records it for each preparation.
`DEADSYNC_SONG_LUA_METRICS=metrics.json` enables it for a full fixture run.
With multiple workers, child files add `.worker-N.json` to that filename.

Supply `scripts/benchmark_song_lua.py` with a configuration such as:

```json
{
  "cases": [
    {"name": "compiler", "kind": "compile", "simfile": "/charts/song.ssc", "renderer": "opengl,software"},
    {"name": "fixture", "kind": "fixture", "archive": "/captures/content-address.tar"}
  ]
}
```

```sh
python3 scripts/benchmark_song_lua.py --config cases.json \
  --fixture-runner /build/full_song_lua --compile-runner /build/song_lua_compile_performance \
  --out before --iterations 3
```

Repeat with the optimized executables and `--out after --compare before/results.json`.
`--perf-command` and `--program-prefix` accept argument prefixes when counter
access needs elevated privileges or the program needs CPU affinity.

The compiler benchmark supports `--chart-index N` for charts with an entry
selection screen. Its default is chart zero. A case that returns no compiled
layers is rejected as unsuitable for a compiler benchmark.

## Measurement scope

Linux `perf` collects user CPU cycles, user instructions and task-clock CPU
time. A control FIFO enables counters around each measured region. Compiler
timing includes production context and timing preparation; initial song/note
parsing and hashing the compiled output happen outside that region. Fixture
timing covers archive validation, extraction, replay, comparison and report
writing. Measurements use immutable executables and unchanged input files.

Allocation counts cover Rust's global allocator, including mlua's Lua allocator
and all threads in the process. Native libraries calling `malloc` directly are
outside that count. Requested bytes include each reallocation's complete new
size; they describe allocation traffic rather than resident memory. The runner
also records maximum resident memory using `/usr/bin/time`.

Compiled fingerprints cover the complete debug representation of each compiled
layer, excluding preparation timings. Fixture fingerprints cover the complete
JSON report, normalizing temporary extraction directories and process-specific
addresses in actor/name/path handles. Handle identities are assigned by first
observation within each scene, preserving alias relationships. Names, ancestry,
values and event order remain exact. Diagnostic prefixes remain diagnostics and
cannot accept a fixture.
