Song-Lua actor-kind extraction now captures only live mode, music length and the already determined graph size. It no longer clones `SongLuaCompileContext` twice for each actor. In fixture replays that context contains the complete recorded host-clock input sequence; copying it was about 17% of Paint the Town Red's sampled CPU work.

Live actor-state reads now reuse separate overlay and tracked-actor buffers. Reads fill scratch storage without holding its mutex across authored Lua calls. Both completed lists are published together; a failed read leaves the previous snapshots intact. The previous vectors become the next scratch buffers. This also removes the combined-vector growth and `split_off` copies, and avoids creating an output allocation for non-state phases.

These changes apply [M-HOTPATH, M-INITIAL-CAPACITY and M-MEM-REUSE](../../rust-performance.md). They preserve field reads, Lua callback order, update frequency, archive validation and parity comparisons. The allocator and SHA implementation are unchanged.

Scoped allocation checks on Linux, outside fixture parsing and archive verification:

| Operation | Baseline allocated bytes | Candidate allocated bytes |
| --- | ---: | ---: |
| Read one Quad's kind, empty recording | 3,390 | 968 |
| Read the same kind, 16,384 clock records | 3,345,726 | 968 |
| Steady update of a scene containing 128 Quads | 251,557 | 13,541–14,053 |

The clock-recording and steady-update regression checks fail on the unchanged baseline for their allocation assertions and pass with the patch. The error/recovery regression passes both versions: a failed state read retains both previous snapshots, and a later successful update publishes the new state. Allocation totals are not elapsed-time or gameplay-FPS measurements.

The Windows candidate also passes these checks: actor-kind reads allocate 992 bytes with either tape length. The large clock-tape saving applies to fixture replays; reusable state buffers also reduce allocation during normal gameplay.

Full-song replays, including archive validation, JSON loading, execution and reporting:

| Fixture | Baseline | Candidate | Elapsed-time reduction |
| --- | ---: | ---: | ---: |
| Paint the Town Red SSC | 6m 19.7s | 4m 50.4s | 23.5% |
| Mr Sandman | 13m 21.8s | 11m 59.3s | 10.3% |
| fof | 7m 45.5s | 7m 45.7s | unchanged (<0.1%) |

Each row is one matched before/after run on the same two-vCPU, 64 GiB Proxmox worker. Both builds start from the same baseline revision, with the candidate applying this patch. Both use release optimization, frame pointers, debug information, disabled LTO and 16 codegen units. Samply records at 99 Hz. Inputs were decompressed before measurement; OS caches were not controlled. The Paint and Sandman baseline times were recovered from precise systemd service timestamps after an artifact-retention bookkeeping error; their unchanged reports and CPU profiles were retained. Other times use the worker's elapsed timer. These are whole-fixture measurements, not a gameplay-FPS benchmark or confidence interval.

Paint's compile-context cloning falls from 17.425% to 0.377% of sampled CPU stacks. Sandman's context cloning falls from 8.090% to 0.007%; the memory-copy hotspot underneath state-vector growth falls from 3.875% to no observed samples. Inclusive percentages overlap; they must not be added to allocator costs. The removed recording copies also avoid freeing their duplicated strings. Hashing and generic JSON loading remain separate optimization opportunities.

Validation completed:

- **117 distinct targeted tests** passed: Windows live compiler (17), live scene (26), live visuals (10) and performance (4); Linux draw (12), framebuffers (7), graphs (7), meshes (9), proxies (8), song meters (9) and song update output (8). Four opt-in benchmarks stayed ignored.
- **77 native-gameplay tests** passed on Linux.
- **16 candidate full-song replays** passed with zero mismatches or errors: these three heavy fixtures, both Sharkmode fixtures, eight routine controls and three score controls. All three matched heavy baselines also passed. Every recorded frame was replayed; heavy before/after comparison counts and pending coverage are identical.
- The unchanged baseline reproduces the candidate's 25 failures against the original Linux frozen snapshots, with identical actual/expected diagnostics. Twenty-four concern Windows-style paths. The remaining failure was a Debug snapshot missing `recorded_max_combo`, introduced earlier by the routine-statistics work. The two affected observation snapshots now include its zero default for both players. The refreshed case passes with both the baseline and candidate; no existing observed value was changed. Corresponding Windows suites pass. Linux path-format portability remains a pre-existing test limitation.

The replay reports retain their existing acceptance basis, **full replay of implemented comparisons**. They still report incomplete production/whole-song coverage. This pass does not resolve unobserved behavior or the wider corpus's existing failures.

The snapshot corrections are published in `deadsync-tests` at `cc1d89d9acd1440ce45f4574396d6a251c9a462d`; deadsync pins that revision. No new native capture or harness change was needed for these allocation-only production changes.

[Machine-readable results](2026-11-16-song-lua-allocations-results.json) retain comparison counts, coverage, build/source hashes, CPU groups and artifact checksums. Full logs, compressed reports, portable symbols and profiles are retained under `target/song-lua-performance-20261116/collected/`. Baseline source is `a9964a104ac4cae0ab5a6a76caf5c263138c258e`; the candidate's five Rust source hashes are included in the results.

Reproduce the focused checks on Windows:

```powershell
cargo test --release --locked -p deadsync-song-lua --features test-support --test live_scene --test live_compiler --test live_visuals --test performance -- --test-threads=2
cargo test --release --locked --test song_lua_itgmania_semantic_parity native_gameplay:: -- --test-threads=2
```

Run a retained full-song input with `cargo test --release --test full_song_lua -- --archive <archive-path> --report <report-path>`. Profiling comparisons require the matching build settings above. Archive checks and comparison coverage must remain enabled.
