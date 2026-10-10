# Library streamlining performance pass

Branch: `perf/1872-library-streamlining-20261009`.
Base: `16a06a2619a6cdcc34c647603d5dfa186e2f3e73`, committed local `main` when the worktree was created.
Version: **0.5.1871 -> 0.5.1872**, including Cargo.toml and all three workspace-version records in Cargo.lock.

## Changes and scope

1. **Background changes:** consume resolved background vectors and move their strings into the cache representation. Valid UTF-8 paths transfer their existing buffers through `OsString::into_string`; invalid OS strings retain the original lossy conversion. Both background layers use the same transfer helper, which returns immediately for empty input. The existing borrowed conversion API is unchanged. The change removes the clone-then-drop sequence for effects, transitions, animation names and paths.
2. **Chart BPM bounds:** replace two filtered scans with one scan that computes both extrema. The positive-finite filter, f32-to-f64 conversion, ordering and empty/invalid defaults are unchanged. The two old helpers are removed.
3. **Artwork:** consume the directory image list after filename-hint classification and move selected paths into the dimension-based results. This removes seven clone call sites. Hint precedence, classification order, image-header reads, selection conditions and early termination remain unchanged.

The pending audit checked 31 distinct unmerged heads (53 local/remote refs). The pending data-churn branch also changes `cache.rs`, but its cache metadata serialization, note grouping and path probing functions are separate from this pass's background conversion. No pending branch changes the selected `song.rs` or `artwork.rs` logic. The test-only `metadata_perf.rs` helper and its lib.rs declaration are identical to those on the pending metadata-ownership branch, so this pass reuses that support instead of introducing another allocator helper.

Production code adds no dependency or persistent cache. Work and commits are isolated from the original checkout, whose ongoing uncommitted work was left untouched. Nothing was merged or pushed.

## Results

- **Background transfer:** 1,024 records improve **3.127x (file), 3.560x (animation), 3.142x (mixed)**. The file/animation cases replace **4,097 allocations with one vector resize**; the mixed case replaces 3,585 allocations with one resize. There are no per-field allocations for these valid UTF-8 inputs. Nonempty cases range from 1.601x to 3.560x; the explicit empty fast path also improves throughput.
- **BPM bounds:** nonempty inputs improve **1.146-2.129x**. For 4,096 BPM entries, the ratios are **1.585x** with finite values and **1.636x** with mixed invalid values. Both versions allocate zero times.
- **Artwork selection:** allocations fall **52 -> 49** for three dimension selections and **97 -> 91** for six selections, with unchanged reallocation counts. Requested bytes fall by 507 and 1,014 respectively. Median full-resolver throughput ratios are 1.032x and 1.024x, but their four-run ranges cross parity; the demonstrated benefit is allocation removal, not a reliable filesystem throughput gain.

Controls are included below. The tagged-artwork control measured 0.987x throughput (1.3% lower), with a four-run range of 0.959-1.026; the empty-artwork control measured 0.995x. Their allocation counts are unchanged. No whole-game speedup is inferred from these timings.

Allocation counts below are shown as **allocations + reallocations**, not a claim of zero heap activity when a vector is resized. Requested bytes include allocation and reallocation requests; they are neither RSS nor net retained memory.

| Case | Original ns/op | Current ns/op | Throughput ratio | Four-run ratio range | Allocations + reallocations, original -> current | Requested bytes, original -> current |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| artwork/empty | 136425.00 | 137056.25 | 0.995 | 0.968-1.014 | 4 + 1 -> 4 + 1 | 996 -> 996 |
| artwork/corrupt | 194737.50 | 190584.38 | 1.022 | 0.983-1.093 | 18 + 2 -> 18 + 2 | 29777 -> 29777 |
| artwork/dimensions3 | 432931.25 | 419462.50 | 1.032 | 0.953-1.089 | 52 + 4 -> 49 + 4 | 360852 -> 360345 |
| artwork/dimensions6 | 731300.00 | 714250.00 | 1.024 | 0.976-1.041 | 97 + 8 -> 91 + 8 | 661920 -> 660906 |
| artwork/hints | 95328.12 | 91670.83 | 1.040 | 0.966-1.104 | 16 + 4 -> 16 + 4 | 3188 -> 3188 |
| artwork/tagged | 229412.50 | 232393.75 | 0.987 | 0.959-1.026 | 9 + 3 -> 9 + 3 | 3482 -> 3482 |
| background/0/0 | 5.24 | 3.12 | 1.675 | 1.628-1.712 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| background/0/1 | 5.07 | 3.12 | 1.628 | 1.568-1.644 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| background/0/2 | 4.91 | 3.06 | 1.602 | 1.570-1.635 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| background/1/0 | 525.05 | 328.05 | 1.601 | 1.573-2.023 | 5 + 0 -> 0 + 1 | 230 -> 152 |
| background/1/1 | 490.19 | 264.48 | 1.853 | 1.666-2.124 | 5 + 0 -> 0 + 1 | 216 -> 152 |
| background/1/2 | 460.27 | 252.32 | 1.824 | 1.412-2.073 | 5 + 0 -> 0 + 1 | 230 -> 152 |
| background/32/0 | 13651.56 | 6175.00 | 2.211 | 2.139-2.286 | 129 + 0 -> 0 + 1 | 7404 -> 5016 |
| background/32/1 | 12550.78 | 4785.16 | 2.623 | 2.272-2.977 | 129 + 0 -> 0 + 1 | 6956 -> 5016 |
| background/32/2 | 11546.88 | 4464.85 | 2.586 | 2.216-3.125 | 113 + 0 -> 0 + 1 | 6832 -> 5016 |
| background/1024/0 | 391675.00 | 125275.00 | 3.127 | 3.042-3.552 | 4097 + 0 -> 0 + 1 | 239444 -> 163704 |
| background/1024/1 | 405525.00 | 113900.00 | 3.560 | 3.300-4.000 | 4097 + 0 -> 0 + 1 | 225108 -> 163704 |
| background/1024/2 | 341225.00 | 108600.00 | 3.142 | 3.105-3.260 | 3585 + 0 -> 0 + 1 | 220542 -> 163704 |
| bounds/0/false | 1.08 | 1.08 | 1.000 | 1.000-1.000 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| bounds/0/true | 1.08 | 1.08 | 1.000 | 1.000-1.000 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| bounds/1/false | 4.77 | 4.04 | 1.179 | 1.147-1.207 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| bounds/1/true | 4.62 | 2.17 | 2.129 | 2.124-2.144 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| bounds/8/false | 28.38 | 24.77 | 1.146 | 1.114-1.157 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| bounds/8/true | 28.93 | 20.37 | 1.421 | 1.395-1.503 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| bounds/128/false | 613.50 | 396.60 | 1.547 | 1.509-1.605 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| bounds/128/true | 509.35 | 317.74 | 1.603 | 1.562-1.626 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| bounds/4096/false | 20409.76 | 12878.32 | 1.585 | 1.539-1.629 | 0 + 0 -> 0 + 0 | 0 -> 0 |
| bounds/4096/true | 15778.91 | 9647.07 | 1.636 | 1.634-1.663 | 0 + 0 -> 0 + 0 | 0 -> 0 |

Ownership reuse retains spare capacity that cloning may have discarded. For the 1,024-file-background fixture, the returned buffers account for **239,444 -> 247,500 bytes** (8,056 more), while allocator requests drop from 4,097 allocations to one resize. Each dimension-based artwork fixture returns three paths whose buffers account for **507 -> 978 bytes**. The normal song assembly subsequently converts artwork paths into final strings. These are capacity tradeoffs of moving existing buffers; no lower RSS or smaller returned objects is claimed. The figures are derived from allocator byte balances: artwork creates all result storage during the measured call; for background conversion, add the identical input storage released by the original implementation to the current call's allocation-minus-free balance. Excess-capacity input parity is also tested.

## Method

Windows/MSVC; Intel Xeon E5-2696 v4 at 2.20 GHz; rustc 1.98.1 (`48a229cea`, 2026-09-01). Release mode with `profile.release.lto=false`. Original and current implementations run in the same binary. The frozen artwork function and BPM helpers match the starting function bodies apart from visibility/formatting. The original background transfer uses the starting borrowed conversion, whose implementation is verified unchanged; the current transfer calls the actual production helper.

Four fixed runs follow exploratory measurements. Each run alternates original/current ordering for ten batches, discards the first batch and reports the median of nine. The table reports medians of the four run medians and the range of paired run ratios. Ratios greater than one favor the change. All 28 cases are retained, including empty, corrupt, hinted and tagged controls. Own build and compatibility processes had completed before fixed timing; unrelated host work was left running. These are microbenchmarks, not whole-game frame-time measurements.

Thread-local allocation counters delegate to the System allocator. Counts are collected separately from timing. The wrapper still checks whether tracking is enabled during timing, which can influence allocation-heavy ratios. Background vectors are cloned outside each timed batch; the timed work consumes input and output, including destruction. Input cloning uses ordinary Clone and does not preserve arbitrary spare capacity. The conversion tests separately exercise excess-capacity inputs. BPM input is prepared outside timing and outputs pass through `black_box`. Artwork benchmarks run the full resolver against warmed local PNG fixtures; creation and cleanup are outside timing. Fixed-width unique fixture names keep path lengths constant across runs. All temporary data is under this worktree's ignored target/tmp.

Background labels are `entry-count/kind`, with kind 0 = file targets, 1 = animation targets, 2 = mixed file/animation/no-background/random targets. Fixtures populate effect, secondary-file and transition fields. BPM labels are `entry-count/contains-invalid-values`; invalid fixtures replace every third BPM with NaN. Artwork dimensions3 selects background/banner/CD title; dimensions6 additionally reaches jacket/disc/CD-image fallback branches. Hints and tagged inputs avoid dimension selection, and empty/corrupt inputs verify failure controls.

## Regression and ITGmania compatibility

- `deadsync_simfile-tests`: 210 passed, 0 failed, 3 ignored. All 203 pre-existing outcomes and all 0 failure diagnostics are unchanged.
- `semantic`: 143 passed, 38 failed, 77 ignored. All 258 pre-existing outcomes and all 38 failure diagnostics are unchanged.
- `actor`: 30 passed, 1 failed, 0 ignored. All 31 pre-existing outcomes and all 1 failure diagnostics are unchanged.
- All three ignored paired benchmarks were explicitly run and passed in each of four complete fixed runs (28 cases per run).
- The 39 existing semantic/actor failures remain unresolved; this pass introduces no detected behavioral regression.
- Six selected full-song archives: **1,650,933 comparisons passed, zero failed**, on both builds, with identical selectors and outcomes.

Seven added regression tests cover serialized background-field equality (including NaN payloads, signed zero, optional fields and all target variants), invalid Windows OS strings, buffer reuse, empty vectors with reserved capacity, generated BPM float bit patterns, artwork output parity and exact allocation savings. The tests preserve serialized/value behavior; capacity and pointer reuse are deliberate ownership changes.

The compatibility harness replays DeadSync against recorded native ITGmania traces. No new ITGmania trace capture was performed. Individual test outcomes and failure diagnostics are compared with a fresh pristine-base run. Panic thread IDs are normalized, and only the known unordered `Sprite.Load has no matching DeadSync actor` lines within `image_texture_aliases_match_native_draws` are sorted. Other diagnostic contents and ordering must match.

| Archive selector | Comparisons passed, baseline and current |
| --- | ---: |
| `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` (319\|TECH SOUP\|[lv.P.Clark] Epidermis) | 363,873 |
| `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` (319\|TECH SOUP\|[lv.P.Clark] Epidermis) | 363,873 |
| `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` (280\|MODS\|[MASTER] Sharkmode) | 304,425 |
| `b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst` (Warp Zone) | 212,220 |
| `0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst` (Let Me Hear That) | 205,071 |
| `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst` (272\|MODS\|[lv.02] Riddle) | 201,471 |

## Reproduce

From this worktree in PowerShell:

```powershell
$env:TEMP = Join-Path $PWD 'target/tmp'
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force $env:TEMP | Out-Null
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib -- --test-threads=1
1..4 | ForEach-Object {
    cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib benchmark_library -- --ignored --nocapture --test-threads=1
}
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
```

Run each archive selector above with:

```powershell
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive-name.tar.zst>
```

For a fresh compatibility baseline, run the same commands at the base commit in a separate worktree. The paired benchmarks contain the starting implementations, so both versions run together without a checkout change. Raw results are retained under ignored `target/baseline-*`, `target/current-*`, `target/final-[1-4].log`, `target/benchmark-results.json` and `target/compatibility-comparison.json`.

Formatting, frozen-function equivalence, unchanged borrowed conversion, the exact patch bump, pending-branch scope, diff whitespace and the explicit staged file list were checked. The four excluded user files and build artifacts are absent from the commit.
