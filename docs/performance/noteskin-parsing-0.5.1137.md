# Noteskin parsing performance - 0.5.1137

This pass follows `M-HOTPATH`, `M-MEM-REUSE`, `M-INITIAL-CAPACITY`, and
`M-THROUGHPUT` in the supplied `rust-performance.md`. It reduces CPU work and
allocation traffic while parsing noteskin Lua declarations and commands. These
measurements exclude disk I/O, Lua execution, texture loading, and rendering.

## Three changes

1. **Start delimiter searches at the requested byte.** Both actor parsing and
   the shared ITG Lua helper now use the same implementation. Previously every
   matching-parenthesis/brace search decoded all preceding characters before
   looking at its own span. Repeated calls along a large script could therefore
   do quadratic prefix work. Searches retain Unicode delimiters, absolute byte
   offsets, off-boundary index behavior, and the existing simple delimiter rules.
   Both implementations are allocation-free.
2. **Borrow scoped command arguments and reserve the output once.** Existing
   scope/color values and unchanged argument text are borrowed with `Cow` until
   appended to the output. Actual color-expression conversion still owns its
   generated text; helper scopes explicitly take ownership where needed. Output
   storage is reserved lazily on the first valid command using the body length
   as an estimate. No-command inputs remain allocation-free. Expanded aliases
   can still require growth if their output exceeds that estimate.
3. **Borrow scripts that need no comment removal.** A quick marker check avoids
   scanning plain scripts. Quoted markers are still checked with the original
   escape/quote rules. Only an actual comment creates an output buffer; unchanged
   source spans are then copied in bulk. Comment delimiters, padding, newlines,
   unterminated comments, and UTF-8 behavior match the former implementation.

No dependencies or unsafe code were added. Returned declarations and command
strings still require owned storage; this is not a claim of zero allocations for
an entire noteskin load.

## Method

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.0, repository release profile
(opt-level 3, full LTO). Baseline: `1d95c241f`, version 0.5.1136.

The four frozen reference routines in `tests/perf/parsing.rs` were checked
against the baseline source, ignoring formatting and documented renaming.
Delimiter and comment comparisons use the old routine directly. The scoped
command comparison uses the same current method/delimiter parser on both sides,
which isolates argument ownership/output reservation from the delimiter gain.

Complete parsing uses a saved baseline release executable and a new release
executable, built with the same corpus benchmark and compiler/profile. The
baseline was saved before the production changes. All 152 bundled Lua files
produce the same canonical declaration digest (`745e4324179c348f`, including
64 sprite declarations) in all six old/new runs. Object keys are sorted before
hashing to remove HashMap iteration order from the comparison. This covers all
serialized declaration fields, including commands and references.

Each workload has three warmups and seven timed batches. Three invocations
alternate old/new, new/old, old/new. Tables show medians of the three invocation
medians. Builds and other tests finish before benchmarking. Elapsed time and
Windows `QueryThreadCycleTime` calling-thread cycles are sampled with allocation
counters disabled. One separate operation counts allocation, reallocation, free,
and requested/freed bytes, including output destruction. All work is on the
measured thread. Fixture loading/construction and digest calculation are excluded.

The delimiter fixtures repeat three calls 16/256/2,048 times and query every
opening parenthesis (48/768/6,144 queries). Batches contain 16/16/2 operations.
Scoped command fixtures repeat five calls 1/16/128 times, with numeric, named
color, and quoted arguments; batches contain 64 operations. Comment fixtures
are 6,656 / 8,192 / 10,496 bytes and use 128 operations per batch. Complete-parser
batches contain four corpus passes or sixteen generated 128-sprite bundles.

## Results

| Workload | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| 152 bundled Lua files | 5,267.400 | 3,596.275 | 11,539,060.2 | 7,882,739.0 | 31.69% |
| 128 generated sprite declarations | 1,300.656 | 326.431 | 2,840,481.0 | 715,967.8 | 74.79% |
| 48 delimiter queries | 10.381 | 0.463 | 23,308.1 | 1,097.5 | 95.29% |
| 768 delimiter queries | 2,630.131 | 7.662 | 5,761,573.1 | 16,942.7 | 99.71% |
| 6,144 delimiter queries | 167,438.400 | 59.600 | 366,914,665.5 | 131,700.0 | 99.96% |
| 5 scoped commands | 2.453 | 1.289 | 5,065.7 | 2,850.1 | 43.74% |
| 80 scoped commands | 27.117 | 18.927 | 59,487.9 | 41,430.6 | 30.35% |
| 640 scoped commands | 207.728 | 150.823 | 455,205.3 | 330,615.0 | 27.37% |
| Comment pass: plain script | 17.160 | 0.321 | 36,872.6 | 715.1 | 98.06% |
| Comment pass: quoted comment markers | 21.191 | 13.798 | 46,498.0 | 30,273.8 | 34.89% |
| Comment pass: actual comments | 20.928 | 19.381 | 45,890.9 | 42,509.3 | 7.37% |

| Workload | Throughput unit | Old units/s | New units/s |
|---|---|---:|---:|
| Bundled corpus | files | 28,856.7 | 42,266.0 |
| Generated bundle | sprites | 98,411.9 | 392,119.3 |
| Delimiter searches | queries | 292,000.6 | 100,228,385.0 |
| Scoped command chains | commands | 3,080,950.2 | 4,243,372.3 |
| Plain comment pass | bytes | 387,875,256.1 | 20,729,148,418.5 |
| Quoted-marker comment pass | bytes | 386,586,049.3 | 593,724,024.7 |
| Actual-comment pass | bytes | 501,526,056.4 | 541,554,337.3 |

### Allocation traffic per operation

Allocation/free call counts and requested/freed byte counts match for each
fixture. Reallocation calls are separate; byte traffic includes the entire new
request for each reallocation. These are allocator traffic counts, not peak RSS,
allocator metadata, or simultaneous live bytes.

| Workload | Old -> new allocation/free calls | Old -> new reallocations | Old -> new requested/freed bytes |
|---|---:|---:|---:|
| 152 bundled Lua files | 2,519 -> 2,370 | 73 -> 27 | 417,772 -> 388,501 |
| 128 generated sprite declarations | 1,026 -> 641 | 251 -> 5 | 118,102 -> 102,148 |
| 48 delimiter queries | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| 768 delimiter queries | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| 6,144 delimiter queries | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| 5 scoped commands | 6 -> 1 | 3 -> 0 | 146 -> 63 |
| 80 scoped commands | 81 -> 1 | 7 -> 0 | 2,456 -> 1,008 |
| 640 scoped commands | 641 -> 1 | 10 -> 0 | 19,704 -> 8,064 |
| Comment pass: plain script | 1 -> 0 | 0 -> 0 | 6,656 -> 0 |
| Comment pass: quoted comment markers | 1 -> 0 | 0 -> 0 | 8,192 -> 0 |
| Comment pass: actual comments | 1 -> 1 | 0 -> 0 | 10,496 -> 10,496 |

The complete bundled corpus uses about 32% fewer cycles and achieves about 46%
higher throughput. It removes 149 allocations and 46 reallocations per pass,
with 7% less requested byte traffic. The generated multi-actor bundle uses about
75% fewer cycles; this larger gain reflects its many repeated delimiter queries.

The isolated scoped-command change saves 27-44% of median cycles and about 59%
of requested bytes on the larger fixtures. The 640-command fixture goes from
641 allocations plus 10 reallocations to one allocation with no growth.
Comment-free scripts eliminate the output copy and its allocation; quoted
comment markers also allocate nothing. Actual comments retain one allocation.
Their approximately 7% median cycle improvement has substantial variation,
including a slightly slower invocation, so it is not a strong independent claim.
The delimiter microbenchmark's 95-99.96% savings apply to repeated short searches
in these long strings, not to all noteskin loading work.

## Validation

- Baseline noteskin unit tests: 221 passed.
- Final noteskin unit tests: 228 passed in debug and release, zero failures;
  two manual benchmarks are ignored.
- Seven new behavior/allocation tests cover every offset around Unicode and
  malformed delimiters, 2,744 comment/quote/UTF-8 combinations, bundled comment
  text, scoped aliases and shadowing, color expressions, nested arguments,
  malformed calls, allocation-free borrowing, and bounded output allocation.
- Noteskin integration tests: 5 pack tests and 26 parsing tests passed;
  one pre-existing manual benchmark is ignored.
- Downstream: 125 assets tests and 370 notefield tests passed.
- `cargo check --workspace --bins` passed.
- Strict production performance lint check passed. The all-target Clippy check
  encounters two pre-existing `cloned_ref_to_slice_refs` violations in
  `tests/packs.rs:242` and `:245`; these tests are unchanged. Other existing
  warnings remain. No warning in the changed parser functions was reported.
- Changed production routines and the new benchmark module pass Rustfmt;
  unrelated existing formatting is preserved.
- Cargo.toml and Cargo.lock increment the patch exactly once:
  0.5.1136 -> 0.5.1137.

## Reproduction

```powershell
cargo test -p deadsync-noteskin --all-targets -- --test-threads=1
cargo test --release -p deadsync-noteskin --lib -- --test-threads=1
cargo test -p deadsync-assets -p deadsync-notefield --lib -- --test-threads=1
cargo check --workspace --bins
cargo clippy -p deadsync-noteskin --lib -- -D clippy::perf
cargo test --release -p deadsync-noteskin --lib noteskin_parsing_bench -- --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-noteskin --lib noteskin_corpus_bench -- --ignored --nocapture --test-threads=1
```

The isolated benchmark includes both references in one executable. Run it three
times with `DEADSYNC_PERF_REVERSE=1` only for the middle run. The corpus benchmark
measures the executable's production parser; compare the baseline and new builds
with the same corpus helper/benchmark, alternating executable order. The local
saved baseline/new executables, three raw runs, summaries, comparison script,
and validation logs are in the ignored `target/noteskin-perf/` directory.
