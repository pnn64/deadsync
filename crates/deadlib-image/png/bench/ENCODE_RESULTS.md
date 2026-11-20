# PNG encoder measurements — 20 November 2026

The optional native **Rle/Adaptive writer** beats both the existing image PNG encoder and direct png Fast/Adaptive on all **13 measured screenshots** in the focused 80 ms run and the matching seven-mode 40 ms confirmation. All three artwork inputs also win. The initial seven-mode run had one slower Wendy result that did not reproduce; it remains documented below. The existing native **Fast/Adaptive default remains a separate compression policy**; the Rle result must not be attributed to default Fast.

The matching full confirmation uses **0.83605× image** and **0.82715× direct png Fast** screenshot CPU cycles. Across the final runs, screenshot cycles are **12.6–16.4% lower than image**. Screenshot PNGs are **2.17% larger by per-input geometric mean**, or **11.69% larger in total bytes**; the largest textured captures account for that difference. Generated inputs still have individual losses, detailed below.

The Rle production change is committed as **`4a41c507d751ed9770c773840b2c15777714cd8d`**, `perf(png): add native zero-run encoding for filtered images`, following the eight shared optimizations listed below. **Screenshot integration has not been performed; Fast remains the native default.**

## Scope and confirmed results

The workload encodes opaque RGBA8 pixels with Adaptive filtering. `native-buffered` and `native-writer` select native Fast; `native-rle-buffered` and `native-rle-writer` explicitly select Rle. References are pinned to **image 0.25.10** and **png 0.18.1**. Image's `PngEncoder::new` uses png Fast/Adaptive; direct png Fast/Adaptive and Balanced/Adaptive are separate controls. Identical preset names do not imply identical compression work or output sizes.

These primary results are from [final-confirm-full.tsv](results/2026-11-20-encode/final-confirm-full.tsv), the matching seven-mode 40 ms confirmation. Cycle and size ratios are per-input geometric means; lower is better. Both references produced the same encoded sizes. [final-confirm-summary.json](results/2026-11-20-encode/final-confirm-summary.json) retains all individual changes and losses.

| Group | Cycles/image | Cycles/png Fast | Wins / losses vs image | Wins / losses vs png Fast | PNG size/reference |
|---|---:|---:|---:|---:|---:|
| 13 screenshots | 0.83605× | 0.82715× | 13 / 0 | 13 / 0 | 1.02171× |
| 3 artwork images | 0.78001× | 0.77976× | 3 / 0 | 3 / 0 | 1.11650× |
| 16 generated inputs | 0.99374× | 0.98704× | 9 / 7 | 10 / 6 | 0.91158× |
| All 32 inputs | 0.90558× | 0.89858× | 25 / 7 | 26 / 6 | 0.97314× |

Screenshot aggregate throughput is **246.75 / 203.19 / 201.59 MPix/s** for native / image / direct png Fast. Corresponding total CPU cycles are **166,319,294 / 201,932,262 / 203,481,811** for one encode per screenshot. Aggregate throughput divides total pixels by summed per-input median times; it is not a parallel throughput measurement.

The narrowest screenshot confirmation margin is Mega initials at **0.91639× image / 0.91363× png Fast** cycles. The focused 80 ms [final-repeat-c.tsv](results/2026-11-20-encode/final-repeat-c.tsv) independently records **13/13 screenshot wins** at **0.83826× / 0.84321×**, with the same output sizes and fingerprints.

Screenshot PNG totals are **4,862,823 native versus 4,353,672 reference bytes**. The largest size differences are:

| Screenshot | Native bytes | Reference bytes | Size ratio |
|---|---:|---:|---:|
| Dyscontrolled production beat 46 | 1,903,043 | 1,629,623 | 1.16778× |
| `windows-before.png` | 1,745,105 | 1,534,249 | 1.13743× |

The matching full confirmation has seven cycle losses against image and six against png Fast: the four 17×13 cases, gradient 640×360 (**1.18687× / 1.20029×**), gradient 1920×1080 (**1.04292× / 1.03089×**), and flat 3840×2160 against image only (**1.02770×**). Gradient 640×360 produces **68,715 versus 76,029 bytes**. The focused 80 ms run had five losses; near-tie generated outcomes vary between runs. These exceptions prevent a claim that Rle is universally faster. Every run's full losses remain in its summary JSON, including the initial full-mode [summary](results/2026-11-20-encode/final-summary.json).

The complete seven-mode [final-cycles.tsv](results/2026-11-20-encode/final-cycles.tsv) uses 40 ms batches and the same executable/source identity as both confirmations. This table retains the initial full-mode run, including its Wendy outlier. Its screenshot Rle writer/png Fast cycle ratio is **0.87793×**; the image ratio is shown below.

| Mode | Screenshot cycles/image | All-input cycles/image | Screenshot wins vs image | Screenshot MPix/s | Screenshot size/image |
|---|---:|---:|---:|---:|---:|
| Native Fast buffered | 4.45536× | 4.42906× | 0/13 | 44.91 | 1.09275× |
| Native Fast writer | 4.10257× | 4.52327× | 0/13 | 44.85 | 1.09277× |
| Native Rle buffered | 1.33828× | 1.18343× | 2/13 | 177.17 | 1.02171× |
| Native Rle writer | 0.87362× | 0.94405× | 12/13 | 242.28 | 1.02171× |
| Image default | 1.00000× | 1.00000× | — | 207.10 | 1.00000× |
| png Fast/Adaptive | 0.99509× | 0.99578× | — | 207.79 | 1.00000× |
| png Balanced/Adaptive | 9.73277× | 11.28341× | — | 13.19 | 0.64880× |

**Unreproduced per-input outlier:** Wendy's initial Rle writer result is **5,215,203 cycles**, versus **4,116,273 image / 4,160,632 png Fast**, or **1.26697× / 1.25346×**. The focused 80 ms result is **0.87586× / 0.89392×**. Repeating the identical seven-mode 40 ms configuration gives **3,521,148 native cycles**, versus **3,924,564 / 3,927,761**, or **0.89721× / 0.89648×**. All other 12 screenshots win in all three runs. The executable/source identities match and output drift is zero. The cause of the initial slower observation is unknown; it is retained rather than discarded or assigned a presumed noise cause.

Default Fast itself improves substantially while preserving all original five-mode dimensions, encoded sizes and FNV fingerprints. Writer after/before cycles are **0.28476× overall / 0.24088× screenshots**, corresponding to **71.52% / 75.91% reductions**; the unchanged image control is **1.00793× / 1.00857×**. Buffered ratios are **0.30642× / 0.26660×**. Despite these reductions, default Fast still loses to image on all 32 inputs; the replacement-speed result applies specifically to explicit Rle writer encoding. See [final-default-change-summary.json](results/2026-11-20-encode/final-default-change-summary.json).

## Corpus and measurement method

The corpus contains **13 real game/menu screenshots, three textured artwork images, and 16 deterministic generated inputs**. Screenshots are all 1600×900. Flat, gradient, UI and noise patterns run at 17×13, 640×360, 1920×1080 and 3840×2160. No separate evaluation/results-screen capture was verified. Tiny inputs receive the same geometric-mean weight as large images, so groups and individual losses are reported separately. See [corpus.json](results/2026-11-20-encode/corpus.json).

External PNGs are decoded to RGBA8 and alpha is set to 255 before measurement, matching the screenshot alpha policy. Each measured operation returns and destroys a fresh PNG `Vec<u8>`; writer APIs collect into that same owned representation. Filtering, compression, checksums, framing and finalization are included. Input preparation, pixel verification, fingerprints and filesystem access are excluded. Screen capture, GPU readback, filesystem writes, buffered file I/O, format dispatch, resizing and previews are outside this encoder-core comparison.

The Windows thread is pinned to logical CPU 0. Each result is the median of five batches; mode order rotates between inputs. `QueryThreadCycleTime` supplies CPU cycles. Time and cycle medians are chosen independently, so their ordering can differ on a marginal result. CPU cycles take priority here. Inputs below one million pixels run in groups of eight, larger inputs one at a time. The initial full baseline used 40 ms batches; matched optimization stages used 20 ms; focused confirmations used 80 ms; the final seven-mode runs used 40 ms.

Native/reference and after/before results use geometric means of per-input ratios. Total cycles, encoded bytes, wins/losses and aggregate MPix/s complement those means. Unchanged reference controls are shown because system conditions move between runs. These results describe this machine, compiler and corpus; they are not statistical confidence bounds or evidence of equal performance on every platform. Timing builds are uninstrumented; allocation builds are separate.

The archive-time [environment snapshot](results/2026-11-20-encode/environment.json) records **Windows 11 build 26100**, **Intel Xeon E5-2696 v4 at 2.20 GHz, 44 logical CPUs**, **rustc 1.100.0 (0ef1bc386, 2026-11-09)** and **LLVM 23.1.5**. Release configuration uses optimization level 3, LTO, one codegen unit and static CRT. These archived host/toolchain facts were collected at archival time; they are not represented as measurements taken independently during every run.

## Eight optimizations preserving existing behavior

The original native encoder used **17.1773× image's screenshot cycles** in writer mode and **16.8552×** buffered; all-input ratios were **16.0105× / 14.5688×**. The first four commits reduced writer cycles by **70.52% on screenshots** and **66.38% overall** against the matched baseline rerun, while image moved only **+0.26%**. Those savings alone did not establish that default Fast was a faster replacement.

The table reports after/before **CPU-cycle ratios** as buffered / writer / unchanged image control (`B / W / I`). Every selected comparison has matching keys/dimensions and unchanged encoded size/FNV fingerprints. Fingerprints are consistency evidence, not cryptographic byte-equality proof.

| Commit | Removed or simplified work | Matched runs | All inputs B / W / I | Screenshots B / W / I |
|---|---|---|---|---|
| `70d69dcab` | Stop adaptive filtering at an unbeatable zero score | baseline-paired → step1 | 0.77825 / 0.79177 / 0.97134 | 0.79063 / 0.80283 / 0.96487 |
| `904d674c4` | Separate boundary bytes from the main filter loop | step1 → step2 | 0.75820 / 0.73529 / 0.99340 | 0.72078 / 0.69156 / 0.99774 |
| `0fe9e0a31` | Resolve repeated rows without evaluating every filter | step2 → step3 | 0.83206 / 0.78436 / 1.05498 | 0.89585 / 0.84754 / 1.05711 |
| `22ac3f77a` | Byte-only Paeth arithmetic and fixed narrow filter chunks | step3 → step5 | 0.71787 / 0.73628 / 0.98490 | 0.61457 / 0.62640 / 0.98520 |
| `a09758862` | Separate byte filtering from narrow score reduction | rle → score | 0.77376 / 0.79536 / 1.01165 | 0.70814 / 0.64614 / 1.02724 |
| `925d32ae6` | Borrow plain byte rows instead of packing allocated copies | word-literals → borrow | 0.97899 / 0.95553 / 1.01721 | 0.97036 / 0.95447 / 1.01367 |
| `f0431c5a1` | Inline the fixed-width filter kernel into the row loop | zero-mask → inline | 0.98177 / 0.98868 / 1.00844 | 0.97632 / 0.93865 / 1.01141 |
| `f036dfb9f` | Stop scoring None once it cannot win | skip-adler → bounded-none | 0.98271 / 0.95468 / 1.01312 | 0.99604 / 0.94727 / 1.04635 |

The first four rows select native Fast buffered/writer plus image. The final four select native Rle buffered/writer plus image, with identical compression behavior on each side of those comparisons. Their percentages must not be multiplied into a final default-Fast result. Step4 was retained byte-only Paeth, combined with step5's narrow chunks in `22ac3f77a`; the rejected Average experiment was separate.

Execution removes repeated scoring, per-byte boundary decisions, widening and row copies. Fixed chunks expose independent lanes to compiler vectorization, and narrow chunk sums avoid unnecessary wide reductions. None retains its original tie preference. Noninterlaced eight-bit rows borrow source slices; other depths/interlacing retain their packing paths. **The retained production changes add no new unsafe Rust.**

Full stage losses are preserved in [step1](results/2026-11-20-encode/step1-summary.json), [step2](results/2026-11-20-encode/step2-summary.json), [step3](results/2026-11-20-encode/step3-summary.json), [step3→step5](results/2026-11-20-encode/step35-summary.json), [scoring](results/2026-11-20-encode/score-change-summary.json), [borrowing](results/2026-11-20-encode/borrow-summary.json), [inlining](results/2026-11-20-encode/inline-summary.json) and [bounded None](results/2026-11-20-encode/bounded-none-summary.json).

## Optional Rle policy and allocation effects

Rle is a new explicit compression choice. It uses a fixed analytic dynamic DEFLATE tree for signed residual magnitudes and zero runs, with no input histogram or match dictionary. Long zero runs also update Adler without rescanning each byte. Safe four-literal stores batch bit output. The final scan checks six-zero prefixes at offsets zero and four under a 16-byte lookahead and otherwise emits eight literals. This removes detailed zero-start masks and avoids unprofitable four/five-zero stops. Encoded bytes differ from Fast and the reference; exact decoded pixels are preserved.

Compared with the four-zero halfword candidate, this scan reduces screenshot writer cycles to **0.95740×** against an image control of **0.99900×**, adding only **14,703 screenshot bytes**. Earlier eight-byte-only scanning also passed the speed gate but produced **5,033,774 screenshot bytes**; six-prefix produces **4,862,823**. No corpus-specific filename/dimension dispatch is involved.

Distinct confirmation runs identify their measured implementations:

| Run | Scan | Batch minimum | Screenshot wins vs image / png Fast |
|---|---|---:|---:|
| final-repeat-a | Eight-byte zero check | 80 ms | 13 / 13 |
| final-repeat-b | Two four-byte zero checks | 80 ms | 12 / 12 |
| final-repeat-c | Two six-zero-prefix checks | 80 ms | 13 / 13 |

Repeat-c matches the six-prefix 20 ms executable SHA256 and all 30 recorded source hashes. Both records report stable sources/inputs, with matching shared dimensions, encoded sizes and FNV fingerprints. Earlier candidate results are not substituted for the selected source.

The calibrated allocation build forwards requests to `System`. Counts include allocation, zeroed allocation and reallocation requests; requested bytes include replacement sizes. They are not peak live memory or RSS. Preparation/verification are outside the counters and instrumented timing columns are zero.

Row borrowing alone removes **exactly two allocation calls per input in each of all four native modes**, verified across 32 inputs. Each native mode saves 64 calls and 396,320 requested bytes; image is unchanged.

| Mode | Calls before → after borrowing | Requested bytes before → after |
|---|---:|---:|
| Fast buffered | 426 → 362 | 656,089,039 → 655,692,719 |
| Fast writer | 552 → 488 | 124,030,423 → 123,634,103 |
| Rle buffered | 373 → 309 | 663,105,498 → 662,709,178 |
| Rle writer | 468 → 404 | 129,997,314 → 129,600,994 |
| Image control | 691 → 691 | 472,892,108 → 472,892,108 |

Final allocation totals cover one encode of every input in the separate instrumented build:

| Mode | All 32: calls | All 32: requested bytes | Screenshots: calls | Screenshots: requested bytes |
|---|---:|---:|---:|---:|
| Native Fast buffered | 362 | 655,692,719 | 142 | 174,912,358 |
| Native Fast writer | 488 | 123,634,103 | 194 | 17,951,022 |
| Native Rle buffered | 298 | 676,796,157 | 116 | 179,938,047 |
| Native Rle writer | 394 | 112,206,868 | 155 | 13,242,507 |
| Image default | 691 | 472,892,108 | 271 | 23,430,812 |
| png Fast/Adaptive | 691 | 472,892,108 | 271 | 23,430,812 |
| png Balanced/Adaptive | 593 | 285,331,976 | 240 | 25,987,653 |

Rle writer uses **42.98% fewer allocation calls and 76.27% fewer requested bytes** than image over all inputs; on screenshots the reductions are **42.80% / 43.48%**. Rle buffered requests more bytes than image overall, so this memory improvement belongs to the writer path. These remain allocation-request metrics, not peak memory.

All 224 timing/allocation input-mode keys match with zero dimension, encoded-size or FNV drift. Against the original allocation baseline, both default Fast modes still save exactly two calls per input and 396,320 requested bytes over 32 inputs; all three controls are unchanged. See [final-alloc-summary.json](results/2026-11-20-encode/final-alloc-summary.json) and [final-alloc-change-summary.json](results/2026-11-20-encode/final-alloc-change-summary.json).

## Rejected alternatives

The retained implementation excludes these measured alternatives. Ratios below are writer after/before versus unchanged image control unless stated otherwise; full losses remain in each experiment's summary JSON.

| Alternative | Evidence and reason for rejection |
|---|---|
| Average identity specialization | All-input 1.07480 / 1.01614; screenshot 0.99383 / 1.00816. No reliable broad gain. Separate from retained byte-only Paeth. |
| Up-only filtering | Screenshot files were 3.31367× image size; unsuitable as the general screenshot policy. |
| Early four-literal scanner | Screenshot 0.98650 / 0.99011; artwork improved, but several large flat/gradient/UI cases regressed. This rejected scanning policy is distinct from the retained safe four-literal stores. |
| AVX2 Adler trial | Screenshot 1.01998 / 0.99982; reverted before the inlining comparison. |
| Exact zero-mask six-run guard | Screenshot 1.04233 / 0.99860, zero screenshot improvements; reverted. Distinct from retained direct six-prefix comparisons. |
| Generic word flush | Screenshot 1.24925 / 0.97993; all-input 1.15893 / 0.97763; reverted. |
| Literal pairs | Screenshot 1.06730 / 1.00381; no output drift; reverted. |
| Three-bit-zero tree | Screenshot cycle ratio to image worsened 0.91856→0.93302; size 1.01432→1.07377 and artwork size 1.10629→1.21605; reverted. |
| Five-literal packing | Screenshot 1.03500 / 1.01232; no output drift; reverted. |
| Packed code table | Screenshot 1.03387 / 1.00620; no output drift; reverted. |
| BMI2 literal writer | Screenshot 0.98663 / 1.00135; all-input 0.96742 / 0.99743. Gain too small to justify new unsafe code; fully reverted. |

BMI2's native assembly contained `SHLX`, and one native differential test passed. Miri was not run; this was not complete soundness verification. No new unsafe production path remains. Benchmark-only cycle FFI and allocator forwarding are separate from that production claim.

## Correctness and validation

Before timing each selected mode, every PNG is decoded and compared with exact input dimensions, RGBA8 format and pixel bytes. Direct Rust png also finishes the stream. The image PNG wrapper and direct png decoder share the underlying png implementation; these exercise two APIs, not two independent decoder engines. Separate native-stream tests use external flate2 as an independent DEFLATE decompression check.

Matched behavior-preserving comparisons require equal input/mode sets, dimensions, encoded sizes and FNV-1a64 fingerprints. Intentional Rle-policy changes explicitly permit and list drift while retaining exact pixel checks. FNV/size agreement is not cryptographic proof of identical encoded bytes.

Final production tests report **179 passed, zero failed and 21 ignored across 44 test summaries**. The 21 optional external-oracle tests were not run. Regular Rust png and flate2 comparisons ran; final source validation includes six-prefix scanner boundary cases at 14–17 bytes.

All three Clippy checks passed with warnings denied: production all-targets, benchmark encode/stages, and the allocation benchmark. Production and benchmark formatting checks passed. The production normal-dependency tree contains only `deadlib-image`. Exact commands, observed child exits and log SHA256 values are in [final-validation.json](results/2026-11-20-encode/final-validation.json). A capture-wrapper cp1252 printing error occurred after successful formatting/dependency checks; remaining work resumed with UTF-8 output, without rerunning those successful checks.

Two independent reviewers completed without blockers. The evidence audit verified all **301 recorded artifact SHA256/size entries** and the 224 matching timing/allocation rows. The archive totals **11,583,890 bytes including its manifest**. Screenshot integration remains unperformed; these checks validate the encoder implementation and its measured API paths.

## Provenance and reproduction

Evidence is stored in [results/2026-11-20-encode](results/2026-11-20-encode/): raw TSVs, logs, run provenance, comparator JSON, input/artifact SHA256 manifests, source-hash snapshots and explicitly included source artifacts. [runs.json](results/2026-11-20-encode/runs.json) indexes measured executable/source identities and exact commands. [environment.json](results/2026-11-20-encode/environment.json) is an archive-time host/toolchain snapshot, not a retroactive per-run environment recording.

The original baseline production HEAD was `cc30c0731`. A run's outer `sources`/`head` describes its invocation-time working tree; **`executable_source_snapshot.sources` records hashes of the measured source files**. The original executable was rerun after later edits, so HEAD alone is insufficient. First-build source-hash snapshots were captured at first measurement immediately after the build; they are useful provenance, not compiler-produced attestations. Executables are not archived.

[baseline-encode.rs](results/2026-11-20-encode/baseline-encode.rs) preserves the original five-mode harness: 23,467 bytes, reconstructed by removing only later Rle benchmark additions, with SHA256 `6b3b184913ab8db948ff4e302542016375fb9c7e7bacb679f9ea7750c1e02753`. Committed production source is recoverable from the listed revisions. Historical experiment source bodies are available only where explicitly included; hash-only snapshots do not make every rejected trial rebuildable.

**All 16 external PNGs are local-only and untracked**, including paths under `tests/fixtures`; they total **6,621,055 bytes**. The archive retains their original paths, dimensions, sizes and hashes, **not their image bytes**. A fresh clone can reproduce generated cases; reproducing this exact screenshot/artwork corpus requires separately supplied images matching [input-manifest.json](results/2026-11-20-encode/input-manifest.json). Original paths under `save`, `target` and `tests/fixtures` describe provenance, not tracked fixtures.

From the repository root, regenerate and measure only the **16 generated inputs**; `--sizes` implies generation:

```powershell
cargo build --release --locked --manifest-path crates/deadlib-image/png/bench/Cargo.toml --bin encode --target-dir target/png-encode-repro
$env:PNG_BENCH_MS = '80'
target/png-encode-repro/release/encode.exe --sizes 17x13,640x360,1920x1080,3840x2160 --modes image-default,png-fast-adaptive,native-rle-writer > target/png-encode-repro.tsv
python crates/deadlib-image/png/bench/compare_encode.py target/png-encode-repro.tsv --corpus crates/deadlib-image/png/bench/results/2026-11-20-encode/corpus.json --output target/png-encode-repro-summary.json
```

To measure the original 32-input corpus, append all 16 matching external PNG paths to the encoder command and retain `--sizes`. Passing `corpus.json` to the comparator labels result groups; it does not supply image bytes to the encoder. For allocations, build separately with `--features allocations` and a different target directory. The [benchmark README](README.md#encoder-workload) documents modes, exact verification and counter semantics.

To reproduce an isolated historical comparison, select shared modes explicitly:

```powershell
python crates/deadlib-image/png/bench/compare_encode.py crates/deadlib-image/png/bench/results/2026-11-20-encode/rle-cycles.tsv crates/deadlib-image/png/bench/results/2026-11-20-encode/score-cycles.tsv --modes image-default,native-rle-buffered,native-rle-writer --corpus crates/deadlib-image/png/bench/results/2026-11-20-encode/corpus.json --output target/png-score-comparison.json
```

These conclusions concern encoder-core work on the stated corpus. File I/O and end-to-end screenshot latency require their own integration measurements.
