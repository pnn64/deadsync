# PNG expansion and BMP palette decoding - 2026-10-25

Three safe Rust optimizations in `deadlib-assets` preserve decoded pixels and errors. Both Lua crates and the pre-existing Lua working-tree changes are excluded.

## Changes and work removed

1. **PNG RGB/grayscale expansion:** select the color layout once, then traverse fixed-size RGB or grayscale/alpha chunks. Remove the per-pixel color-type match, dynamic channel stride, and variable-length source indexing, giving LLVM a fixed channel count and stride.
2. **Packed PNG indices:** unpack an entire source byte into a fixed-size array of palette colors. The bit depth and mask become compile-time constants for the 1-, 2-, and 4-bit formats. Remove per-pixel multiplication, byte addressing, remainder, and depth-dependent shift calculations. Handle the last partial byte separately so unused bits never become pixels. An initial version using variable-size chunks had inconsistent benefits and was replaced with fixed arrays before retention.
3. **Indexed BMP rows:** reuse that byte unpacker for 1-, 4-, and 8-bit BMPs. Remove the per-pixel format match and palette bounds comparison. Complete palettes cannot contain an invalid index. For incomplete palettes, validate the decoded row using the existing alpha byte: supplied entries have alpha 255, while missing entries retain zero. Validation still precedes color keying and the next row read, preserving error precedence and ignoring row padding.

Production code removes **28 lines** and adds **52**, net **24 added lines**, including comments. The additions are fixed-format loops and one private unpacker shared by both formats. No production allocation, unsafe code, dependency, CPU feature requirement, lookup table, public API, or dispatch layer is added. The existing palette storage supplies all lookups and BMP validation state.

## Measurement method

Intel Xeon E5-2696 v4, Windows x86_64, rustc 1.99.1 / LLVM 23.1.1. Release optimization level 3, `--config profile.release.lto=false`, static MSVC CRT, and the default generic CPU target are identical across stages.

Each change was measured before editing its production implementation and again after editing it. Source snapshots and executables were archived. The final matrix rebuilt four production stages with identical benchmark instrumentation and fixtures:

- `before`: production decoder at `574ea9c9b`.
- `png`: change 1 only.
- `packed`: changes 1 and 2.
- `after`: all three changes.

Four fresh processes per stage and format family rotate stage order; each stage occupies each position once. Each benchmark pins its thread to logical processor 2, performs sixteen complete decodes per batch, discards one warmup batch, and selects the median of eleven batches ranked by Windows `QueryThreadCycleTime`. Tables report medians across four processes. CPU throughput is before cycles divided by after cycles. Process ratio ranges are descriptive minimum/maximum paired ratios, not confidence intervals.

These measure **complete cached-file decodes**, including opening/reading the file, decompression where applicable, pixel allocation/conversion, and output destruction. PNG fixtures use deterministic byte patterns, legal palettes, and `Compression::Fastest`. BMP fixtures use full palettes and bottom-up padded rows. Fixture generation and allocation probes are outside timing. No task-owned compilation overlaps the reported measurements. This is a shared host; CPU accounting and affinity do not remove cache, frequency, filesystem, or other process noise. These results do not measure whole-game FPS.

## Isolated changes: 1024 x 1024 images

| Change / workload | Prior stage cycles/image | Next stage cycles/image | CPU throughput | Process ratio range |
| --- | ---: | ---: | ---: | ---: |
| 1: RGB PNG | 17,219,143.84 | 14,981,012.38 | 1.149x | 1.098-1.203x |
| 1: grayscale PNG | 9,729,735.41 | 9,381,505.41 | 1.037x | 1.009-1.063x |
| 1: grayscale/alpha PNG | 13,803,710.31 | 12,999,174.21 | 1.062x | 1.005-1.169x |
| 2: 1-bit PNG | 6,978,385.19 | 4,467,387.47 | 1.562x | 1.475-1.652x |
| 2: 2-bit PNG | 7,227,702.71 | 4,701,724.32 | 1.537x | 1.492-1.583x |
| 2: 4-bit PNG | 7,673,562.00 | 5,080,245.18 | 1.510x | 1.454-1.623x |
| 3: 1-bit BMP | 9,219,823.15 | 7,220,980.69 | 1.277x | 1.225-1.332x |
| 3: 4-bit BMP | 10,229,893.47 | 7,899,667.85 | 1.295x | 1.228-1.330x |
| 3: 8-bit BMP | 8,835,012.34 | 8,263,351.90 | 1.069x | 1.043-1.076x |

All nine isolated comparisons improve in each of the four measured processes. At 512 x 512, the packed-PNG change improves 1.43-1.51x and the packed-BMP change improves 1.20-1.21x. The unchanged 1024 x 1024 RGBA and 8-bit indexed PNG controls finish within 1% of their starting throughput.

Small files have substantial process variation. In the primary matrix, 64 x 64 BMP4/BMP8 finish at 0.823x/0.901x starting throughput. A separate four-pair BMP confirmation measures the isolated third change at 1.181x/1.050x for those small cases, and 1.266x/1.265x for 1024 x 1024 BMP1/BMP4. BMP8 at 512 x 512 measures 0.979x in that confirmation. Retention is based on the repeatable large-image gains; no universal small-file speedup is claimed. Both datasets are retained.

## Allocations and correctness

The **600 operation probes** across the primary and confirmation matrices preserve allocation/reallocation/free counts and peak bytes for each workload. BMP uses four allocations; RGB/grayscale PNG uses nine; indexed PNG uses ten. Reallocation counts depend on decoder format and image size and are unchanged. Requested/freed byte totals vary by up to four bytes between processes, consistent with varying PID digits in temporary fixture paths; these are not claimed as optimization gains. Pixel storage and peak bytes are identical. Output destruction is included in the probes.

The complete release asset suite passes **102 tests across five executables** (93 unit and nine integration tests), with twelve benchmark tests ignored. All four archived stage binaries pass the four decoder parity tests. The four targeted parity tests also pass with debug overflow checks enabled. Clippy completes with existing repository warnings and none in the changed loops or new test code. Rustfmt checks the new Rust test file; Python byte-compilation and scoped `git diff --check` pass.

Differential checks cover RGB, grayscale, and grayscale/alpha PNGs at 8 and 16 bits; all indexed PNG depths; every partial-byte remainder; first-match palette color keying; transparency; BMP1/4/8; top-down and bottom-up rows; full and incomplete palettes; invalid indices; unused nonzero padding bits/bytes; truncation and allocation-limit errors; and the existing native bitmap fixtures. The reference decoders were already frozen in the repository before these changes.

The unpacker's private call sites use groups of 1, 2, 4, or 8 pixels. Index shifts are therefore bounded to 0-7 and every palette index is at most 255. The decoded width bounds the output extent; padding is excluded. BMP validation relies on alpha 255 assigned to every supplied palette entry before any color key is applied. No new unsafe soundness obligation is introduced.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --lib --tests -- --test-threads=1
cargo test --locked -p deadlib-assets --lib indexed_pixels_perf -- --skip benchmark --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-assets --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --lib benchmark_png_expansion -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --lib benchmark_bmp_expansion -- --ignored --nocapture --test-threads=1
python scripts/benchmark_decoder_expansion.py --stage before=target/decoder-expansion/before.exe --stage png=target/decoder-expansion/png.exe --stage packed=target/decoder-expansion/packed.exe --stage after=target/decoder-expansion/after.exe --output docs/performance/2026-10-25-decoder-expansion.tsv
python scripts/benchmark_decoder_expansion.py --stage packed=target/decoder-expansion/packed.exe --stage after=target/decoder-expansion/after.exe --kind bmp --output docs/performance/2026-10-25-decoder-expansion-bmp-confirm.tsv --logs target/decoder-expansion/bmp-confirm
```

To reconstruct archives, use separate worktrees at `574ea9c9b` and this commit. Copy the test-only `indexed_pixels_perf.rs`, `decode/expansion_perf.rs`, and `decode/preparation_perf.rs` from this commit into the baseline tree. Build baseline production for `before`; apply only the nonindexed PNG expansion hunk for `png`, then the unpacker and packed-PNG hunk for `packed`; build this commit for `after`. Archive the `deadlib_assets` test executable emitted by Cargo's `--message-format=json` artifacts. Run comparisons after compilation finishes.

[Primary measurements](2026-10-25-decoder-expansion.tsv) contain all 33 workloads at four stages across four processes, including cycles, elapsed time, pixels/cycle, megapixels/second, and every allocation counter. [BMP confirmation](2026-10-25-decoder-expansion-bmp-confirm.tsv) contains the additional 72 measurements. Local source snapshots, measured executables, build/test/lint logs, and process logs are under `target/decoder-expansion`.

| Measured stage | Executable SHA-256 |
| --- | --- |
| `before` | `114888fe8bf120535d7e71f64a80fdf7c2f12b486e8a23782a4a5925f97f720f` |
| `png` | `5fc7bcc46b8d49679946220af7f3bdb277c86d8cfc02c4fa3d29bc0d35d4a52d` |
| `packed` | `46e6666fe49ed11f219f78a99db0ab4bdd2a6e80bfbe2dfaec31fd79cf962df6` |
| `after` | `fa872745b3960d974bcd6fd04d2ff1f77833f83da3fbdf3b2e6e9504b884f42c` |
