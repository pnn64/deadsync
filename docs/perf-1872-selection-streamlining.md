# Song selection performance pass

Branch `perf/1872-selection-streamlining-20261009`, from committed main `4d64b1a8be48ae1f28bf60f288d5896adec05a63`. Version **0.5.1871 -> 0.5.1872** in Cargo.toml and all three corresponding Cargo.lock records.

Audited 33 distinct unmerged perf heads (59 local/remote refs). None modifies `song_search.rs` or `song_sort.rs`. No Song Lua implementation changed.

## Changes

1. Search original title/subtitle once. Search transliteration only when its resolved text differs. Partial transliteration and whitespace fallback retain the existing display methods and boundary matcher.
2. Build candidate difficulty labels in one reusable String using the existing formatter, then copy each final label into its existing Arc. Remove the per-candidate temporary String allocation. Reserve the normal 32-byte formatting capacity at the first match; no-match searches allocate no scratch buffer.
3. Keep the owned input vector for title/artist lists occupying one alphabetic bucket. Preserve stable sorting and spare-capacity trimming. Empty/singleton lists skip bucket-index allocation; single groups release indices before sorting.

Representative throughput: 1,024-song fallback-title search **1.36x**, 1,024 wide-meter candidates **1.12x**, 8,192-song single title group **1.10x**. Full controls and allocation counts follow.

## Measurement

Windows x64/MSVC, Intel Xeon E5-2696 v4 @ 2.20 GHz, rustc 1.98.1. Release optimization with `profile.release.lto=false`, locked dependencies. Original functions are frozen from the starting commit in test-only modules; the original inline annotation is preserved. Both versions run in the same executable.

Four fixed runs, each alternating original/current order for ten batches, discarding the first batch and taking the median of nine. Tables show the median of four run medians; throughput ratios divide those time medians, and ranges use the four paired run ratios. Search batches calibrate toward 2 ms. Owned grouping inputs are cloned outside timing; output destruction is timed. Measurements used the shared development host without CPU pinning. These are local throughput measurements, not frame-rate or hardware-cycle claims.

Thread-local System-allocator accounting is measured separately. The wrapper remains installed with tracking disabled during timing, so allocation-heavy timing ratios can reflect its counter-state check. Requested bytes measure churn, not RSS. Grouping inputs exist before counting; the raw counter peak is not total live memory.

### Throughput: all fixed cases

| Case | Original ns/op | Current ns/op | Throughput ratio | Paired range |
|---|---:|---:|---:|---:|
| candidate-meters-standard-1 | 1708.29 | 1778.73 | 0.960x | 0.956-1.046x |
| candidate-meters-wide-1 | 1765.93 | 1817.12 | 0.972x | 0.966-0.998x |
| candidate-meters-edits-1 | 1391.30 | 1496.71 | 0.930x | 0.739-1.005x |
| candidate-meters-standard-16 | 19800.39 | 19830.47 | 0.998x | 0.974-1.083x |
| candidate-meters-wide-16 | 21573.82 | 20357.42 | 1.060x | 1.014-1.163x |
| candidate-meters-edits-16 | 17101.17 | 16575.78 | 1.032x | 0.984-1.082x |
| candidate-meters-standard-1024 | 1492356.25 | 1460006.25 | 1.022x | 1.009-1.051x |
| candidate-meters-wide-1024 | 1622081.25 | 1446531.25 | 1.121x | 1.092-1.153x |
| candidate-meters-edits-1024 | 1256231.25 | 1178575.00 | 1.066x | 1.039-1.104x |
| search-title-fallback-miss | 162.38 | 98.28 | 1.652x | 1.541-1.754x |
| search-title-identical-miss | 171.66 | 108.59 | 1.581x | 1.483-1.617x |
| search-title-whitespace-miss | 181.57 | 114.30 | 1.589x | 1.548-1.617x |
| search-title-original-hit | 26.98 | 26.34 | 1.024x | 0.980-1.084x |
| search-title-boundary-hit | 81.28 | 79.94 | 1.017x | 0.963-1.060x |
| search-title-translit-hit | 64.70 | 68.12 | 0.950x | 0.935-1.016x |
| search-title-translit-miss | 166.63 | 164.33 | 1.014x | 0.955-1.036x |
| search-title-short-miss | 35.20 | 30.91 | 1.139x | 1.028-1.272x |
| search-title-empty | 28.19 | 28.79 | 0.979x | 0.941-1.022x |
| search-catalog-fallback-miss-1024 | 257728.12 | 189215.62 | 1.362x | 1.330-1.402x |
| search-catalog-translit-miss-1024 | 263081.25 | 265131.25 | 0.992x | 0.963-1.048x |
| search-catalog-all-match-1024 | 1283268.75 | 1224750.00 | 1.048x | 1.023-1.096x |
| search-catalog-selective-1024 | 278656.25 | 191665.62 | 1.454x | 1.285-1.477x |
| alpha-title-single-0 | 32.13 | 20.94 | 1.535x | 1.505-1.595x |
| alpha-artist-single-0 | 38.94 | 21.21 | 1.835x | 1.667-1.999x |
| alpha-title-single-1 | 187.11 | 129.21 | 1.448x | 1.389-1.619x |
| alpha-artist-single-1 | 190.96 | 125.57 | 1.521x | 1.478-1.611x |
| alpha-title-single-16 | 3706.79 | 3367.62 | 1.101x | 1.072-1.131x |
| alpha-artist-single-16 | 2679.25 | 2446.49 | 1.095x | 1.077-1.153x |
| alpha-title-mixed-16 | 2420.76 | 2463.57 | 0.983x | 0.945-1.021x |
| alpha-artist-mixed-16 | 2441.89 | 2433.35 | 1.004x | 0.996-1.050x |
| alpha-title-single-1024 | 84357.82 | 80445.31 | 1.049x | 0.995-1.104x |
| alpha-artist-single-1024 | 60260.94 | 54051.56 | 1.115x | 1.033-1.154x |
| alpha-title-single-1024-shuffled | 366792.19 | 361687.50 | 1.014x | 0.994-1.041x |
| alpha-artist-single-1024-shuffled | 266348.44 | 259028.12 | 1.028x | 1.017-1.052x |
| alpha-title-mixed-1024 | 76253.13 | 76609.38 | 0.995x | 0.959-0.996x |
| alpha-artist-mixed-1024 | 57390.62 | 59064.06 | 0.972x | 0.955-1.055x |
| alpha-title-mixed-1024-shuffled | 205231.25 | 198998.44 | 1.031x | 1.012-1.058x |
| alpha-artist-mixed-1024-shuffled | 153371.88 | 152364.06 | 1.007x | 0.981-1.021x |
| alpha-title-single-8192 | 761904.17 | 691629.17 | 1.102x | 1.065-1.135x |
| alpha-artist-single-8192 | 663237.50 | 611800.00 | 1.084x | 1.074-1.092x |
| alpha-title-mixed-8192 | 831579.17 | 840575.00 | 0.989x | 0.978-1.050x |
| alpha-artist-mixed-8192 | 727166.67 | 739541.67 | 0.983x | 0.959-1.003x |

Ratios above 1 favor the change. All controls, including lower medians, are retained. The no-match catalog case isolates duplicate-title removal; empty-query meter cases isolate buffer reuse. Selective/all-match text queries can exercise both. Alpha cases cover title/artist sorting, reverse/shuffled input, one/many buckets, and empty/singleton input.

Controls below parity in all four paired runs: candidate-meters-wide-1: 0.972x, 51.19 ns/op higher; alpha-title-mixed-1024: 0.995x, 356.24 ns/op higher. Other lower medians have paired ranges crossing parity. These measurements do not establish an all-input speedup.

### Allocation churn

| Case | Alloc calls old -> new | Realloc calls old -> new | Requested bytes old -> new |
|---|---:|---:|---:|
| candidate-meters-standard-1 | 8 -> 8 | 0 -> 0 | 264 -> 264 |
| candidate-meters-wide-1 | 8 -> 8 | 1 -> 1 | 368 -> 368 |
| candidate-meters-edits-1 | 8 -> 8 | 0 -> 0 | 217 -> 248 |
| candidate-meters-standard-16 | 98 -> 83 | 0 -> 0 | 3984 -> 3504 |
| candidate-meters-wide-16 | 98 -> 83 | 16 -> 1 | 5648 -> 4208 |
| candidate-meters-edits-16 | 98 -> 83 | 0 -> 0 | 3232 -> 3248 |
| candidate-meters-standard-1024 | 6147 -> 5124 | 0 -> 0 | 344080 -> 311344 |
| candidate-meters-wide-1024 | 6147 -> 5124 | 1024 -> 1 | 450576 -> 352368 |
| candidate-meters-edits-1024 | 6147 -> 5124 | 0 -> 0 | 295952 -> 294960 |
| alpha-title-single-0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| alpha-artist-single-0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| alpha-title-single-1 | 2 -> 1 | 0 -> 0 | 49 -> 48 |
| alpha-artist-single-1 | 2 -> 1 | 0 -> 0 | 49 -> 48 |
| alpha-title-single-16 | 3 -> 2 | 0 -> 0 | 192 -> 64 |
| alpha-artist-single-16 | 3 -> 2 | 0 -> 0 | 192 -> 64 |
| alpha-title-mixed-16 | 18 -> 18 | 0 -> 0 | 912 -> 912 |
| alpha-artist-mixed-16 | 18 -> 18 | 0 -> 0 | 912 -> 912 |
| alpha-title-single-1024 | 4 -> 3 | 0 -> 0 | 17456 -> 9264 |
| alpha-artist-single-1024 | 4 -> 3 | 0 -> 0 | 17456 -> 9264 |
| alpha-title-single-1024-shuffled | 4 -> 3 | 0 -> 0 | 17456 -> 9264 |
| alpha-artist-single-1024-shuffled | 4 -> 3 | 0 -> 0 | 17456 -> 9264 |
| alpha-title-mixed-1024 | 28 -> 28 | 0 -> 0 | 10464 -> 10464 |
| alpha-artist-mixed-1024 | 28 -> 28 | 0 -> 0 | 10464 -> 10464 |
| alpha-title-mixed-1024-shuffled | 28 -> 28 | 0 -> 0 | 10464 -> 10464 |
| alpha-artist-mixed-1024-shuffled | 28 -> 28 | 0 -> 0 | 10464 -> 10464 |
| alpha-title-single-8192 | 4 -> 3 | 0 -> 0 | 139312 -> 73776 |
| alpha-artist-single-8192 | 4 -> 3 | 0 -> 0 | 139312 -> 73776 |
| alpha-title-mixed-8192 | 28 -> 28 | 0 -> 0 | 74976 -> 74976 |
| alpha-artist-mixed-8192 | 28 -> 28 | 0 -> 0 | 74976 -> 74976 |

For 1,024 candidates, buffer reuse removes 1,023 temporary allocations; wide meters also remove 1,023 reallocations. An 8,192-song single alphabetic group removes one 65,536-byte handle-buffer allocation. Final group capacities and song identities are preserved. Standalone title matching remains allocation-free. The reusable scratch buffer lives until the call returns: normally 32 bytes, 64 for the wide case. An edit-only singleton therefore requests 31 more temporary bytes than its former one-byte buffer, while larger matching lists remove per-song heap operations; returned storage is unchanged.

## Behavioral and compatibility checks

- deadsync_simfile-tests: 208 passed, 0 failed, 3 ignored; 0 failure diagnostics identical to pristine baseline.
- semantic: 143 passed, 38 failed, 77 ignored; 38 failure diagnostics identical to pristine baseline.
- actor: 30 passed, 1 failed, 0 ignored; 1 failure diagnostics identical to pristine baseline.

Five new regression tests compare candidate text and pointer identity, Unicode/partial transliteration, query filters, stable ties, buffer ownership, churn, and alternating wide/narrow/edit-only difficulty labels. Three ignored benchmark tests passed separately.

The focused native suites retain pre-existing failures; they are not green. Every pre-existing test outcome was compared. Failure comparison removes panic thread IDs and sorts only the known unordered Sprite.Load missing-actor diagnostic lines; all other text and ordering are exact.

Fresh pristine-baseline and final Song Lua harness replays compare recorded native ITGmania traces; no new native capture was made.

| Song | Archive | Passed comparisons | Failed |
|---|---|---:|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` | 363873 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` | 363873 | 0 |
| 280\|MODS\|[MASTER] Sharkmode | `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` | 304425 | 0 |
| Warp Zone | `b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst` | 212220 | 0 |
| Let Me Hear That | `0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst` | 205071 | 0 |
| 272\|MODS\|[lv.02] Riddle | `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst` | 201471 | 0 |

All 1,650,933 full-song comparisons passed on both versions. No changed behavioral outcomes were detected.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib benchmark_selection -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
# Repeat for each archive in the table:
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive>
```

Raw run logs and comparison JSON remain in the ignored `target/` directory. Committed tests and frozen originals reproduce paired measurements. Source edits were confined to the new worktree; builds reused the ignored target cache from the earlier data-churn worktree. The original checkout was untouched; no merge or push was performed. The four user-excluded files are absent from the commit.
