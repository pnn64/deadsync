# Profile XML import - 0.5.1158

Baseline: `29b11efb8` (0.5.1157). This pass uses the supplied
`rust-performance.md` guidance on measuring CPU and allocation costs (M-HOTPATH),
reusing owned storage (M-MEM-REUSE), reserving known capacities
(M-INITIAL-CAPACITY), and batching useful work (M-THROUGHPUT).
The guide itself is excluded from this commit.

Complete in-memory import preparation uses 33.26% fewer median
thread cycles for 128 scores and 27.91% fewer for 512 scores.
The 512-score case reduces requested-byte churn by 16.58%.
These figures cover byte ownership, XML parsing and DTO extraction; they do not
include filesystem access, chart resolution, persistence or game rendering.

## Changes

1. **Borrow direct XML text until ownership is needed.** A plain text run or
   CDATA span borrows from the parser input. Leading indentation is discarded
   before buffering because the completed direct text was already trimmed.
   Whitespace-only parent nodes therefore need no text allocation, and a
   single scalar run gets an exactly sized final string. Mixed content joins
   into owned storage, retaining interior whitespace and entity boundaries.
2. **Copy literal entity spans in bulk.** Entity decoding appends whole plain
   spans instead of copying one Unicode character at a time. Fixed named
   references are recognized directly without a delimiter search. Unknown nested
   references reuse the next semicolon search, and an unterminated suffix is
   copied once. Named/numeric decoding and literal preservation keep the old
   behavior. The decoder still supports a reusable caller-owned output buffer.
3. **Reuse valid plain-file UTF-8 storage.** Plain `Stats.xml` loading converts
   the owned file bytes with `String::from_utf8`, retaining their allocation.
   Invalid UTF-8 keeps the former lossy replacement policy. Valid conversion
   itself has zero allocations, reallocations and frees; the benchmark includes
   a common input-buffer clone on both sides. Gzip and capped head readers
   already reused storage and are unchanged.

## Method

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8. Release optimization level 3 and full LTO.
The complete original parser/decoder is frozen in a test-only baseline file;
only visibility and shared unchanged type imports differ. The old plain-file
conversion accepts an explicit owned Vec instead of the temporary fs::read
result, excluding disk work. GeneralData and owned score extraction use the
same unchanged production functions on both sides.

All 32 old/new pairs run in one release test executable with black-boxed inputs.
Three invocations alternate old/new, new/old, old/new. Each invocation uses
three warmups, seven timing batches and a separate allocation-counted operation.
Tables report medians of the three per-invocation medians. No Cargo build runs
alongside the benchmarks. Source hashes are checked before/after timing and
again before commit.

Calling-thread CPU cycles use Windows QueryThreadCycleTime. The existing scoped
test allocator delegates to System; allocation counting is disabled during
timing. Returned values and temporaries are dropped inside measured operations.
Requested-byte totals include reallocations' replacement buffers. They describe
allocation churn, not peak RSS, committed pages, cache misses or measurements
of the production allocator. Small differences can reflect compiler layout,
CPU frequency and scheduling; these are local microbenchmarks, not confidence
intervals or whole-game speedup estimates.

Batch sizes:

- Standard XML fixtures: 8,192 operations for 0/1 scores, 32 for 128/512.
- Additional XML cases: 128 operations above 1,000 input bytes, otherwise 32,768.
- Entity decoding: 32,768 below 32 input bytes, otherwise 256.
- Byte conversion: 8,192 for 0/1 scores, 256 for 128, 32 for 2,048.
- Combined import preparation: 8,192 for one score, 32 for 128/512.

Stats fixtures have a GUID, combo and one Steps/HighScore per song, with grade,
date, percentage, modifiers, survival seconds, four tap fields and two hold
fields. Pretty fixtures insert indentation between tags. Labels contain Unicode;
entity fixtures also contain named, numeric and unknown references. Zero-score
compact/pretty fixtures are identical controls. XML mixed content repeats
text, CDATA, a child, a named entity and a comment 128 times. Whitespace-only
content contains ASCII and Unicode whitespace.

Entity fixtures cover plain text, one escape between long spans, 768 adjacent
named/numeric references, repeated unknown references, 4,096 ampersands with a
final semicolon, 4,096 without a semicolon, a short escape, and empty input.
Their two output buffers are sized before timing, retained, and cleared for
each operation. This measures warm-buffer decoding; all eight old/new pairs
have zero measured heap churn.

Byte-conversion fixtures use pretty plain Stats documents of 120, 706, 75,274
and 1,205,282 bytes. Lossy variants replace the middle byte with 0xff. Both sides
clone the same source bytes inside every operation to model the owned buffer
normally supplied by fs::read; the new valid path eliminates the additional
output allocation and copy. Combined import fixtures use pretty entity-bearing
documents and include that common clone, decoding, full XML tree construction,
GeneralData extraction, owned score DTO extraction, and disposal.

## CPU time and throughput

Positive saved percentages mean fewer cycles; negative values expose regressions.
Throughput below is complete measured operations per second, calculated from
median ns/op. Raw benchmark output additionally reports bytes/s for XML/entities/
byte conversion and score records/s for combined import preparation.

### XML parsing

| Case | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | Operations/s old -> new |
| --- | ---: | ---: | ---: | ---: |
| `xml_0_compact` | 857.0 -> 836.8 | 1,878.1 -> 1,835.5 | 2.27% | 1,166,861.1 -> 1,195,028.7 |
| `xml_0_pretty` | 858.7 -> 823.1 | 1,878.1 -> 1,805.1 | 3.89% | 1,164,551.1 -> 1,214,919.2 |
| `xml_1_compact` | 4,828.2 -> 4,762.2 | 10,577.3 -> 10,432.2 | 1.37% | 207,116.5 -> 209,987.0 |
| `xml_1_pretty` | 7,187.1 -> 5,309.8 | 15,757.9 -> 11,640.5 | 26.13% | 139,138.2 -> 188,331.0 |
| `xml_128_compact` | 605,065.6 -> 590,312.5 | 1,326,287.6 -> 1,293,835.9 | 2.45% | 1,652.7 -> 1,694.0 |
| `xml_128_pretty` | 918,093.8 -> 707,103.1 | 2,012,122.2 -> 1,549,745.4 | 22.98% | 1,089.2 -> 1,414.2 |
| `xml_512_compact` | 2,472,862.5 -> 2,400,309.4 | 5,417,932.2 -> 5,259,631.6 | 2.92% | 404.4 -> 416.6 |
| `xml_512_pretty` | 3,592,346.9 -> 2,688,146.9 | 7,871,331.7 -> 5,887,669.1 | 25.20% | 278.4 -> 372.0 |
| `xml_entities` | 963,812.5 -> 707,974.2 | 2,112,135.3 -> 1,550,851.6 | 26.57% | 1,037.5 -> 1,412.5 |
| `xml_mixed` | 29,896.9 -> 30,111.7 | 65,565.3 -> 66,040.3 | -0.72% | 33,448.3 -> 33,209.7 |
| `xml_empty` | 90.6 -> 90.7 | 198.6 -> 199.0 | -0.20% | 11,037,527.6 -> 11,025,358.3 |
| `xml_scalar` | 173.4 -> 165.1 | 380.3 -> 362.1 | 4.79% | 5,767,012.7 -> 6,056,935.2 |
| `xml_whitespace` | 7,102.3 -> 5,829.7 | 15,613.6 -> 12,806.5 | 17.98% | 140,799.5 -> 171,535.4 |

### Entity decoding

| Case | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | Operations/s old -> new |
| --- | ---: | ---: | ---: | ---: |
| `entity_plain` | 302.0 -> 307.0 | 667.9 -> 679.1 | -1.68% | 3,311,258.3 -> 3,257,329.0 |
| `entity_single` | 16,158.6 -> 314.1 | 35,432.1 -> 694.5 | 98.04% | 61,886.5 -> 3,183,699.5 |
| `entity_dense` | 7,133.6 -> 6,026.2 | 15,646.2 -> 13,200.9 | 15.63% | 140,181.7 -> 165,942.1 |
| `entity_unknown` | 29,145.7 -> 6,485.2 | 63,886.5 -> 14,223.8 | 77.74% | 34,310.4 -> 154,197.2 |
| `entity_amp_storm` | 828,737.1 -> 20,433.6 | 1,816,972.1 -> 44,823.4 | 97.53% | 1,206.7 -> 48,939.0 |
| `entity_no_semicolon` | 818,796.9 -> 360.5 | 1,794,812.9 -> 795.7 | 99.96% | 1,221.3 -> 2,773,925.1 |
| `entity_short` | 30.3 -> 18.8 | 66.3 -> 41.4 | 37.56% | 33,003,300.3 -> 53,191,489.4 |
| `entity_empty` | 5.2 -> 4.7 | 11.5 -> 10.3 | 10.43% | 192,307,692.3 -> 212,765,957.4 |

### Owned byte conversion

| Case | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | Operations/s old -> new |
| --- | ---: | ---: | ---: | ---: |
| `bytes_0_valid` | 176.6 -> 79.4 | 387.3 -> 174.2 | 55.02% | 5,662,514.2 -> 12,594,458.4 |
| `bytes_0_lossy` | 267.0 -> 284.3 | 585.8 -> 623.8 | -6.49% | 3,745,318.4 -> 3,517,411.2 |
| `bytes_1_valid` | 419.6 -> 111.5 | 920.4 -> 244.9 | 73.39% | 2,383,222.1 -> 8,968,609.9 |
| `bytes_1_lossy` | 525.0 -> 597.6 | 1,151.4 -> 1,310.2 | -13.79% | 1,904,761.9 -> 1,673,360.1 |
| `bytes_128_valid` | 127,000.8 -> 19,417.6 | 278,332.0 -> 42,541.0 | 84.72% | 7,874.0 -> 51,499.7 |
| `bytes_128_lossy` | 58,436.3 -> 49,137.1 | 128,050.0 -> 107,692.2 | 15.90% | 17,112.7 -> 20,351.2 |
| `bytes_2048_valid` | 1,356,521.9 -> 523,043.8 | 2,971,508.6 -> 1,142,902.2 | 61.54% | 737.2 -> 1,911.9 |
| `bytes_2048_lossy` | 1,821,915.6 -> 1,772,031.2 | 3,986,991.2 -> 3,881,048.0 | 2.66% | 548.9 -> 564.3 |

### Combined import preparation

| Case | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | Operations/s old -> new |
| --- | ---: | ---: | ---: | ---: |
| `import_1` | 8,376.3 -> 6,172.6 | 18,364.9 -> 13,530.9 | 26.32% | 119,384.5 -> 162,006.3 |
| `import_128` | 1,237,156.2 -> 825,643.8 | 2,710,934.8 -> 1,809,338.6 | 33.26% | 808.3 -> 1,211.2 |
| `import_512` | 4,412,290.6 -> 3,178,781.2 | 9,661,155.4 -> 6,964,495.0 | 27.91% | 226.6 -> 314.6 |

At 512 scores, score throughput changes from
116,040 to 161,068 records/s.

## Allocation and byte churn

Counts are per operation. Freed-byte totals equal requested-byte totals for
every row on both sides, including reallocation accounting; buffers returned
by the operation are dropped within the measurement. Entity buffers are
retained and have zero frees as well as zero allocations.

| Case | Allocations old -> new | Reallocations old -> new | Frees old -> new | Requested bytes old -> new |
| --- | ---: | ---: | ---: | ---: |
| `bytes_0_valid` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 240 -> 120 |
| `bytes_0_lossy` | 2 -> 2 | 1 -> 1 | 2 -> 2 | 480 -> 480 |
| `bytes_1_valid` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 1,412 -> 706 |
| `bytes_1_lossy` | 2 -> 2 | 1 -> 1 | 2 -> 2 | 2,824 -> 2,824 |
| `bytes_128_valid` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 150,548 -> 75,274 |
| `bytes_128_lossy` | 2 -> 2 | 1 -> 1 | 2 -> 2 | 301,096 -> 301,096 |
| `bytes_2048_valid` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 2,410,564 -> 1,205,282 |
| `bytes_2048_lossy` | 2 -> 2 | 1 -> 1 | 2 -> 2 | 4,821,128 -> 4,821,128 |
| `import_1` | 67 -> 59 | 10 -> 1 | 67 -> 59 | 7,054 -> 5,976 |
| `import_128` | 6,671 -> 5,901 | 1,292 -> 133 | 6,671 -> 5,901 | 742,754 -> 619,610 |
| `import_512` | 26,639 -> 23,565 | 5,136 -> 519 | 26,639 -> 23,565 | 2,969,954 -> 2,477,402 |
| `xml_0_compact` | 9 -> 9 | 0 -> 0 | 9 -> 9 | 827 -> 821 |
| `xml_0_pretty` | 9 -> 9 | 0 -> 0 | 9 -> 9 | 827 -> 821 |
| `xml_1_compact` | 54 -> 54 | 1 -> 1 | 54 -> 54 | 4,961 -> 4,909 |
| `xml_1_pretty` | 61 -> 54 | 10 -> 1 | 61 -> 54 | 5,241 -> 4,909 |
| `xml_128_compact` | 5,642 -> 5,642 | 133 -> 133 | 5,642 -> 5,642 | 505,165 -> 499,271 |
| `xml_128_pretty` | 6,411 -> 5,642 | 1,292 -> 133 | 6,411 -> 5,642 | 542,021 -> 499,271 |
| `xml_512_compact` | 22,538 -> 22,538 | 519 -> 519 | 22,538 -> 22,538 | 2,019,661 -> 1,996,103 |
| `xml_512_pretty` | 25,611 -> 22,538 | 5,136 -> 519 | 25,611 -> 22,538 | 2,167,109 -> 1,996,103 |
| `xml_entities` | 6,411 -> 5,642 | 1,292 -> 133 | 6,411 -> 5,642 | 547,141 -> 504,391 |
| `xml_mixed` | 131 -> 131 | 13 -> 13 | 131 -> 131 | 28,924 -> 28,412 |
| `xml_empty` | 1 -> 1 | 0 -> 0 | 1 -> 1 | 1 -> 1 |
| `xml_scalar` | 2 -> 2 | 0 -> 0 | 2 -> 2 | 9 -> 2 |
| `xml_whitespace` | 2 -> 1 | 0 -> 0 | 2 -> 1 | 5,121 -> 1 |
| All eight `entity_*` cases | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |

## Limits and regressions

Measured cycle regressions in this run: `bytes_0_lossy` (6.49% more cycles),
`bytes_1_lossy` (13.79% more cycles), `xml_mixed` (0.72% more cycles),
`xml_empty` (0.20% more cycles), and `entity_plain` (1.68% more cycles).

Invalid UTF-8 must still allocate replacement text and can incur an additional
validation pass; its allocation churn is unchanged. Mixed XML content can still
need owned concatenation buffers. The public AST continues to own tags, attributes,
children and final text, so this is not a zero-allocation XML parser. Fewer
requested bytes alone do not prove lower peak process memory.

## Behavior and checks

All 60 import tests pass in debug and release, including five new tests:

- Full old/new AST and error comparisons across plain/Unicode whitespace,
  comments, children, CDATA, encoded text, malformed/tolerated documents,
  compact/pretty Stats documents and 512 deterministic generated documents.
- Exact entity-output equality for named, unknown, nested, adjacent, numeric,
  overflowing, surrogate, zero and unterminated references, plus 1,024
  deterministic generated entity strings and existing-output prefixes.
- Zero scratch churn for initial indentation and borrowed text, bounded
  one-allocation concatenation, and zero churn for warmed entity output.
- Lossy decoding equivalence for malformed/truncated UTF-8 and 1,024 generated
  byte arrays; valid conversion retains its original pointer with zero churn.
- Decoded text, complete AST/errors, GeneralData and owned score DTO equivalence
  for compact/pretty, plain/entity, valid/invalid Stats inputs at three sizes.
  DTO comparison uses their deterministic Debug representation because those
  types do not implement PartialEq. Existing filesystem and invalid-byte import
  tests also pass.

`cargo clippy -p deadsync-import --all-targets --locked -- -D clippy::perf`,
`cargo check --all-targets --locked`, targeted rustfmt checks and `git diff --check`
pass, with existing Clippy warnings outside the changed code. The frozen
baseline matches the prior commit, and the new measured sources
match the committed implementation. Cargo.toml and the three workspace-version
entries in Cargo.lock change exactly once from 0.5.1157 to 0.5.1158.

Reproduce from the repository root (PowerShell):

```powershell
cargo test -p deadsync-import --lib --locked
cargo test -p deadsync-import --lib --release --locked
cargo test -p deadsync-import --lib --release --locked profile_import_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-import --lib --release --locked profile_import_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-import --lib --release --locked profile_import_bench -- --ignored --test-threads=1 --nocapture
cargo clippy -p deadsync-import --all-targets --locked -- -D clippy::perf
cargo check --all-targets --locked
```

The two benchmark tests are ignored during ordinary test runs. No dependency
is added; benchmark helpers and frozen implementations compile only for tests.
