# Metadata ownership performance pass

Branch: `perf/1872-metadata-ownership-20261009`.
Base: `16a06a2619a6cdcc34c647603d5dfa186e2f3e73`, the committed local `main` at pass start.
Version: **0.5.1871 -> 0.5.1872** in Cargo.toml and all three workspace-version lockfile records.

## Scope and behavior

The pending-branch audit checked 52 local/remote refs representing 30 distinct unmerged heads. None changes these three production files. Pending simfile optimizations concern `cache.rs` and `scan.rs`; this pass leaves those implementations alone. Work was done in a separate worktree. Nothing was merged or pushed.

1. **Pack sync preferences:** stop allocating a lowercased name for each ASCII pack. Normalize the requested name once, use ASCII case-insensitive comparison for ASCII pack names, and retain the original Unicode lowercase comparison for other names. Normalizing the request first preserves Unicode requests that lowercase into ASCII (for example the Kelvin sign). Empty pack lists return before allocating the search name. The complete scan, all matching updates, changed flag, locking and generation updates are preserved.
2. **Course loading progress:** remove the two owned progress-label strings and the completion closure. Parse and validate, store the outcome, then borrow the labels from its stored path. The callback still runs after storing each success/failure and before opening the next file. Errors, output order, validation cache, counters and initial empty progress report are preserved.
3. **Gameplay event intro:** borrow the recognized event name until constructing its `Arc<str>`, removing the intermediate `String` and copy. The public `event_intro_name_for_pack` API still returns `Option<String>`; recognized SRPG/ITL names, whitespace, case and unlock suffix rules remain unchanged. This runs at gameplay initialization, so it is an allocation/initialization improvement, not a claimed frame-time improvement.

Production changes add no dependency, persistent cache or new runtime data structure. Frozen starting function bodies and instrumentation are compiled only for tests. The frozen functions differ from the base only in visibility and rustfmt formatting (including a trailing signature comma).

## Measurement

Windows/MSVC, Intel Xeon E5-2696 v4 at 2.20 GHz, rustc 1.98.1 (`48a229cea`, 2026-09-01), release optimization with `profile.release.lto=false`. Both implementations run in the same test executable. Four completed fixed runs follow one exploratory run; every run alternates implementation order over ten batches, discards the first batch and reports the median of nine. The table shows medians across the four run medians and their ratio range. Ratios above 1 favor the change.

Allocation accounting delegates to the System allocator and is thread-local. Accounting is enabled only for separate allocation measurements, not timing. The allocator wrapper still checks whether counting is enabled during timing, which can affect allocation-heavy ratios. Counts and requested bytes are the principal deterministic evidence; they are not RSS measurements. Allocations and reallocations are distinguished. The table's allocation column excludes reallocations; requested bytes include both allocation and reallocation requests.

Sync/intro inputs are constructed outside timing and outputs are consumed through `black_box`. The sync allocation sample changes preferences; its timing samples measure repeated setting to the same preference, with the full search still performed. Course inputs are freshly cloned outside each timed batch; consumption of paths, loading, validation, callback and destruction of results are timed. Course fixtures and temporary directories live under this worktree's ignored `target/tmp`. File I/O uses warmed caches. Own builds and compatibility runs had completed before the fixed timing runs; unrelated work on the shared host was not interrupted. These are microbenchmarks, not whole-game benchmarks. One attempted fixed run stopped with Windows error 112 (disk full) while creating a course fixture; it was retained separately and excluded. Once free space returned, all four complete fixed runs were restarted without source changes or deletion of existing worktrees/caches.

Sync labels are `pack-count/mixed-Unicode`. With mixed Unicode enabled, every thirteenth pack is ASCII `TARGET`; other names contain non-ASCII letters. At zero/one entries these controls contain no Unicode. Course labels are `course-count/outcome/callback-enabled`; mixed outcomes cycle successful parse, missing file and missing song reference. Intro `unicode` is a recognized ITL name with a non-ASCII prefix. Empty, one-item, failure and fallback cases are included below rather than discarded.

The clearest throughput win is the ASCII pack scan: 1,024 packs improve **8.460x**, with **1,025 -> 1 allocations** and **16,466 -> 6 requested bytes**. Across nonempty all-ASCII inputs, throughput ratios are 1.651-10.724x. Empty scans remove their sole allocation.

Recognized event intro names remove one allocation (**2 -> 1**) and 13-21 requested bytes in these fixtures; throughput improves **1.130-1.291x**. Fallback allocation counts are unchanged.

Course loading removes up to two label allocations per input; all named course-file fixtures save **two allocations per input**, with or without a callback: 64 successful files use **321 -> 193 allocations**, and 64 mixed results use **448 -> 320**. Both save **1,280 requested bytes**. Reallocation counts are unchanged. Disk access and parsing dominate elapsed time, so no broad course-loading throughput gain is claimed.

Tradeoffs: the 8,192-pack mixed-Unicode control has **0.973x throughput** (2.7% lower), while still removing 631 allocations; its four-run ratios are 0.906-0.998. This retains the original Unicode normalization plus an ASCII check. The mixed-course/no-callback control has **0.989x throughput** (1.1% lower; four-run ratios 0.978-0.999). Other course/control timings vary around parity. These small timing regressions are retained and disclosed for the deterministic allocation reduction and simpler ownership; the changes have no detected behavioral regression. These results do not establish lower frame time or whole-process memory use.

| Case | Original ns/op | Current ns/op | Throughput ratio | Four-run ratio range | Allocations original -> current | Requested bytes original -> current |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| courses/0/success/false | 26.58 | 25.78 | 1.031 | 0.873-1.059 | 0 -> 0 | 0 -> 0 |
| courses/0/success/true | 29.45 | 29.74 | 0.990 | 0.954-1.035 | 0 -> 0 | 0 -> 0 |
| courses/1/success/false | 101961.72 | 101639.45 | 1.003 | 0.984-1.009 | 6 -> 4 | 633 -> 613 |
| courses/1/success/true | 100150.78 | 97766.02 | 1.024 | 1.000-1.037 | 6 -> 4 | 633 -> 613 |
| courses/64/success/false | 6631437.50 | 6546250.00 | 1.013 | 1.005-1.039 | 321 -> 193 | 40748 -> 39468 |
| courses/64/success/true | 6483825.00 | 6447375.00 | 1.006 | 0.973-1.029 | 321 -> 193 | 40748 -> 39468 |
| courses/64/missing/false | 3281375.00 | 3317250.00 | 0.989 | 0.953-1.051 | 322 -> 194 | 88416 -> 87136 |
| courses/64/missing/true | 3426425.00 | 3435050.00 | 0.997 | 0.981-1.018 | 322 -> 194 | 88416 -> 87136 |
| courses/64/references/false | 6995312.50 | 6874150.00 | 1.018 | 0.977-1.026 | 706 -> 578 | 72096 -> 70816 |
| courses/64/references/true | 7132725.00 | 7009100.00 | 1.018 | 1.005-1.033 | 706 -> 578 | 72096 -> 70816 |
| courses/64/mixed/false | 5522962.50 | 5583950.00 | 0.989 | 0.978-0.999 | 448 -> 320 | 68553 -> 67273 |
| courses/64/mixed/true | 5301562.50 | 5273725.00 | 1.005 | 0.977-1.021 | 448 -> 320 | 68553 -> 67273 |
| intro/srpg10 | 381.82 | 304.06 | 1.256 | 1.228-1.322 | 2 -> 1 | 46 -> 32 |
| intro/srpg9 | 376.54 | 291.59 | 1.291 | 1.219-1.399 | 2 -> 1 | 45 -> 32 |
| intro/itl | 446.25 | 361.89 | 1.233 | 1.165-1.284 | 2 -> 1 | 47 -> 32 |
| intro/unlocks | 549.50 | 461.44 | 1.191 | 1.157-1.205 | 2 -> 1 | 47 -> 32 |
| intro/unicode | 571.80 | 505.83 | 1.130 | 1.035-1.196 | 2 -> 1 | 61 -> 40 |
| intro/fallback | 332.37 | 325.98 | 1.020 | 0.983-1.063 | 1 -> 1 | 24 -> 24 |
| sync/0/false | 76.19 | 5.03 | 15.162 | 13.860-15.175 | 1 -> 0 | 6 -> 0 |
| sync/0/true | 75.69 | 4.96 | 15.244 | 13.847-15.292 | 1 -> 0 | 6 -> 0 |
| sync/1/false | 142.65 | 86.42 | 1.651 | 1.618-1.666 | 2 -> 1 | 12 -> 6 |
| sync/1/true | 145.38 | 86.10 | 1.689 | 1.574-1.739 | 2 -> 1 | 12 -> 6 |
| sync/32/false | 2272.61 | 211.91 | 10.724 | 10.170-11.366 | 33 -> 1 | 479 -> 6 |
| sync/32/true | 4812.63 | 4680.10 | 1.028 | 1.007-1.086 | 33 -> 30 | 392 -> 374 |
| sync/1024/false | 78975.00 | 9335.00 | 8.460 | 8.212-8.788 | 1025 -> 1 | 16466 -> 6 |
| sync/1024/true | 169321.88 | 168412.50 | 1.005 | 0.988-1.031 | 1025 -> 946 | 13631 -> 13157 |
| sync/8192/false | 595037.50 | 84321.88 | 7.057 | 6.848-7.271 | 8193 -> 1 | 138866 -> 6 |
| sync/8192/true | 1383521.88 | 1421709.38 | 0.973 | 0.906-0.998 | 8193 -> 7562 | 116183 -> 112397 |

## Regression and compatibility checks

- `deadsync_simfile-tests`: 210 passed, 0 failed, 3 ignored; all 203 pre-existing outcomes unchanged, with no failures.
- `semantic`: 143 passed, 38 failed, 77 ignored; all 258 pre-existing outcomes unchanged, including 38 failure diagnostics.
- `actor`: 30 passed, 1 failed, 0 ignored; all 31 pre-existing outcomes unchanged, including its failure diagnostic.
- The three ignored paired benchmarks were explicitly run: all three passed in each of four complete fixed runs (28 cases per run).
- Compatibility failure comparisons normalize panic thread IDs and sort only the known unordered `Sprite.Load has no matching DeadSync actor` lines in `image_texture_aliases_match_native_draws`; every other diagnostic line and ordering must agree. The existing 39 failures remain unresolved by this metadata pass.
- Six full-song archives: **1,650,933 comparisons passed, zero failed**, on both the pristine base and modified build. Selectors and outcomes match exactly.

The compatibility checks replay DeadSync against the repository's recorded native ITGmania traces; no new ITGmania trace capture was performed. They exercise broad Song Lua/actor behavior. Direct original-versus-current tests establish the semantics of the three modified metadata paths, which are not all directly covered by the Lua corpus.

Seven added regression tests cover Unicode case mapping (including Kelvin sign, dotted I and Greek sigma), duplicate/multiple matching packs, no-op preferences, name/fallback parity, course successes and errors, reversed and duplicate inputs, callback ordering (a callback modifies the next file before parsing), empty input and exact allocation reductions.

| ITGmania archive selector | Comparisons passed, original and current |
| --- | ---: |
| `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` (319\|TECH SOUP\|[lv.P.Clark] Epidermis) | 363,873 |
| `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` (319\|TECH SOUP\|[lv.P.Clark] Epidermis) | 363,873 |
| `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` (280\|MODS\|[MASTER] Sharkmode) | 304,425 |
| `b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst` (Warp Zone) | 212,220 |
| `0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst` (Let Me Hear That) | 205,071 |
| `920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst` (272\|MODS\|[lv.02] Riddle) | 201,471 |

## Reproduction

From the committed worktree (PowerShell):

```powershell
$env:TEMP = Join-Path $PWD 'target/tmp'
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force $env:TEMP | Out-Null
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib -- --test-threads=1
1..4 | ForEach-Object {
    cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib benchmark_metadata -- --ignored --nocapture --test-threads=1
}
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
```

Run each archive selector in the table with:

```powershell
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive-name.tar.zst>
```

For fresh baseline compatibility results, use the base commit in a separate worktree with the same commands and archive selectors. The paired metadata benchmarks already contain the frozen starting functions, so both versions are measured without checking out another revision. Local raw logs are retained in ignored `target/baseline-*`, `target/current-*`, `target/final-[1-4].log`, `target/benchmark-results.json` and `target/compatibility-comparison.json`.

Formatting, `git diff --check`, frozen-function equivalence, exact version-only manifest/lockfile changes, pending-branch overlap and explicit staged-file checks were verified. The four excluded user files and all build artifacts are absent from the commit.
