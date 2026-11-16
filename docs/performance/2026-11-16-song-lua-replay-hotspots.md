This pass optimizes the remaining Song-Lua replay hotspots after `d3154cbf5`.

The full-song runner parses its privately extracted native trace from a read-only memory mapping using `serde_json::from_slice`. The parsed values remain owned, and the mapping is dropped before Lua replay begins. Paths resolving outside the owned extraction use the ordinary buffered reader. This avoids byte-at-a-time reader overhead without allocating another full-sized input buffer. `memmap2` was already in the lockfile; it is now also a direct test dependency.

Coverage is gathered in one event traversal with borrowed fixed-key lookups. Command/query selection also uses fixed-key lookups instead of parsing JSON pointers. Missing caller sources still fall back to the event source; explicit null or non-string sources do not. Nested theme operations, modifier coverage and tail-call uncertainty retain their previous meaning and ordering.

The `--archive` path reuses the content digest already checked by extraction instead of making a second full-archive hashing pass before replay. Content-address validation before extraction, every member's size and SHA-256, and the final archive hash after replay remain enabled. The SHA algorithm and backend are unchanged.

These changes follow [M-HOTPATH](../../rust-performance.md): remove repeated processing in measured hot paths while retaining observed behavior and validation.

Measured full-song replays of retained **uncompressed tar captures**, including the runner's validation, parsing, replay, comparisons and reporting:

| Fixture | Baseline | Accepted patch | Observed change |
| --- | ---: | ---: | --- |
| Paint the Town Red SSC | 5m 10.0s | 4m 07.9s | 20.0% less time |
| fof | 8m 05.9s | 6m 17.3s | 22.3% less time |
| Mr Sandman | 11m 32.0s | 11m 44.4s | 1.8% more time; no demonstrated gain |

Hash-verification samples fell by about 25% in all three cases. JSON-loading samples fell by 21.0% for Paint and 33.8% for fof. Paint's coverage scan fell from 2,547 to 699 samples, a 72.6% reduction. These inclusive CPU categories overlap and must not be added together. Allocation, generic JSON objects and the actor/Lua execution paths still account for substantial work.

Each comparison uses the same two-vCPU, 64 GiB Proxmox worker and input archive. Builds use release optimization with LTO disabled, 16 codegen units, debug information and frame pointers; Samply samples at 99 Hz. Decompression and setup hashing happen before measurement. OS caches were uncontrolled, and an experimental build ran between the baseline and final accepted build. These single-run comparisons are not confidence intervals or gameplay-FPS measurements. Compressed-archive workflows spend less time hashing their input archive, so their percentage gains can differ.

The Sandman baseline elapsed time was recovered from precise systemd start/deactivation timestamps after its SSH-scoped controller stopped; the profiling/replay service continued successfully. Its unchanged report and profile were retained. Other measurements use the worker's elapsed timer.

A compile-time specialization of the production actor-state reader was also tested and removed. It did not reduce total state-read work: Sandman recorded 16,753 baseline samples versus 16,848 with the specialization, and elapsed time increased from 692.002 to 705.534 seconds. The accepted patch changes only fixture-runner code and its test dependency. The per-frame actor/Lua bottleneck remains.

Final validation:

- **79 native-gameplay tests on Windows and 78 on Linux** passed. Windows includes one platform-specific math check.
- **Archive integrity and parser equivalence passed on both platforms.** The checks cover owned parsed data after mapping/extraction cleanup, buffered fallback outside the owned extraction, malformed JSON, corrupt members and same-size archive corruption.
- **16 accepted full-song replays** passed with zero mismatches or callback errors: the three profiled songs, both Sharkmode captures, eight routine controls and three score controls.
- All recorded frames were replayed. Full coverage objects, stage-comparison counts, command/query counts and the ordered native event selections match the baseline for all 16 captures. The three heavy baselines were freshly replayed; control equivalence uses the retained successful reports from the preceding pass.
- The accepted Linux and Windows test proofs bind the same normalized source hashes as the working tree. No production crate source changed.

Acceptance remains **full replay of implemented comparisons**. Existing production/whole-song coverage gaps remain explicit in the reports. Existing fixtures and golden expectations were reused; no new native capture or submodule update was required.

[Machine-readable results](2026-11-16-song-lua-replay-hotspots-results.json) include baseline/experimental/accepted measurements, comparison coverage, source and runner hashes, CPU groups and artifact checksums. Full profiles, portable symbols, reports and logs are retained in `target/song-lua-performance-next-20261116/collected/`. The accepted runner SHA-256 is `c4fac7317085b2bb7aee232ace88f86d7fa9199561251a9c7c15335206be5231`.

Reproduce the changed runner checks:

```powershell
cargo test --release --locked --test song_lua_itgmania_semantic_parity native_gameplay:: -- --test-threads=2
cargo test --release --locked --test song_lua_itgmania_semantic_parity whole_song_archives::uncompressed_candidates_retain_member_validation -- --nocapture
cargo test --release --locked --test full_song_lua -- --archive <archive-path> --report <report-path>
```

Use the matching build settings above when comparing profiler results. Both temporary replay workers were restored to their original stopped state and memory configuration after collecting the evidence.
