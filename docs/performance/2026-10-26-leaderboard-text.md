# Leaderboard pane parsing, refresh queues, and tag buffers

Baseline runtime source: `2d1ef2b30`; measured on 2026-10-26. Three changes improve plain pane parsing **7.513x**, occupied refresh updates **2.783x**, and Unicode machine-tag formatting **3.222x**. The public request-state fixture improves **1.841x** and composed row text **1.172x**. The cold insertion/clear fixture costs 14.8% more cycles; nonempty tags reserve more bytes up front. These tradeoffs are included below. No whole-application latency, network throughput, or gameplay FPS gain is claimed.

## Retained changes

1. `arrowcloud_pane_kind_from_type` filters ASCII alphanumeric bytes into a six-byte stack buffer, sized to `hardex`, then matches byte slices. It removes the temporary `String` and Unicode decoding. A seventh normalized byte rejects the type immediately. Only normalized length is bounded: arbitrary ignored punctuation/Unicode remains accepted around valid names. Empty/fully ignored/unknown values still return `None`; all four aliases and case rules remain unchanged.
2. `queue_player_leaderboard_refresh` updates an existing value through `get_mut`, cloning the key only when inserting it. This removes three transient `String` clones per occupied update and the owned Entry chain. Key fields, maximum request counts, map capacity and existing stored key identity remain identical in differential replays. New keys in a nonempty map perform a second lookup, explaining the measured cold-path cost.
3. `scorebox_machine_tag` keeps its original trimming, optional override, empty fallback, four-scalar limit and ASCII uppercase loop. Its initial byte capacity changes from four to sixteen, the maximum UTF-8 size of four Unicode scalar values. This guarantees sufficient space before pushing characters and removes all measured reallocations. It intentionally increases reserved/requested bytes for nonempty ASCII tags.

The production diff changes only these three bodies: **20 inserted / 16 removed lines**, including one capacity comment. It adds no helper, public type, dependency, unsafe Rust, or architectural layer. The discarded prefix-walking and ASCII-special-case candidates are absent from production.

## Consecutive before/after measurements

Each step uses eight fresh processes per version, alternating execution order. Columns show median cycles per complete fixture and median-before / median-after CPU throughput. Ranges include every paired process ratio.

### Pane step

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `pane/plain` | 62,037.05 | 9,124.69 | 6.799x | 5.926-10.043x |
| `pane/decorated` | 73,971.50 | 21,590.74 | 3.426x | 3.232-4.960x |
| `pane/long_rejected` | 68,777.24 | 1,749.14 | 39.321x | 32.869-49.311x |

### Queue step

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `queue/hot` | 322,159.97 | 116,529.63 | 2.765x | 2.350-2.841x |
| `queue/empty_key` | 64,497.85 | 53,244.18 | 1.211x | 1.004-1.287x |
| `queue/cold` | 40,664.10 | 47,243.95 | 0.861x | 0.812-0.908x |
| `queue/mixed` | 353,610.21 | 166,265.24 | 2.127x | 2.037-2.338x |
| `queue/long_keys` | 657,039.82 | 429,937.90 | 1.528x | 1.484-1.555x |
| `queue/request_state` | 442,242.77 | 242,732.70 | 1.822x | 1.803-1.858x |

### Tags step

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `tags/ascii` | 71,102.57 | 73,360.15 | 0.969x | 0.927-1.022x |
| `tags/unicode` | 284,741.22 | 90,407.42 | 3.150x | 2.680-3.321x |
| `tags/mixed` | 113,791.02 | 70,123.39 | 1.623x | 1.445-1.802x |
| `tags/long_name` | 68,094.73 | 68,493.43 | 0.994x | 0.947-1.065x |

Raw tables include all fifteen workloads and controls: [pane step](2026-10-26-leaderboard-pane.tsv), [queue step](2026-10-26-leaderboard-queue.tsv), and [tag step](2026-10-26-leaderboard-tags.tsv).

The queue-only step improves occupied updates 2.765x and the borrowed-key public request-state fixture 1.822x, while cold insertion/clear is 0.861x. Tag-only Unicode/mixed formatting improves 3.150x/1.623x; ASCII tags measure 0.969x in that series. Unchanged functions also vary between builds/processes. The combined comparison below gives a fresh original/final result, including the regressions and approximately flat controls.

## Combined original/final measurements

Twelve fresh processes per version compare all three changes against the baseline. [Complete data](2026-10-26-leaderboard-final.tsv).

| Fixture | Before cycles | After cycles | CPU throughput | Before -> after ns | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: |
| `pane/plain` | 62,293.42 | 8,291.27 | 7.513x | 28,623.83 -> 3,817.57 | 6.624-9.218x |
| `pane/decorated` | 74,912.95 | 21,569.30 | 3.473x | 34,162.89 -> 9,838.28 | 2.944-4.250x |
| `pane/long_rejected` | 67,650.59 | 1,636.82 | 41.331x | 30,848.05 -> 741.41 | 36.125-51.690x |
| `tags/ascii` | 72,078.30 | 70,853.05 | 1.017x | 32,866.41 -> 32,296.88 | 0.831-1.124x |
| `tags/unicode` | 285,965.64 | 88,761.17 | 3.222x | 130,410.94 -> 40,467.19 | 2.532-3.456x |
| `tags/mixed` | 115,265.80 | 70,125.96 | 1.644x | 52,553.12 -> 32,003.12 | 1.381-1.966x |
| `tags/long_name` | 70,949.09 | 69,111.63 | 1.027x | 32,342.57 -> 31,664.07 | 0.845-1.185x |
| `queue/hot` | 324,775.98 | 116,687.41 | 2.783x | 148,257.03 -> 53,220.71 | 2.280-3.525x |
| `queue/empty_key` | 64,471.27 | 51,717.11 | 1.247x | 29,388.28 -> 23,582.03 | 1.096-1.562x |
| `queue/cold` | 40,712.96 | 46,721.79 | 0.871x | 18,564.85 -> 21,387.10 | 0.801-1.075x |
| `queue/mixed` | 353,393.28 | 165,765.36 | 2.132x | 161,355.08 -> 75,613.28 | 2.106-2.621x |
| `queue/long_keys` | 662,203.21 | 429,691.83 | 1.541x | 302,233.20 -> 196,198.04 | 1.341-1.855x |
| `queue/request_state` | 445,747.92 | 242,078.48 | 1.841x | 203,307.04 -> 110,407.03 | 1.499-2.130x |
| `compose/row` | 167,380.76 | 142,831.05 | 1.172x | 76,347.65 -> 65,192.97 | 1.019-1.384x |
| `control/rank` | 104,753.81 | 105,659.24 | 0.991x | 47,792.97 -> 48,266.40 | 0.836-1.194x |

Pane parsing, occupied refresh updates, the mixed queue cycle, long-key updates, request-state handling, Unicode/mixed tags, and composed rows have combined paired ranges above 1x. The 41.331x overlong-name result is a stress input with 256 extra ASCII letters; normal plain/decorated names improve 7.513x/3.473x. ASCII/long ASCII tag medians are approximately flat to slightly faster, with ranges that include slower runs. The untouched rank formatter measures 0.991x.

The cold queue fixture is slower: 0.871x throughput, or **14.8% more cycles**. It includes 64 distinct insertions and clearing their stored keys. The mixed fixture adds 512 occupied updates before clearing and improves 2.132x. These results support coalescing existing refreshes; they do not establish faster first insertion. The request-state fixture calls `PlayerLeaderboardCacheState::request_leaderboard` with prepared borrowed keys; it does not measure profile-key construction, the global mutex wrapper, or network work.

## Allocations and requested memory

The existing scoped System-allocator wrapper probes one complete fixture separately from timing. Each version/workload has identical counters across process runs. Allocations equal frees, and requested bytes equal freed bytes within each fixture; each corresponding table cell reports that common value.

| Fixture | Allocations/frees before -> after | Reallocations before -> after | Requested/freed bytes before -> after | Scoped peak bytes before -> after |
| --- | ---: | ---: | ---: | ---: |
| `pane/plain` | 448 -> 0 | 0 -> 0 | 1,600 -> 0 | 6 -> 0 |
| `pane/decorated` | 448 -> 0 | 0 -> 0 | 8,960 -> 0 | 36 -> 0 |
| `pane/long_rejected` | 64 -> 0 | 0 -> 0 | 16,608 -> 0 | 262 -> 0 |
| `tags/ascii` | 512 -> 512 | 0 -> 0 | 2,048 -> 8,192 | 4 -> 16 |
| `tags/unicode` | 512 -> 512 | 1,024 -> 0 | 14,336 -> 8,192 | 16 -> 16 |
| `tags/mixed` | 512 -> 512 | 204 -> 0 | 3,680 -> 5,720 | 8 -> 16 |
| `tags/long_name` | 512 -> 512 | 0 -> 0 | 2,048 -> 8,192 | 4 -> 16 |
| `queue/hot` | 1,536 -> 0 | 0 -> 0 | 73,728 -> 0 | 144 -> 0 |
| `queue/empty_key` | 0 -> 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `queue/cold` | 192 -> 192 | 0 -> 0 | 9,216 -> 9,216 | 9,216 -> 9,216 |
| `queue/mixed` | 1,728 -> 192 | 0 -> 0 | 82,944 -> 9,216 | 9,360 -> 9,216 |
| `queue/long_keys` | 1,536 -> 0 | 0 -> 0 | 532,480 -> 0 | 1,040 -> 0 |
| `queue/request_state` | 1,536 -> 0 | 0 -> 0 | 73,728 -> 0 | 144 -> 0 |
| `compose/row` | 368 -> 256 | 50 -> 0 | 2,336 -> 2,448 | 16 -> 24 |
| `control/rank` | 512 -> 512 | 0 -> 0 | 4,096 -> 4,096 | 8 -> 8 |

Pane parsing removes all 448 allocations in each 512-name plain/decorated fixture. Occupied and request-state updates remove all **1,536 allocations/frees and 73,728 requested bytes per 512 calls**. The mixed queue cycle removes 1,536 of 1,728 allocations, retaining the 192 allocations required for its 64 stored keys. The empty-key control already allocates nothing and still improves, reflecting the avoided clone/drop work beyond heap requests.

The 512 Unicode tags remove **1,024 reallocations**, reducing requested/freed bytes from 14,336 to 8,192 (42.9%). Mixed tags remove 204 reallocations while requested bytes rise from 3,680 to 5,720. ASCII tags retain 512 allocations but request 8,192 rather than 2,048 bytes, and scoped peak requested bytes rise from four to sixteen. Empty tags still use the original four-byte `----` result. This is bounded spare capacity, rather than a claim that every tag uses less memory.

Composed row text removes 112 of 368 allocations and all 50 reallocations per 128 rows. Requested bytes increase 4.8% (2,336 -> 2,448) and scoped peak rises from 16 to 24. These are allocation requests/live requested bytes; allocator size classes and process RSS were not measured. Prepared input keys, map buckets and other fixture storage are outside the counters.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Archived integration-test binaries use `--release --locked`, repository-default LTO, the baseline CPU target and static MSVC runtime. The existing helper pins the benchmark thread to logical processor 2 and reads `QueryThreadCycleTime`. Each process discards one warmup sample and reports the median of eleven measured samples, each repeating its fixture **128 times**. The driver alternates before/after process order and reports medians of process medians. Four TSVs retain all **1,080 process/workload records**, elapsed time, allocation counters and executable SHA-256 values. No compilation started by this task overlaps retained comparisons; other shared-machine activity is uncontrolled.

Fixtures/results pass through `black_box`; preparation is outside timing. Pane/tag fixtures process 512 prepared inputs, except the 64 overlong names. Pane inputs include accepted case variants, unknown, empty, ignored-only and Unicode values. Decorated names insert punctuation and an ignored Unicode character after every scalar. Tags alternate explicit overrides and fallback names, using ASCII, multi-byte Unicode, blank/short/combining inputs and 256-byte names.

The hot queue fixture makes 512 updates over 64 existing keys. Ordinary keys contain 16/64/64-byte strings; the long-key variant uses 16/512/512 bytes. A single empty-key fixture exercises clone/drop work without heap allocation. Cold/mixed maps reserve bucket capacity outside timing, then insert 64 keys and clear them inside each cycle; mixed adds 512 occupied updates. The request-state fixture preloads both `in_flight` and `pending_refresh`, uses an empty ready cache and `refresh_cached=true`, and makes 512 requests through the actual public state method. Standard HashMaps retain `RandomState`, so hash seeds/probe layouts can vary across processes. Row composition makes 128 tuples of pane kind, formatted machine tag and the unchanged score-value formatter. The rank control formats 512 ranks.

Both binaries use the same scoped System-allocator wrapper. Counter tracking is enabled only for the untimed probe and disabled during timing; allocation calls still pass through that wrapper. The results describe these warmed kernels and state paths.

Four new checks compare against the three production bodies frozen from `2d1ef2b30`. Pane checks cover every ASCII case mask of all aliases, insertion of every ASCII byte and several Unicode forms at every boundary, overlong normalized input and kilobytes of ignored raw input. Tag checks cover override precedence, Unicode trimming, ASCII controls, combining/joining characters, all UTF-8 widths sampled across Unicode scalar space at each prefix position, and long names. Queue replays use four starting capacities and 4,096 operations each, including clears/removals, zero/maximum requests and changes to every key field. They compare complete maps, capacity and the three stored string pointers after occupied updates. A composed-result/state check verifies pane/tag tuples and in-flight request coalescing through the public state method.

Validation passes: **494 release tests** across score/profile (9 ignored), **255 debug tests** across score (5 ignored), and Clippy for all score targets with existing warnings and no diagnostics in changed functions or new test/reference files. All four measured default-LTO archives pass the four new compatibility tests. The three complete changed function bodies and both new Rust files pass rustfmt. A full `leaderboard.rs` formatting check retains pre-existing differences outside the edited bodies.

Source audits compare frozen bodies with `2d1ef2b30`, reconstruct the entire starting production file by reversing just these three bodies, verify the fixed benchmark/reference/shared-helper sources, compare final production bytes with the measured snapshot and check every raw record's executable digest. Build configuration matches the baseline. Trial implementations, source snapshots, executable archives and logs remain under `target/leaderboard-text/`.

Exact-size prefix copying and two ASCII shortcuts removed Unicode growth but had weaker ASCII measurements. The retained tag change uses the original loop and one capacity bound, with its requested-memory tradeoff measured explicitly.

```powershell
cargo test --release --locked -p deadsync-score --test leaderboard_text --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_leaderboard_text --log-dir target/leaderboard-text/runs --before target/leaderboard-text/before/score.exe --after target/leaderboard-text/pane/score.exe --output docs/performance/2026-10-26-leaderboard-pane.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_leaderboard_text --log-dir target/leaderboard-text/runs --before target/leaderboard-text/pane/score.exe --after target/leaderboard-text/queue/score.exe --output docs/performance/2026-10-26-leaderboard-queue.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_leaderboard_text --log-dir target/leaderboard-text/runs --before target/leaderboard-text/queue/score.exe --after target/leaderboard-text/after/score.exe --output docs/performance/2026-10-26-leaderboard-tags.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_leaderboard_text --log-dir target/leaderboard-text/runs --before target/leaderboard-text/before/score.exe --after target/leaderboard-text/after/score.exe --output docs/performance/2026-10-26-leaderboard-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-score -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadsync-score -- --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-score --all-targets
```

Keep the committed integration harness fixed. Restore the three production bodies from `2d1ef2b30` for `before`; apply pane parsing for `pane`, borrowed occupied updates for `queue`, then tag capacity for `after`. Build/archive the single reported integration executable and run its four ordinary checks at each stage. Consecutive stages use eight paired runs; original/final uses twelve. Broader verification disables release LTO to reduce linking time; all performance archives retain default LTO.
