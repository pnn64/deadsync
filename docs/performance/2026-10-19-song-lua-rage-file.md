# Song Lua buffered file reads and virtual output lookup

This pass starts from `d63852fe4`, following the [mesh emission pass](2026-10-19-song-lua-mesh.md). It makes three changes guided by M-HOTPATH, M-FAST-HASHER and M-INITIAL-CAPACITY in [rust-performance.md](../../rust-performance.md):

1. Return borrowed buffered bytes from `RageFile::read` and `get_line`. The Lua bindings create their strings directly from those slices, removing a temporary vector allocation and copy for nonempty results.
2. Find newline and NUL delimiters with the existing `memchr` dependency. Cursor advancement, CRLF trimming and EOF discovery keep the original rules.
3. Keep the original vector for up to four virtual closed-output files, then move its entries into an `FxHashMap` reserved for the known entry count. Reopening a larger store avoids a linear path search. Overwrites replace the existing value; open handles retain their private byte snapshots.

These functions run in the song Lua worker's compile/replay path. Their virtual writes remain local to one Lua VM. This pass adds no unsafe code or dependency. The userdata borrow remains held while mlua copies bytes into its own string; no borrowed slice escapes that call. Existing readers still own their buffers, so closing a later writer cannot change an already-open reader.

## Measurements

The [raw results and validation receipt](2026-10-19-song-lua-rage-file.json) contain six alternating-order rounds, 54 retained samples per variant/workload and 1,080 samples overall. Tables show medians before → after. Both variants run in one release executable; the baseline is the original implementation from `d63852fe4` with visibility adapters for direct checks.

| Case | CPU cycles/read | Allocations/read | Requested bytes/read | Reads/second |
| --- | ---: | ---: | ---: | ---: |
| Read 64 bytes | 225.44 → 33.07 | 1.00 → 0.00 | 64 → 0 | 9,640,180 → 66,192,289 |
| Read 64 KiB | 83,566.83 → 2,151.54 | 1.00 → 0.00 | 65,536 → 0 | 26,205 → 1,019,368 |
| GetLine, 64-byte final line | 265.29 → 44.20 | 1.00 → 0.00 | 64 → 0 | 8,266,000 → 49,182,344 |
| GetLine, 64-KiB final line | 128,257.03 → 4,013.01 | 1.00 → 0.00 | 65,536 → 0 | 17,065 → 543,080 |

Core reads reset the cursor and EOF flag before each call. The payload is already buffered. GetLine cases contain no newline, so they scan the entire final line. Each old nonempty read allocates a vector; each new read borrows the same bytes with zero allocation. These core cases combine copy removal and delimiter scanning. They do not include Lua string creation.

| Case | CPU cycles/pair | Allocations/pair | Requested bytes/pair | Pairs/second |
| --- | ---: | ---: | ---: | ---: |
| ReadBytes, 64 bytes | 2,000.08 → 1,810.76 | 1.00 → 0.00 | 64 → 0 | 1,095,950 → 1,202,501 |
| ReadBytes, 64 KiB | 76,150.21 → 12,502.93 | 1.00 → 0.00 | 65,536 → 0 | 28,744 → 175,210 |
| GetLine, 64-KiB final line | 135,477.80 → 12,280.00 | 1.00 → 0.00 | 65,536 → 0 | 16,111 → 177,016 |

Each pair calls the real Lua binding for `Seek(0)` and then `ReadBytes(size)` or `GetLine`, including the returned LuaString handle drop. The fixture repeatedly reads identical bytes; Lua may reuse interned strings. Lua setup, file opening, final VM teardown and deferred collection at teardown are excluded. Median throughput changes across these three cases range from +9.7% to +998.7%.

| Case | CPU cycles/open | Allocations/open | Requested bytes/open | Opens/second |
| --- | ---: | ---: | ---: | ---: |
| Last of one file | 4,323.76 → 4,662.66 | 5.00 → 5.00 | 275 → 275 | 506,004 → 468,114 |
| Last of eight files | 6,405.58 → 4,535.12 | 5.00 → 5.00 | 275 → 275 | 341,550 → 481,837 |
| Last of 256 files | 62,184.74 → 4,558.91 | 5.00 → 5.00 | 275 → 275 | 35,168 → 478,907 |

Each open reopens the final file from an existing in-memory virtual-output store in read mode. Name normalization, lookup and the existing 64-byte snapshot clone are timed; file creation and filesystem access are excluded. Allocations for the snapshot and path handling remain. At 256 files throughput changes by +1261.8% and CPU cycles fall 92.7%; per-round throughput changes range from +1189.9% to +1328.1%. The single-file median throughput changes by -7.5%, with per-round changes from -10.0% to +4.7%.

The vector-to-map transition adds hash-table storage and a one-time construction cost after the fourth distinct closed file. That setup cost and retained store memory are not measured separately here. Small stores keep the vector path. The larger-store benchmark deliberately looks up the final entry, which is costly for the original vector; earlier entries have lower baseline search cost. These measurements establish helper-level effects for these fixtures, without a whole-song or gameplay-FPS claim.

Allocation counts and requested bytes cover current-thread Rust GlobalAlloc, including mlua, and exclude direct native malloc. Requested bytes describe allocation traffic rather than RSS. Raw samples retain reallocations and frees. QueryThreadCycleTime measures scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting overhead is included in timing/cycles.

## Behavior and validation

Four new release checks compare against the frozen implementation. They cover empty and long buffers; delimiter boundary lengths; NUL truncation while advancing over all consumed bytes; CRLF and unterminated CR handling; repeated EOF calls; exact-length, zero and invalid reads; cursor positions beyond the buffer; error precedence for closed, destroyed and wrong-mode handles; Lua read/seek/close/destroy behavior; path aliases; 128-file stores; overwrite snapshots; and VM isolation. A counted 64-KiB core read verifies one allocation before and zero after. The store test also checks transition at the fifth distinct file and replacement of an existing entry.

The default-feature release library suite has 705 passes, 13 failures and seven ignored performance benchmarks. All four new checks pass. Every failure matches the preceding commit's normalized diagnostic, and the retained baseline log hash matches its committed receipt. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. The JSON records each failure and diagnostic hash, source/executable hashes and the build-receipt hash. The frozen implementation was checked against `git show d63852fe4` after reversing visibility adapters. Embedded production Lua and path normalization were verified unchanged. Existing native fixtures, acceptance criteria and tolerances are unchanged.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit d63852fe4 --test-filter rage_file::perf::benchmark_rage_file --out docs/performance/2026-10-19-song-lua-rage-file.json
```

Run the executable with `--test-threads=1` for the full library suite. Each round performs one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Core samples run 1,000 or 20,000 reads, API samples run 128 or 1,000 seek/read pairs, and lookup samples run 512 opens. Fixture construction is excluded. Both variants use the same counted System allocator on Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled. No builds or other benchmarks overlap measurements.
