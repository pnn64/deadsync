# Prepared uploads, alphamap cleanup and cache option hashes — 2026-11-04

Baseline: `294b5139a0b28c46973abcbd1a617431ea4c1fac`. Three safe Rust optimizations remove repeated work:

1. Prepared uploads pass their existing source-size metadata into push_inner. This removes the second HashMap lookup and the temporary None metadata value. All other enqueue methods pass None, retaining ordinary replacement behavior. The private helper is inlined; public signatures, order handling, saturated byte accounting and recycler Drop code are unchanged.
2. After an applied alphamap, every complete pixel already has white RGB. When any alpha is nonzero, native hidden-alpha cleanup would only rewrite those same values, so it is skipped. An entirely transparent result still clears all complete pixels to zero. The optimization also checks is_default: public hints can have alphamap set while non_default is false, in which case the original generic cleanup remains necessary.
3. Cache option hashing has only two valid inputs. Their exact XXHash64(seed 0) results for bytes `[1, enabled]` are precomputed. A bounded boolean lookup replaces hasher initialization, two writes and final mixing. Cache paths, filenames and the streamed path hash retain their behavior.

No production fields, dependencies, unsafe blocks or public API parameters are added. One private helper gains a source-size argument; five call sites supply the existing value or None. Inlining those entry points has an unmeasured application code-size impact. The slower returned-entry and noninlined constructor trials were discarded. A factored budget check showed no useful benefit and was restored exactly.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, rustc 1.99.1 / LLVM 23.1.1. All formal measurements execute the real deadlib-assets unit-test binary built by Cargo with release opt-level 3, fat LTO and the static CRT. Five fresh processes use twelve alternating original/current rounds on CPU 2, discard the first and retain the median of eleven. Original references share the actual package dependencies and types where applicable; the frozen queue duplicates its unchanged private type definitions and checks their size, alignment and field offsets.

Cycle columns are independent process medians; throughput is the median of paired ratios. Raw TSVs retain nanoseconds. Inputs/files are prepared outside timing. Enqueue samples perform 100,000 operations; option-hash samples process 128 options for 10,000 fixtures. Path samples use 1,000 operations. Native PNG samples use 64 operations (16 at 1024²). Preparation-only samples scale from 16 to 7,812 operations by image size. Function pointers and arguments are black-boxed. Background development builds were present; the thread-cycle and identical-source controls quantify the comparison.

`prepare/body` isolates the post-resize pixel work and reuses identical evolving buffers. Its modes are 0=black, 1=white, 3=mixed; after warmup visible alphamaps are white. Both sides receive the same buffer evolution. This kernel is mechanically derived from each compiled decode_texture source, replacing only the image reborrow syntax. The driver verifies the derivation before compilation. `decode` uses real encoded PNG files, file I/O, decoding, allocation, preparation and destruction. `options/path` includes real canonicalization and returned-buffer construction. These are separate scopes; a pixel-loop gain is not a complete decoder or application gain.

| Workload | Original cycles | Current cycles | Paired throughput |
| --- | ---: | ---: | ---: |
| `decode/black/256` | 2206036.70 | 2258802.50 | 1.005× |
| `decode/default/256` | 2265754.45 | 2219048.95 | 1.001× |
| `decode/gray/256` | 2375429.03 | 2318918.08 | 1.003× |
| `decode/hidden/256` | 573584.38 | 541523.67 | 1.063× |
| `decode/mixed/16` | 215391.25 | 217768.00 | 0.999× |
| `decode/mixed/256` | 2180938.28 | 2202919.16 | 1.012× |
| `decode/white/1024` | 33245758.25 | 33068415.81 | 1.014× |
| `decode/white/16` | 283340.20 | 279306.91 | 1.044× |
| `decode/white/256` | 2226405.70 | 2238094.00 | 1.002× |
| `options/hash/0` | 5224.39 | 586.99 | 8.913× |
| `options/hash/1` | 5186.43 | 603.91 | 8.597× |
| `options/hash/2` | 5150.19 | 607.07 | 8.686× |
| `options/path/false` | 227659.91 | 228753.68 | 1.003× |
| `options/path/true` | 227436.24 | 228391.95 | 0.996× |
| `ordinary/cycle` | 148.82 | 134.45 | 1.107× |
| `prepare/body/1024/0` | 3150661.62 | 3313503.31 | 0.960× |
| `prepare/body/1024/1` | 2330801.94 | 1708423.38 | 1.341× |
| `prepare/body/1024/3` | 2283225.19 | 1739633.44 | 1.343× |
| `prepare/body/16/0` | 821.30 | 854.26 | 0.959× |
| `prepare/body/16/1` | 524.45 | 425.40 | 1.267× |
| `prepare/body/16/3` | 535.46 | 425.37 | 1.266× |
| `prepare/body/256/0` | 183384.93 | 187782.23 | 0.983× |
| `prepare/body/256/1` | 147891.80 | 104767.37 | 1.316× |
| `prepare/body/256/3` | 140436.10 | 106267.23 | 1.301× |
| `prepared/cycle` | 147.83 | 134.26 | 1.104× |
| `prepared/replace/1` | 91.41 | 83.24 | 1.120× |
| `prepared/replace/256` | 94.74 | 90.29 | 1.118× |
| `prepared/replace/64` | 87.71 | 81.04 | 1.094× |

Prepared-upload workloads improve **1.094×–1.120×**, visible-alphamap preparation **1.266×–1.343×**, and option hashing **8.597×–8.913×**. Ordinary upload cycling improves 1.107×. The retained tradeoff is fully black alphamap preparation: 16² and 1024² inputs run at **0.959× and 0.960×**, about **4.2% more CPU** by paired ratios; the 256² result is 0.983× and remains near control variation. Fully black images still require the clearing pass.

Identical-source control medians span **0.978×–1.044×**, with individual process pairs 0.856×–1.130×. Small differences near that spread are inconclusive. Native decoding improves 1.063× for the constant-color hidden-alpha fixture; other complete decoder and canonicalized-path results remain inconclusive. In particular, the white 16² decoder result is 1.044× and its identical-source control is also 1.044×. These results support gains in the measured kernels and queue operations, without establishing a general decoder or application speedup.

## Allocation behavior

All six tracked allocation fields match between original/current for every fixture and process. **17 of 28 workloads remain allocation-free**: reused upload operations, all preparation-only kernels and the three option-hash workloads. Native decoding and path construction retain their existing allocations, frees, bytes and peaks. The table below gives representative final-process counts; complete per-process counters are in the TSV.

| Fixture | Allocs | Reallocs | Frees | Allocated bytes | Freed bytes | Peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `decode/black/256` | 8 | 1 | 8 | 683372 | 683372 | 552056 |
| `decode/default/256` | 8 | 1 | 8 | 683372 | 683372 | 552056 |
| `decode/gray/256` | 8 | 1 | 8 | 683372 | 683372 | 552056 |
| `decode/hidden/256` | 8 | 1 | 8 | 683372 | 683372 | 552056 |
| `decode/mixed/16` | 8 | 1 | 8 | 38508 | 38508 | 36984 |
| `decode/mixed/256` | 8 | 1 | 8 | 683372 | 683372 | 552056 |
| `decode/white/1024` | 8 | 1 | 8 | 4615532 | 4615532 | 4484216 |
| `decode/white/16` | 8 | 1 | 8 | 38508 | 38508 | 36984 |
| `decode/white/256` | 8 | 1 | 8 | 683372 | 683372 | 552056 |
| `options/path/false` | 4 | 0 | 4 | 537 | 537 | 370 |
| `options/path/true` | 4 | 0 | 4 | 537 | 537 | 370 |

## Exact behavior and gates

- **117 complete asset tests pass in release**, with 28 ignored benchmark/trace fixtures. The same **117 tests pass with opt-level 0, debug assertions and overflow checks enabled** for deadlib-assets. Cargo's recorded artifact profiles verify these settings; dependencies remain the cached release builds. This avoids claiming an untested all-dependency debug build.
- Queue tests compare **8,192 mixed mutations** and **3,456 boundary budget cases**, including duplicate/stale order entries, handle zero/MAX, all enqueue variants, source-size replacement, saturated counters, zero-byte images, zero/MAX budgets and bounded recycler channels. Private order, capacities, entries, bytes, sampler/source metadata, image bytes, Arc identities/counts and exposed YUV conversion bits match. Borrowed YUV exposes a slice rather than its capacity; returned recycler buffer capacities are checked instead. Separate tests verify returned RGBA buffers retain their original allocation address.
- Native preparation checks compare **605 complete decoder results**, including pixels, dimensions, original source sizes, capacities and errors. They exercise contradictory public hint flags, grayscale, resize limits, invalid minimum sizes, malformed/missing files and every low RGB triple that rounds to zero luminance. **640 additional hint/storage cases** cover complete chunks, incomplete tails, excess storage and in-place capacities in the derived preparation kernel.
- Option-hash verification exhausts both boolean inputs. Native path checks compare exact PathBuf/String values and capacities, Unicode filenames, relative/absolute cache roots and missing files. The path-building body itself is unchanged.
- Final release, opt-level-zero and the expanded control preserve the following exact traces. Arc addresses and temporary filenames are checked within a process and excluded from serialized output.

| Trace | Bytes | SHA-256 |
| --- | ---: | --- |
| upload | 4,005,697 | `d3b7c2d4daeb5c05ddd191f636c7b650611c2fcccea0959c805a53380248289b` |
| alpha | 214,321 | `fca7b556289011637a7049f804c9f0c13f983bd96c47917f6a5110069c266eae` |
| options | 16 | `5b155547df569dfc1b17e721788b99e11a24f2f3f6207899a9b6e78e7e8dc850` |

The first baseline has 19 workloads and a shorter native-only alpha trace. The expanded baseline adds nine preparation-only workloads and 640 storage cases. Its upload/options traces match the first baseline, and its alpha trace has the original trace as an exact prefix. An initial comparison incorrectly expected the longer trace to equal the shorter one; the already verified 117-test binary was reused, with binary/source hashes checked, to finish the expanded control record. No completed earlier timing record was overwritten.

`expanded-baseline`, `debug` and `final` are the authoritative corpus. `baseline` remains an earlier complete 116-test/19-workload control. The audit JSON checks reference provenance, production scope, generated kernel fidelity, every source/archive/compiler profile, binary/log/trace hash, raw timing row and allocation field. The normal production-only release check is fingerprinted against the current files. Clippy gates changed functions and new test logic; unused contains/dimensions methods in the unchanged frozen queue reference are recorded rather than edited. Other existing diagnostics are retained in the Clippy log.

The frozen queue prefix's final separator blank line was removed after measurement to satisfy the publication whitespace gate. The audit records both source hashes and verifies this is exactly one final blank line, with every compiled statement unchanged. Recorded artifacts retain their original source snapshots.

## Reproduction

```powershell
$env:PYTHONUTF8='1'
$env:DEADSYNC_UPLOAD_PREPARATION_ARCHIVE='D:\deadsync-upload-prep-20261104'
python scripts/benchmark_upload_preparation.py expanded-baseline --baseline 294b5139a0b28c46973abcbd1a617431ea4c1fac --runs 5 --reference expanded-baseline
python scripts/benchmark_upload_preparation.py debug --debug --reference expanded-baseline
python scripts/benchmark_upload_preparation.py final --runs 5 --reference expanded-baseline
```

The driver verifies and extends the prior partner-chart source archive, overlays package/source changes and snapshots the existing external decoder fixtures from the main checkout. Their root can be supplied with DEADSYNC_NOTEFIELD_FIXTURE_ROOT. Baseline production files come from the specified Git revision with only test registrations added; its preparation-only kernel is derived from that same source. New fixtures are generated in owned temporary directories. Compilation uses D: temporary storage; tests and benchmarks retain the host C: temporary directory. Source snapshots, executables, logs and traces are archived under `D:\deadsync-upload-prep-20261104`. Raw TSVs and manifests accompany this report. Lua source, device I/O and the user's main checkout are unchanged. No full-application or cross-platform speedup is claimed.
