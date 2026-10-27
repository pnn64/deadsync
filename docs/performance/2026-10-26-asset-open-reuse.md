# Native texture file reuse - 2026-10-26

Three independently measured changes in `deadlib-assets` reuse the file opened
for signature detection when decoding PNG, GIF, and indexed BMP textures. The
signature read remains eight bytes. A successful rewind replaces the second open
and close; decoder configuration, limits, palette processing, pixels, and errors
remain unchanged. No public API or dependency changes were made.

`image_source` returns the signature and its owned file together. `image_reader`
rewinds that file, explicitly dropping and reopening it if seeking fails. Native
decoders take the ready `BufReader<File>` directly. Small path constructors remain
under `cfg(test)` for the existing kernel tests. The GIF header-only metadata path
keeps its existing path constructor and open behavior.

Unsupported BMPs consume and drop the native reader before the generic fallback.
Other formats drop the signature file before their existing fallback. This preserves
resource closure at those boundaries. No generalized I/O layer or unsafe code was
introduced; the extra reader entry points express concrete ownership reuse.

## Separate before/after gates

Each format change was checked and benchmarked before the next edit. Values are
CPU cycles per decode, median of three process runs, each reporting the median of
eleven measured batches after warmup. Throughput is before/after CPU cycles.

| Isolated gate | Before | After | CPU throughput |
| --- | ---: | ---: | ---: |
| PNG, 16x16 | 415173.96 | 230485.30 | 1.801x |
| GIF, 16x16 frame | 339827.16 | 193274.89 | 1.758x |
| BMP8, 16x16 | 386856.75 | 197904.97 | 1.955x |
| BMP32, native rejection then fallback | 639388.09 | 452087.69 | 1.414x |

The unchanged-format controls stay within roughly 5% in these comparisons, while
the intended native cases improve 1.23x-2.01x. Raw rows include all controls:
[PNG gate](2026-10-26-asset-open-png.tsv),
[GIF gate](2026-10-26-asset-open-gif.tsv),
[BMP gate](2026-10-26-asset-open-bmp.tsv).

## Original versus final implementation

A fresh alternating comparison uses the final formatted source, with all three
changes applied. The allocation counts include file-path conversion, decoder
storage, and output image storage; results are dropped within the operation.

| Workload | Before cycles | After cycles | CPU throughput | Allocations |
| --- | ---: | ---: | ---: | ---: |
| PNG 1 | 500667.50 | 323220.59 | 1.549x | 9 -> 8 |
| PNG 16 | 423727.61 | 235597.24 | 1.799x | 9 -> 8 |
| PNG 256 | 731415.19 | 479456.62 | 1.526x | 9 -> 8 |
| GIF 1 | 344208.56 | 177409.16 | 1.940x | 12 -> 11 |
| GIF 16 | 353226.94 | 186605.86 | 1.893x | 12 -> 11 |
| GIF 256 | 947540.25 | 777194.56 | 1.219x | 12 -> 11 |
| BMP 1 | 380853.09 | 188349.87 | 2.022x | 4 -> 3 |
| BMP 16 | 380669.58 | 191618.37 | 1.987x | 4 -> 3 |
| BMP 256 | 577490.75 | 399627.19 | 1.445x | 4 -> 3 |
| BMP32 fallback | 620797.41 | 445303.76 | 1.394x | 13 -> 12 |

Every measured native-format and BMP fallback workload removes one Rust allocation
and free. Requested-byte savings depend on the temporary path length. Reallocation
counts remain unchanged. On the normal seekable route there is one file open for
native decoding instead of two; the unsupported-BMP route uses two opens instead
of three, including the generic fallback. A failed seek retains the original
reopen behavior and its costs.

The final throughput range is 1.22x-2.02x for the native test fixtures; BMP fallback
improves 1.39x. These results describe warm-file decoding on this machine, not game
frame rate, cold storage, or the complete startup corpus. Other jobs ran during the
session and wall times show substantial descheduling; selection uses CPU cycles
and allocation counts. All 240 process/workload rows, including control movement,
are retained in the four TSVs. [Final rows](2026-10-26-asset-open-final.tsv).

## Correctness and validation

The frozen reference includes signature detection, dispatch, dimension discovery,
and all three original native decoders from `0e6c38a56`. After replacing only the
reader-initialization statements and applying rustfmt, all three new decoder bodies
match those originals. Pixel math, color keying, allocation limits, and container
validation were not rewritten.

Each archived benchmark binary passes two common differential checks. They compare
complete images, top-level error categories, error text, and source dimensions for
both color-key modes, malformed/truncated fixtures, missing files, directories,
and the committed PNG/GIF/BMP/JPEG image corpus. Incorrect extensions are included;
the benchmark fixtures intentionally share a `.png` path and native dispatch uses
content signatures.

Real handle tests cover successful rewind without reopening (the supplied fallback
path deliberately does not exist), failed seeking on a pipe with successful
reopen, and failed seeking followed by the same missing-file error. Windows uses a
hidden Rust test child, not shell quoting or unsafe handle construction. Unix uses
a local socket pair converted through owned descriptors.

Validation:

- `cargo test -p deadlib-assets --locked`: **118 passed**, 20 ignored, 0 failed;
  [log](2026-10-26-asset-open-cargo-tests.log).
- All five optimized archives pass both frozen-reference checks; check output and
  executable hashes are in each comparison JSON. Successful checks are cached by
  executable SHA-256, so an unchanged archive is not redundantly retested.
- `cargo clippy -p deadlib-assets --locked --lib --tests --no-deps`: passed with
  existing chunk/DSL warnings and frozen-reference warnings;
  [log](2026-10-26-asset-open-clippy.log).
- Scoped rustfmt, Python compilation, fixed dependency hashes, source hashes,
  decoder-body equality, and staged whitespace checks pass.
- The earlier palette benchmark builder also compiles successfully against the
  reader-based entry points. Existing private kernel tests retain their path
  constructors; two signature-inspection tests now select the returned bytes.

Some worktree files were normalized to CRLF after measurement. Their staged LF
bytes match the recorded hashes exactly, and their text is unchanged.

## Measurement and reproduction

Windows x86-64, Xeon E5-2696 v4, rustc 1.99.1 (`58cf8f9c3`, LLVM 23.1.1).
Optimized snapshots use level 3, fat LTO, one codegen unit, static CRT, and the
same fixed dependencies. `QueryThreadCycleTime` measures the current benchmark
thread pinned to logical processor 2. Setup is excluded; signature I/O, file
opening, rewind, decoding, and result destruction are included.

The script compiles complete production dispatch, decoder, and fallback bodies.
It does not replace file I/O with a simulated kernel. Exact arguments and hashes
are in five `.build.json` manifests; the shared
[dependency manifest](2026-10-26-asset-open-dependencies.json) records their paths
and SHA-256 values. On another machine, build matching dependencies with Cargo
and supply those paths and hashes.

```powershell
python scripts/benchmark_asset_open.py build NAME --dependencies docs/performance/2026-10-26-asset-open-dependencies.json
python scripts/benchmark_asset_open.py compare --before BEFORE.exe --after AFTER.exe --output results.tsv --runs 3
```

For the baseline, restore production bodies from `0e6c38a56` while retaining the
new test files and their module declaration. The final decode source is reproduced
by applying [the final patch](2026-10-26-asset-open-final.patch) with
`git apply --unidiff-zero`. The manifests and frozen reference identify the
individual gates. [Audit](2026-10-26-asset-open-audit.json).
