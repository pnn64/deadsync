# Browser and palette text performance — 2026-10-10

Branch: `perf/1872-browser-palette-20261010`. Base: `8711fc76304beb7ec26866e1a7a99e5647469e64` (committed main at worktree creation). Version: **0.5.1871 → 0.5.1872** in Cargo.toml and Cargo.lock. No merge or push.

Audited 73 unmerged local/remote perf refs (47 distinct commits). Existing changes to content-browser metadata, badges, and doubles rendering are separate functions. This pass changes no Song Lua code.

## Changes

1. Write comma-separated browser counts into a small stack buffer, then allocate the final string once. Removes the temporary decimal String and its copy, preserving zero, separator boundaries, and the full usize range.
2. Format palette-editor RGB text directly into its final string. Removes three component strings and an intermediate vector for each of seven color rows, preserving padding, spaces, selected-channel brackets, rounding and saturation.
3. Pass static content-browser tab labels, context headings and footer credit directly to text actors. Removes 10–11 string copies per composition while preserving all visible actor properties.

## Paired release benchmarks

Intel Xeon E5-2696 v4, Windows x86-64, rustc 1.98.1. Release with LTO disabled, unchanged compiler flags and feature graph for both implementations. Original function bodies are frozen from the base commit in test-only modules and verified against git. Four fresh processes, each pinned to logical CPU 2 at AboveNormal priority; three warm calls, calibration, then nine alternating timed batches per variant. Values below are medians across the four process medians. Timed calls consume outputs with black_box and include output destruction; actor vectors are preallocated and reused. Benchmarks run without concurrent builds or compatibility tests from this pass.

The scoped System allocator counts allocations, reallocations and requested bytes separately. Peak means additional live requested bytes during the measured operation, excluding pre-existing inputs/buffers and allocator overhead; it is not process RSS. Timing uses the same instrumented allocator for both variants with counting disabled.

| Workload | Original ns | Current ns | Throughput | Paired range | Allocations | Reallocations | Requested bytes | Peak added bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| chrome/pad | 3,076.10 | 2,229.79 | 1.380× | 1.350–1.416× | 20 → 10 | 1 → 1 | 697 → 600 | 0 → 0 |
| chrome/search | 3,867.14 | 2,909.51 | 1.329× | 1.293–1.393× | 22 → 11 | 3 → 3 | 792 → 681 | 0 → 0 |
| chrome/beginner | 3,862.94 | 2,907.66 | 1.329× | 1.303–1.403× | 22 → 11 | 2 → 2 | 882 → 768 | 0 → 0 |
| commify/0 | 138.72 | 67.69 | 2.049× | 1.930–2.062× | 2 → 1 | 0 → 0 | 2 → 1 | 2 → 1 |
| commify/9 | 136.76 | 69.20 | 1.976× | 1.903–2.005× | 2 → 1 | 0 → 0 | 2 → 1 | 2 → 1 |
| commify/999 | 147.73 | 75.73 | 1.951× | 1.883–1.988× | 2 → 1 | 0 → 0 | 7 → 3 | 7 → 3 |
| commify/1000 | 153.69 | 78.15 | 1.967× | 1.895–2.084× | 2 → 1 | 0 → 0 | 9 → 5 | 9 → 5 |
| commify/123456 | 156.00 | 81.45 | 1.915× | 1.790–1.987× | 2 → 1 | 0 → 0 | 14 → 7 | 14 → 7 |
| commify/123456789 | 173.61 | 87.97 | 1.973× | 1.841–2.057× | 2 → 1 | 0 → 0 | 21 → 11 | 21 → 11 |
| commify/18446744073709551615 | 208.09 | 108.28 | 1.922× | 1.844–1.980× | 2 → 1 | 0 → 0 | 46 → 26 | 46 → 26 |
| readout/pad | 370.66 | 298.08 | 1.243× | 1.179–1.282× | 3 → 2 | 0 → 0 | 49 → 44 | 44 → 44 |
| readout/years | 365.26 | 293.93 | 1.243× | 1.201–1.276× | 3 → 2 | 1 → 1 | 35 → 30 | 22 → 22 |
| readout/search | 543.08 | 466.72 | 1.164× | 1.130–1.216× | 3 → 2 | 2 → 2 | 76 → 71 | 44 → 44 |
| palette/idle | 8,231.34 | 5,391.45 | 1.527× | 1.515–1.541× | 53 → 25 | 0 → 0 | 965 → 300 | 82 → 0 |
| palette/adjust | 8,663.46 | 5,619.69 | 1.542× | 1.516–1.579× | 53 → 25 | 1 → 0 | 977 → 304 | 82 → 0 |
| palette/rename | 9,040.42 | 5,763.72 | 1.569× | 1.512–1.614× | 53 → 25 | 1 → 1 | 993 → 328 | 82 → 0 |

Number-formatting cases cover zero, small counts, separator boundaries, larger counts, and usize::MAX. Readout cases measure the complete production formatter for PAD, YEARS and SEARCH with 12,345 results. Palette cases measure complete editor composition when its retained presentation must be rebuilt; unchanged cache hits are outside these timings. Chrome cases compose the complete tab strip, context band and footer. These are local CPU/allocation measurements, not end-to-end FPS claims.

A grouped course-index prototype was rejected after its 100 single-song-pack control increased requested allocation bytes from 29,340 to 44,288. Its source snapshot and allocation-probe log are retained under target/ for audit.

## Regression and ITGmania compatibility

Both variants were built from the same combined graph:
```text
cargo test --release --locked --config profile.release.lto=false
  -p deadsync -p deadsync-simfile -p deadsync-theme-simply-love
  --lib --test song_lua_itgmania_semantic_parity
  --test itgmania_actor_conformance --test full_song_lua
  --no-run --message-format=json
```

The resulting simfile/theme unit tests and semantic/actor compatibility executables were run with --test-threads=1. The full-song executable was run against six recorded ITGmania archives. Root library was compiled, not executed. All 848 tracked noteskin assets were checked against their git blob hashes before the baseline run.

| Suite | Passed | Failed | Ignored |
|---|---:|---:|---:|
| deadsync_theme_simply_love-tests | 1270 | 1 | 6 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| semantic | 144 | 40 | 77 |
| actor | 30 | 1 | 0 |

Every pre-existing individual test outcome and normalized failure diagnostic matched baseline. Added regressions cover over 30,000 number-formatting inputs, powers of ten, usize::MAX, readout counts and quoting, palette channel boundaries and nonfinite inputs, focus/caret states, and browser text/geometry/order across all tabs and zones. Resource checks cover final-string allocation, palette vectors and static-label copies. Only thread IDs and separately re-proven unordered diagnostics are normalized.

| Native capture | Comparisons | Failures |
|---|---:|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis — `0f2ef3f98744` | 363,873 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis — `af2f887d212d` | 363,873 | 0 |
| 280\|MODS\|[MASTER] Sharkmode — `b05379b7d12b` | 304,425 | 0 |
| Warp Zone — `ded0f7ff1951` | 212,220 | 0 |
| Let Me Hear That — `2a77063dd2ab` | 205,071 | 0 |
| 272\|MODS\|[lv.02] Riddle — `7ffacb89fd95` | 201,471 | 0 |

## Reproduction and evidence

Run the paired benchmarks in the same combined test feature graph with the filter `benchmark_course_data --ignored --nocapture --test-threads=1`. The test-only original modules identify their source commit. For allocation measurements, leave this branch’s scoped allocator support enabled.

Ignored worktree target/ holds baseline/current build logs, executable manifests and hashes, individual suite logs, six archive outputs per variant, four benchmark logs, benchmark-summary.json, compatibility-comparison.json, pending-branch audit, original-checkout snapshots, source hashes and diagnostic reruns. A shared generated Cargo cache on C: avoids duplicating dependencies; source changes and commits are confined to this D: worktree. Both completed semantic/actor/full-song runs use the same local temporary extraction directory in that cache. Earlier network-temp compatibility attempts were interrupted at the I/O bottleneck and their partial logs are retained separately; they are excluded from the final comparison.

Excluded from the commit: deadsync-song.json.gz, rust-performance.md, optimize.sh and optimize.ps1. The original checkout was never written by this pass; its main branch and Song Lua work may advance independently.
