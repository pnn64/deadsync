# Profile import performance pass

- Branch: `perf/1864-import-streamlining-20261008`.
- Starting committed main: `db09a02d3ed2d8585c4144d709279298cbf445a8`.
- Version: `0.5.1863` → `0.5.1864`, including all three workspace-version lockfile packages.
- The 16 unmerged `perf/` branches were reviewed before selection. None changes these import paths. Work was isolated in a new worktree; no merge or changes to the original checkout.

## Changes

1. Check an existing profile GUID before preparing imported options, resolving the library, materializing scores, and collecting favorites. Both public preparation and import orchestration share the same preparation body, with GUID derivation performed once. Creation, cancellation, cleanup, and subsequent write callbacks are preserved.
2. Reuse a double-ended path iterator for resolver fallbacks. Read the song and nested pack from the back, then read any outer pack from the remaining front. Remove the second full-path scan and redundant lookup of an already-missed key, along with the separate nested-path helper. Preserve nested-pack precedence, outer-pack fallback, trimming, separator handling and ASCII-insensitive matching; hashing is unchanged.
3. Remove the Simply Love settings section from its temporary INI parser and move its keys and values into the returned map. This eliminates all key/value clones; conversion between the existing map hashers still occurs. Missing files, invalid UTF-8, section case sensitivity, and repeated-section precedence are preserved.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.1 / LLVM 22.1.8. Release optimization with LTO disabled for both implementations, two build jobs. Original implementations are frozen from the starting commit; an audit reconstructs and formats them to verify they differ only in module paths/visibility. Both variants run in the same instrumented test executable.

Ten alternating pairs per case, discard the first pair, report the median of nine samples per implementation. Inputs are prepared outside timing. Result destruction is included. Settings timings include warm filesystem reads. Resolver lookup figures are per complete batch of the indicated number of queries. Tiny keys use a one-character pack and three-character song folders. Build and successful-lookup controls are effectively unchanged (roughly 1% variation for the large controls). These are focused workload measurements, not overall game frame-rate claims. Allocation counters forward to the system allocator and record only the current thread.

| Workload | Original ns/op | Current ns/op | Throughput |
|---|---:|---:|---:|
| settings-32 | 173,113.67 | 163,580.96 | 1.06× |
| settings-128 | 235,286.04 | 207,614.84 | 1.13× |
| duplicate-32 | 97,523.14 | 519.92 | 187.57× |
| duplicate-2000 | 7,524,106.25 | 506.25 | 14862.43× |
| resolver-build-32 | 20,594.02 | 20,861.38 | 0.99× |
| resolver-lookup-32 | 7,772.25 | 7,543.75 | 1.03× |
| resolver-miss-32 | 8,164.26 | 4,356.56 | 1.87× |
| resolver-fallback-32 | 12,190.97 | 9,086.07 | 1.34× |
| resolver-deep-fallback-32 | 16,070.91 | 9,155.21 | 1.76× |
| resolver-build-2000 | 1,425,773.05 | 1,432,222.27 | 1.00× |
| resolver-lookup-2000 | 527,850.78 | 531,054.49 | 0.99× |
| resolver-miss-2000 | 517,108.59 | 277,322.85 | 1.86× |
| resolver-fallback-2000 | 776,871.68 | 599,228.12 | 1.30× |
| resolver-deep-fallback-2000 | 1,180,996.88 | 650,804.30 | 1.81× |
| resolver-tiny-keys-32 | 1,810.12 | 1,801.33 | 1.00× |

| Workload | Allocations, original → current | Requested bytes, original → current | Peak tracked live bytes, original → current |
|---|---:|---:|---:|
| settings-32 | 144 → 80 | 13,734 → 12,730 | 9,010 → 7,995 |
| settings-128 | 530 → 274 | 51,566 → 47,434 | 34,082 → 29,939 |
| duplicate-32 | 339 → 4 | 52,105 → 82 | 52,055 → 66 |
| duplicate-2000 | 20,019 → 4 | 3,285,873 → 82 | 3,285,823 → 66 |

Duplicate fixtures have eight high scores and two charts per song. The large case contains 16,000 scores. Peak tracked bytes are scoped allocation accounting, not process RSS; allocator bookkeeping and transient realloc storage are excluded. Resolver lookups remain allocation-free. No network requests or writes to user profiles are performed by the added tests.

## Validation

- `deadsync_import-tests`: 66 passed, 0 failed, 3 ignored. All 61 baseline outcomes are preserved.
- `semantic`: 105 passed, 68 failed, 74 ignored. All 247 baseline outcomes are preserved. Existing failure diagnostics are identical after normalizing panic thread IDs.
- `actor`: 30 passed, 1 failed, 0 ignored. All 31 baseline outcomes are preserved. Existing failure diagnostics are identical after normalizing panic thread IDs.
- All three ignored release benchmarks were explicitly run and passed.
- Added differential coverage compares serialized scores, summaries, GUID callback calls, profile-creation failures, cancellation/deletion, favorites, stats and ITL callback order. Resolver coverage includes aliases, nested paths, mixed case, Unicode, duplicate/case-variant catalog entries, varying path lengths, Edits, and missing songs/charts. An additional path corpus covers optional roots, repeated and mixed separators, trimmed components, and redundant pack names. Settings coverage includes absent/empty/invalid files, Unicode, empty keys/values, and repeated/case-distinct sections.
- Two Epidermis archives and one Sharkmode archive were attempted with the full-song native harness. Both builds stop at the same existing noteskin fixture hash guard before comparisons; these are not successful full-song parity runs.

Existing full-song guard:

- `noteskin dependency changed: common/common/Fallback Receptor.lua`.
- Actual SHA-256: `52ffa6df0701b426d6af887f9c91cd61895d8b32f349f0cd7ddfa48bae41e74d`.
- Expected SHA-256: `96623726284f5ae0c5b12e05e0ae841eae40e100341a20042db68c9b80b52c74`.

The initial baseline builds failed because C: ran out of space. Filesystem compression preserved the earlier perf build cache and recovered about 4.1 GB; further space subsequently became available. Successful retries supply the reported baselines. Failed logs are retained under the ignored `target/` directory. Final benchmarks ran after baseline compatibility and current import tests finished, before the current compatibility rebuild, with no concurrent build or harness from this pass. Unrelated user workloads may still influence wall-clock timings.

## Reproduction

```powershell
$env:CARGO_BUILD_JOBS = '2'
cargo test --release --locked --config profile.release.lto=false -p deadsync-import --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-import --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
$archives = @(
  'b40903481944c3395db4c848c671e27a383d2cbea51c74b029b7dea9dbccf16a.tar.zst',
  'a867a12d25c984a49c2dd29fa06d47f88afe01841ee9d410dbf1332800797336.tar.zst',
  'db60b3a89af8957a5e4fb1be2ec3157d09c436a564938536ec91b8ab064afa45.tar.zst'
)
foreach ($archive in $archives) {
  cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- $archive
}
```

Run the semantic/actor commands on the base and this branch and compare individual outcomes and normalized diagnostics. The existing native failures described above are expected on both. The full-song harness accepts archive filenames from `tests/fixtures/full_song_lua/index.json`; the exact selectors and raw logs from this run are retained in this worktree's ignored `target/` directory. No golden data or compatibility guards were changed.
