This pass optimizes production actor-state reads after profiling `8331c0b6c`. Mr Sandman's full replay took **6.0% less time**, and actor-state collection used **32.6% fewer CPU samples**.

The spin override now uses the existing versioned raw-field cache. Only the outer field lookup is remembered: nested `baked` and `rotation` reads, authored metamethods, conversions and errors still execute. Actors without a raw spin override avoid repeated static-key lookups.

After the initial class/Sprite reads, the live state reader skips the remaining override checks only when every consulted field is already known to be absent at the current table version. Unknown or present fields retain the full reader, including nested vector reads. Sprite callbacks refresh the version before the shortcut, and retained FOV normalization still runs. Preparation reads are unchanged. This follows the measured-hot-path guidance in [rust-performance.md](../../rust-performance.md).

| Measurement | Baseline | Patch | Reduction |
| --- | ---: | ---: | ---: |
| Mr Sandman full replay | 669.492 s | 629.533 s | 6.0% |
| Actor-state collection samples, inclusive | 17,101 | 11,529 | 32.6% |
| 512-Quad update benchmark, median per frame | 0.778 ms | 0.627 ms | 19.4% |

The fresh full-song pair used the same two-vCPU, 64 GiB Proxmox worker and uncompressed capture. Release builds disabled LTO, used 16 codegen units, debug information and frame pointers; Samply sampled at 99 Hz. Decompression and setup hashing preceded measurement; archive validation, JSON loading, replay, comparison and reporting were included. The update benchmark ran on the eight-vCPU, 16 GiB builder, with no concurrent compilation. Both use the Xeon E5-2697 v2 host. OS caches were uncontrolled, and the full-song comparison is one run per version. These measurements do not establish gameplay FPS gains.

Actor-state collection's share fell from 25.98% to 18.62%. The remaining Mr Sandman hotspots include Lua interpreter execution (12.2% exclusive), JSON object key lookups (8.5% exclusive), and the state reader itself (7.9% exclusive). Inclusive categories overlap and must not be summed. Unchanged JSON lookup work also varied between runs, so the reduction in targeted samples is larger than the overall elapsed-time improvement.

Paint passed in 253.442 seconds and fof in 373.165 seconds. These were validation replays, without new paired baselines. Their profiles remain dominated by fixture processing: hashing and JSON loading account for about 20.2% and 19.0% of Paint's samples, and 47.4% and 22.4% of fof's samples. This actor patch demonstrates no general speedup for those workloads.

The new regression checks warm reads, raw-field additions and removals, nested table mutations, repeated spin metamethods, a Sprite callback adding an override during the read, invalid values, and recovery after errors. It passes against both the unchanged baseline and the candidate. A separate opt-in benchmark measures complete updates of a live scene containing 512 idle Quads, after warm-up, over five rounds of 300 frames.

Validation completed:

- 79 native-gameplay tests on Windows and 78 on Linux passed.
- All 84 Windows live-scene/compiler/visual tests, 64 selected Linux renderer/actor/performance tests, and both field-cache unit checks passed. The final formatted regression passed again on both platforms.
- All 16 full-song replays passed with zero reported mismatches or callback errors: Mr Sandman, Paint, fof, both Sharkmode captures, eight routine controls and three score controls.
- Full coverage objects, replayed/native frame counts, stage comparisons, command/query counts and ordered native event selections match their baseline reports exactly. Mr Sandman uses the fresh baseline; the other captures use retained successful reports from the preceding pass.

Acceptance remains full replay of implemented comparisons. Existing uncaptured/uncompared behavior remains listed in the reports; these passes do not prove complete production/whole-song parity. Existing native captures and golden expectations were reused, so no harness or fixture-submodule change was needed.

[Machine-readable results](2026-11-16-song-lua-state-reads-results.json) preserve source/build hashes, all timings, profile summaries, exact comparison coverage and artifact checksums. Full profiles, symbols, reports and logs are retained under `target/song-lua-performance-state-20261116/collected/`. The candidate runner SHA-256 is `0559939c9b0c5a96d63b00aa68667b5132b748d1a17bb6730be3df560d9e7fa0`.

Both temporary replay VMs were returned to their original stopped state and memory settings, profiling permissions were restored, and generated uncompressed scratch archives were removed. Captures and collected evidence remain available.

```powershell
cargo test --locked --release -p deadsync-song-lua --features test-support --test live_scene live_state_cache_observes -- --nocapture
cargo test --locked --release -p deadsync-song-lua --features test-support --test live_scene benchmark_live_actor_snapshots -- --ignored --nocapture --test-threads=1
```
