# JPEG RGBA8 qualification evidence

The run began on 2026-11-19 UTC. [RESULTS.md](../../RESULTS.md) reports the final
measurements and remaining limitations. The production baseline is
`9a6f4ebabe71934202b55df110dae03883ca512c`; its cumulative source patch is empty.
Every stage used the same benchmark and snapshot helpers from this change.

## Measurement boundary and host

Windows 11, Intel Xeon E5-2696 v4 (Broadwell, AVX2), Rust 1.100.0
(`0ef1bc386`, LLVM 23.1.5), `x86_64-pc-windows-msvc`. The isolated benchmark uses
release optimization level 3, fat LTO, one codegen unit and abort-on-panic.
The repository Windows configuration enables the static MSVC runtime.
Runtime AVX2 dispatch remains enabled. The benchmark asserts that affinity to
logical CPU 0 succeeds. `RAYON_NUM_THREADS=1`; process-wide
`QueryProcessCycleTime` includes reference worker threads.

Native uses compatible input handling (`strict: false`), centered linear
interpolation and owned gray/RGB-to-RGBA8 conversion. The primary reference is
`image 0.25.10` through `load_from_memory_with_format(..., Jpeg)` followed by its
own `into_rgba8().into_raw()`. This is the in-memory decode/RGBA equivalent of
the loader's successful JPEG path. File I/O, signature detection, format retries,
color keying and upload are excluded; decoding, conversion, allocation
and destruction are included. Direct `zune-jpeg 0.5.15` requests gray/RGB and
uses the shared RGBA adapter, rather than its direct RGBA output option.
`jpeg-decoder 0.3.2` retains its default Rayon implementation with one worker.

Each timing is the median of five batches, with independent cycle and wall-time
medians. Individual stage and final reference runs use at least 80 ms per
batch. Paired stage qualification uses 60 ms, rotates version order per file,
then reverses that order in its second round. No builds, tests or corpus hashing
were run concurrently with timed decoding. Allocation counters run in a separate
instrumented executable, calibrated against three requests totaling 144 bytes.
Requested bytes are cumulative allocation/reallocation requests, not peak memory.

## Fixed inputs

The 105 performance inputs contain 109,965,477 pixels. Selection preceded timing.

| Inputs | Count | Selection |
| --- | ---: | --- |
| Shipped JPEG assets | 5 | Every shipped JPEG |
| User artwork available on this host | 19 | Deduplicated, stratified by process, color and sampling; size extrema/median selected deterministically |
| Small compatibility fixtures | 21 | Ten sequential, ten progressive, one YCCK |
| Kodak baseline 4:2:0 | 24 | All 24 pinned source photographs |
| Kodak progressive 4:2:0 | 24 | Same photographs and quality |
| Kodak grayscale, CMYK and encoded RGB | 12 | Photos 01, 06, 13 and 23 in each mode |

The JPEG derivatives were made with Python 3.13.14, Pillow 12.3.0 and
libjpeg-turbo 3.1.4.1 (libjpeg API 8.0), quality 85, explicit sampling, no
optimization, and empty EXIF/ICC. Encoded RGB uses `keep_rgb=True`. The original
Kodak manifest records source URLs, source/output hashes, dimensions, encoder
options and encoder implementation hashes. The generator validates source bytes
and actual JPEG process/sampling/color before recording an output.

The **19 runtime artworks are not redistributed**. [corpus-public-repo.txt](corpus-public-repo.txt)
lists the separate 86-input repository/public subset;
[corpus-local-runtime.txt](corpus-local-runtime.txt) identifies the additional
inputs needed for the original 105-file measurement. An 86-input reproduction
must be reported as that subset, with newly calculated totals.

Six 4:1:0, 4:1:1 and multiscan diagnostic fixtures are separate from performance
totals because their known reference interpolation/scan behavior differs. They
are included in exact native correctness checks. No input was excluded based on
timing. Every performance input was accepted by all four decoders in preflight.

## Stage identities

| Frozen name | Change from preceding retained stage | Decision |
| --- | --- | --- |
| `baseline` | Original native source | Reference |
| `cmyk` | Chunked exact CMYK/YCCK conversion | Retained |
| `vertical` | Direct full-width vertical interpolation | Retained |
| `entropy` | Reused Huffman prefix and combined progressive AC lookup | Retained |
| `mcu` | Cached MCU coordinates/restart boundary | Rejected: less than 0.5% paired gain |
| `left` | Left-aligned generic entropy bit buffer, based on `entropy` | Rejected: slower |
| `traversal` | Direct noninterleaved progressive block traversal, based on `entropy` | Final retained source |
| `packed` | Packed canonical/AC lookup, based on `traversal` | Rejected: 4.3–4.6% slower in both progressive comparisons |

`raw/*-build.json` records executable and source hashes, compiler identity and
the cumulative production patch from the baseline. Patches are gzip-compressed
to preserve their original bytes. Rejected candidates are retained as evidence,
not applied production changes. The standalone padded-block regression test was
added after freezing `traversal`; it does not affect production/benchmark code.
After rejecting `packed`, all 35 production/harness source identities were
verified against `traversal-build.json` before finalizing the change.

`raw/paired-stages-*-round{1,2}.tsv` records all five retained stages with
alternating order per file. `raw/final-rgba8-run{1,2}.tsv` records the final
four-codec comparisons. The earlier `*-rgba8.tsv` and CMYK/MCU repeat files show
the measurements used while choosing each change. Run JSON and paired journals
retain commands, environment, order, completion status and binary identities.

## Exact behavior and validation

The original binary captured 675 fixture/asset/photo inputs and 966 deterministic
malformed-input cases. Each of the 1,641 input cases exercises ten API/policy
combinations: pixels with both strict policies and both upsamplers, and
coefficient, lossless and hierarchical decode with both policies. Snapshots
include input bytes, dimensions, precision/sample variants, every public output
array, quantizers, ordered metadata, scan/consumed counts and exact error values.
Each candidate matched those original serialized bytes. Snapshots use lossless
compression and exact duplicate-result backreferences; equality is not hash-only.

The stored snapshots (about 605 MB) and JPEG payloads remain outside Git. Capture
and verification logs, generators and manifests are archived. The nine snapshot
helper checks include rejection of changed input, corrupt/truncated/extended
snapshots and attempted recapture. Default resource limits are captured; tighter
limits, stateful abbreviated sessions and reconstruction convenience APIs retain
their separate integration tests.

Final validation passed 190 tests normally, 190 with forced portable dispatch,
and 55 focused tests on i686. Strict Clippy passed for the crate and benchmark;
changed Rust files passed rustfmt. A transient Cargo launch failure during an
earlier MCU trial is preserved alongside the successful serial retry; it is not
counted as a passed run. No new production unsafe code or dependencies were added.

## Reproduction

All commands run from the repository root. `manifest.json` records both original
and stored SHA-256/lengths for every evidence file. Gunzip `.gz` without text
conversion. Restore root-level files from `raw/` to `target/jpeg-perf-20261119`;
the original scripts intentionally derive the repository root from that path.
Keep nested error-corpus manifests as references until the generator creates its
fresh output directory. Keep the archived evidence unchanged and use new names
for new measurement runs.

The archived TSVs/manifests suffice to recompute the report without images:

```powershell
python target/jpeg-perf-20261119/final-summary.py
```

To regenerate the public photo supplement, install the recorded encoder versions,
restore the helpers and fetch the already pinned source PNGs:

```powershell
python crates/deadlib-image/png/bench/results/windows-x86_64-20261119-rgba8/raw/fetch_corpus.py --output target/png-kodak-20261117
python target/jpeg-perf-20261119/jpeg-corpus-kodak.py
python target/jpeg-perf-20261119/jpeg-entropy-error-corpus.py
```

The Kodak generator refreshes its working manifest and list even when JPEG bytes
already match. Keep the archived manifest authoritative and compare regenerated
JPEG hashes with that archived copy before measuring.
The error-corpus generator requires a new `target/jpeg-perf-20261119/jpeg-error-corpus`
directory. Its 966 cases use six pinned 8/12-bit sequential/progressive sources,
up to 80 boundary-focused truncations and 80 single-bit entropy flips per source.

Build the benchmark and allocation executable separately:

```powershell
cargo build --manifest-path crates/deadlib-image/jpeg/bench/Cargo.toml --release --locked --bin jpeg-decode-bench --bin native_snapshot --target-dir target/jpeg-decode-bench
cargo build --manifest-path crates/deadlib-image/jpeg/bench/Cargo.toml --release --locked --features allocations --bin jpeg-decode-bench --target-dir target/jpeg-alloc-bench
python target/jpeg-perf-20261119/measure.py freeze rerun
python target/jpeg-perf-20261119/measure.py run rerun rerun-rgba8 --ms 80
python target/jpeg-perf-20261119/measure.py run rerun rerun-alloc --allocations
```

These default to all 105 original inputs. If the runtime artwork is unavailable,
pass `--files-from crates/deadlib-image/jpeg/bench/results/windows-x86_64-20261119-rgba8/corpus-public-repo.txt`
to each `measure.py run` command and label the result as the 86-input subset.
The fixed-corpus `analyze.py` and `final-summary.py` deliberately require 105
inputs; they must not silently turn a missing corpus into a complete result.

To reproduce an earlier stage, use a separate checkout of the baseline commit,
copy the benchmark `src/main.rs` and `src/bin/native_snapshot.rs` from this change,
then apply that stage's single cumulative patch before building. Preserve the
same Cargo.lock, release configuration and input bytes. Capture the original
native snapshots with that baseline binary, then verify them with the candidate
binary using identical lists. See the benchmark README and `native_snapshot --help`
for the capture/verify interface.
