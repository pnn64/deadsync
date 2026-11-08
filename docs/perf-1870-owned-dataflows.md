# Search allocation reductions, 0.5.1870

- Branch: `perf/1870-owned-dataflows-20261009`.
- Base: committed main `0a26982ae881411282041cfeeb5b4a0396df7b23`.
- Version: `0.5.1869` to `0.5.1870`, exactly one patch; Cargo.toml and all three workspace package entries in Cargo.lock updated.
- Checked all 20 unmerged perf branches at the start. None changes `smo_search.rs`. This pass is independent of the pending subtitle, popularity-entry, and archive-range changes.

## Changes

1. `runtime_search` borrows the trimmed query for its already-running/already-ready check. It creates owned strings only when publishing a new query or starting a worker. The shell calls this from content-browser frame synchronization after the input debounce. Repeated nonempty queries now allocate nothing.
2. Search ordering borrows the date text in the catalogue instead of cloning two strings for each tied-score comparison. The existing stable sort and cap now live on the accumulator so the exact production operation can be tested. Score descending, date descending, pack ID ascending, stable equal keys, and the 200-row cap are preserved.
3. URL encoding appends hexadecimal digits directly to its output instead of formatting each escaped byte into a temporary String. It preserves unreserved characters, `%20` spaces, uppercase hex, and UTF-8 byte escaping. The existing output-buffer growth policy is unchanged.

No Song Lua implementation, fixture archive, reference hash, network endpoint, or dependency changed.

## Measurement

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 at 2.20 GHz; rustc 1.98.1 (48a229cea), LLVM 22.1.8. Release optimization, LTO disabled, System allocator. Each row uses ten alternating original/current pairs, discards the first pair, and reports the median of nine. Allocation counting is disabled during timing. Own builds and compatibility harnesses are idle during the benchmark; unrelated user work is left running.

The original request and encoding functions are frozen from the base commit; the original sorting block is wrapped without changing its body. An audit verifies their source against that commit. Tests and benchmarks call the production replacements. Input construction is outside the timed region; consumed-input and output destruction is inside. Sorting includes truncation and destruction, but excludes snapshot publication and network access.

Allocations, reallocations, and newly requested bytes are recorded in separate single-operation measurements. Bytes include every allocator request, including resized buffers; they are not peak live memory or RSS. These are operation-level throughput results, not whole-application frame-rate or network-speed claims.

| Case | Original ns/op | Current ns/op | Throughput | Allocations old/new | Reallocations old/new | Requested bytes old/new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| encoding/empty | 12.54 | 6.82 | 1.84x | 0 / 0 | 0 / 0 | 0 / 0 |
| encoding/unreserved | 103.78 | 93.14 | 1.11x | 1 / 1 | 0 / 0 | 18 / 18 |
| encoding/spaces | 216.84 | 202.16 | 1.07x | 1 / 1 | 1 / 1 | 72 / 72 |
| encoding/punctuation | 1911.42 | 223.58 | 8.55x | 8 / 1 | 8 / 1 | 148 / 78 |
| encoding/unicode | 3117.83 | 319.35 | 9.76x | 13 / 1 | 14 / 2 | 239 / 119 |
| encoding/long | 104010.30 | 2500.89 | 41.59x | 449 / 1 | 450 / 2 | 9408 / 4928 |
| repeat/empty | 23.75 | 23.16 | 1.03x | 0 / 0 | 0 / 0 | 0 / 0 |
| repeat/ready | 86.41 | 26.34 | 3.28x | 1 / 0 | 0 / 0 | 8 / 0 |
| repeat/loading | 85.51 | 24.16 | 3.54x | 1 / 0 | 0 / 0 | 8 / 0 |
| repeat/trimmed | 90.77 | 32.03 | 2.83x | 1 / 0 | 0 / 0 | 8 / 0 |
| repeat/unicode | 86.52 | 27.99 | 3.09x | 1 / 0 | 0 / 0 | 6 / 0 |
| ordering/empty | 15.49 | 15.57 | 0.99x | 0 / 0 | 0 / 0 | 0 / 0 |
| ordering/single | 86.19 | 87.56 | 0.98x | 0 / 0 | 0 / 0 | 0 / 0 |
| ordering/visible-dated | 3112.40 | 1075.40 | 2.89x | 26 / 0 | 0 / 0 | 260 / 0 |
| ordering/page-dated | 348986.60 | 101383.80 | 3.44x | 3305 / 1 | 0 / 0 | 41040 / 8000 |
| ordering/page-mixed | 170085.00 | 69186.20 | 2.46x | 1195 / 1 | 0 / 0 | 18225 / 8000 |
| ordering/page-undated | 26163.20 | 19571.20 | 1.34x | 1 / 1 | 0 / 0 | 8000 / 8000 |
| ordering/page-distinct-scores | 20902.00 | 15138.20 | 1.38x | 1 / 1 | 0 / 0 | 8000 / 8000 |
| ordering/broad-dated | 2729568.00 | 954612.00 | 2.86x | 26307 / 1 | 0 / 0 | 311060 / 48000 |
| ordering/catalogue-dated | 28091180.00 | 10839260.00 | 2.59x | 253283 / 1 | 0 / 0 | 2892820 / 360000 |

Eighteen of the twenty medians improve. The empty and single-hit sort controls perform no comparisons and retain zero allocations; their measured differences are +0.08 ns and +1.37 ns respectively. All cases exercising the removed work improve.

## Regression and native compatibility checks

Five new behavioral/allocation tests cover 20 repeated-query inputs across Ready/Loading, 20 short-query phase transitions with generation/revision wrapping, 96 sorting/capping combinations plus stable duplicate keys, sort allocation counts, and every ASCII byte plus UTF-8 boundaries and long mixed input. The request tests preserve snapshot identity and restore the local runtime; they start no network workers.

- Online crate: 359 passed, 0 failed, 4 ignored; every one of the 355 baseline outcomes is unchanged.
- ITGmania semantic parity: 139 passed, 37 failed, 74 ignored; every one of the 250 baseline outcomes is unchanged.
- ITGmania actor conformance: 30 passed, 1 failed, 0 ignored; every one of the 31 baseline outcomes is unchanged.
- All three ignored paired benchmarks were explicitly run and passed.
- Native failure bodies match after normalizing panic thread IDs and only the unordered `Sprite.Load has no matching DeadSync actor` lines in `image_texture_aliases_match_native_draws`. That diagnostic iterates a HashMap; its messages and counts remain identical. No parity failure is suppressed or reclassified.

Full-song archives are selected identically before and after:

| Source simfile | Result before/after | Comparisons passed |
| --- | --- | ---: |
| `319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super - _TRUE GAMERS CLICK HERE - EXTRA CHARTS]/Venetian Snares - Epidermis.ssc` | PASS | 363873 |
| `319-TECH SOUP-[lv.P.Clark] [Tech Spectrum Super]/Venetian Snares - Epidermis.ssc` | PASS | 363873 |
| `280-MODS-[MASTER] Sharkmode/Sharkmode.ssc` | PASS | 304425 |
| `(R10) Warp Zone/warp zone.ssc` | PASS | 212220 |
| `(R5) Let Me Hear That/let me hear that.sm` | PASS | 205071 |
| `272-MODS-[lv.02] Riddle/Riddle.ssc` | PASS | 201471 |

All six selected archives pass before and after, totaling 1,650,933 comparisons per run with zero comparison failures. The semantic/actor suites still have the existing failures listed above.

Two initial baseline build attempts encountered Windows `STATUS_DLL_INIT_FAILED` when starting compiler/linker processes. The successful baseline used a hidden background runner with one Cargo job; current validation uses the same process isolation and two jobs. The failed logs are retained separately.

The worktree was created with `git -c core.autocrlf=false worktree add` to preserve the committed LF bytes required by native noteskin SHA-256 guards. No noteskin files were modified.

## Reproduce

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
```

For each full-song row, use `cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive-name>` with its archive from `tests/fixtures/full_song_lua/index.json`. The full-song CLI does not accept libtest arguments.

Raw baseline/current logs, selected archive metadata, allocation measurements, comparison JSON, and audit scripts remain under this worktree's ignored `target/` directory. The four user-excluded files are absent from the commit. The original checkout is untouched; the branch is committed locally and unmerged.
