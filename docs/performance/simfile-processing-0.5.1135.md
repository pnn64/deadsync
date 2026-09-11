# Simfile processing performance - 0.5.1135

This pass applies `M-HOTPATH`, `M-MEM-REUSE`, `M-INITIAL-CAPACITY`, and
`M-THROUGHPUT` from the supplied `rust-performance.md`. The workloads cover
metadata extraction, background/foreground change parsing, and saving timing
adjustments. Measurements isolate in-memory processing; they do not measure disk
I/O, whole-library loading, or gameplay FPS.

## Three changes

1. **Accelerate tag searches.** Replace the scalar byte loops used to locate
   hashes and tag terminators with `memchr`/`memchr2`. Artwork lookup and extra
   background/foreground tag extraction can skip large note payloads efficiently.
   Escape parity, newline recovery, duplicate selection, and decoding stay in
   the existing parser. Borrowed tag iteration already allocated nothing and
   continues to allocate nothing; returning decoded owned values still allocates.
2. **Borrow and stream background fields.** The internal consumers now process
   one record at a time, borrowing fields from the decoded tag with `Cow<str>`.
   Eleven standard fields fit inline in `SmallVec`; wider records spill. Only
   fields requiring newline removal allocate strings. This removes the temporary
   vector of records and ordinary per-record/per-field allocations, and lets Lua
   detection stop at its first match. Filename matching order, delimiters inside
   filenames, terminal empty fields, and the legacy release-mode field-counter
   wrap are preserved. The public owned-returning wrapper remains available and
   reserves its outer collection before converting the borrowed fields.
3. **Rewrite offsets directly into the output.** Search for candidate hashes and
   terminating semicolons in chunks and format each normalized value directly
   into the output byte vector. The temporary formatting string per offset is
   gone. Quantization, signed-zero normalization, non-finite values, error text,
   whitespace, and untouched bytes are unchanged. Output can still grow when
   replacements exceed the initial `input length + 64` capacity.

All production changes use safe Rust. `memchr` 2.8.3 and `smallvec` 1.16.0 become
explicit dependencies of `deadsync-simfile`; both versions were already present
in Cargo.lock. No dependency versions were upgraded.

## Method

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.0, repository release profile
(opt-level 3, full LTO). Frozen routines in `tests/perf/legacy_*.rs` come from
`ec9914795` / 0.5.1134. Their bodies were verified against that commit, ignoring
formatting; relevant inline attributes are preserved. Old and new run inside the
same executable with the same inputs and allocator instrumentation.

Each workload has three warmups followed by seven timed batches. Three invocations
alternate old/new, new/old, old/new order. Tables report the median of those three
invocation medians. Compilation and other tests finish before timing begins.
Allocation counters are disabled during timing and enabled for one additional
operation. Windows thread cycles come from `QueryThreadCycleTime`; these routines
run on the measured thread. Output destruction is included. Input construction,
filesystem access, path resolution, and decoded-tag creation are excluded.

Tag/offset fixtures have 4 KiB of note payload and one chart, 128 short chart
metadata groups without note payload, or 1 MiB of note payload and 32 charts.
Each chart has an offset and foreground-change tag; a song offset precedes them.
Batches run 4,000 / 1,000 / 100 operations respectively. Tag iteration computes a
checksum of borrowed values. Offset rewriting applies +0.001 seconds and returns
the complete rewritten byte buffer and changed-tag count.

Background fixtures have one or 512 eleven-field records, including directory
entries with comma/equals signs in their filenames. A third fixture inserts a
newline inside one filename per record. Batches run 10,000 operations for one
record and 100 for 512 records. The main comparison consumes every field into a
checksum, mirroring the production consumers' one-pass use; the separate owned
comparison returns and drops all records through the compatibility API.

Requested/freed bytes measure allocation traffic, including complete reallocation
requests, not peak live memory or process RSS. Both allocation and free traffic
are recorded. Zero heap churn in the borrowed tokenizer does not imply that
subsequent filesystem resolution or final owned change objects allocate nothing.

## Results

### Tag extraction and offset rewriting

| Workload | Old us/op | New us/op | Old cycles/op | New cycles/op | Old -> new MB/s |
|---|---:|---:|---:|---:|---:|
| Tags, 4 KiB payload | 2.700 | 0.216 | 5,911.6 | 474.6 | 1,551.783 -> 19,355.435 |
| Tags, 128 metadata groups | 10.129 | 10.227 | 22,217.3 | 22,432.7 | 774.000 -> 766.583 |
| Tags, 1 MiB payload | 652.136 | 33.414 | 1,428,038.5 | 73,282.3 | 1,610.952 -> 31,440.713 |
| Offsets, 4 KiB payload / 2 offsets | 5.019 | 0.539 | 11,004.3 | 1,182.2 | 834.554 -> 7,774.324 |
| Offsets, metadata / 129 offsets | 38.057 | 24.053 | 82,883.6 | 52,731.4 | 206.008 -> 325.951 |
| Offsets, 1 MiB payload / 33 offsets | 1,550.647 | 480.962 | 3,395,410.4 | 1,051,506.0 | 677.498 -> 2,184.289 |

### Background field parsing

| Workload | Old us/op | New us/op | Old cycles/op | New cycles/op | Old -> new million records/s |
|---|---:|---:|---:|---:|---:|
| Borrowed fields, 1 record | 1.234 | 0.286 | 2,703.9 | 626.1 | 0.811 -> 3.499 |
| Borrowed fields, 512 records | 829.420 | 149.239 | 1,801,623.1 | 327,004.5 | 0.617 -> 3.431 |
| Borrowed fields, 512 multiline records | 657.190 | 196.696 | 1,429,772.5 | 431,304.3 | 0.779 -> 2.603 |
| Owned wrapper, 1 record | 1.228 | 1.191 | 2,685.0 | 2,611.9 | 0.814 -> 0.840 |
| Owned wrapper, 512 records | 655.742 | 622.338 | 1,426,122.2 | 1,362,221.4 | 0.781 -> 0.823 |
| Owned wrapper, 512 multiline records | 647.078 | 630.657 | 1,414,879.4 | 1,380,354.3 | 0.791 -> 0.812 |

### Heap churn per operation

Allocation/free calls are equal, as are requested/freed bytes, for every measured
case. Reallocation calls are listed separately; their requested/freed bytes are
included in the traffic column. MB/s above uses decimal megabytes.

| Workload | Old -> new allocation/free calls | Old -> new reallocations | Old -> new requested/freed bytes |
|---|---:|---:|---:|
| Tags, 4 KiB payload | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| Tags, 128 metadata groups | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| Tags, 1 MiB payload | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| Offsets, 4 KiB payload / 2 offsets | 3 -> 1 | 0 -> 0 | 4,269 -> 4,253 |
| Offsets, metadata / 129 offsets | 130 -> 1 | 0 -> 0 | 8,936 -> 7,904 |
| Offsets, 1 MiB payload / 33 offsets | 34 -> 1 | 0 -> 0 | 1,050,888 -> 1,050,624 |
| Borrowed fields, 1 record | 13 -> 0 | 1 -> 0 | 476 -> 0 |
| Borrowed fields, 512 records | 6,145 -> 0 | 512 -> 0 | 244,970 -> 0 |
| Borrowed fields, 512 multiline records | 6,145 -> 512 | 512 -> 0 | 245,482 -> 7,680 |
| Owned wrapper, 1 record | 13 -> 13 | 1 -> 0 | 476 -> 380 |
| Owned wrapper, 512 records | 6,145 -> 6,145 | 512 -> 0 | 244,970 -> 195,818 |
| Owned wrapper, 512 multiline records | 6,145 -> 6,145 | 512 -> 0 | 245,482 -> 196,330 |

Tag scanning saves 92% of cycles in the small-payload fixture and 95% in the large
one. The dense short-tag fixture is effectively flat (median about 1% slower;
individual invocations span small gains and losses). Chunk searching pays off
when there is meaningful payload to skip, rather than guaranteeing a win on every
short field.

The borrowed tokenizer saves 77% of cycles for one plain record and 82% for 512
records; the multiline fixture saves 70% and removes 96.9% of requested allocation
traffic. Dense plain baseline timings varied across invocations (650-841 us/op),
but even the fastest old median remains over four times slower than the new one.
The compatibility wrapper removes all measured reallocations and roughly 20% of
requested allocation traffic. Its small timing improvements (2-4% cycles) are
within the variability of allocation-heavy measurements and are not a strong
standalone throughput claim.

Offset rewriting saves 89% / 36% / 69% of cycles for small / metadata-only / large
fixtures. The output buffer remains the dominant allocation size for large files,
so the reduction from 34 allocations to one mainly removes allocator churn,
not megabytes of output storage. File read/write time is outside these results.

## Validation and reproduction

- Unmodified baseline: 177 simfile unit tests passed.
- Final debug and release: 185 passed, zero failures, one manual benchmark ignored.
- Eight new tests compare legacy results across truncations, randomized delimiters,
  invalid UTF-8, escaping, duplicate tags, newline recovery, entry matching order,
  inline-capacity spill, release counter wrap, floating-point corner cases,
  output growth, and allocation budgets. Existing background/foreground resolution
  and filesystem tests also pass.
- `cargo check --workspace --bins` passed.
- `cargo clippy -p deadsync-simfile --all-targets -- -D clippy::perf` passed; existing
  warnings outside the performance lint group remain.
- Rustfmt checks for the changed Rust files and benchmark modules passed.
- Cargo.toml and Cargo.lock advance the workspace patch exactly once,
  0.5.1134 -> 0.5.1135.

Run behavior tests:

```powershell
cargo test -p deadsync-simfile -- --test-threads=1
cargo test --release -p deadsync-simfile --lib -- --test-threads=1
```

Run three benchmark invocations after builds and other tests have finished:

```powershell
Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
cargo test --release -p deadsync-simfile --lib simfile_processing_bench -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test --release -p deadsync-simfile --lib simfile_processing_bench -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test --release -p deadsync-simfile --lib simfile_processing_bench -- --ignored --nocapture --test-threads=1
```

The committed benchmark prints median/range time, calling-thread cycles,
throughput, and allocation/reallocation/free traffic for each old/new workload.
Local raw results and test logs are in the ignored `target/simfile-perf/` directory.
