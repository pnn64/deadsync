# Test cleanup verification

All non-deferred tests are verified on this Windows checkout. The ignored-test
audit completed with zero failures. Long-song deferrals remain explicitly listed.

## Scope and references

All tests built for this Windows checkout must pass. Only complete, expensive
song-fixture replays are deferred. Benchmark and hardware ignores are verified.
The [ignored-test inventory](ignored-tests.md) lists source reasons, expanded names
and completed execution status.

Parity references: ITGmania `704cdd5624155f743b59b8b64fedeb0588dae067` and
Simply Love `bebf3f3bfba7cb5f73b1c6d74a0cbb69b8a6ebe6`. They were used to
check behavior during development; the tests do not need either checkout.

## Current results and oracle freshness

The refreshed workspace run completed successfully on Windows x86_64 MSVC:
**8,520 tests and 29 doctests passed**, with zero failures and 327 default ignores.
It used Rust 1.98.1 and the current capture/decoder changes. Receipts are
`target/test-cleanup/workspace-current-result.json` and
`workspace-current-summary.json`; the complete log is `workspace-current.log`.

Two native observer datasets were updated during that run. Their follow-up
validation passes **all 480 active notefield library tests** and **all three
provenance tests**, including the final focused native-geometry receipt. Those
results are in `native-gameplay-parity-current.json`,
`native-gameplay-provenance-current.json` and `native-all-provenance-current.json`.
The native update did not change production code or numerical tolerances. One
provenance guard was added after the full run and passes in the three-test
follow-up, bringing active coverage across the full run and follow-ups to 8,521
tests. The only Rust sources or Cargo manifests modified after the full run are
the provenance test and archive helper, both covered by their targeted follow-ups
(`current-coverage-reconciliation.json`). The effective inventory combines the
successful workspace artifact receipt with the two successfully rebuilt root
targets (`workspace-effective-artifacts.jsonl`): 118 libtest targets, 8,521 active
cases and 327 ignored cases. Its ignored identities exactly match the completed
audit; the composite receipt retains both source build results.

A follow-up clean-checkout audit found two unused archive aliases pointing to
uncommitted local captures. Those aliases are removed; no semantic case used
them. All 658 remaining aliases resolve to 654 tracked files through an explicit
reference catalog. The strengthened check passes and rejects an intentional
alias to an existing untracked capture, then passes again after restoration.
Committed archive entries also require their files even if old metadata labels
them local-only. The earlier green full run had not exposed that local-file
dependency. The final index/alias follow-up passes all nine active archive checks; its two expensive
archive audits remain deferred (`reference-index-current.json`). A separate
catalog reconciliation confirms that all 932 selected archives and all 654
reference targets are Git-tracked and present (`committed-inputs-final.json`).

The completed ignored-test audit verifies **225 passing cases and 102 permitted
long-song deferrals**, with zero failures. Of those passing cases, **214 were
rerun against current artifacts** and 11 generated-input benchmarks retain their
prior pass after exact executable SHA-256 and original one-test pass-log checks.
Those benchmarks generate their inputs in code and consume no refreshed oracle
capture. The original log hashes are retained in the execution records.

There were 216 fresh passing executions because the sprite GPU case ran once
for each of OpenGL, Vulkan and Vulkan-WGPU. All hardware cases ran again, each
in a fresh process. The runner requires one passing named test and rejects
zero-match filters; any required backend failure remains a failure. The obsolete
unconfigured GPU probe is superseded by its three explicit backend runs.

The successful release library build used `--workspace --release --lib --no-run`
and completed in 2,650.89 seconds. Every required release ignored case is a
library test. Its receipts are `release-library-current-build-result.json` and
`release-library-current-artifacts.jsonl`. An earlier broad release build was
intentionally canceled while compiling examples; that cancellation is separate
from the successful final build and test results.

The final execution receipts are `ignored-current/summary.json`,
`ignored-current/executions.json` and `ignored-current-driver.log`; the driver
finished with exit code 0. Classification and its missing-backend rejection are
recorded in `ignored-current-classification.json`. The completion audit checks
current test identities, executable hashes, named pass logs, all three GPU
backends, source pins and repository fixture catalogs (`completion-audit.json`).

The pinned sibling harness is **0.1.95**, commit
`91f69c2333db9600c7ab251145ac7269a46cfd64`. The schema-based
[capture inventory](oracle-fixture-inventory.json) distinguishes the actual
capture products:

- **All 133 short semantic traces have current receipts**: 46 from the official
  harness and 87 from an explicitly identified isolated adapter.
- **All 67 independent native actor captures have official 0.1.95 receipts**.
- **Both direct ScreenGameplay observer captures have current receipts from
  primary ITGmania `704cdd`**, including observed clock settings, native source
  hashes, observer hashes and archived input hashes.
- The current native rejection receipt covers two negative bitmap cases. The
  primary-codec capture covers 35 defined source images, 33 complete prepared
  uploads and two header-only observations.
- The focused full-song notefield capture was regenerated with official 0.1.95
  against the primary native executable: 21,920 model draws and 1,546 strips,
  including 25 first-arrow frames and both cap windows. Generation has no dropped
  observations. Its expensive compiled-song replay remains an explicit long-song
  deferral. The superseded 0.1.94 file is retained as historical data; no active
  parity test consumes it; its historical integrity hash remains checked.

The previous count of 134 short traces included two native notefield products.
The inventory now classifies these separately instead of treating an unused
historical capture as a short semantic test needing regeneration.

The active empty-layer sanity test previously compared default player values
from an old Bank Account archive. It now uses a two-second, no-Lua chart captured
by official 0.1.95: 121 updates and two native player tracks. Its exact check
passes, compares nonzero native observations, and still rejects an invented
uncompiled root. The refreshed provenance checks and all 284 active semantic
cases pass (`empty-control-verification-current.log` / `.json`, exit code 0).
This adds one short control without changing the test count. Other cheap
archive-helper checks validate archive integrity and rejection behavior. Two active whole-song regressions, BroGamer authored modifier windows
and Step Your Game Up critical render states, still compare historical archives.
They pass as legacy regression checks; their results do not establish current
full-song parity. Recapturing and validating those complete songs remains part
of the permitted long-song deferral, alongside the historical corpus. Fixture
content checks and synthetic tests that use old traces as templates are also
separate from the current short-control parity claim.

The 87 adapted traces retain their original sampling limits and explicit Lua
roots. Native-clock controls use real committed simfiles; the generator rejects
replacement by a symbolic clock. Thirteen originally linear-clock controls retain
explicit symbolic contexts. Thirty contexts use existing SM/SSC files; 29 small
committed SSC controls make formerly temporary timing inputs explicit. The
[context input documentation](../../tests/fixtures/itgmania-song-lua-micro/context-inputs/README.md)
explains their scope and regeneration.

Their adapter version is `0.1.95+primary-codecs.micro-controls2`. Its Rust semantic
evaluator modules are unchanged. Alongside primary image-codec integration, it
fixes a harness setup error: scoring switched the native game identity after the
noteskin manager was initialized, making valid skin setters and modifier queries
fail after loading a chart. The setup patch, CLI patch, driver hash, binary hash
and source identities remain recorded. It is explicitly distinguished from the
official unmodified harness.

All 284 active root semantic cases pass in the complete workspace run. Updated
coverage pins retain additional color, crop and shadow observations. Multi-fixture
controls compare every case before asserting their counts; the ease-table control
maps named observers instead of old actor ordinals. All 197 active playback tests
also pass, and both ignored playback GPU checks pass in separate fresh processes.
Running both GPU checks in one process hits the window library's event-loop
recreation restriction.

The refreshed Model/proxy readers retain their geometry, material and UV checks.
All 1,027 draws in the earlier Model refresh and all 668 draws in its final three
captures preserve their previous numeric observations at native float precision.
The two current direct gameplay observers retain 142 and 70 actual native frames.
Their nominal 120 Hz fixed update preference is serialized by ITGmania as
0.008333 seconds, matching the original native profile; this exact observed input
is now recorded alongside the -8 ms global offset.

The provenance guards verify harness/native identities, recipes, Lua/resource
inputs, adapter hashes and capture hashes. They also tie **67 RGBA exports
(147,648 bytes)** to exact arrays in current native captures. Text hashes normalize
CRLF for portable checkouts. Normal tests use repository inputs and do not need a
harness binary or sibling checkout. Oracle generation uses explicitly selected
external builds; it extracts the committed song archive instead of reading the
original song directory. The target and affected captures must be advanced
together when the harness changes.

The official Rust harness embeds ITGmania `5c737928` (v1.2.0). Its receipts retain
that identity. The primary checkout replaced PNG, BMP, GIF and JPEG loaders with
STB. An isolated codec adapter executes those primary sources, with 17 support-file
equivalence proofs. It found real BMP/GIF differences, now corrected in production:
BMP nibble order, palette bounds and top-down rows, plus GIF canvas dimensions,
frame placement and color-key handling. Undefined native palette reads are
excluded from pixel expectations; their defined headers remain checked and
initialized variants cover the intended pixel cases. All 83 active low-level
asset tests and all 154 asset integration tests pass after these changes.
The independent full-engine gameplay observations execute the actual clean
primary `704cdd` build, with `NoteDisplay.cpp` and `CubicSpline.cpp` hashes stamped
into it. No baseline capture is relabeled as a primary-engine result.

The retained full-song catalog contains 932 archives, including historical
versions. The 451 indexed archives declare harness versions no newer than 0.1.80.
Their older passing results are regression evidence, not proof of later harness
corrections. Long-song recapture/replay remains an allowed deferral; these runs
are not included in the current passing totals.

## Main corrections

- Fixed mismatched WASAPI test dependencies, an endpoint callback race,
  offline actor-method installation, immediate Sound play capture, initial
  effect-clock anchors, and premature interpolation before a modifier's first
  authored setter.
- Replaced original song/skin/download dependencies with temporary extraction
  of bundled files, committed parser inputs, generated images, and local HTTP
  fixtures. The explicit catalog selects 932 tracked song archives and ignores
  optional local captures. Mawaru9 now has a separate input-only bundle: all
  656 members match recorded hashes, and its pinned chart passes corpus coverage.
- Repaired stale assertions against current owners/APIs, native alpha/geometry
  behavior, NaN handling, valid ActorFrame update callbacks and native approach
  constraints. Original sibling update order remains checked.
- Repaired the multitap regression using the authored factory from an existing
  committed archive, generated lane data and a repository font alias. Startup
  and update errors are checked explicitly.
- Kept private code-generator examples as executable internal checks instead
  of invalid public doctests. Moved the manual performance program out of
  automatic test discovery. Custom replay harnesses accept Cargo's serial-test
  option.

## Deferred full-song work

There are 102 explicit long-song deferrals in the final Windows inventory.
This includes corpus-wide audits and individual whole-song replays, rather
than missing-file skips. These runs are not declared passing.

Seven whole-song controls were moved to this group after the initial sweep:
Bank Account complete semantics, Delightful Day movie/hidden layers, Mawaru5
draw colors, Mawaru7 idle holds, Mawaru8 local messages, and two Edgar controls
that each replay all 9,197 frames. The reduced multitap boundary regression
remains enabled. Before the final patches, the first five reported Bank Account
proxy/Mini differences, a Delightful Day coverage-count mismatch, Mawaru5
geometry/color differences with historical input selection still unvalidated,
and Mawaru7/8 scripts requiring a live callback context. These long controls
have not been verified on the final source and are not declared passing. An earlier explicit I (Ai) replay also failed and is
retained as a deferred whole-song failure, not a passing result. Mawaru9's full
replay remains deferred; its inputs and pinned chart are now self-contained.

## Reproduction

Use Rust 1.98.1 for this checkout:

```powershell
cargo +1.98.1 test --workspace -- --test-threads=1
```

Ignored benchmarks use fresh processes because several measure global allocator
state. Run a selected benchmark or hardware case with `--ignored --exact
--test-threads=1 --nocapture`; the GPU capture case also requires
`DEADSYNC_F0_BACKEND` set to `vulkan`, `opengl`, or `vulkan-wgpu`.

Regenerate the inventory from a Cargo JSON build receipt:

```powershell
python -B -X utf8 scripts/list_ignored_tests.py --artifacts target/test-cleanup/workspace-effective-artifacts.jsonl --executions target/test-cleanup/ignored-current/executions.json --output docs/testing/ignored-tests.md
```

Build and execution receipts are under `target/test-cleanup`, including
`workspace-current.log` and `workspace-current-summary.json`. The current capture
inventory is separate from the test execution receipts; historical full-song
archives do not establish parity with the current harness.

Audit actual captures independently of metadata:

```powershell
python -B -X utf8 scripts/audit_fixture_freshness.py --output docs/testing/oracle-fixture-inventory.json
```

Regenerate the five official-harness groups with the explicitly pinned harness
(these commands replace the requested fixture outputs):

```powershell
cargo +1.98.1 build --manifest-path ../itgmania-harness/Cargo.toml
python -B -X utf8 scripts/refresh_native_actor_fixtures.py --harness ../itgmania-harness/target/debug/itgmania-harness-rs.exe --harness-source ../itgmania-harness --output-root .
python -B -X utf8 scripts/refresh_semantic_fixtures.py --harness ../itgmania-harness/target/debug/itgmania-harness-rs.exe --harness-source ../itgmania-harness --output-root .
python -B -X utf8 scripts/refresh_semantic_fixtures.py --harness ../itgmania-harness/target/debug/itgmania-harness-rs.exe --harness-source ../itgmania-harness --manifest tests/fixtures/itgmania-song-lua-micro/empty-song/refresh-inputs.json --output-root .
python -B -X utf8 scripts/refresh_native_actor_fixtures.py --harness ../itgmania-harness/target/debug/itgmania-harness-rs.exe --harness-source ../itgmania-harness --manifest tests/fixtures/itgmania-song-lua-micro/native-refresh-inputs.json --output-root .
python -B -X utf8 scripts/refresh_native_actor_fixtures.py --harness ../itgmania-harness/target/debug/itgmania-harness-rs.exe --harness-source ../itgmania-harness --manifest tests/fixtures/itgmania-song-lua-micro/legacy-native-refresh-inputs.json --output-root .
python -B -X utf8 scripts/export_native_capture_pixels.py --output-root .
cargo +1.98.1 test -p deadsync --test oracle_fixture_provenance --test itgmania_actor_conformance --test song_lua_itgmania_semantic_parity -- --test-threads=1
```

The adapted group uses the isolated adapter described in the [context input documentation](../../tests/fixtures/itgmania-song-lua-micro/context-inputs/README.md). Stage its outputs, verify numerical parity, then accept the receipt.

The current ignored-test audit can also be run explicitly after both artifact
builds complete successfully:

```powershell
python -B -X utf8 scripts/run_ignored_test_audit.py --debug-artifacts target/test-cleanup/workspace-current-artifacts.jsonl --release-artifacts target/test-cleanup/release-library-current-artifacts.jsonl --prior target/test-cleanup/ignored-executions-final-input.json --output target/test-cleanup/ignored-current
python -B -X utf8 scripts/list_ignored_tests.py --artifacts target/test-cleanup/workspace-effective-artifacts.jsonl --executions target/test-cleanup/ignored-current/executions.json --output docs/testing/ignored-tests.md
```

The direct full-engine recipes are documented in the
[gameplay controls note](../../tests/fixtures/itgmania-song-lua-micro/gameplay-controls.md)
and the [focused notefield fixture note](../../tests/fixtures/itgmania-song-lua-micro/seventh-current-704cdd-native.md).
