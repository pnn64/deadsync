# Production Lua migration: fixed acceptance scope

The [verbatim user objective](objective.txt) is authoritative. This is a new,
explicitly authorized integration milestone. The interpreter and C API removal
milestones remain closed. This ledger records incomplete work; it is not a
completion report.

## Current migration work (2026-11-08)

The earlier full song snapshot plus launch-only retries passes 2,190 checks
with the recorded pre-existing mesh assertion. Core (262), noteskin (321) and
GIF native comparisons (112) pass. Replay-hook repair passes 23 retained-scene
checks; normal release build/link and a strict 600-frame rendered smoke pass.
These are earlier source snapshots; final-source refresh remains required.

The eight-package downstream run finished with 3,883 passes and 61 failures.
The [failure index](production-downstream-failure-index-v1.json) separates actual
failures from expected panic controls. Genuine actor-loading, large-scene,
semantic and diagnostic replay failures remain under repair/triage; independent
UI/input/model changes and lock-poison cascades have separate source evidence.
The 300-actor GC/native control and the unchanged Multitap and 7th Gear
regressions now pass. The expanded adapter supplement passes 121 checks before
the final queued-root fix. All nine complete prepared-payload comparisons of
the existing failing microfixtures now match native output after effect-state
publication, speculative broadcast isolation and deferred capture-mode repair.
The subsequent observer supplement passes 302 checks, including all 213
activation controls; its sole failure reverses two allocation-sensitive coverage
diagnostics. The effect/scope supplement passes all 17 live compiler, 10 effect,
27 actor and 13 message checks. The original gameplay prefix now passes all
72 checks after the Sprite-specific observation and retained texture fixes.
The compiler/mesh/render supplement passes all 46 checks. Sharkmode's missing
existing background-fit helper is repaired: its original direct/texture draw
test and the full shell suite (424 passes, zero failures, 16 original ignored)
pass unchanged.

The refreshed original full semantic target finishes with 306 passes, one
retained intermediate setter-history failure and 94 original ignored tests.
All nine original Rust test files are exact baseline bytes. The startup queue
branch now retains the initial self-queued command for replay: the unchanged
alpha-loop regression passes all 21,593 comparisons, including Model meshes.
The refreshed shell suite again passes 424/424 with its 16 original ignored tests.
The [order review](production-order-diagnostics-review-v1.json) explains the
retained test-support-only setter history and unranked coverage diagnostics
without weakening their assertions.

The complete original 88-target song scope has now run: 2,210 passes and three
failures, with original ignore attributes intact. The frozen no-Lua mesh and
unranked diagnostic-order assertions remain red and explained. The third failure
exposed overly broad tail-loader fallback. Production host entry now enables
that fallback while raw declaration loading retains upstream's missing-source
rejection. All 109 affected adapter checks, the original Multitap regression,
and all 72 gameplay checks pass against this final loader fix with strict source
guards. The development input-frame example's shared-type import is also fixed
and type-checks. No test assertions were changed.

The refreshed [dependency graph](production-lua-graph-v3/result.json) has 399
packages and no legacy runtime in either normal or instrumented normal/build
edges. All 33 changed Rust files pass formatting. The latest song Clippy run
reaches only the separately recorded frozen `cmd.rs:349` denied lint, with its
source guard intact.

Final release/link, rendered and measurement refreshes are running. Builds now
retain both the full workspace guard and the fixed migration input guard:
original inventory sites, the seven original Lua regression packages, application
sources and manifests. Independent audio edits remain explicit qualifications;
they do not add a workspace-freeze requirement to this goal. Completion remains
unproven until the original rendered and measurement gates finish.

The original gates remain unchanged: final-source regressions, build/link/graph
and rendered verification, formatting/Clippy attribution, representative matched
measurements and accurate documentation. See [completion-audit.md](completion-audit.md).
No new features or performance target have been added.

## Historical progress checkpoints

The dated entries below retain their original intermediate status statements.
They are evidence for those snapshots, not the current task list.

The [completion audit](completion-audit.md) keeps the original finish line
explicit. The [fixed inventory review](production-inventory-audit-v1/README.md)
now accounts for all original source, raw-access, handle and state/GC sites.
The cache/session source review is recorded separately; the full song regression
run remains active. A read-only formatting audit also identified migration
formatting cleanup to apply after that run releases its source snapshot.

The affected song regression run and its byte-identical launch retries are now
terminal: 543 checks passed across eleven targets. The sole assertion failure
is the already recorded renderer/test alpha mismatch, whose fixture constructs
its mesh directly without executing Lua. See
`production-affected-song-review-v2.json`. These results precede the subsequent
host compatibility changes and are not final-source verification.

Existing basic/version/date/table/JSON/offline/file/color/enum convenience
bindings have now been adopted into the Rust song adapter. The static 47-name
review and the additional 38 runtime type gaps all refer to the frozen host
surface; the latter references are recorded in
`production-dynamic-global-baseline-scope-v2.json`. This restores existing
bindings without reopening the completed interpreter or C API milestones.

`production-host-compat-oracles-v6-song-windows-debug` now passes 45 checks:
16 host-helper comparisons, nine file, seven framebuffer, eight utility-manager
and five public-routing checks. The source guard and formatting pass; see
`production-host-compat-review-v3.json`. The public error-service test covers
GC, recursive reporting suppression, callback failure and guard cleanup in both
preparation and live contexts. All original framebuffer probes are retained.

Earlier attempts preserve the initialization-order mistake, its visibility
compile error, the protected formatter-boundary mismatch and a test-only error
accessor compile error. The unchanged formatter's infinite-minute debug overflow
is now caught inside the song binding, matching legacy's Lua pcall behavior.
The original infinite inputs remain compared, with explicit native overflow and
Rust runtime recovery controls. Their expected panic-hook output is recorded;
no panic-hook or warning suppression was introduced. Native formatter and debug
walk equality to frozen source is audited in
`production-host-compat-native-reference-v2.json`. Normal production routing
without the oracle feature also passes all five checks in
`production-host-compat-normal-v1-song-windows-debug`, against the same unchanged
sources. Its 403 library warnings remain visible; final Clippy is outstanding.

Actor DeepCopy now exports the frozen live pose, flag and base-field metadata,
keeps explicit default base values, decodes copied declarations into independent
actors, and restores that metadata around speculative captures. Preparation
copies preserve the native replay getter exports and immediate flag presence;
untouched actors retain their ordinary raw shape. The current ten-target run
passes all 137 checks, including nine new copy controls. Source guards, unchanged
interpreter sources and changed-file formatting are verified in
`production-actor-copy-review-v2.json`. All five normal routing checks also
pass without the oracle feature against identical sources. The two measurement
examples now compile with explicitly labeled Rust heap estimates; their scope
notes distinguish those estimates from the frozen legacy allocator count. This
is tooling verification, not a completed performance measurement. The full song
regression suite is running in `production-full-song-after-copy-v1-song-windows-debug`.

Historical attempts retain the new-test syntax/setup mistakes, the materialized
pose and sampled-base defects, the before-playback export defects, and the new
flag test's incorrect receiver. The final flag control uses ActorFrame and
explicitly preserves the absent Quad method. No existing probes were removed
or weakened. Native BaseTransform bodies and tests match the frozen source,
with only cfg gates separating pure methods from legacy conversions.

Final core/adapter/downstream/content checks, source/lifetime/cache audits, the
final normal application build/rendered smoke/link checks, formatting/Clippy and
matched performance/memory measurements remain required by the same objective.
Older sections below preserve the evidence at their respective source stages.

## Frozen baseline

`baseline-sources.json` records SHA-256 hashes of 16,404 parent-repository files.
`baseline-copies.json` identifies the 1,289 source/configuration files preserved
under `target/rust-lua-production-before`. The fixture submodule is recorded
separately in `fixtures.json` and `fixture-sources.json`: 3,980 files at
`f896283ab5e3a861ba8517569f156b1b34fa2ab6`, with local status preserved. No missing
whole-song resources were downloaded or replaced with a different chart.

`inventory.json` records the three direct production consumers, ten downstream
packages, 84 source files referencing Lua handles, 384 raw/identity access sites,
and 949 retained-handle field/parameter candidates. These are conservative
source candidates, including inline tests and performance controls; each must
be classified during migration. Package dependency kinds establish the
production edges. `production-tree.stdout` preserves the original normal/build
dependency graph. Historical reports are not final-source evidence.

The previous C API removal verification script referenced by earlier chat is
absent from the current worktree. Its unavailable invocation is preserved in
`prior-removal-verifier.stderr`; it is not being recreated or used to reopen that
completed milestone.

## Requirements and evidence needed

| Requirement | Acceptance evidence | Current status |
| --- | --- | --- |
| All production consumers inventoried | Manifest edges, source references, raw calls and retained handles in `inventory.json` | Initial inventory recorded; classify every candidate during implementation |
| Fixed supported behavior and fixtures | Frozen source/fixture hashes, existing regression commands, whole-song resource identities | Initial full song run recorded; subsequent eleven-target retest and launch retries pass 543 checks with only the documented pre-existing renderer/test mismatch; final-source run pending |
| Full production song path | Actor/method/command/manager/input/message/media/clock/RNG/file/baking/live regression results and frozen output comparisons | Public preparation/live/reload facade is unconditionally Rust; final supported-content and regression validation pending |
| Noteskin preparation and callbacks | All bundled and regression skin output comparisons plus downstream assets/rendering tests | Unconditional Rust engine; 312 tests pass with unchanged sources; downstream rendering pending |
| GIF and other production consumers | All bundled GIF definitions and existing behavior tests; no remaining production legacy consumers | GIF loader switched unconditionally; 112 isolated comparisons and 15 production tests pass; final-source run pending |
| Ordering, shared state, errors and sandbox | Applicable existing tests and differential checks, including callback reentry and all original limits | GIF bounded execution and initial actor comparisons pass; full song integration pending |
| Root ownership and teardown | Forced GC while retaining callbacks, reentry, reload, session destruction and thread ownership checks | Pending full production paths |
| Core/domain separation | Generic core facilities only; actor/song/noteskin/GIF policy remains in owning crates | Required for every change |
| Existing preparation/live/fallback semantics | Production preparation and live-scene tests; every fallback uses the Rust engine | Pending |
| Cache correctness | Versioned/incompatible artifact invalidation with cache regression results | Noteskin version 30 and old-cache replacement verified; song integration pending |
| Unconditional normal engine | Normal application build, normal/build dependency tree, build/link evidence without legacy or experimental flags | Normal Windows debug application build and recorded gameplay smoke pass without an engine flag; normal and instrumented graphs contain no legacy Lua; final-source/link validation pending |
| Remove duplicate production backends | Final source/manifests/tooling audit; any legacy oracle isolated to dev/test use | Native bindings restricted to an explicit development-oracle configuration; final source/manifests/tooling audit pending |
| Final-source regressions | Core, adapters and applicable downstream tests; all failures explained or fixed without removing coverage | Pending |
| Actual application smoke | Song loading, gameplay and rendered noteskins through normal application, with artifact/screenshots | Normal migrated application passes the recorded DREAMING-ING!! gameplay case with rendered cel receptors and no warnings/errors; broader content and downstream regressions remain required |
| Formatting and Clippy | Applicable checks against final source; platform gaps explicitly reported | Pending |
| Before/after preparation, live and memory | Preserved baseline binaries, matched inputs, raw measurements and allocation/RSS scope | Song baseline and paired production noteskin timing recorded; migrated song/live/memory measurements pending |
| Accurate active documentation | Final architecture/status/validation match production code | Architecture authorization updated; final report pending |

Completion requires every row to be proven. Passing only the interpreter, one
adapter, a headless smoke, or a dependency-tree check does not complete this goal.

The completed interpreter and C API removal milestones remain closed. The
remaining work belongs to the separately authorized migration in `objective.txt`;
its acceptance scope does not expand when a check passes. In particular, the
`ProductID` failure in `production-rendered-dreaming-windows-v1/run.log` is an
omitted existing song-adapter binding, not a new interpreter requirement. That
run exited successfully and produced a screenshot, but its Lua compile warnings
make it a failed migration validation. The revised smoke recorder rejects app
warnings and errors in addition to checking process status and artifacts.

`production-product-identity-oracle-v2-song-windows-debug` passes all seven
utility-manager comparisons; the first new comparison's missing basic-global
setup is retained in the preceding failed run. The normal application rebuild
then passed in `production-product-identity-build-v1-application-windows-debug`.
The revised `production-rendered-dreaming-windows-v2` correctly fails validation
on the song INI loader's missing `RageFileUtil` binding. The replacement preserves
in-session write visibility and prevents writes to disk. Its public facade
check passes alongside the previous routing checks (four tests, no engine
feature) in `production-rage-file-contract-v2-song-windows-debug`.
`production-rage-file-oracle-preservation.json` proves that the native file
implementation bodies and all original file-test probes remain unchanged.

The first file comparisons exposed an adapter stack-window/return-value defect
and a mistaken new-test assumption about a removed Close method. The corrected
file comparisons pass all nine tests. The next smoke reached the authored INI
warnings but failed the strict log gate: the existing native `Warn` ignores its
arguments and records a side effect, while the Rust replacement had logged text
and missed that accounting. The replacement now preserves the original behavior
and `Trace` conversion rules. The smoke recorder still rejects warnings/errors;
no warning allowlist was introduced.

`production-warning-file-oracles-v1-song-windows-debug` passes all 17 file and
utility comparisons. `production-warning-public-contract-v1-song-windows-debug`
passes five checks without an engine feature. The normal application rebuild
passes in `production-warning-build-v1-application-windows-debug`.
`production-rendered-dreaming-windows-v4/process.json` records a successful
49.25-second gameplay smoke with a screenshot and no warnings or errors; its
binary SHA-256 is
`33f2306febd28a75ea16b3746dbeb0fe0ffe2f2f9c103d3d56c561c43d5d4284`.
This debug/cold-cache run is functional evidence, not a matched performance
comparison with the preserved release baseline. The sources stayed unchanged
through each successful check. Full migration validation remains incomplete.

The cutover test-harness repairs preserve the existing Lua probes and both
comparison engines. Native-only API controls now explicitly request the
development oracle; the complete oracle suite still includes them.
`production-regression-harness-source-audit-v1.json` verifies native input and
compiler body preservation and the extracted layer helper's unchanged body.
`production-all-song-tests-check-v3-song-windows-debug` successfully type-checks
every existing test target. The full regression run is recorded separately in
`production-all-song-regressions-v1-song-windows-debug`, which completed with
2,135 passing tests and six failed assertions across 84 executed targets. Two
additional test binaries failed to launch with Windows error 740. Byte-identical
copies with neutral filenames ran all 18 of their tests successfully, without
elevation or a policy change; hashes and source guards are retained in
`production-song-neutral-controls-v1/result.json`. The original Cargo run remains
failed. Its 43 existing ignored performance controls were not changed.

`production-full-song-review-v1.json` classifies the six assertion failures.
The missing existing SetFOV alias is fixed: two paired FOV checks and the original
manual-RGB playback check pass in the `production-set-fov-*` records. The mesh
alpha assertion also failed in the preserved pre-migration run and constructs its
mesh directly without running Lua. Message phase and retarget playback mismatches
are migration defects under investigation. The loader/sound controls first failed
inside their Native setup because their expected path spelling assumed Unix
separators. Preserving exact path comparisons with OS-native expectations exposed
a real Rust LoadActor source-directory spelling mismatch. Its fix passes the
saved-callback/scope-restoration control in `production-native-path-loader-v2`.
`production-native-sound-paths-v3` passes three comparisons, including exact
mixed/native separator spelling and deduplication. The original sound operations,
return-identity, invalid-input and GC checks remain present.

Message playback phase now follows actor traversal independently of a queued
callback timestamp. The original phase playback control passes in
`production-phase-playback-v1`. Frame publication also stops reusing cleared
capture timing as a fresh zero-duration scheduled write. The existing
phase-update and retarget-update fixtures pass complete paired compiler
comparisons in `production-retarget-full-pair-v2`. These new comparisons group
independent actor tracks while preserving target order, all samples and every
other field; the existing comparison callers retain their exact list ordering.

The original combined tween playback test then reached its ninth existing
fixture, spin-update, and exposed missing fallback spin/tween interpolation.
The first spin correction matches the fade, then exposes the exported-getter
handoff overwriting the retained render rotation on a later Turn command.
The final correction retains the spin pose/rotation queue in owned Rust actor
capture state and reads that pose for rendering. All earlier failed runs remain
preserved.

`production-spin-handoff-pair-v4` passes the complete compiler comparisons for
all three existing phase/retarget/spin fixtures. The new fixture comparison
groups tracks and stateful write records by independent actor, preserving target
order, sample order, every within-actor write and all values. Existing comparison
callers retain their original exact list ordering. The original combined playback
test passes all nine preserved tween datasets in
`production-spin-handoff-playback-v6`: one test function, with its original
assertions and tolerances. Sources were unchanged through both checks.

`production-regression-repair-source-review-v3.json` audits all 14 changed source
files and proves the 70 root song source files, interpreter sources and original
playback assertions unchanged from the full-suite repair baseline. Formatting
passes for all 14 files in `production-regression-repair-format-v1.json`.
The broader 11-target `production-affected-song-regressions-v2` run is in progress
against the frozen source. It runs every selected target through failures and
retains the documented pre-existing mesh assertion. If Windows 740 recurs, the
launch-only retry recorder runs byte-identical neutral copies without elevation;
its results must be reviewed separately. These focused checks do not claim a
green full suite or final migration completion.

`production-static-global-candidates-v1.json` records a read-only review of the
existing native host/compat/file installers. It flags 47 existing global names
that have no mention in the Rust adapter code, including compatibility/version,
JSON/INI and date helpers. These require runtime verification and classification
against the frozen supported surface; a source mention alone would not prove
installation or behavior. This is an inventory audit within the original
migration scope. The passing single-song smoke does not close that requirement.

## Baseline regression results

Commands and source hashes are retained in each check directory. The current
verified results are:

- `baseline-core-linux-debug`: 938 tests passed, source hashes unchanged.
- `baseline-noteskin-linux-debug`: 298 tests passed, source hashes unchanged.
- `baseline-song-windows-release`: compile failure before tests, source hashes
  unchanged. `actor_input_render` includes `playback/prepare_tests.rs`, whose
  `crate::tests::init_paths()` does not exist in that integration test crate.
  Repair the shared test initialization and rerun the full suite. Do not remove
  the included preparation tests or the rendering test to obtain a pass.
- `baseline-gifs-windows-release`: all 15 existing GIF tests passed, source
  hashes unchanged.
- The normal Windows application baseline Cargo build succeeded. The wrapper
  reported source changes in the independent core, which was outside that
  build's dependency graph. `baseline-application-windows-release/dependency-source-audit.json`
  records the dependency-file check and preserved executable hash. The original
  wrapper result remains unchanged. This is build evidence, not rendering smoke.

`checks.py` records complete commands, logs, environment, source hashes, elapsed
time and exit status. It refuses to overwrite an existing check directory.

## Performance and runtime workloads

`build_workloads.py` builds and preserves the existing `song_lua_execution`
normal shell binary, the `song_lua_performance` preparation example and the
noteskin `compare_lua_engines` example. The normal shell binary is built
separately from example/dev dependency feature unification. Each executable has
its own retained hash. Matched before/after runs must use the same fixtures and
configuration; preparation and live timing remain separate.

Measure both Rust allocation counters and process memory where available:
legacy Lua's C allocations are outside Rust global allocator counters, so those
counters alone cannot establish total memory parity. Record this distinction
with the raw measurements. Use the existing application `--perf-case` support
for actual rendering validation, separately from the headless live harness.

The initial Linux workload snapshot command accidentally traversed nested
ignored `target` directories. Its verified Python process was terminated before
starting Cargo, and the runner now enumerates the frozen source manifest. The
first filtered invocation failed because a non-login WSL command lacked Cargo
on PATH. The subsequent `baseline-pathfixed` invocation uses the login shell.
These are harness failures, not application results.

Linux production workload builds also needed D-Bus and systemd development
metadata. `linux_native_deps.py` downloaded pinned Debian packages into the
existing isolated prefix; package hashes are in `linux-native-deps.json`. No
system packages were installed. `baseline-dbus` successfully built and preserved
the normal headless live binary and preparation example, with source hashes
unchanged (`baseline-dbus-workloads-linux-debug/changed-sources.json`).
The noteskin comparison executable was preserved by `baseline-native`.

`baseline-noteskin-adapter.json` verifies complete native output equality for all
26 bundled dance/pump skins, with three comparison pairs per skin. This was a
debug build run alongside compilation: its timings are functional-run records,
not the requested release performance comparison. Before/after release timings
and process memory measurements remain pending.

The `pulse-driver` and `late-wrapper` microfixtures produced prepared actors but
no retained VM under the unchanged production backend. The live harness rejects
such baked-only cases by design; both raw reports are preserved as
`baseline-live-*.json`. They are not new migration failures and must not motivate
forcing otherwise eligible scenes into live execution. The full Cursed Metamorph
song also prepared into native actors and was rejected by this live-only harness;
`baseline-live-cursed-v2-linux-debug` records its ten input hashes, output and
99,233,792-byte process peak RSS. Selecting a supported interactive/input-dependent
source for the retained-callback baseline remains required. The existing
production fallback selects live execution on the explicit input-runtime request;
ordinary successful compilation preserves native playback.
Bad Trails registers `AddInputCallback` in its existing `lua/game.lua`; its
71-file input snapshot and retained-callback baseline run are under
`baseline-live-bad-trails-linux-debug`. It completed with a retained VM, 481
updates and no Lua errors, but only one visual change, failing the existing
live harness assertion. That failed baseline is preserved; the assertion was
not weakened. The script waits for input to advance its game.

`baseline-live-dreaming-windows-release` passed the same unchanged harness for
DREAMING-ING!!: retained VM, 481 updates, three changed visual frames, no Lua
errors and 92,045,312-byte peak working set. Its 25 input hashes are pinned.
`baseline-prepare-dreaming-windows-release` records three preparation samples
(1.3652, 1.3332 and 1.3354 seconds), the compiled output hash, and an
80,695,296-byte peak working set. Both used preserved legacy executables.
These runs overlapped compilation; controlled before/after timing comparisons
remain required. Windows memory now uses GetProcessMemoryInfo high-water
working-set and commit counters, plus private-byte samples.

All three Windows workload executables were built and preserved successfully.
Their wrapper also reported independent-core changes;
`baseline-workloads-windows-release/dependency-source-audit.json` distinguishes
the unchanged normal song binaries from the comparison tool's interim Rust
adapter. The latter's Rust side is not final-source evidence.
The first Cursed invocation lost the path suffix through nested shell quoting
and was stopped during input enumeration, before launching the application.
The runner now rejects nonexistent executables/simfiles before enumeration;
the replacement passes the complete path directly to WSL and pins ten files.

## Work in progress

The GIF replacement in `step_stats_gifs/rust_engine.rs` is now the unconditional
production capture implementation. The old implementation and direct legacy
dependency were removed from the theme crate. At that stage the song dependency
still brought legacy into the application. The song facade has since cut over
and normal application graphs omit legacy; final cutover validation is incomplete.
`gif_probe.py` builds an isolated comparison against the exact
frozen capture implementation; its enum shims represent only the two player and
clock variants used by capture. The probe uses bounded VMs and checks oversized
runtime strings/tables, but is not final theme or application validation.

The comparison exposed and fixed an integer-overflow conversion difference:
the legacy Lua 5.1 bridge distinguishes exact integers from other numbers when
converting frame indices. The current probe passes 28 cases, each under both
players and aspect ratios (112 comparisons), including all 12 bundled scripts,
callback ordering, forced GC/reentry, numeric conversion, sandbox globals and
runaway root/callback scripts. Raw per-attempt results and source hashes are in
`gif-probe-runs`. Failed attempts remain recorded.

The generic core now offers a bounded VM and compilation through that VM's
budget. State buffers, retained parser roots, scratch strings and loaded
programs are charged to their owning domain. Compiler and bytecode-export
workspaces conservatively charge allocation requests through job completion;
streaming loads retain those charges across reader calls without activating
the workspace around Lua/host callbacks. Nine focused tests pass, covering
initialization, alignment, retained programs, runtime strings/tables, loadstring,
reader reentry, loadfile, bytecode dumping and recovery after memory errors.
This is not a process-RSS bound; caller-owned source, opaque Rust binding data
and operating-system resources are outside it.

The first broad run, `bounded-loading-owned-core-linux-debug`, exposed an
existing protected-error invariant after loader storage became VM-owned.
The loader now obtains the first reader result before allocating parser state,
so an already-raised reader memory error is returned without another allocation.
The original failing test passes unchanged; the full rerun under
`bounded-loading-fixed-core-linux-debug` passed all 947 tests with source hashes
unchanged (including the original 938 tests and nine new budget tests).

The first strict all-target Clippy invocation stopped on three existing legacy
oracle warnings (`manual_range_patterns`, `chunks_exact_to_as_chunks` and
`unused_enumerate_index`). Its log is `bounded-loading-clippy-linux.log`.
A `--no-deps --all-targets -- -D warnings` check now passes for the Rust core
(`bounded-loading-clippy-fixed-linux.log`). A new test's `err().expect()` was
changed to an explicit match; the test assertions and production code were
unchanged. The legacy oracle warnings remain recorded separately.

The missing song-test initialization was moved verbatim into
`tests/support/paths.rs` and shared by both playback integration crates. No
preparation or rendering tests were removed. The queued Windows baseline rerun
under `baseline-repaired-song-windows-release` failed compilation after the
noteskin manifest changed while it waited for the target lock: Cargo had
resolved the old optional dependency graph. It is not a backend behavior result.
The fresh `song-after-noteskin-song-windows-release` invocation completed with
1,130 tests passed, one failure and 43 existing ignored tests. The failure is
`song_lua_actor_multi_vertex_builds_textured_mesh_overlay`: the renderer replaces
glow RGBA with `[1; 4]`, while the assertion expects the source alpha. Both the
renderer and test exactly match the frozen baseline. This direct native overlay
test executes no Lua. `glow-baseline-audit.json` records the hashes and source
analysis; neither the renderer nor its failing assertion was changed. This is
a pre-existing source mismatch, not an executed frozen-baseline test result.
The run's broad source hashes changed during Rust actor work, so it remains
interim evidence. The original wrapper counted 916 tests in passing binaries;
the audit includes the additional 214 passes in the failing playback binary.
Future wrapper reports count passes in both successful and failing binaries.
`gif-cutover-gifs-windows-release` passed all 15 production tests; its wrapper
reported source changes during the run (noteskin cutover and core test lint fix).
This remains interim evidence, not a final-source application check.

## Noteskin production cutover

`compiler::compile_data` now always uses the shared Rust engine. The engine
feature and duplicate production compiler were removed. The normal/build tree
in `noteskin-cutover-normal-tree.stdout` contains `deadlib-lua` and no legacy
runtime. Legacy is a dev dependency used only by `tests/support/legacy.rs`,
historical optimization controls, integration comparisons and the comparison
example. The oracle's 16 engine function bodies match the frozen baseline
(two require rustfmt normalization); `noteskin-oracle-source-audit.json` records
their hashes and the import/visibility adaptations needed for an external test.

Compiler version 30 changes the source/cache identity. A regression verifies
that a version-29 cache is rebuilt, remains on disk unchanged, and the new
bundle is subsequently reused. Differential checks assert the exact 29-to-30
transition before comparing all payload fields and serialization round trips.

`noteskin-cutover-oracle-fixed-noteskin-linux-debug` passed 311 tests. The further
`noteskin-callbacks-complete-noteskin-linux-debug` run passed 312 tests with source
hashes unchanged: 299 unit tests, five existing integration tests and eight
migration tests. Comparisons cover all 26 bundled skins, six authored fixtures,
seven regression fixtures under both games (including the existing explicit
pump rejection), fallback closures, color resources, errors and property
metamethods. The added byte-for-byte callback fixture test also runs against the
production Rust entry point. New production tests retain the historical callback/alias coverage,
force GC during callbacks and aliases, check panic propagation and destruction,
and verify the old safe-library restrictions. The Rust adapter now removes
`debug` from globals and loaded modules and preserves the disabled C-loader
behavior. Historical C-bridge panic transport tests remain dev-oracle-only;
the production Rust host contract propagates panics to Rust and drops the VM.

The first cutover run failed because the external oracle needed a private
comment parser helper and a module alias. The helper was copied from the frozen
implementation; no failing test was removed. The strict noteskin Clippy run
reports existing warnings in unchanged code and historical test controls; its
raw log is `noteskin-cutover-clippy-linux.log`. Ordinary Clippy passed with no
diagnostics in the new adapter or migration tests; the source comparison,
diagnostic classification, format check and exact lint scope are recorded in
`noteskin-cutover-lint-results.json`. Final downstream/application
checks and controlled release performance comparisons remain pending.

## Rendered application baseline

`baseline-rendered-dreaming-windows` preserves a completed run of the normal
legacy application, including its strict case manifest, normalized isolated
configuration, result, process exit, application log and inspected gameplay
screenshot. The executable SHA-256 matches the preserved baseline build.
The run selected DREAMING-ING!!'s Challenge dance-single chart
`2f76e1ebbb68262e`, using OpenGL, a 1280x720 window and WASAPI shared audio,
with 120 warmup and 480 measured frames. It exited successfully in 17.44 seconds.
The screenshot shows the gameplay screen, four receptors, the cel noteskin
label, song background and autoplay indicator. Its early capture does not
establish later moving-note behavior or input handling. No warning/error was
logged in this run. Concurrent compilation makes the timing unsuitable for a
controlled performance comparison.

The two earlier attempts remain recorded under
`target/rust-lua-production-before/render-baseline`: an invalid case name,
then a strict monitor-refresh mismatch (239000 versus actual 240000 mHz).
The successful attempt uses the actual refresh and the normalized isolated
configuration's hash. This is before-migration evidence; a normal migrated
application build and rendered smoke test remain required.

## Song actor migration in progress

The resumed actor experiment is still behind the song crate's `rust-engine`
feature and is not the production song backend. It implements initial
Actor/ActorFrame/Quad instances, rooting, release, basic transforms, synchronous
commands and initial tween/update lifecycle bindings. Other actor classes and
methods, managers, input/media, preparation/baking and full live scenes remain
to be migrated.

Production actor controls exposed assumptions in the deferred experiment that
did not match the current application's behavior. The production constructor
materializes every child before child-first Init, supplies no Init parameter,
retains declaration fields on each instance, exposes its metatable, and does
not synthesize inherited `ctx` tables. Duplicate-child groups dispatch through
the current last instance; visibility follows the production boolish coercion.
`actor-production-aligned-song-linux-debug` passed all ten focused tests with
source hashes unchanged. The original experiment and its assertions remain
preserved under `analysis/lua/deferred-actor-instances-2026-11-04`.

`actor-command-production-login-song-linux-debug` passed thirteen tests with
source hashes unchanged, including direct production comparisons for dynamic
command fields and message fallback, parameter arity and identity, reentry/error
recovery, traversal, duplicate-child groups and public class lookup. The initial
direct WSL invocation could not find Cargo; its spawn error is retained under
`actor-command-production-song-linux-debug`. The login-shell retry used the
installed toolchain. The production
implementation, rather than additional upstream features, defines acceptance.
In particular, its exposed `RunCommandsRecursively` currently reports that it
has no semantic implementation; supplying a new implementation is outside this
migration. Final actor validation and production song cutover remain pending.

`actor-scope-scalars-song-linux-debug` passed fifteen tests and caught a default
visibility mismatch: production Actor instances start visible. The adapter was
fixed; `actor-scope-scalars-visible-song-linux-debug` passed all sixteen tests
with source hashes unchanged. The comparisons also cover invalid setter values,
numeric strings, NaN/infinity behavior, signed sizes, draw-order rounding,
per-instance metatables, command-name conversion, callback directory restoration
through reentry/errors/GC, and restoration after fatal instruction limits.
Authored command errors propagate; reserved theme callback errors report and
continue. Dynamic handlers remain rooted when replaced during a callback.

`actor-complete-adapter-regressions-song-linux-debug` passed all 54 current Rust
adapter tests with unchanged sources: 16 actor, eight clock, twelve declaration,
eleven loader and seven RNG tests. These verify the current partial adapter;
they do not establish full production song migration. `actor-format.json`
records a passing formatting check for the changed Rust adapter files.
`actor-bindings-lint-song-linux-debug` records a Clippy failure on the existing
denied `while_immutable_condition` lint in `cmd.rs:349`. Its baseline hash is
identical. No diagnostics were emitted in the Rust adapter, but Clippy stopped
before completing the requested integration-test lint. The baseline audit
records that limitation; neither the source nor lint policy was changed.

`actor-lifecycle-initial-song-linux-debug` records an initial test-harness
compile failure (four integer literals supplied to a floating-point phase).
After correcting those literals and exposing the production numbered child
entries, `actor-lifecycle-children-song-linux-debug` passed all 23 actor tests
with unchanged sources. Its timeline comparisons use the actual production
constructor, startup functions and compiled update traversal, not a copied
legacy queue. They cover native float timing, current/destination getters,
sleep, hibernation, easing/Bezier controls, stop/finish/hurry, overflow,
command propagation/reentry, update rates, callback replacement, GC and partial
update failures. Screen activation, message broadcasting and the remaining
production host are still pending; these subtree APIs do not establish a
complete song startup or live session.

`actor-lifecycle-adapter-regressions-song-linux-debug` passed all 64 Rust
adapter tests with unchanged sources: 26 actor, eight clock, twelve declaration,
eleven loader and seven RNG tests. Added checks preserve production's cached
traversal/invalidation behavior, resolve replaced callbacks at invocation,
enforce the existing 10,000-command queue bound, and verify fatal callback
limits restore directory scope and permit subsequent recovery. The legacy
lifecycle, tween math and value-conversion source files match the frozen
baseline byte-for-byte (`actor-lifecycle-controls-audit.json`).
`actor-lifecycle-format.json` records a passing format check for the five
changed Rust adapter/test files. Full production migration remains pending.
`actor-lifecycle-lint-song-linux-debug` again fails on the pre-existing denied
`while_immutable_condition` in `cmd.rs:349`, whose hash still matches the frozen
baseline. Sources stayed unchanged and no diagnostic refers to the new Rust
adapter. Clippy did not reach the integration-test lint; its baseline audit
records this limitation without changing the source or lint policy.

## Song message dispatch and template ownership

The Rust adapter now supplies `MESSAGEMAN:Broadcast`/`SetLogging`, queued
messages, native host broadcasts and authored Judgment normalization. Listener
discovery snapshots the owned registry; delivery uses stable VM object
identities, looks up the current handler at invocation, and permits synchronous
reentrant broadcasts while retaining the direct-command recursion guard.
Omitted and explicit nil arguments remain distinct. Parameter tables retain
identity, queued messages create empty shared parameters, suppression and
broadcast history follow the existing host, and active-message/directory scope
is restored after ordinary errors and fatal instruction limits.

The only core addition is a host-call accessor for the existing stable opaque
object identity query. Message policy and note/result bindings remain in the
song crate. Note/result getters capture traced Lua values, including a retained
result after public fields are removed, GC runs or the actor scene is released.

`actor-messages-initial-song-linux-debug` records a test-helper borrow error.
`actor-messages-comparison-song-linux-debug` then passed the 26 actor tests and
two message tests; eight message comparisons exposed an extra production
registry entry: the global live `NullActor`. The adapter now creates
that native template, retains it independently of scenes, and makes independent
instances for declaration occurrences. The declaration-source suite retains
the unchanged upstream functions but obtains its NullActor control from the
actual production constructor; its metatable/type and native `getaux` behavior
are checked explicitly. This corrects the earlier assumption that the complete
host's NullActor was only the upstream plain declaration.

`actor-messages-null-production-song-linux-debug` passed all 49 actor,
declaration and initial message tests. `actor-messages-template-controls-song-linux-debug`
passed 12 message tests and exposed missing native state when cloning a
modified template. The adapter now copies its destination pose, immediate
fields and independent timer into a fresh actor without advancing or copying
the template's existing tween queue. The failing observation is retained.
`actor-messages-complete-adapters-song-linux-debug` passes all 77 current Rust
adapter tests with unchanged sources: 26 actor, 13 message, twelve declaration,
eight clock, eleven loader and seven RNG tests. The two added controls cover
metamethods that force GC during Judgment normalization and modified-template
state across subsequent frames.

Cross-engine message comparisons import the actual legacy subscriber identities
through a test-only control, as the existing production oracle does. Allocator
addresses are not equal across engines. A separate test checks unmodified Rust
delivery against the VM's own object identities and proves scene release removes
listeners while the global template survives. No identity override is present
without `test-support`.

`actor-messages-core-core-linux-debug` passes all 947 default core tests with
unchanged sources; `actor-messages-core-lint-core-linux-debug` passes the core
library's Clippy check. `actor-messages-format.json` records successful targeted
format checks. `actor-messages-controls-audit.json` confirms the production
message host, lifecycle, conversion, recording and class-binding files used by
the controls still match their frozen baseline hashes.

`actor-messages-song-lint-song-linux-debug` finished with the existing denied
`while_immutable_condition` in `cmd.rs:349` and one new `manual_contains`
warning in the test-only subscriber-rank check. The latter is corrected in
the subsequent actor render work. Its baseline audit preserves both findings;
this run is not a passing Clippy result.

This is still the opt-in song adapter. Full message capture/baking, screen and
input integration, the remaining managers/actor methods/media, production song
cutover and final application validation remain required. Passing these tests
does not establish the migration's completion.

## Song actor render properties

The Rust instance host now binds diffuse/glow and corner/edge colors, crop/fade,
skew, accumulated quaternion rotation, base transforms, alignment, shadows,
zoom-to-size and rectangle scaling, blending, culling, recursive depth settings,
texture filtering and ActorFrame projection settings. Current local render
state is exposed in the actor snapshot for the pending song playback adapter.
Tween destinations use the existing native `TweenPose`; immediate properties
are stored separately and survive template cloning without copying its queue.
Color getters return independent Lua tables. The adapter's zoomed-size getters
now include the immediate base scale in production's multiplication order.

`actor-render-initial-song-linux-debug` passes seven differential tests with
unchanged sources. They compare every named field in local render state with
the production reader and actual current tween queue, plus Lua-visible getters.
Coverage includes tween boundaries/stop/finish, gradients and alpha retention,
raw table color reads and coercion, nonfinite values, flipped/degenerate
rectangles, recursive depth write ordering, strict enum/boolean errors,
independent getter tables after scene release, reentrant messages and update
callbacks that force GC. The legacy binding, conversion, quaternion, tween and
render-state sources match the frozen baseline byte-for-byte
(`actor-render-controls-audit.json`).

`actor-render-complete-adapters-song-linux-debug` passes all 84 current adapter
tests with unchanged sources: 26 actor, thirteen message, twelve declaration,
eleven loader, eight clock, seven RNG and seven render tests.
`actor-render-format.json` records the passing targeted format check.
`actor-render-lint-song-linux-debug` stops at the existing denied
`while_immutable_condition` in `cmd.rs:349`; its baseline hash is unchanged and
no diagnostic refers to the Rust adapter. The earlier `manual_contains`
warning is gone. Clippy did not complete the requested integration-test lint,
so this is still a failed check, not a pass.
`actor-render-core-source-audit.json` verifies all 240 core source and root
Cargo/configuration entries are unchanged since the 947-test core run; no core
implementation changed in this actor-only work.

This is still the opt-in song adapter. These local-state comparisons do not
establish compiled output, ancestor/effect composition, media rendering or a
complete live session. Those paths, remaining actor/manager/input bindings,
production song cutover and final application validation remain required.

## Song actor effect clocks

The Rust actor host now owns effect selection/defaults, color/magnitude/timing
parameters, timer/music/beat phase, no-offset song timing and native spin before
tween updates. Clock state is retained across frames, effect reselection and
template cloning. The render-state sampling API exports the retained effect
phase and spin-applied flag for the pending production song playback adapter.
Music clock interpretation shares the runtime's existing live/preparation mode,
rate and origin; no engine-specific behavior was added to `deadlib-lua`.

`actor-effects-initial-song-linux-debug` passed four comparisons and failed
three. The failures exposed cached rotation getters: spin advances current
render rotation without necessarily publishing the same value to Lua getters.
`actor-effects-error-control-song-linux-debug` retains those failures and a
fourth failing control for the partially created timer after its first external
clock read errors. Assertions and production controls remain in place.

The adapter now preserves rotation publication at explicit setters,
stop/finish, diffuse writes and preparation queue steps. Its retained-playback
boundary preserves the existing change in diffuse-write publication behavior.
Failed first clock reads retain an uninitialized phase with the existing
getter/error and recovery behavior. Runtime tables are explicitly rooted while
metamethods detach them, reenter actor bindings or collect.

`actor-effects-clock-fix-song-linux-debug` passed 17 effect/render comparisons.
Adding preparation mode to the spin comparison exposed a test-control mismatch
in `actor-effects-complete-adapters-song-linux-debug` (55 passed, one failed;
Cargo stopped before the remaining test binaries). The control removed live
mode before construction, leaving its actors without native tween replay, while
the Rust actor always had that storage. `actor-effects-replay-controls-song-linux-debug` retained
the failure because the newly owned preparation root is only registered during
Init. The control now initializes actual production replay storage by walking
the owned tree before Init. All original state/getter assertions remain.
`actor-effects-owned-replay-controls-song-linux-debug` passes all 94 adapter
tests with unchanged sources. These tests compare local native replay in both
clock modes; they do not establish the compiler's distinct startup-capture and
baking paths. Full compiled-output comparisons remain required.

`actor-effects-controls-audit.json` confirms the production binding, conversion,
tween, runtime and render-state sources still match the frozen baseline.
Production song cutover, full captured output and final application validation
remain pending; these subtree bindings do not establish migration completion.

## Song wrapper actors

The Rust host now implements `AddWrapperState`, `GetWrapperState` and
`GetNumWrapperStates`. Wrappers use ActorFrame methods and have separate
membership from ordinary children. Scene snapshots expose their ordered indices;
the scene registry roots their Lua fields and callbacks and retires them on
release. Wrapper creation inherits the current script/song directories and the
existing player/layer tags, including coercion, error and partial-registration
behavior. Temporary construction tables stay rooted during collecting lookups.

Update plans advance nested wrappers before their owner, after its hibernation
gate and before its rate multiplier. A wrapper callback can change the owner's
rate for the same frame. Creation invalidates subsequent plans while the active
frame retains its original traversal. Child command propagation, recursive
finish/hurry and tween-time queries continue to exclude wrappers; broadcasts
include registered wrappers.

`actor-wrappers-initial-song-linux-debug` passed six comparisons;
`actor-wrappers-complete-adapters-song-linux-debug` passed 101 after adding
directory/GC and tag-coercion coverage. `actor-wrappers-owned-final-song-linux-debug`
and the final-source `actor-wrappers-clean-song-linux-debug` pass 102 tests with
unchanged sources: 26 actor, thirteen message, twelve declaration, eleven
loader, ten effect, eight clock, eight wrapper, seven RNG and seven render tests.
The added error-path comparison verifies that directory failures precede
registration, while tag failures leave registered actors without a parent.

`actor-wrappers-final-format.json` records nine passing targeted Rust format
checks. `actor-wrappers-controls-audit.json` confirms eight production control
files, including `cmd.rs`, still match the frozen baseline. All 240 core and
root Cargo/configuration entries remain unchanged since the 947-test core run
(`actor-wrappers-core-source-audit.json`). No engine implementation changed.

`actor-wrappers-lint-song-linux-debug` identified a redundant binding in the new
update traversal; it was removed before the final 102-test run.
`actor-wrappers-final-lint-song-linux-debug` still fails at the unchanged denied
`while_immutable_condition` in `cmd.rs:349` (164 existing warnings, no diagnostic
in the Rust adapter). The library failure prevents completion of the requested
integration-test lint. This remains a failed Clippy check, with baseline source
identity recorded separately; no lint or test coverage was disabled.

These comparisons cover owned local actors and their lifecycle. Wrapper render
composition, full compiler capture/baking and complete production live scenes
still require integration and baseline comparisons. The song adapter remains
opt-in; the migration finish line and final application validation are unchanged.

## Song sound actors and shutdown

The opt-in Rust actor host now owns Sound handles, immediate play/pause/stop
events, file loading, media-prefix lookup and cached decoded lengths. Per-handle
pitch and volume preserve existing coercion and chaining. Handle functions retain
their actor and handle through traced Lua captures; their native length cache is
owned callback data. `HostCall::new_host_function_with_data` forwards the VM's
existing generic constructor so callbacks can create these closures without
putting actor policy in the engine. Its GC/drop regression covers creation during
a collecting callback and destruction both during collection and VM teardown.

Preparation sound capture preserves beat, nested message scope, queued-startup
phase and the existing ordering of captured events before writes to the Lua
sound-call list. Live sound operations retain each owner's identity and run
immediately, independently of tween cursors. Prefix misses and length failures
retain their existing cache behavior. Runtime path errors are ignored as before;
fatal engine limits remain fatal.

`actor-sounds-initial-song-linux-debug` passes all eight Sound comparisons with
unchanged sources. These use actual PCM WAV files and the production bindings,
including borrowed methods, independent handles, file changes, prefix selection,
negative caches, nested getters that collect, errors after partial capture and
retained resources after actor retirement. Scene retirement checks owner Stop
events separately from command callbacks.

The added End phase detaches the top-screen global, dispatches End to ordinary
descendants of the synthetic scene root without advancing queues, then stops
owned sounds. Repeated End is inert, ended trees reject updates, and subsequent
release does not repeat successful shutdown's Stop events. Authored End failures
retain partial effects and mark the scene ended. The actual production End
walker supplies the callback control; its subsequent Stop policy is checked
separately against `SongLuaLiveScene::end`. This does not yet integrate the full
screen manager, input/draw callbacks or reload path.

`actor-sounds-end-song-linux-debug` retains nine passing tests and one setup
failure. The new normal-End fixture incorrectly called `GetWrapperState` on the
wrapper returned by `AddWrapperState`; production rejected that fixture before
shutdown ran. The authored-End-error comparison passed in that run.
The fixture now assigns its handler to the returned wrapper directly, retaining
the original assertions. `actor-sounds-final-adapters-song-linux-debug` passes
all 112 adapter tests with unchanged sources: the previous 102 plus ten Sound
and End tests. Both normal and failed shutdown preserve the expected partial
effects, callback order, queue state and once-only owner cleanup.

`actor-sounds-core-core-linux-debug` passes 948 interpreter tests with unchanged
sources, including the new callback construction/rooting/drop check.
`actor-sounds-core-source-audit.json` verifies all 240 core and root Cargo/config
entries still match that passing run after the song fixture correction.
`actor-sounds-format.json` records eleven passing targeted Rust format checks.
`actor-sounds-core-lint-core-linux-debug` passes the core library Clippy check.

`actor-sounds-lint-song-linux-debug` fails at the existing denied
`while_immutable_condition` in `cmd.rs:349`, with 164 existing warnings and no
Rust-adapter diagnostic. The library failure prevents the requested Sound test
lint from completing. The command source remains byte-identical to the frozen
baseline; this is still a failed check, not a waiver or passing result.

`actor-sounds-controls-audit.json` verifies nine production control files against
the frozen baseline. `lua_util.rs` and `live.rs` have only the two exact,
test-feature-gated capture/output accessors recorded in the audit; after removing
those accessors their production text matches the baseline. Other control files
remain byte-identical. No production binding behavior or failing assertion was
changed to make these comparisons pass.

Full song cutover, complete captured output, final application validation and
the representative song performance/memory comparison remain required.

## Song Sprite resources and texture capture

The opt-in Rust song host now constructs Sprite instances and owns texture loads,
retained texture metadata, image frame geometry, animation/UV properties and
preparation texture events. Texture handles retain their original metadata across
later loads and direct field edits, while loop/rate methods address their owning
actor. Traced captures preserve those handles through collecting callbacks and
release their cycles after the last Lua reference disappears. Quad construction
retains the existing empty-path, 1x1 default texture.

The public media-capture boundary is now `begin_actor_capture` /
`end_actor_capture`; it covers Sound, Sprite texture and judgment texture output.
The Sprite comparisons check actual production events, actor identity, epochs,
frame sheets, state lists, queued-startup suppression and errors after partial
mutation. They also compare complete local render snapshots, including the native
tween pose, and Lua observations. Real PNG fixtures exercise sheet dimensions,
texture hints, retained handle dimensions, crop/scale operations and repeated
authored Texture lookups that reenter the host and collect.

`actor-sprites-initial-song-linux-debug` preserves a compile failure caused by
using an internal alias instead of the exported texture-offset helper.
`actor-sprites-controls-song-linux-debug` preserves two passing tests and five
failures. Four exposed an incorrectly published render epoch or CropTo's missing
zoom reset; the fifth was a fixture assertion that incorrectly expected Quad to
have no texture. That assertion now checks the production default texture's
path, dimensions and frame count. The render/event comparisons remain intact.
`actor-sprites-corrections-song-linux-debug` passes six tests and retains the
CropTo failure. `actor-sprites-crop-song-linux-debug` passes all eight focused
tests, including immediate and queued zoom resets and recovery after malformed
frame sheets/state lists. Each run records unchanged source hashes.
`actor-sprites-final-adapters-song-linux-debug` passes all 120 adapter tests on
the formatted final source: the previous 112 plus the eight Sprite controls.
That full run also records unchanged source hashes.

`actor-sprites-format.json` records ten passing targeted format checks.
`actor-sprites-core-source-audit.json` verifies that all 240 core and root
Cargo/config entries still match the 948-test passing interpreter run.
`actor-sprites-controls-audit.json` verifies the same nine unchanged production
controls used by the Sound comparison, allowing only the previously recorded
test-feature-gated accessors. This work adds no interpreter feature or C API.

`actor-sprites-lint-song-linux-debug` fails at the unchanged denied
`while_immutable_condition` in `cmd.rs:349`, with 164 existing warnings and no
Rust-adapter diagnostic. The library failure prevents the requested Sprite and
Sound integration-test lint from completing. The failed check and unchanged
sources are recorded; no lint was disabled and this is not a passing result.

These are local resource/callback controls, not full movie playback, renderer
integration or compiler/baked-output validation. Remaining actor classes,
manager/input bindings, complete song capture/live scenes, production cutover
and final application/performance/memory checks remain within the existing
migration acceptance scope. The completed interpreter and C API-removal
milestones remain closed.

## Song ActorMultiVertex uploads and mesh reads

The opt-in song host now constructs ActorMultiVertex instances, implements the
existing vertex upload/draw-state/texture operations, and exposes
`actor_multi_vertex_mesh` for renderer-facing mesh reads. Its 39 public method
names match the production class inventory. Methods that already report missing
semantics in production retain that behavior; this adds no StepMania features.
The triangulation and line-strip geometry body matches the frozen production
implementation, ignoring whitespace and the new module visibility.

Uploads copy scalar position, color and UV data into VM-owned userdata. The
private weak-key registry checks buffer provenance without retaining buffers.
The generic `HostCall::try_new_userdata` facility delegates to the existing
fallible VM allocator in the native callback's environment; a memory ceiling
failure propagates as a Lua error. The new bounded-VM regression checks charged
bytes, retained storage across collection, release after unrooting, error kind
and successful allocation after a rejected request. This is a migration-required
host facility, not a reopened interpreter or allocation-failure campaign.

Eight mesh controls compare the actual production constructor and mesh reader,
complete vertex/texture results, local render state and Lua observations. They
cover upload copy/lifetime, shared buffers, rejected publication with collection,
draw ranges, supported geometry modes and line joins, scalar conversion, default
and loaded textures, immediate updates across tweens, collecting/reentrant
draw-state getters, partial writes, invalid input and recovery without stale
output. A standalone weak-registry control isolates host rooting from adapter
registry ownership. The same tests check both preparation and live clock modes
where texture/tween behavior differs.

The initial mesh check failed on invalid Rust floating-point literals in the new
fixture. The first runtime check then exposed a fixture assertion that called
the intentionally absent public `InitState`, and a real retained-buffer defect.
The fixture now checks that method's absence. The original weak-collection
assertions remain intact. The buffer-lifetime/query/provenance checks preserved
under `actor-meshes-*` narrowed the retention defect to the adapter: helper slot
8 already held MESSAGEMAN, overwriting the intended weak registry. Buffers now
use a named private helper entry, and a regression checks that no userdata keys
appear in MESSAGEMAN. `actor-meshes-registry-fix-song-linux-debug` passes all eight
mesh and thirteen message tests. `actor-meshes-fallible-allocation-core-linux-debug`
passes all ten memory-limit tests. Both record unchanged source hashes.
`actor-meshes-final-adapters-song-linux-debug` passes all 128 adapter tests on
unchanged source: the previous 120 plus eight mesh controls, including the
fallible upload allocator.
After the buffer-loop cleanup, `actor-meshes-verified-adapters-song-linux-debug`
again passes all 128 tests on unchanged source. This is the current adapter
regression result; earlier runs remain preserved.
`actor-meshes-final-core-core-linux-debug` passes 949 interpreter tests on
unchanged source. `actor-meshes-core-lint-core-linux-debug` passes library and
memory-limit-test Clippy. The source audit confirms all 240 interpreter and root
Cargo/config entries still match that passing full interpreter run after the
subsequent adapter-only buffer-loop cleanup.

`actor-meshes-controls-audit.json` verifies the same nine production oracle files
against the preserved baseline, allowing only the previously recorded test
accessors. `actor-meshes-geometry-audit.json` records the unchanged geometry body;
`actor-meshes-public-methods-audit.json` records the method inventory comparison.
`actor-meshes-format.json` records nine successful targeted formatting checks;
the initial non-login WSL invocation could not locate rustfmt and was repeated
in the normal login environment.
`actor-meshes-final-format.json` records all nine checks again after the cleanup.

The first song lint check, `actor-meshes-song-lint-song-linux-debug`, preserves
four new constant-chunk iteration warnings and the previously recorded denied
`while_immutable_condition` in `cmd.rs:349`. The buffer loops now use fixed-size
array chunks. The denied source file still has baseline SHA-256
`7405d85dc6d7cc6b276beb0b7e682ab6dfdcee1de062bfa59f2e7d6230557b3e`.
No lint was disabled.
`actor-meshes-verified-lint-song-linux-debug` confirms the four new warnings are
gone, with no Rust-adapter diagnostic and the same 164 existing library warnings.
It still fails at that unchanged denied lint, so the requested mesh integration
test lint could not complete. Both failed runs record unchanged source hashes;
`actor-meshes-final-lint-analysis.json` records the diagnostic/source comparison.

These controls validate the binding and mesh read boundary. Full compiled output,
live playback composition, production cutover and final application validation
remain required. In particular, they do not resolve or bypass the separately
recorded pre-existing downstream mesh/glow alpha assertion.

## Song ActorProxy targets

The opt-in song host now constructs ActorProxy instances with the existing public
`SetTarget` / `GetTarget` methods and Actor inheritance. Bound callbacks retain
their owning table through traced captures, and the owner retains its target as
a Lua field. Public forwarding preserves mutable class/bound methods, receiver
checks and multiple return values. As in production, every actor has a private
bound SetTarget, while the public ActorProxy class exposes it. Fresh template
instances receive independent bound methods.

`actor_proxy_target` reads the existing renderer target classification: actor,
player, notefield, judgment, combo, score, underlay or overlay. It preserves
classification priority, numeric coercion, visibility, partial writes, ordinary
getter errors and callback reentry. Returned actor identities are local to the
runtime and still require the production compiler/live scene's overlay-index
resolution. Type tags are now initialized on Rust actors for the existing raw
target validation; `visible` also publishes the field consumed by proxy reads.
Those supporting changes are checked against the existing actor controls.

`actor-proxies-initial-song-linux-debug` passes the first six proxy controls and
all 26 existing actor tests. `actor-proxies-lifecycle-song-linux-debug` passes all
eight final focused tests. The unchanged native proxy fixture checks inventory,
inheritance, identity, invalid-target recovery and live class replacement. Other
controls compare actual production target reads, local render states and Lua
observations for every classification, collecting/reentrant getters, errors
after partial publication, immediate changes during tweens, queued retargeting,
retained bound callbacks and malformed renderer input. Runtime-specific checks
also cover cloned proxy independence, rejected reads after retirement, and
collection after the last retained setter/getter is dropped. Both runs record
unchanged source hashes.
`actor-proxies-final-adapters-song-linux-debug` passes all 136 adapter tests on
unchanged final source: the previous 128 plus eight proxy controls.

`actor-proxies-controls-audit.json` verifies the same nine production control
files against the frozen baseline, allowing only the previously recorded test
accessors. `actor-proxies-fixture-audit.json` verifies the committed native
fixture is unchanged from the baseline. `actor-proxies-format.json` records five
passing format checks. `actor-proxies-core-source-audit.json` confirms that all
240 interpreter and root Cargo/config entries still match the 949-test passing
interpreter run. No interpreter change was needed for this binding.

`actor-proxies-lint-song-linux-debug` fails at the unchanged denied
`while_immutable_condition` in `cmd.rs:349`, with the same 164 library warnings
and no Rust-adapter diagnostic. Its requested integration-test lint could not
complete because of the library error. The source/diagnostic comparison is in
`actor-proxies-lint-analysis.json`; no lint was disabled.

Full captured output, target remapping and playback composition remain pending
alongside the remaining song bindings, production cutover and final application
validation. This actor binding does not complete the migration.

## Song ActorFrameTexture allocation and custom drawing

The Rust song host now constructs ActorFrameTexture frames, freezes allocation
dimensions/name/buffer flags at Create, validates collisions against existing
captures and textures, and dispatches existing ScriptError messages on creation
failures. Renderer-facing allocation reads and retained texture metadata preserve
the existing distinction between later actor requests and the created resource.
Texture handles include traced BeginRenderingTo/FinishRenderingTo callbacks.
The existing Actor.SetSize spelling is also available through the Actor class.

Actor Draw and ActorFrame SetDrawFunction/GetDrawFunction now execute through
the Rust host. Capture retains immutable call-time poses, wrapper/caller stacks,
cameras and mesh vertices. Target nesting resets and restores the caller stack;
unbalanced or mismatched targets retain their native error behavior. Ordinary
live frames replace earlier ordinary frames while immediate command submissions
remain pending. Preparation retains frames and quiet tails for all callback
owners. The existing 256-operation preparation bound and 256 MiB capture budget
are preserved; live execution uses the byte bound without the operation bound.
Draw callbacks on owned tween-replay actors defer queued commands until Update.

The immutable capture structures, opacity predicate and source converter moved
from `draw_capture.rs` to Lua-independent `draw_commands.rs`. The production
legacy traversal still uses those structures. `actor-draw-source-audit.json`
proves the original source matches the frozen baseline, the remaining traversal
text is unchanged, and the transferred code differs only by visibility and
formatting. Nine legacy control files still match the frozen production text
after removing the previously recorded test helpers and new module declaration.

`actor-draw-legacy-controls-song-linux-debug` passes all 13 existing draw controls
in 238.8554 seconds (including library-test compilation), with unchanged sources.
These execute the frozen traversal comparisons, source conversion/error-order
controls and live extraction checks. Log SHA-256:
`a9432e827c3411494759a80de6d967089f3d320b80296fb9abaacb9871ba3113`.

Six framebuffer controls compare allocation freezing, buffer validation,
dimensions, collision/retry and ScriptError reentry, collecting field getters,
partial publication, malformed resource reads and retained handle lifetime.
Twelve draw controls compare nested targets, wrappers/visibility/proxies,
immediate submissions and live replacement, callback errors/queues, target error
recovery, collection during reentry and release, operation limits, mesh payloads,
quiet tails, missing source errors, transition coercion, binary texture fields
and tween/spin sampling across preparation and live update boundaries.
Each compares the actual production traversal and complete emitted frames.

The first framebuffer run (`actor-framebuffers-initial`) passed four checks and
failed two. `actor-framebuffers-size` fixed the stale native render size after
an authored request vector and passed five. Its remaining failure was in the
legacy control: native texture callback registry handles retain their texture
cycle. The test now explicitly asserts that observed legacy retention and
separately requires collection in Rust. It preserves both assertions after the
Rust Begin/Finish callbacks are installed. `actor-framebuffers-lifetime` passed
all 14 framebuffer/Sprite checks before draw integration, with unchanged sources.

`actor-draw-initial` passed six draw controls and failed two: SetSize was missing
from the Rust class binding, and the clock comparison had advanced only the Rust
runtime. The binding and test setup were corrected. `actor-draw-size-clock`
passed all 14 draw/framebuffer checks; `actor-draw-boundaries` passed all 17 after
adding the owner, source-resolution and coercion controls. All runs record
unchanged source hashes and preserve their failing logs.

The subsequent animation comparison (`actor-draw-animation-initial`) exposed
the native preparation reader's destination-field semantics; live drawing uses
the current tween pose instead. The first fix (`actor-draw-animation-fixed`)
passed eleven draw controls but still failed that comparison: preparation also
uses separately published rotation fields, rather than the updated spin pose.
The Rust reader now uses the existing cached rotation fields in preparation and
the current pose/baked spin flag in live mode. Queue state and timing are unchanged.

`actor-draw-all-adapters-song-linux-debug` passed the then-current 153 Rust adapter tests in
56.4705 seconds with unchanged source hashes. Its log SHA-256 is
`ab230ce8ab5dbe81f2ef4aceb14277d60a4c4a1a99d8f531fc70297d2e98fb1d`.
After the preparation sampling fixes,
`actor-draw-final-adapters-song-linux-debug` passes all 154 adapter tests in
146.4260 seconds, including the new tween/spin comparison, with unchanged sources.
Its log SHA-256 is
`62177a8ca4a4ac0dc140365e869815f12d04d63604b70380d5e0161ab23856de`.
The frozen-control source audit verifies all 178 legacy/shared/performance-control
source files still match the passing 13-control run; only the Rust draw reader
and its integration test changed afterwards.
The eleven Rust/extracted source and test formatting checks pass. Formatting
checks on the two touched legacy files (`lib.rs` and `draw_capture.rs`) fail in
both the current source and frozen baseline; their logs are retained separately.
`actor-draw-core-source-audit.json` confirms all 240 interpreter/root Cargo/config
entries still match the 949-test interpreter run. No core change was needed.

`actor-draw-lint-song-linux-debug` and `actor-draw-final-lint-song-linux-debug`
retain the recorded `cmd.rs:349`
`while_immutable_condition` error and the same 164 library warning categories.
There are no Rust-adapter diagnostics; the two large-enum warnings moved with
the shared draw types. Integration-test lint did not complete because of that
library error. The lint audit records the unchanged `cmd.rs` hash.
The final lint log is byte-identical to the first lint run. The final review
records matching full source manifests between the 154-test and final-lint runs,
and verifies the unchanged interpreter and legacy/shared control sources.

This provides draw submission for the owned actor host. Full compiler scheduling,
manual draw state projection into compiled actors, remaining actor/manager/input
bindings, complete live scenes, production song cutover and final application
and performance validation remain required. The interpreter and C API-removal
milestones remain closed; the migration acceptance scope is unchanged.

## Production noteskin release comparison

`noteskin-production-workloads-windows-release` records the successful release
build and preserved comparison executable. Its extra engine-source manifest
includes the untracked replacement and isolated legacy oracle. The paired run
in `noteskin-production-paired-windows-release` verifies those source hashes and
all 848 noteskin input files remained unchanged.

Every one of the 26 bundled skins matched the legacy output after the explicit
29-to-30 cache-version normalization. Five measured preparations per backend
and skin alternate engine order after warming both paths. The sum of each
skin's median preparation time was 283.9215 ms for legacy and 342.5666 ms for
Rust (1.2066 times legacy). The corresponding sums of median thread cycles
were 620,383,171 and 748,338,937 (1.2063 times legacy). Raw per-case samples and
the aggregation are preserved; this is a measured regression, not a performance
target or a LuaJIT comparison.

The measurement includes source reads, compilation, preparation and VM
destruction, with warmed OS/source/path caches. It excludes discovery, output
comparison and bundle destruction. Other work on the machine may contend for
CPU. This tool does not compare per-backend allocation or peak memory; the
migrated song preparation/live/memory comparisons remain pending.


## BitmapText and LoadFont Rust bindings

The Rust song host can now materialize BitmapText, with the existing 20 public
native methods and five fallback theme helpers. The bindings preserve text
conversion, strict boolean arguments, layout/max-dimension ordering, text-specific
fit geometry, strokes, attribute aliases/deduplication and owned attribute reads.
The existing `color` call is available for the theme helpers and retains their
dynamic global/method lookup behavior.

`LoadFont` creates reusable live text templates. Concatenation clones their native
state and mutable text data, composes authored commands, and defers Init until
materialization. Preparation templates retain the production capture-block rule
for relative positions: an earlier captured x can still take precedence over a
later replay destination until playback begins. The paired test explicitly
checks preparation's 6+3=9 and live replay's 10+3=13 for that same command sequence.
This preserves the existing behavior; it does not change the production oracle.

`actor_bitmap_text` returns owned font/text/stroke/attribute data. Ordinary getters
can collect or reenter the host; failed reads do not return an earlier payload.
Retained payloads survive subsequent table mutations and scene retirement.
Live tests update text and attributes across GC, queued poses and custom draw
callbacks, then compare the resulting owned draw submissions with production.
The fixture's inheritance/assertion section is reused from
`bitmap-methods-input.json`; its bytes match the frozen fixture inventory.

The font fallback and stable name policy was moved verbatim, then formatted,
into the Lua-independent `bitmap_font.rs`. `bitmap-text-font-extraction-audit.json`
proves that transfer and limits the production `lua_util.rs` change to the
extraction/import and `lib.rs` change to its module declaration. No interpreter
source changed: the 240 entries in `bitmap-text-core-source-audit.json` still
match the source verified by the earlier 949-test core run.

Validation on the final sources:

- `bitmap-text-final-source-song-linux-debug`: **162 passed**, zero failed,
  156.456 seconds, sources unchanged. This runs all 16 Rust song adapter
  targets, including eight BitmapText production comparisons.
  Log SHA-256 `c0bdc813054053132adf311f9846b8afb808acd2d5599fd0963df6ddecd06497`.
- `bitmap-text-final-lint-song-linux-debug`: exit 101 in 24.094 seconds,
  sources unchanged and identical to the final test manifest. The existing
  `cmd.rs:349` `while_immutable_condition` denial and 164 legacy warnings remain;
  no Rust adapter diagnostic was emitted. Library failure prevented test linting.
  Log SHA-256 `1cead4a62d934c88a454b090e25df6de7652f7fd1365855694b70630b4eba2d0`.
- All eight changed adapter/shared-policy Rust files pass rustfmt checks.
  `lib.rs` and `lua_util.rs` fail file-based checks both before and after this
  stage; their failures are preserved in `bitmap-text-existing-format-results.json`.
  An earlier stdin-based formatter check returned zero while emitting formatted
  text; it is explicitly invalidated in `bitmap-text-existing-format-stdin-invalid.json`
  and is not used as passing evidence.
- `bitmap-text-final-review.json` ties the source manifests, lint, core and
  extraction audits together. All 1036 watched files matched the final
  adapter source manifest during review.

Earlier runs are preserved. The initial six text comparisons passed. The first
template extension failed compilation because the wrapper constructor still used
the prior helper signature; the corrected call is covered by the final wrapper
suite. The next run passed 32 tests and failed two: it exposed the captured-position
rule above and an incomplete test draw-source map. Both were corrected, with the
callback owner added to both draw controls. The affected suites then passed 73
checks; the first full adapter run passed 162, followed by the final-source run
after moving a misplaced API documentation comment.

This remains an adapter dependency, not the production cutover. Complete text-change
capture/publication into compiled and live overlays, remaining actor/manager/input
bindings, whole-song compilation and live scenes, unconditional production engine
selection/removal of legacy dependencies, normal application/rendered validation,
and final preparation/live/memory comparisons remain required. The interpreter
and C API-removal milestones stay closed. The migration finish line is unchanged.


## Text-display construction and numeric bindings

RollingNumbers, BPMDisplay, DeviceList and InputList can now be materialized by
the Rust song host. They inherit the public Actor class and retain private
receiver-bound text and numeric methods. The bindings preserve dot/colon receiver
rules, target-before-format error publication, the existing rolling formats,
dynamic string.format lookup and direct Text assignment. DeviceList/InputList
resource reads preserve the existing fixed labels and disregard authored text
style payloads. Load retains the metric branch for RollingNumbers and the texture
branch for the other displays. Live callbacks, GC, template copies and scene
retirement are covered by production comparisons.

Validation of this stage:

- Initial affected suites: 50 passed; the added Load coverage then passed all seven
  display tests. Both runs retain unchanged source manifests.
- `text-displays-final-adapters-song-linux-debug`: **169 passed**, zero failed,
  across all 17 Rust song adapter targets, in 68.530 seconds. Sources unchanged.
  Log SHA-256 `e17988607ebfa1f799d1086e8d238f906cc3ebd9e1547903333ddb61a52ae5b0`.
- `text-displays-final-lint-song-linux-debug`: exit 101 in 23.391 seconds; the
  existing cmd.rs:349 while_immutable_condition error and 164 library warnings
  prevent integration-test lint. No Rust adapter diagnostics were emitted.
  Its log is byte-identical to the preceding BitmapText lint run.
- All six changed Rust files pass file-based rustfmt checks. The source audit
  verifies 1,030 untouched source entries and all 240 core/root Cargo entries
  against the earlier evidence, retaining the 949-test core result.
- `text-displays-final-review.json` verifies all 1,038 watched source hashes,
  matching final test/lint manifests and the formatting evidence.

The broader private methods on unlinked classes are still pending. This stage
adds construction and text/numeric bindings, not complete production scenes.
Compiler capture, live integration, remaining bindings, unconditional cutover and
final application/performance validation remain required by the fixed objective.


## Banner and shared Sprite media bindings

The Rust song host now materializes Banner with its existing public Sprite
inheritance and Banner methods: both scaling names, song/course/cached loading,
character/unlock/group/sort path queries, and scrolling state. Empty Banner
construction preserves the production renderer's textured default. Ordinary
path assignments and cached loads preserve custom animation state; LoadFromSong
uses the existing full-load reset and texture-event publication rules. Its missing
song fallback uses the actual bundled 4-by-2 transparent bitmap.

Sprite's position/loop/rate and media helpers are also available. The
current-background helper remains a separate Lua closure on each actor, retaining
its theme source name and dynamic GAMESTATE/trail/receiver calls. The cropto
helper preserves its source name and dynamic CropTo call. Public forwarders still
consult the private methods table, including authored replacements and the
existing missing-implementation errors. No new LoadFromSongBanner behavior was
invented.

Six new production comparisons cover construction/inheritance, coercion and
fallbacks, path query ordering, scrolling/error publication, reentrant getters
and GC, actual helper caller provenance, retained texture metadata and live
texture events/animation. The initial run failed all six at construction because
Banner's empty-texture default had been treated like Sprite's. The next run
passed five and failed a test assumption: the production debug global is a
compatibility table, so it could not verify actual caller frames. The test now
uses mlua's stack inspector for that control and retains explicit source-name
assertions. Failing logs and both source manifests are preserved.

Validation on the final stage sources:

- `banner-provenance-song-linux-debug`: 26 passed across Banner, Sprite and draw,
  zero failed, in 112.701 seconds; sources unchanged.
- `banner-final-adapters-song-linux-debug`: **175 passed**, zero failed, across
  all 18 Rust song adapter targets in 74.373 seconds; sources unchanged.
  Log SHA-256 `8a3fd7fdadf22b6584c6504c1ee40a572c6dc8ec753d697c9df2b0c889f2caad`.
- `banner-final-lint-song-linux-debug`: exit 101 in 22.168 seconds. The unchanged
  cmd.rs:349 while_immutable_condition denial and the same 164 library warnings
  prevent integration-test lint. No Rust-adapter or shared blank-texture
  diagnostic was emitted. Log SHA-256
  `1279c3d2a9693c9fd531e568e294d0d0f922ad35a4e09d3c8355f2895c3ab900`.
- All seven changed/new adapter and shared Rust files pass file-based rustfmt
  checks. The two touched legacy files fail both before and after this stage;
  those separate logs are retained.
- `banner-extraction-audit.json` proves the blank bitmap preparation is an exact
  formatted transfer, with only a delegation wrapper in lua_util.rs and a module
  declaration in lib.rs. The image bytes are unchanged.
- `banner-source-audit.json` verifies 1,030 untouched source entries and all 240
  interpreter/root Cargo entries against earlier evidence. The existing 949-test
  core result remains applicable. No interpreter changes were needed.
- `banner-final-review.json` verifies all 1,041 watched source files against the
  full test manifest, the matching lint manifest and the formatting evidence.

The fixed production migration remains incomplete. Remaining classes and private
methods, managers/input, whole-song compiler capture and live scenes, production
cutover/legacy removal, final application/rendered checks, and final fixture and
performance/memory comparisons are still required. This stage does not reopen
the closed interpreter or C API-removal milestones.


## GraphDisplay construction, life records and native children

The Rust song host now materializes GraphDisplay with ActorFrame inheritance,
two unnamed body children and a named Line child. These native children remain
separate from the authored array. Their parent links, named aliases, duplicate
groups, lookup snapshots and registration order match the production control.
Graph dimensions preserve the zero/one/two-player policy. Load and Set retain
the metric and stats fields, receiver binding, life-record callback arguments,
fallback samples and error publication order. The renderer-facing
`actor_graph_display` API returns owned sample and child-style data that survives
later mutations, collection and scene retirement.

Native children participate in Begin, recursive commands, End, depth propagation
and FinishTweening, including queue-overflow finishing. They do not participate
in ordinary On/startup, frame advancement, HurryTweening or GetTweenTimeLeft's
authored-child traversal. The comparison checks exercise these distinctions.
Message discovery checks registration order directly through observable getter
calls; delivery comparisons import native allocator identities using the
existing test-only mechanism. Production delivery continues to use VM identities.
Duplicate Line names produce a fresh group with default raw render fields, as
in the legacy renderer reader.

Seven new comparisons cover construction/inheritance and player-dependent size,
Load/Set and retained private receivers, sample conversion, suppressed query and
callback errors, partial mutations, named-child groups and read-error recovery,
startup/update/commands, message delivery, GC and retirement. The initial build
failed compilation; the next run passed 30 tests and failed three. It exposed
missing native-child finishing and two incorrect test assumptions: resolving Set
after changing the actor type, and comparing allocator-dependent delivery order
without identity mapping. The retained-Set test and identity mapping now exercise
the intended behaviors. The failing logs remain preserved; no coverage was removed.

Validation on the final stage sources:

- `graphs-lifecycle-fixed-song-linux-debug`: **53 passed**, zero failed, across
  GraphDisplay, actors, messages and text displays in 108.502 seconds. Sources
  unchanged. The finish correction also covers the queue-overflow path.
- `graphs-final-adapters-song-linux-debug`: **182 passed**, zero failed, across
  all 19 Rust song adapter targets in 91.687 seconds. Sources unchanged.
  Log SHA-256 `08f1557e2811a0cb2099f3297ec5090816d204fe9d45b02f59bc24dc23cdc773`.
- `graphs-final-lint-song-linux-debug`: library Clippy exited 101 in
  379.447 seconds, including a native shader compiler dependency rebuild.
  The unchanged cmd.rs:349 while_immutable_condition denial and the same 164
  library warnings remain. No Rust adapter diagnostic was emitted. Integration
  tests were not linted; the library denial prevents that check.
  Log SHA-256 `a28e5a952ec63bf563247a6f8e4f9d1163a6ed519c04255632c0c97ed2288f85`.
- `graphs-final-review.json` verifies all 1,044 watched source hashes, matching
  final test/lint manifests, unchanged diagnostic categories and formatting evidence.
- All nine changed/new adapter Rust files pass file-based rustfmt checks.
  The touched legacy compile.rs file fails formatting both before and after;
  separate logs are retained in `graphs-existing-format-results.json`.
- `graphs-source-audit.json` verifies 1,031 untouched source entries and all 240
  interpreter/root Cargo entries against earlier evidence, preserving the
  applicability of the 949-test core result. No interpreter change was needed.
  Its exact comparison also proves the only compile.rs edit is in the test-only
  control: pass the actual context player count instead of a hardcoded one.

This stage supplies GraphDisplay bindings and owned resource reads. Whole-song
compiler publication and live integration, remaining classes/private methods and
manager/input bindings, unconditional production song cutover, legacy removal,
final fixtures/application/rendering and performance/memory validation remain
required. The production migration is incomplete; its finish line is unchanged.


## SongMeterDisplay streams and cached traversal ownership

The Rust song host now materializes SongMeterDisplay and its Stream declaration,
with ActorFrame inheritance, retained width methods and owned stream render data.
StreamWidth conversion, renderer thresholds, absent streams, bound receivers,
public class overrides and error recovery match the production control. The
stream is named before the parent's Init, initializes after that Init command,
and receives its existing startup and frame updates. The authored array and
named stream retain their separate traversal rules for commands, HurryTweening,
GetTweenTimeLeft and FinishTweening.

GraphDisplay and SongMeterDisplay share `native_children.rs` and the renamed
`native_children.lua`. The Lua registry, lookup, grouping and direct-child helpers
were transferred unchanged; the Rust graph constructor now uses the shared
registry entry point. Group render reads still use the production defaults.

Meter stream replacement preserves the cached update plan until invalidation.
Plan rebuilding adopts ordinary children in traversal order, including the case
where a stream target also appears later under another parent. A duplicate Stream
name produces a proxy group. That group inherits its last child's initialization
guard, so the original stream can remain unregistered: its queue and getters stay
observable, while updates call its callback without advancing the queue. Those
behaviors are compared explicitly, including both frames after invalidation.

Cached plans now own a traced table for temporary groups. An active frame roots
that table separately while callbacks invalidate the cache and collect. Cache
invalidation and scene retirement release the retained roots. Tests force GC
during those transitions and verify that the retired scene is collectible.
All of this ownership remains in the song adapter; no interpreter change was
needed.

Nine meter comparisons cover construction and aliases, width conversion and
thresholds, lifecycle/parent ordering, stream replacement, callback reentry,
failed reads, partial-scene cleanup, registration timing, duplicate groups and
retirement. One comparison executes the exact meter declaration extracted from
the bundled SongInfoBar.lua at both 310 and 417 layout widths, with a deterministic
color callback. It preserves the original Quad stream's initialization and texture.
Full SongInfoBar loading, managers, title updates and screen rendering remain part
of the pending production integration.

Validation of this stage:

- `song-meters-registration-song-linux-debug`: **50 passed**, zero failed,
  across meter, GraphDisplay, actor and wrapper targets in 90.759 seconds;
  sources unchanged.
- `song-meters-final-adapters-song-linux-debug`: **191 passed**, zero failed,
  across all 20 Rust song adapter targets in 186.952 seconds; sources unchanged.
  Log SHA-256 `bdc187a28bc85a72278c4ed74fd7babb4969cac73555f77407a2605742abc878`.
- `song-meters-final-lint-song-linux-debug`: exit 101 in 23.803 seconds,
  with unchanged sources. The existing cmd.rs:349 while_immutable_condition denial
  and the same 164 library warnings remain; no Rust adapter diagnostic was emitted.
  The library denial prevents the requested integration-test lint checks.
  Log SHA-256 `1279c3d2a9693c9fd531e568e294d0d0f922ad35a4e09d3c8355f2895c3ab900`.
- `song-meters-final-review.json` verifies all 1,047 watched source hashes,
  matching final test/lint manifests, diagnostic categories and formatting evidence.
- All nine changed/new Rust files pass file-based rustfmt checks.
- `song-meters-source-audit.json` verifies 1,035 untouched source entries and
  all 240 interpreter/root Cargo entries, retaining the applicability of the
  earlier 949-test core result. Legacy production sources are unchanged.
  It also verifies the Lua helper transfer and the unchanged bundled asset hash
  `a6974b5520feb5a0655ac152fec9224598ac695d87378cdf51c62359051b11ee`.

Earlier failures remain recorded. The first run passed 34 tests and failed five
at the stream's pre-Init name. The next passed 39 and failed two, exposing proxy
group construction and later parent adoption. The group run passed 40 and failed
one at unregistered-stream queue advancement; a focused probe confirmed that its
immediate and queued getters match while its queue stays unadvanced. These were
adapter fixes; the production control and failing coverage were preserved.

The production migration remains active. Remaining classes/private methods and
manager/input bindings, complete compiler capture and live scenes, unconditional
song cutover and legacy removal, final fixture/application/rendering checks and
performance/memory measurements are still required. The interpreter and C API
removal milestones remain closed.


## CourseContentsList bindings

The Rust song adapter now constructs CourseContentsList and implements its existing
scroller methods, transform positioning and SetFromGameState callbacks. The
production constructor attaches Display itself. This preserves both plain Def
tables without instance methods and existing actor tables such as LoadFont
objects, including shared identity across multiple lists. Init selects and
compacts raw children at traversal time, preserves native naming and stable draw
order, and runs only once per shared display. On follows the same shared guard.
Ordinary child materialization observes constructor changes to a shared source's
Name. No DeadSync-specific behavior was added to deadlib-lua.

Scene ownership now retains shared displays, descendants and wrappers until the
last owner retires. Wrappers added after the display was shared join those scene
owners. Registry roots and cached update plans are released at final retirement;
malformed declarations and callback failures leave the runtime usable. Raw
metadata preserves deferred errors: a non-string descendant Name can load, but a
name query rejects it, matching the production control.

Scroller methods retain tolerant setter coercion, typed reads, native f32
arithmetic, clamping, boolean argument presence, transform precedence and bound
receiver lookup. SetFromGameState publishes its state before callbacks, builds
fresh parameters for each player, shares each player's parameters with leaf
callbacks, and preserves the existing two passes for a leaf Display. GAMESTATE
lookups and step methods remain dynamic and can reenter or collect. These tests
supply manager methods; complete production manager integration is still pending.

The 11 course tests compare construction, method visibility, raw/existing/shared
displays, duplicate Display groups, Init/On/frame traversal, scroller values,
collecting transforms, per-player callbacks and failure timing against the legacy
host. A Rust ownership test additionally verifies cross-scene retirement, later
wrappers and weak collection after the last release. No bundled course-list asset
was substituted for a production application smoke test.

Final validation:

- `courses-final-verified-song-linux-debug`: **202 passed, zero failed**
  across all 21 Rust song adapter targets in 187.580 seconds;
  sources unchanged. Log SHA-256 `a42f3dc7a47cee7bd79a5d26f600b73d45b251e2f863e8c8844a9dfdb75c7e18`.
- `courses-final-lint-verified-song-linux-debug`: exit 101 in
  166.847 seconds, including waiting for the test build lock;
  sources unchanged. The existing cmd.rs:349 while_immutable_condition denial
  and the same 164 library warnings remain. No Rust adapter diagnostic was emitted;
  the library denial prevents the requested integration-test lint checks.
  Log SHA-256 `5564601758140ed8fc415b15d8dd0c52041be4a410bc3f64256a5b4a870c642c`.
- `courses-final-review.json` verifies all 1,049 watched source hashes and the
  complete file set, matching final test/lint manifests, warning/error headings
  and five passing file-based rustfmt checks.
- `courses-source-audit.json` verifies 1,042 unchanged source entries and all
  240 interpreter/root Cargo entries against the previous meter stage. The
  earlier 949-test core evidence remains applicable. The only existing code or
  manifest changes are the actor host, Lua helpers, lifecycle, wrappers and song
  manifest; courses.rs and rust_courses.rs are new. Legacy production is unchanged.
- `courses-edit-baseline.json` retains the five existing files before these edits.

Earlier reports remain intact. The initial run passed 31 tests and failed one
because its assertion compared backend-specific exception text and stack traces;
it now checks rejection and the function-conversion error meaning. The ownership
run passed 34 and failed one because the test assumed BitmapText exposed
SetUpdateFunction; the native control established direct update-field assignment
instead. The native-contract run then passed 33 tests across four targets.
The first full run passed 58 and failed one when a malformed-descendant test
assumed an eager name error. The binding and coverage now preserve the actual
load-success/query-failure timing. The corrected full run passed 202 tests. Its
first lint found one new collapsible-if warning, which was fixed before the final
full test and lint runs above. No failing case was removed or ignored.

This is an adapter milestone within the existing production migration. The
production song compiler still uses the legacy engine. Remaining constructor and
private-method bindings, managers/input, full compiler capture and live scenes,
unconditional song cutover, legacy dependency removal, final application/rendered
checks and performance/memory measurements remain required. The interpreter and
C API removal milestones stay closed.

## Model resources and material clocks

The Rust song adapter now constructs Model with its existing Actor inheritance,
depth/culling/wrapping defaults and private sprite and child-query methods. It
retains duplicate child groups, registry aliases, receiver-bound private methods
and public class overrides. Model still has no public Model class table or
ActorFrame update/draw setter inheritance, matching the production host.

`actor_model_paths` reads Meshes, Materials and Bones in order, resolves them in
the actor's script directory and uses the first supplied path for missing fields.
It preserves getter reentry/GC, unresolved-path errors and cleanup after a failed
read. `actor_model_layers` feeds the existing asset loader and returns owned
geometry/material layers. No domain behavior was added to deadlib-lua.

Preparation captures reuse the existing Rust material clocks. The extracted
`Clock::with_initial_state` performs the same initial state selection and first
sample as the previous production install loop. Updates retain the original
incoming delta, including a model's own hibernation; ancestor hibernation and
parent update rates still affect traversal. Queued state changes precede material
updates, and update callbacks follow them. Public state selection updates both
diffuse and additive clocks. Static layer positions remain present in the output.
Clocks own their frame data until captures are drained or their actor is retired;
returned sample arrays survive release.

The unchanged model-material and model-texture-order fixture scripts exposed a
missing declared-camera field: the Rust adapter ignored FOV=0. Render construction
now reads the declaration, command setters override it, and -1 disables the camera.
The added render comparison covers zero, a numeric string, disabled perspective and
Init-command precedence. The Model suite compares actual owned geometry/materials
and material samples, plus construction, child queries, path/error behavior,
state selection, callback ordering, capture reset and retirement across GC.

Final validation:

- `models-final-adapters-song-linux-debug`: **210 passed, zero failed**, across
  all 22 Rust song adapter targets in 357.778 seconds, including build-lock wait;
  sources unchanged. Log SHA-256
  `58d8fc6c0a48724663f5425798873e05f0bc8efc7171e2b88e14bb82082025b4`.
- `models-final-clock-song-linux-debug`: **two passed, zero failed** in
  250.505 seconds, including the library-test build. The existing material-prefix
  and additive-state regressions compare against the earlier clock algorithm;
  sources unchanged. Log SHA-256
  `270350b8e5137ef17481afaf2cd260340f438d17572680cab323b02eafeb48ec`.
- `models-final-lint-song-linux-debug`: exit 101 in 356.563 seconds, including
  build-lock wait. The existing cmd.rs:349 while_immutable_condition error and
  the same 164 library warnings remain. No Rust adapter diagnostic was emitted;
  the library failure prevents the requested integration-test lint checks.
  Log SHA-256 `5564601758140ed8fc415b15d8dd0c52041be4a410bc3f64256a5b4a870c642c`.
- `models-final-review.json` verifies all 1,051 watched source hashes and the
  complete file set, matching final test/clock/lint manifests, and unchanged
  diagnostic headings. All 11 changed/new Rust files pass file-based rustfmt.
- `models-source-audit.json` verifies 1,038 unchanged source entries and all
  240 interpreter/root Cargo entries against the prior course stage. The earlier
  949-test core result remains applicable. Eleven existing source/manifest files
  changed; models.rs and rust_models.rs are new. The legacy material-clock file
  contains the shared initialization extraction, test-support accessors and
  formatting cleanup; its pre-existing formatting failure is recorded in
  `models-clock-before-format.json`.
- `models-fixture-audit.json` confirms 16 checked Model fixture/resource hashes
  still match the original inventory. `models-edit-baseline.json` and
  `models-doc-edit-baseline.json` retain the pre-edit source and documentation.

Earlier failures are retained. The first compile exposed a private constant name
and helper visibility errors. The next run passed 25 tests and failed the existing
construction-error cleanup case because it used Model as an unsupported class.
That case now uses an explicitly unimplemented class and retains all error,
weak-reference cleanup and recovery assertions. It was not removed or ignored.
The first fixture run passed 40 tests and failed four: the test control lacks
LoadActor, Model has private child queries, and the two Def implementations need
the same script-directory context. The harness now executes the original fixture
source with matching directories and sets path-test fields after construction.
The next run passed 42 tests and failed only the two declared-FOV comparisons.
After the camera fix and child-query coverage, the focused run passed all 53
tests. Fixture files were unchanged, and existing cleanup/recovery coverage was
retained.

This remains an intermediate adapter milestone. The production song compiler
still uses legacy Lua. Remaining private methods and manager/input bindings,
complete compiler capture and live scenes, unconditional song cutover and legacy
dependency removal, final application/rendered comparisons and performance/memory
measurements are still required by the fixed objective. The interpreter and C API
removal milestones remain closed.

## Preference and display managers

RustSongRuntime now installs the existing PREFSMAN and DISPLAY interfaces and
their associated logical-screen globals. Preference defaults preserve the
production context values, ASCII-insensitive default lookup, prefix defaults and
unknown-key behavior. Stored overrides keep their original spelling and can hold
arbitrary Lua values. GetPreference retains the original value table even after
its public field is replaced; GetScreenAspectRatio instead resolves the current
global manager and reads its public value table. Preference getters/setters
preserve ordinary table metamethods, reentry, collection and failure order.
SetPreference, reset and save count side effects before validating the key. The
actor message bindings and manager bindings share the existing counter operation.

DISPLAY retains the physical mode dimensions and render-to-texture capability
from the compile context. Repeated spec, supported-mode and current-mode queries
preserve table identity and authored mutations. Logical SCREEN_* constants,
_screen, aspect scaling and WideScale keep their existing f32 arithmetic and
rounding, independently of preference changes. The aspect helper preserves
typed conversion, raw field reads and context fallback. All retained tables are
traced callback captures. Saved preference getters retain their value store;
saved display queries retain their results. Discarded manager cycles and drained
references can collect in the Rust VM.

Nine tests compare defaults and case variants, mutation and argument handling,
side-effect counters, collecting/reentrant store metamethods, malformed reads,
physical/logical dimension separation, spec identity and dynamic global lookup
against the unchanged legacy implementations. A Rust ownership test checks
weak collection, saved queries and isolation between two runtimes. The existing
`tests/fixtures/song_lua/display-size.lua` runs unchanged in preparation and live
contexts, comparing its full local actor render states and all three framebuffer
allocation payloads. This is a host/actor comparison, not an application rendering
smoke test.

A test-only setup hook lets the existing legacy actor control install the real
production preference/display managers before evaluating that fixture. Its normal
test callers still use an empty setup function. The source audit verifies that
this hook is the only change in compile.rs; production legacy manager functions
and defaults remain unchanged. No DeadSync-specific behavior was added to the
interpreter, and no existing test or fixture was removed or weakened.

Validation:

- `preferences-display-initial-song-linux-debug`: **35 passed, zero failed**
  across preference/display, message, framebuffer and Model targets in 114.910
  seconds; sources unchanged. Log SHA-256
  `f304bee03d88d07e2c5706a64039bb1899e24d0f3eccfc5bf36637932b0e880f`.
- `preferences-display-final-adapters-song-linux-debug`: **219 passed, zero
  failed**, across all 23 Rust song adapter targets in 129.987 seconds; sources
  unchanged. Log SHA-256
  `e51dff94803d197ec01d71062cc423f58a5f0eab751e0221e740739adb964817`.
- `preferences-display-final-lint-song-linux-debug`: exit 101 in 24.906 seconds;
  sources unchanged. The existing cmd.rs:349 while_immutable_condition denial and
  the same 164 library warnings remain. Warning/error headings match the prior
  Model stage, and no Rust adapter diagnostic was emitted. The library denial
  prevents the requested integration-test lint checks. Log SHA-256
  `59dbe36d78e1de1a2711d691f33bf3811c1b3ee7d643a54ec75dc61d78d15727`.
- `preferences-display-final-review.json` verifies all 1,055 watched source
  hashes and the complete file set, matching test/lint manifests. Seven changed
  adapter/test Rust files pass file-based rustfmt checks. compile.rs still fails
  whole-file formatting; its pre-edit failure is retained in
  `preferences-display-compile-before-format.json`, and both results are recorded
  in `preferences-display-format.json` and the final review.
- `preferences-display-source-audit.json` verifies five changed existing files,
  four new files, 1,046 unchanged source entries and all 240 unchanged
  interpreter/root Cargo entries. The earlier 949-test interpreter evidence
  remains applicable. `preferences-display-fixture-audit.json` verifies the
  display-size fixture against the original inventory hash. Source and document
  pre-edit snapshots are retained in the two preferences-display baseline files.

The production migration remains active. These managers join the opt-in Rust song
host; the normal production song compiler still uses legacy Lua. Other host
globals/managers, gameplay-created actors and remaining private methods, input,
complete compiler capture and live scenes still need migration before song
cutover. Legacy normal/build dependencies must then be removed, followed by the
required final fixture, application, rendering, performance and memory checks.
The interpreter and C API removal milestones remain closed.


## THEME metrics, strings, paths and virtual actors

The completed interpreter and C API removal milestones remain closed. The prior
turn clarified that boundary without advancing the migration implementation.
This turn resumed the explicitly authorized production integration and completed
the pending THEME adapter against the existing production host.

RustSongRuntime now installs THEME's identity, language, string, path and metric
methods. Pure string, path, enumeration and numeric policies are reused from the
existing song crate. GetMetric preserves dynamic SL lead-in and scoring lookups,
f64 music-rate arithmetic, arbitrary mapped Lua values and the existing boolean
conversion. GetMetricF/GetMetricI continue to query numeric definitions only;
HasMetric recognizes gameplay lead-in definitions without evaluating SL. Strict
argument handling, missing results, fresh arrays, multiple path-info returns and
side-effect ordering remain intact. Saved queries capture the original globals
and conversion primitive; they follow replaced SL state without retaining a
discarded THEME table. Intermediate values remain rooted across ordinary,
collecting and reentrant table getters.

The actor loader now accepts the existing virtual theme prefix, including paths
returned through the Rust host entry. It preserves authored spelling and selects
Actor, Sprite or Sound using the production media extension lists, without
filesystem resolution. The private classifier is captured by the Lua declaration
helper and removed from the global table after installation. Virtual theme
sprites preserve the production factory's distinct initial state: animation is
disabled, movie metadata and the decoder field remain unset, and GetDecodeMovie
can infer the movie type independently of the initial render flag. Ordinary
Def.Sprite construction retains its existing initialization.

Twelve THEME tests compare against the actual production methods. Coverage
includes screen/player context metrics, enumerated metric definitions,
localized strings, strict and malformed arguments, dynamic SL values and lookup
order, reentry/GC/error recovery, saved query ownership, and the unchanged Simply
Love mode-table fixture for all three modes and both rescore settings. Path tests
include the bundled Bats bitmap alias and 32 media/non-media suffixes with and
without leading slashes. Actor tests compare owned font payloads and full local
render states in preparation and live contexts. They also distinguish a virtual
movie sprite's getter from its raw decoder field and ordinary sprite construction.

A test-only file-loader hook supplies the real production actor factory to the
legacy scene control. An initial test callback that constructed through Def was
not a faithful control for virtual resources; it was replaced with that hook.
A speculative redecoration of an already-materialized Sprite also failed inside
the unchanged legacy method installer. The resource comparison instead uses a
fresh ordinary Sprite declaration with the same texture to compare the supported
construction paths. The failed runs remain recorded in theme-resources,
theme-resources-fixed and theme-production-control. The authoritative comparison
then exposed the initial animation difference, retained in theme-initialization
and corrected in theme-animation. No pre-existing test or frozen fixture changed.

Validation:

- `theme-metrics-song-linux-debug`: **47 passed, zero failed** in 14.152 seconds,
  validating the manager before virtual-resource integration. The earlier
  theme-initial compile failure was a malformed Rust format string in a new test.
- `theme-animation-song-linux-debug`: **32 passed, zero failed** in 111.079
  seconds across THEME, sprite and definition tests; sources unchanged. Log
  SHA-256 `e449ddd1c9689aa6b894e84400b7c04cd8003eb02d2102b6261dd53759761d46`.
- `theme-final-adapters-song-linux-debug`: **231 passed, zero failed** across
  all 24 Rust song adapter targets in 101.537 seconds; sources unchanged. Log
  SHA-256 `432805470436b57b6fc9c40decef9fa491d0f500f2d209143abcdcd869d35361`.
- `theme-final-lint-song-linux-debug`: exit 101 in 24.706 seconds. The existing
  cmd.rs:349 while_immutable_condition denial and the same 164 library warnings
  remain, with matching warning/error headings and no Rust adapter diagnostic.
  The library denial prevents the requested integration-test lint checks. Sources
  unchanged; log SHA-256
  `aa5a0eadf1550917b797a0239c7ea79dc36598e0e7b14712945232507f717657`.
- `theme-final-review.json` checks all 1,057 watched source hashes and the full
  file set against both final manifests. Six changed Rust adapter/test files
  pass rustfmt. compile.rs still has its pre-existing whole-file format failure,
  captured before editing in `theme-compile-before-format.json`; its only change
  is the test-only file-loader hook. Results are retained in `theme-format.json`.
- `theme-source-audit.json` records seven changed existing files, two new files,
  1,048 unchanged entries and all 240 interpreter/root Cargo entries unchanged.
  The earlier 949-test interpreter evidence remains applicable.
  `theme-fixture-audit.json` verifies the original mode-table and ActorDef source
  hashes against the frozen inventory. Pre-edit source and documentation
  snapshots are retained in the theme baseline JSON files.

The migration remains active. THEME joins the opt-in Rust song host; the normal
production song compiler still uses legacy Lua. SL initialization and other host
globals/managers, gameplay-created actors, input, complete compiler capture and
live scenes remain required before song cutover. Legacy normal/build dependency
removal and the final application, rendered smoke, fixture, performance and
memory checks are still outstanding. These actor-state comparisons do not claim
an application rendering smoke test.


## Simply Love state and theme preference storage

The previous goal turn advanced THEME integration. This turn added its existing
SL state and theme-preference dependencies to the Rust song host. The production
migration remains the active goal; completed interpreter and C API removal
milestones were not reopened.

RustSongRuntime initializes ThemePrefs, SL_CustomPrefs and SL in production order.
SL preserves the global/player tables, profile modifiers, stage statistics,
column judgments, streams, colors, mode preferences and scoring/life metrics.
Tables that were distinct remain distinct, including player timing windows,
stage records, judgment colors and caches. Music-rate validation and profile
speed/filter values reuse existing song policies. The initialization callback
owns only the rate and player contexts, with no retained Lua values hidden in
Rust binding data. Mode preferences invoke the current ThemePrefs.Get separately
for ITG and FA+, preserving construction order and truthiness. Construction roots
its intermediate tables across callbacks and publishes SL only after success.

ThemePrefs retains its private value store through traced method captures.
Stored keys remain case-sensitive while defaults use ASCII case folding; values
may be arbitrary Lua objects. Set, Save and InitAll preserve side-effect ordering
and zero-result returns. InitAll keeps existing values, checks definition key and
table types, and evaluates only missing defaults. Ordinary default-getter errors
remain ignored; fatal VM limits propagate. SL_CustomPrefs returns fresh default,
choice and value tables using the existing option specifications. Its Init and
Validate methods preserve their current side-effect-only behavior. Option-row UI
bindings remain part of the unfinished manager integration.

ParseChartInfo preserves stream identity, default repair, player argument rules,
callback order and partial mutations on failure. It passes the player number to
all three statistics methods, retains the original target streams when callbacks
replace SL, and counts the side effect only after successful parsing. Its method
lookups remain ordinary table operations while result writes remain raw.
ParseColumnCues runs the exact existing production Lua helper; the source-body
comparison and hashes are recorded in `sl-column-cues-source.json`.

Ten tests compare complete initialized SL trees across seven profile/rate
contexts, separate table identities, preference defaults and mutations, fresh
custom definitions, InitAll getter behavior, malformed inputs and error ordering.
The unchanged Simply Love mode-table fixture checks all modes with both rescore
settings, and the resulting values feed the migrated THEME queries. Collecting
and reentrant getters verify construction and parsing roots, including replaced
globals, partial failures and recovery. Saved-query tests verify collection of
discarded managers/state and isolation between runtimes. A test-only rebuild
helper exercises the actual Rust initializer after changing preference callbacks;
the legacy initializer, option definitions, compatibility helpers and defaults
remain unchanged.

Validation:

- `sl-initial-song-linux-debug` recorded an initial compile failure: the new
  private field-conversion helper omitted u8 profile percentages. The conversion
  was added before executing the comparisons; no coverage was removed.
- `sl-context-song-linux-debug`: **51 passed, zero failed** across SL, THEME,
  preference/display, graph and message targets in 94.026 seconds; sources
  unchanged. Log SHA-256
  `ce137cf811aee96e0f0dbb39272ba9ee64a887ff6688210636502c05758794d4`.
- `sl-final-adapters-song-linux-debug`: **241 passed, zero failed** across all
  25 Rust song adapter targets in 113.881 seconds; sources unchanged. Log SHA-256
  `533778ca5ed8802a20a6a562ec59570edb807cd7f5bb2cee31c407aeaec0b5da`.
- `sl-final-lint-song-linux-debug`: exit 101 in 22.642 seconds; sources unchanged.
  The pre-existing cmd.rs:349 while_immutable_condition denial and 164 library
  warnings remain. The complete log matches the previous THEME lint run, with no
  Rust adapter diagnostic. The library denial prevents integration-test lint.
  Log SHA-256 `aa5a0eadf1550917b797a0239c7ea79dc36598e0e7b14712945232507f717657`.
- `sl-final-review.json` verifies all 1,062 watched source hashes and the complete
  file set against the final test/lint manifests. All six changed Rust files pass
  file-based rustfmt checks, recorded in `sl-format.json`.
- `sl-source-audit.json` verifies three changed existing files, five new files,
  1,054 unchanged entries and all 240 interpreter/root Cargo entries unchanged.
  The earlier 949-test interpreter evidence remains applicable. The unchanged
  legacy controls remain independent of the Rust implementation.
  `sl-fixture-audit.json` verifies the mode fixture against the frozen inventory;
  source and documentation pre-edit snapshots remain in the SL baseline files.

The normal production song compiler still uses legacy Lua. Remaining gameplay
managers and generated actors, option-row UI/input, full compiler capture and
live scenes still require migration before song cutover. Legacy normal/build
removal, final fixture/downstream runs, the normal application build, rendered
smoke and representative performance/memory comparisons remain outstanding.
This stage does not claim full application validation or goal completion.


## Song catalog, chart and timing bindings

The preceding turn clarified scope without changing code (no progress). This
turn reread the saved objective, which explicitly authorizes the separate
production migration and keeps the interpreter/C API milestones closed. The
migration acceptance scope is unchanged.

RustSongRuntime now retains a song catalog and installs SONGMAN and SongUtil.
Song and Steps getters preserve authored metadata, resolved paths, fixed note
bounds, display BPM identities and timing owners. Chart selectors share immutable
metadata and retain their chart tables through traced captures. Editing the
public steps array or replacing chart getters does not change native selection.
Selected charts preserve their catalog identities; absent selections receive the
same per-player fallback Steps objects as the legacy host. Courses and trails
preserve shared arrays, entry identities, fallback entry lookup and fresh radar
queries. SONGMAN retains its existing single-song/group/course policy, lookup
aliases, shared lists, fixed counts and preference side effects.

TimingData methods use the existing Rust timing traversal, including note-row
rounding for BPM queries, float elapsed/position conversion, pauses, warps and
split chart timing. Their current music-rate table is retained independently of
public getters. Density/radar initialization preserves chart row mapping, measure
counting, judgability and the final-row rule; density is cached before current
options are installed. Column cues instead call the current Steps timing methods
on each query, preserve float traversal and strict function/result conversions,
and return fresh mutable cue/column tables. Suppressed rows allocate no column
table. Immutable selector metadata is shared across methods. Intermediate timing
objects, callbacks and result tables remain rooted across reentry and collection.
SongUtil preserves ordinary getter lookup, argument forwarding and first-result
behavior, including errors and non-function methods.

The catalog root is staging state for the remaining GAMESTATE/option-level
integration. It is not a replacement GAMESTATE facade. Test-only helpers expose
that root, release it for collection checks, and expose the actual legacy host's
objects/current option table. The legacy host, song tables, timing table methods
and core library sources remain unchanged.

Twelve new differential/ownership tests cover complete object metadata,
selectors and invalid numeric enums, mutable arrays, course/trail identity,
SONGMAN aliases and side effects, SongUtil reentry, timing/rate queries, cached
density/radar, fresh cues and collecting/recursive timing callbacks. They also
check malformed chart initialization, retained callback cleanup and runtime
isolation. The frozen song-steps and split-timing fixtures are parsed through the
existing loaders, with native values checked directly. All six fixture and
provenance hashes match the migration baseline (`catalog-fixture-audit.json`).

Validation history and final evidence follow below.

- `catalog-initial-song-linux-debug` records three constructor borrow-checking
  errors from the first build; they were corrected before the comparisons ran.
- `catalog-context-song-linux-debug`: **56 passed, zero failed** across catalog,
  clock, SL, course-display, banner and song-meter targets in 138.830 seconds;
  sources unchanged. Log SHA-256
  `871d335a1aca8ff4e7806a964eba2532e0c6eb06f48b91695e71513320d96933`.
- `catalog-final-adapters-song-linux-debug` and
  `catalog-reviewed-adapters-song-linux-debug` each passed all 253 tests before
  the final allocation/metadata-sharing review. The first lint run identified
  one new manual-range warning, fixed without changing numeric enum behavior.
  The reviewed lint returned to the pre-existing diagnostic set. These runs
  remain preserved; the following runs validate the final source.
- `catalog-owned-adapters-song-linux-debug`: **253 passed, zero failed** across all
  26 Rust song adapter targets in 253.233 seconds; sources unchanged.
  Log SHA-256 `17a3c19d14d407dbe6b30d9a3028a0e341d17f6abd6b5c6357dbd09e43f0d382`.
- `catalog-owned-lint-song-linux-debug`: exit 101 in 21.591 seconds;
  sources unchanged. The entire log matches the preceding SL lint run: the
  pre-existing cmd.rs:349 while_immutable_condition denial and 164 library
  warnings, with no Rust adapter diagnostic. The library denial prevents
  integration-test lint. Log SHA-256
  `aa5a0eadf1550917b797a0239c7ea79dc36598e0e7b14712945232507f717657`.
- `catalog-final-review.json` verifies the complete 1,066-file watched set
  and hashes against the final test/lint manifests. All seven changed Rust
  files pass file-based rustfmt checks (`catalog-format.json`).
  `catalog-source-audit.json` records four changed existing files, four new
  files, 1,058 unchanged entries and all 240 interpreter/root Cargo entries
  unchanged. The earlier 949-test interpreter evidence remains applicable.
  Pre-edit source and documentation snapshots are preserved separately.

The normal production song compiler still uses legacy Lua. GAMESTATE, player and
song option levels, remaining managers/input and generated gameplay actors,
complete compiler capture and live scenes still require migration. Normal/build
dependency removal, final fixture/downstream checks, application build and actual
rendered smoke, plus representative performance/memory comparisons remain
required. This stage does not claim full application validation or completion of
the migration goal.


## Song option objects and group operations

The preceding clarification turn made no implementation progress. This resumed
turn revalidated the saved migration objective and completed the pending
SongOptions review. The interpreter milestone stays closed and the production
migration acceptance scope stays fixed.

The Rust catalog now owns separate Preferred, Stage, Song and Current
SongOptions objects. Current adopts the existing music-rate table retained by
TimingData; density remains cached before these objects are initialized. Typed
setters preserve previous-value returns, receiver rules, chaining, float
conversion, raw field reads and partial writes before errors. The string parser
and formatter retain the production grammar, field order and rounding.

Private group operations select and describe levels, assign parsed values,
apply incremental strings, restart stages, clear attacks and advance the song
phase of current options. Copies preserve owner/method identities and unrelated
fields. Captures retain the actual level owners independently of public test
aliases. A saved object method retains its owner without retaining peer levels.
All Lua references use traced captures, including across callbacks and GC.

The group operations are staging plumbing for GAMESTATE. PlayerOptions groups,
chronological clock validation, PREFSMAN/player update ordering and the full
GAMESTATE host remain incomplete. No production song cutover is claimed.

Eleven differential tests cover initial contexts, setter coercion/chaining,
malformed previous values, parser and assignment semantics, restart/attack
copies, f32 approach and timing-rate coupling, partial update errors, raw lookup,
original tonumber retention, immutable captured level references, collection and
runtime isolation. The unchanged song-options-string fixture runs through real
actor/message bindings and a test-only bridge for its two GAMESTATE song-option
calls. This bridge does not test or stand in for a complete GAMESTATE host.
All five fixture/provenance files match the frozen inventory.

The first focused run recorded 38 passes and three failures caused by test setup:
an array with a nil first element relied on undefined Lua length, the legacy
fixture lacked its actual NOTESKIN manager, and a native error wrapper was
passed directly to string.find. The repaired tests use explicit argument counts,
the actual native noteskin factory and tostring(error). The input cases and
error/state assertions remain intact; no engine behavior or coverage was removed.
The repaired focused run passed all 41 checks. Both histories remain preserved.

The initial full adapter run passed all 264 tests across 27 targets in 170.303
seconds with unchanged sources (`song-options-final-adapters-song-linux-debug`,
log SHA-256 `16f8d7411624767341d833806cabeeaff908666a4d4b16a0cc5e8f7ce9f383ca`).
The subsequent review removed one extra blank line before the test-only native
forwarder. The original native file byte prefix still matches its pre-edit
SHA-256; no native production implementation was modified. Final reviewed run
results are recorded below.

`song-options-format.json` verifies five new/changed Rust files and the appended
test-only hook. Whole-file song_tables.rs formatting diagnostics exactly match
its pre-edit source. `song-options-source-audit.json` records five changed files,
two additions, 1,061 unchanged entries and all 240 interpreter/root Cargo entries
unchanged; the earlier 949-test interpreter evidence remains applicable.
The unchanged host/lib controls, original native prefix, pre-edit snapshots and
`song-options-fixture-audit.json` preserve the independent comparison evidence.

- `song-options-reviewed-adapters-song-linux-debug`: **264 passed, zero failed**
  across 27 targets in 253.891 seconds; sources unchanged.
  Log SHA-256 `a641324bff5abffc15382ff52c477537f117b0c5669f6aa00e8da8c58f9f47a7`.
- `song-options-reviewed-lint-song-linux-debug`: exit 101 in 23.006
  seconds; sources unchanged. Its full log matches the pre-existing denial at
  cmd.rs:349 and 164 library warnings (SHA-256 `aa5a0eadf1550917b797a0239c7ea79dc36598e0e7b14712945232507f717657`).
  The library error prevents integration-test lint; this is not a clean Clippy run.
- `song-options-final-review.json` verifies identical test/lint source manifests,
  all 1,068 recorded current hashes, and the five unchanged fixture files.

The remaining player/gameplay managers, generated actors, compiler capture and
live song execution still require migration. Production cutover, legacy graph
removal, final downstream/fixture checks, application build, actual rendered
smoke and representative performance/memory measurements remain required.


## Game, character, card, unlock and hook managers

The Rust song runtime now installs GAMEMAN, CHARMAN, MEMCARDMAN, UNLOCKMAN and
HOOKS with the complete surfaces already exposed by the production host. This
moves existing bindings into the independent engine without adding new gameplay
or platform behavior. The interpreter and migration acceptance scopes are
unchanged.

GAMEMAN returns a fresh single-style list for each request, independently of the
requested game and session style. Style metadata and column getters retain the
existing receiver/argument conversion, rounding and clamping rules. Mutating
public style fields or getters does not rewrite captured metadata. Character
queries retain their existing empty/nil results. Unlock list queries return
fresh mutable tables, including two distinct results for the steps query.

Memory-card and hook operations retain their sandbox return values and record
side effects; they do not perform platform I/O. Unlock mutators record the side
effect before returning their original owning manager, including when scripts
replace the public global. Counter lookup/write errors, coercion, callback
reentry and collection preserve the production order. All retained Lua values
are traced captures; independent getters and style queries do not retain their
manager or style owner. Saved chaining methods retain their owner until released.

Six new tests compare the complete native method surfaces, ignored arguments,
return arity, fresh lists and nested identities, column metadata/coercion,
side-effect counters and failures, original tonumber retention, reentrant GC and
owner capture. They also verify disposal of manager cycles and runtime isolation.
The comparisons call the existing native factories through
install_manager_globals; tables.rs, host.rs and lib.rs are unchanged.

- `utility-managers-context-song-linux-debug`: **38 passed, zero failed** across
  utility managers, preferences/display, SongOptions and catalog targets in
  107.358 seconds; sources unchanged. Log SHA-256
  `dd6f74a89358c687bdfbe4bfe57b868d3e5db7b70d8b4458e3f0625ec3cb222f`.
- `utility-managers-format.json`: all four changed/new Rust files pass file-based
  rustfmt checks.
- `utility-managers-source-audit.json`: three changed files, two additions and
  1,065 unchanged entries; all 240 interpreter/root Cargo entries and all three
  native control files are unchanged. The earlier 949-test interpreter evidence
  remains applicable. The pre-edit source/documentation snapshot is preserved in
  `utility-managers-edit-baseline.json`.
- `utility-managers-final-adapters-song-linux-debug`: **270 passed, zero failed**
  across all 28 Rust adapter targets in 172.167 seconds;
  sources unchanged. Log SHA-256 `16c5ef2c48eed94b9f7580808b2e2c5e8af74209b129e223f6cf44adc7c82d10`.
- `utility-managers-final-lint-song-linux-debug`: exit 101 in
  21.601 seconds; sources unchanged. The complete log matches
  the existing cmd.rs:349 while_immutable_condition denial and 164 library warnings,
  with no new adapter diagnostic. Log SHA-256
  `aa5a0eadf1550917b797a0239c7ea79dc36598e0e7b14712945232507f717657`.
  The library failure prevents integration-test lint; this is not a clean run.
- The full test run reported a deprecated `mlua::String` alias in the new test
  harness. After validation finished, it was changed to its exact underlying
  `mlua::LuaString` type. `utility-managers-harness-review.json` preserves the
  pre-edit text and proves that this was the only change. Production sources and
  every other integration test are unchanged from the 270-test run.
- `utility-managers-reviewed-harness-song-linux-debug`: **6 passed, zero failed**
  in 9.704 seconds against the corrected test source, with
  unchanged sources and the warning removed. Log SHA-256
  `7a2cb6331dc66f53ced670c05b027f322d69db638fbdf45d396261d015b3379f`.
- `utility-managers-final-review.json` verifies the complete final 1,070-file
  watched set and hashes against that run, the 1,069 hashes unchanged from the
  full suite/lint, final formatting, unchanged core/native controls and five
  unchanged SongOptions fixture files. Thus the full suite's unchanged targets
  and the rerun changed target cover the final adapter sources.

The normal production song compiler still uses legacy Lua. PlayerOptions and
PlayerState/GAMESTATE, remaining managers/input, generated gameplay actors,
complete compiler capture and live song execution remain migration work.
Unconditional production cutover, legacy normal/build graph removal, final
fixture/downstream checks, normal application build and actual rendered smoke,
plus representative performance/memory comparisons remain required. The goal
remains active; no interpreter milestone or acceptance criterion was reopened.


## PlayerOptions object bindings

The preceding goal turn made verified progress on SongOptions and utility
managers. This turn reread the fixed migration objective, verified every one of
the preceding 1,070 watched source hashes, and continued the existing player
binding migration. The interpreter milestone remains closed.

The retained Rust catalog now owns eight separate PlayerOptions objects, one
for each player and Preferred/Stage/Song/Current level. Initial speed mode,
perspective, autoplay, Mini, reference BPM and noteskin name come from the same
compile context as the native host. Eager methods and lazy method caching retain
the existing receiver rules; deleting an eager method takes the same generic
fallback path as native Lua. Methods resolve mutable state and approach tables
on every call, retaining temporary tables through reentrant metamethods and GC.
A saved scalar method retains only its owning level and required primitive.

Scalar, boolean, enum, perspective and speed bindings preserve previous-value
returns, chaining, numeric conversion, raw versus metamethod-aware reads and
partial writes before errors. Perspective aliases share tilt/skew fields.
Direct speed fields and X/C/M setters retain their different write sets and
approach behavior; A/CA setters preserve their existing optional-value semantics.

FromString retains token precedence, amount/approach parsing, shared speed fields,
perspective writes, reset behavior and noteskin lookup order. The implementation
reuses existing pure Rust parsing/classification policy. Derived reverse and
attack queries observe current style and state, including replacement during
callbacks. Timing-window methods retain enum validation, fresh query results,
side-effect ordering and return rules. NoteSkin operates on its supplied receiver
and retains writes made before a later counter error.

The objects remain private catalog state. PlayerState, option-level propagation
and approach, chronological clock ordering and GAMESTATE wiring are still
pending. NOTESKIN manager migration is also still pending; the differential
object tests install controlled Lua callbacks in the public NOTESKIN slot to
compare callback dispatch, lookup precedence, reentry and failures. They do not
claim a migrated manager or complete frozen gameplay-fixture execution.

The tests compare the actual native objects from SongLuaOptionLevels, exposed by
the test-support catalog helper. Native song_tables.rs, player_options.rs, host.rs
and lib.rs remain unchanged. Coverage includes every declared PlayerOptions
method and all 16 columns for each supported multi-column prefix, profile
initialization, mutable state/speeds, perspective and enum errors, speed-mode
transitions, parser/reset grammar, noteskin receiver behavior, derived queries,
reentrant GC, lazy method recreation and runtime isolation.

- `player-options-initial-song-linux-debug`: **9 passed, zero failed** in
  105.292 seconds; sources unchanged. Log SHA-256
  `a9cbfce2b873f62ef83fdd69d4c3fb4e7ce4ac59ed0d46e39295c585d6a3ccbe`.
- The source audit records five existing files changed, five added and 1,065
  unchanged entries. All 240 interpreter/root Cargo entries and all four native
  control files remain unchanged, preserving the earlier 949-test interpreter
  evidence. The complete pre-edit snapshot is in `player-options-edit-baseline.json`.
- `player-options-final-adapters-song-linux-debug`: **280 passed, zero failed**
  across 29 Rust adapter targets in 211.851 seconds; sources unchanged.
  Log SHA-256 `2179d8e8d6282944ce636d938dcf4fe50ba32719dd6e2db21b5e80e8fade20e5`.
- `player-options-final-lint-song-linux-debug`: exit 101 in 25.383
  seconds; sources unchanged. Its complete log matches the established native
  cmd.rs:349 denial and 164 library warnings, with no new adapter diagnostic.
  Log SHA-256 `aa5a0eadf1550917b797a0239c7ea79dc36598e0e7b14712945232507f717657`.
- Review found two explicit native diagnostic texts that were not reproduced
  exactly: negative scalar/direct-field speeds and invalid ModTimerSetting
  arguments. The messages were corrected after validation finished. An eleventh
  differential test now asserts these messages (plus the other explicit option
  diagnostics) and the resulting partial state. No failing test or input case
  was removed. `player-options-error-review-baseline.json` preserves the source
  before this correction. Final reviewed results follow below.
- `player-options-reviewed-adapters-song-linux-debug`: **281 passed, zero failed**
  across all 29 Rust adapter targets in 299.285 seconds;
  sources unchanged. This includes all eleven PlayerOptions tests. Log SHA-256
  `05992f5f2b77b3e668fa2be21cf988dedeab9514a73d6093cbc6734d120dd4cd`.
- `player-options-reviewed-lint-song-linux-debug`: exit 101 in
  25.558 seconds; sources unchanged. The complete log still
  matches the pre-existing cmd.rs:349 while_immutable_condition denial and 164
  library warnings, with no new Rust adapter diagnostics. Log SHA-256
  `aa5a0eadf1550917b797a0239c7ea79dc36598e0e7b14712945232507f717657`.
  The library denial prevents integration-test lint; this is not a clean Clippy run.
- `player-options-format.json` verifies all nine changed/new Rust files with
  file-based rustfmt checks. `player-options-final-review.json` verifies the full
  1,075-file watched set and current hashes against both final test and lint
  manifests, unchanged interpreter/root Cargo entries and the four native controls.

The production song compiler still uses legacy Lua. PlayerState/GAMESTATE level
propagation and approach, remaining managers including NOTESKIN, input/generated
gameplay actors, complete compiler capture and live execution are still pending.
Production cutover, legacy normal/build graph removal, final downstream/fixture
runs, application build and actual rendered smoke, plus representative performance
and memory comparisons remain required. This stage closes no additional goal or
interpreter milestone and does not claim full application validation.


## PlayerState and chronological option phases

The active objective was reread before continuing. The intervening scope-only
turn made no implementation progress; this work resumes the separately authorized
production migration. The completed interpreter and C API removal milestones stay
closed. `player-state-edit-baseline.json` preserves the source before this work.

The song adapter now constructs both PlayerState objects with stable Preferred,
Stage, Song and Current references. GetPlayerOptions, GetCurrentPlayerOptions,
GetPlayerOptionsString/Array and SetPlayerOptions preserve the existing argument,
identity, formatting, return-count and side-effect policies. Strings retain native
ordering, percentage quantization, perspective aliases and skin capitalization.
Assignments preserve owner and state/speed-table identity, reset timing windows,
retain an existing skin for an empty source and write Current before lower levels
where the native operation does. Failures keep the same earlier writes.

Each PlayerState retains a distinct SongPosition wrapper. Preparation wrappers
share the original song backing; live wrappers retain their respective player
backings. Controller and health getters read a host-owned store of published phase
results, with the existing construction-time fallback before live values exist.
The host setter publishes those values without invoking Lua or update phases.
Live health calculation and HealthStateChanged dispatch are still pending in the
complete live-scene migration; these getters do not stand in for that phase.

Combined option operations preserve the chronological microsecond clock, stage
restart, attack rebuilding/clearing and modification propagation. Advances commit
the clock, advance SongOptions, call the current PREFSMAN preference getter, then
advance P1 and P2 with their respective deltas. The CMod approach keeps its native
150 multiplier and rate independence. Discrete fields and copied timing windows
retain their native behavior. Callback reentry, GC and errors preserve retained
owners and commit boundaries. Sampling positions does not advance option groups.
RustSongRuntime exposes these explicit host phases for the subsequent compiler
and live integration; the Lua objects remain private catalog state until GAMESTATE
is fully installed.

The legacy controls call the actual PlayerState objects and production group
functions. A test-only helper appended to `live.rs` publishes controller and health
values into the real native stores. The entire original file is an unchanged byte
prefix, and its appended helper passes formatting. `song_tables.rs`, the native
PlayerOptions implementation, `host.rs` and `lib.rs` are unchanged. Existing parser
and SongOptions helpers only gain the visibility needed by the combined phases.

Validation:

- `player-state-focused-song-linux-debug`: initial **11 passed, 2 failed**, in
  105.067 seconds; source unchanged. One test used mlua's implicit-expression eval
  against statement execution, and one indexed an intentionally absent speed
  table before initializing it. The corrections make the first script explicitly
  return no values and construct the second test's speed table. Neither scenario
  was removed; `player-state-test-setup-baseline.json` preserves the first source.
- Fourteen new differential/ownership tests cover both preparation/live contexts,
  initial surfaces and selectors, option strings/arrays, all declared display
  fields and column modifiers, propagation and partial errors, restart/attacks,
  f32 approach and separate clocks, published statuses, original primitives,
  malformed raw fields, callback reentry/collection and released owners. The host
  entry points are also compared with the actual native operations.
- `player-state-adapters-song-linux-debug`: **295 passed, zero failed**, across
  all **30 Rust adapter targets**, in 229.843 seconds; source unchanged. This
  includes all fourteen new tests. Log SHA-256
  `27e2ab92cd35204bbb54a3120187aa6dd9c5b82314aebb4c53a9e2088ddb1d98`.
- `player-state-lint-song-linux-debug`: exit 101 in 49.295 seconds (including
  waiting for Cargo's build-directory lock); source unchanged. The library
  diagnostics exactly match the prior cmd.rs:349 while_immutable_condition error
  and 164 warnings. No new adapter diagnostics appear. The library error prevents
  integration-test lint, so this is not a clean Clippy result. Log SHA-256
  `1acc34c15b9a3cfaac3d74b5220816590cadedae6a58da7c567f3737685da6f9`.
  `player-state-lint-review.json` records the comparison; the only full-log
  difference is Cargo's lock-wait line.
- `player-state-format.json`: eleven changed/new Rust files pass rustfmt checks;
  the native live file's original prefix and appended helper are checked separately.
- `player-state-final-review.json`: all **1,080 watched source paths and hashes**
  match final test and lint manifests. Relative to the prior 1,075-file review,
  five files are new, eight changed and 1,067 unchanged; no source was removed.
  All 240 interpreter/root Cargo entries match the prior 949-test core run.

The production song compiler still uses legacy Lua. Full GAMESTATE, remaining
managers including NOTESKIN, input/generated gameplay actors, compiler capture
and live execution remain migration work. The unconditional cutover, removal of
legacy from the normal/build graph, final downstream/fixture suites, application
build and actual rendered smoke, and performance/memory comparisons remain
required. This stage does not complete the migration or reopen another milestone.


## GAMESTATE script bindings and selection

The previous goal turn made implementation progress on PlayerState and option
phases. This turn reread the fixed migration objective and verified all 1,080
previously validated source hashes before editing. The interpreter and C API
removal milestones remain closed. `gamestate-edit-baseline.json` preserves the
original files; `gamestate-test-review-baseline.json` preserves the test sources
before the error-comparison and ownership-setup updates.

The Rust song host now installs GAMESTATE and its writable GameState class table,
including the protected instance metatable and normal instance/class overrides.
The complete production method-name surface is compared against the native host.
Methods expose the retained song, course, style, player states, options and trails.
Environment values, enabled-player lists, controller queries, preference reads,
clock, layout and display-BPM helpers preserve their existing argument, identity,
return-count and side-effect policies. Saved callbacks retain their original
slots independently of public getters and catalog aliases. IsAutoplay runs the
same Lua helper against the current public PlayerState controller.

Selection keeps authored chart metadata in Rust and chart identities in traced
Lua captures. SetCurrentSteps validates the current style through its public
GetStepsType callback, writes and broadcasts synchronously even for an unchanged
pointer, then re-reads the main selection before deciding whether to force other
players. Reentrant broadcasts may change that selection. Errors retain preceding
writes and restore message/error guards. GetEasiestStepsDifficulty uses immutable
metadata and the existing ScriptError behavior for missing charts. The existing
actor host provides actual message delivery and guarded diagnostics; its error
reporter is shared instead of implementing a second dispatcher.

ApplyStageModifiers, SetSongOptions and ApplyGameCommand use the retained option
operations. Game commands keep their one-based numeric player argument, quoted
semicolon handling, clause order and propagation. PlayerIsUsingModifier parses
copies and compares Current song/player values, preserving underlying speed-field
and modifier semantics. The original window, current-option and speed-query JSON
assertions run unchanged through actual GAMESTATE/PlayerState APIs; only their
clock/enum entry points are adapted to the normal host API. The existing
song-options-string actor fixture also uses installed GAMESTATE methods now,
replacing its earlier two-method facade. NOTESKIN remains a controlled dependency
in these tests; this does not claim the NOTESKIN manager has been migrated.

GAMESTATE introduces real global owners for the catalog objects. Four existing
collection tests now explicitly retire those public owners before retaining only
the callback under test; all prior collection assertions are preserved. A new
lifetime test first proves GAMESTATE keeps the objects alive, then verifies that
retiring its public owners allows the instance/class/player states to collect
while a saved song getter retains only its selection. Native control files,
including host.rs, song_tables.rs, player_options.rs, runtime.rs, live.rs and
lib.rs, are unchanged.

Validation:

- `gamestate-focused-song-linux-debug`: initial compile failure in 25.092 seconds,
  with unchanged source. The installer re-export needed visibility in the parent
  runtime module; this was corrected.
- `gamestate-focused-fixed-song-linux-debug`: **9 passed, 2 failed** in 104.082
  seconds, with unchanged source. Two table constructors accidentally included
  pcall's error value and compared mlua error userdata with the Rust engine's error
  string. They now explicitly compare status, while a dedicated test checks the
  native diagnostic text and unchanged selection. All invalid-call cases remain.
- Thirteen new tests cover the complete class surface, selector/receiver rules,
  mutable class methods, saved queries, environment metamethods, style and clock
  helpers, dynamic display-BPM getters, command propagation, modifier predicates,
  authored selection metadata, synchronous message reentry, suppression and error
  recovery, collection, and the three frozen option assertion bodies.
- `gamestate-adapters-song-linux-debug`: **308 passed, zero failed**, across all
  **31 Rust adapter targets**, in 315.766 seconds; source unchanged. This includes
  all thirteen new tests and the updated ownership and song-option fixture tests.
  Log SHA-256
  `cca20b1ea89a7c79ee3b350239de115d3c907ae7a8da09ae55df8f3db190b752`.
- `gamestate-lint-song-linux-debug`: exit 101 in 149.964 seconds, including waiting
  for Cargo's build-directory lock; source unchanged. The entire log matches the
  previous run: the established cmd.rs:349 while_immutable_condition error and
  164 library warnings, with no new adapter diagnostics. The library error still
  prevents integration-test lint; this is not a clean Clippy run. Log SHA-256
  `1acc34c15b9a3cfaac3d74b5220816590cadedae6a58da7c567f3737685da6f9`.
  `gamestate-lint-review.json` records the comparison.
- `gamestate-format.json`: all twenty changed/new Rust files pass rustfmt checks.
- `gamestate-final-review.json`: the full **1,085-file watched set** and its hashes
  match final test and lint manifests. Five source files are new, sixteen changed
  and 1,064 unchanged relative to the previous review; none were removed. All 240
  interpreter/root Cargo entries still match the prior 949-test interpreter run.
  The six native controls and the referenced frozen fixture hashes are recorded.

The normal production song compiler still uses legacy Lua. Remaining managers
(including NOTESKIN, profiles/stats and UI helpers), input/generated gameplay
actors, screen-transition selection initialization, compiler capture and complete
live orchestration remain. The unconditional cutover and removal of legacy from
normal/build dependencies, final downstream/fixture suites, application build and
actual rendered smoke, and representative performance/memory comparisons remain
required. No goal or interpreter milestone is reopened or marked complete here.

## Song-side NOTESKIN manager and template environments

The Rust song adapter now installs the existing NOTESKIN method surface through
`RustSongRuntime::new_with_noteskin_resolver`. It reuses the application's resolver
and the existing default-skin selection policy. Song-local metrics directories
are enumerated once at construction; resolver skin lists remain fresh on each
request. String, float, integer, boolean and command metrics retain their native
defaults, conversion and argument rules. Metric commands use the retained
original compiler, including the distinct percent-expression path.

Lua templates retain separate NOTESKIN and Var environments. Nested skin loads,
relative loadfile calls, later commands, collecting callbacks and replacements
of public globals preserve each template's selected skin, button, element and
original fallback Var function. The file loader's private entry point accepts
an explicit environment and retains its existing context restoration and error
handling. Image and unresolved-resource templates use the actor factory without
running Init; native state and composed commands survive independent template
concatenation. Actor concatenation now uses the existing template-cloning path
for all supported classes, rather than restricting that path to text actors.

Var's existing GameCommand fallback is installed alongside these bindings. Its
constant methods and fresh identities follow the production host, while song,
style, steps, course, trail and group queries resolve current public GAMESTATE
methods. ApplyToStyle preserves the existing side-effect counter.

Nine new differential tests exercise the actual production noteskin manager and
actor factory, including selection defaults, method enumeration, strict receiver
and string arguments, numeric conversions, local-name snapshots, command errors,
nested and relative file loads, tag metamethods, failed-load context restoration,
template setters/clones, real PlayerOptions calls, saved-query collection and Var
queries. A scene lifecycle test runs delayed commands after collection in both
preparation and live contexts, then checks End and actor retirement. The frozen
song-options-string fixture now uses the installed NOTESKIN manager; its native
assertions are unchanged. Other option tests retain controlled callbacks where
they specifically test authored replacement and callback failure behavior.

`noteskin-manager-edit-baseline.json` preserves pre-edit source and documentation.
`noteskin-manager-review-baseline.json` preserves the helper and test before the
final formatting and relative-file review. The legacy compiler's original bytes
remain an unchanged prefix; the sole addition there is a cfg-gated test helper
that forwards to the real manager/factory. No interpreter changes were needed.

Validation:

- `noteskin-manager-focused-song-linux-debug`: initial compilation failure in
  26.868 seconds. The new factory call needed the existing helper's module
  qualification; no tests ran. Sources were unchanged during validation.
- `noteskin-manager-focused-fixed-song-linux-debug`: eight passed, one failed in
  101.788 seconds. The failing test reversed ApplyStageModifiers' player/text
  arguments and failed on the production host. The call was corrected without
  removing the modifier or lifetime assertions.
- `noteskin-manager-focused-reviewed-song-linux-debug`: **20 passed, zero failed**
  in 17.329 seconds, covering all nine noteskin and eleven song-option tests.
- `noteskin-manager-adapters-song-linux-debug`: **317 passed, zero failed** in
  264.571 seconds. The initial format review then identified formatting in the
  newly added native test helper. That helper was formatted and moved after the
  unchanged original compiler source; the nested template test also gained a
  relative loadfile check. These changes triggered final validation below.
- `noteskin-manager-final-adapters-song-linux-debug`: **317 passed, zero failed**,
  across all **32 Rust adapter targets**, in 345.710 seconds; sources unchanged.
  Log SHA-256
  `bee8aad9e7593f4f231d356081e6d58f3f19c3131b4e1d2c8629b73cecd3971c`.
- `noteskin-manager-final-lint-song-linux-debug`: exit 101 in 25.226 seconds;
  sources unchanged. After build-status output, the complete diagnostics match
  the previous GAMESTATE baseline: cmd.rs:349's existing while_immutable_condition
  denial and 164 library warnings. No new adapter diagnostic was introduced.
  The library error prevents integration-test lint; this is not a clean Clippy
  result. Log SHA-256
  `7b54f849f226da0372ef4c88492df1e604de5cc528ca39a689ae6c137c64bc9d`.
  `noteskin-manager-final-lint-review.json` records the exact comparison.
- `noteskin-manager-final-format.json`: nine changed/new Rust files and the
  added native test helper pass their applicable rustfmt checks. Unrelated
  existing formatting in the legacy compiler was preserved.
- `noteskin-manager-final-review.json`, produced by
  `review_noteskin_manager.py`, checks all **1,088 watched source files** against
  both final validation manifests, all declared Rust adapter targets, log hashes,
  frozen option fixtures, unchanged native controls, the compiler's original
  byte prefix, formatting hashes, and the interpreter/root entries from the prior
  949-test core run. All 240 interpreter/root entries are unchanged. Three watched
  files were added, nine changed, none removed and 1,076 unchanged compared with
  the previous adapter run. The overall goal is explicitly recorded as incomplete.

Production song compilation still uses legacy Lua. Profiles/statistics, remaining
compatibility and UI helpers, input/generated gameplay actors, screen transitions,
full compiler capture and live orchestration remain. Unconditional cutover,
removal of legacy from normal/build dependencies, final downstream/fixture suites,
the normal application build and rendered smoke, and representative preparation,
live-cost and memory comparisons are still required. The completed interpreter
and C API removal milestones remain closed.

## PROFILEMAN and high-score objects

The Rust song host now installs the complete existing PROFILEMAN/profile method
surface from tables.rs. Machine, Player 1, Player 2, fallback and local profiles
keep distinct identities and the native selector rules. Stateful fields preserve
their defaults, string-only setter inputs, typed getter conversions, f32 calorie
arithmetic, boolean parsing, arbitrary character/goal values, and mutable user
tables. Saved methods address their original owner, including calls supplied with
an alternate receiver. Save/screenshot methods retain the current return values
and side-effect counter policy.

High-score list factories return independent lists with retained mutable arrays
and the production default Machine score. Highest-score and rank queries traverse
raw sequence slots, stop at the first nil, and call each score's current GetName
method. A callback can replace or remove entries, append another score, trigger
collection, or fail; the lookup preserves native ordering and roots its selected
object through reentry. High-score radar queries return fresh default objects.

Eight differential tests cover complete method/field inventories, manager slots
and defaults, score/list/radar identities, all profile setter families, invalid
and non-finite inputs, typed getter failures, missing versus explicit-nil goals,
user-table identity, calorie rounding and display, metamethod reentry, partial
writes before errors, save return arity, original primitive conversions, dynamic
score lookup and retained callbacks after global replacement. A Rust ownership
test additionally verifies that retaining one profile getter does not retain the
manager or other profiles, and that releasing it permits its profile cycle to
collect. The production control implementation is unchanged.

`profiles-edit-baseline.json` preserves the pre-edit sources and documentation.
The new bindings are confined to the song adapter; no interpreter changes were
needed. The existing profile-mini fixtures cover PlayerOptions Mini and spline
behavior, rather than PROFILEMAN; they are not claimed as profile-manager tests.

Validation:

- `profiles-focused-song-linux-debug`: **8 passed, zero failed**, in 99.218
  seconds; sources unchanged. Log SHA-256
  `d5d7654d4831c3797f9c3752545bf6d06572e4f53e095de97517dc719d7e6d00`.
- `profiles-adapters-song-linux-debug`: **325 passed, zero failed**, across all
  **33 Rust adapter targets**, in 265.174 seconds; sources unchanged. Log SHA-256
  `53cfb10e9ec7467d2ba63bf0e656a33ccd95c0bf1bdc6f1cb0b2f1d351066bd9`.
- `profiles-lint-song-linux-debug`: exit 101 in 23.898 seconds; sources unchanged.
  The complete log is byte-identical to the preceding noteskin-manager lint run:
  the existing cmd.rs:349 while_immutable_condition denial and 164 library
  warnings. No new adapter diagnostics were introduced. The library error still
  prevents integration-test lint; this is not a clean Clippy result. Log SHA-256
  `7b54f849f226da0372ef4c88492df1e604de5cc528ca39a689ae6c137c64bc9d`.
  `profiles-lint-review.json` records that comparison.
- `profiles-format.json`: all four changed/new Rust files pass rustfmt checks.
- `profiles-final-review.json`, produced by `review_profiles.py`, verifies the
  complete **1,091-file watched set** against both final manifests, all declared adapter
  targets and log hashes, formatting hashes, unchanged native controls and frozen
  option fixtures, and the interpreter/root entries from the prior 949-test run.
  All 240 interpreter/root entries are unchanged. Three watched files were added,
  two changed, none removed and 1,086 unchanged since the preceding stage. The audit
  records this as a completed adapter stage within an incomplete goal.

STATSMAN/live statistics publication, remaining compatibility/UI helpers,
input/generated gameplay actors, screen transitions, full compiler capture and
live orchestration still require migration. The normal production song path
remains legacy. Unconditional cutover, removal of legacy from normal/build
dependencies, final downstream/fixture checks, normal application build and
rendered smoke, and preparation/live/memory comparisons remain required. The
interpreter and C API removal milestones remain closed.


## STATSMAN and explicit statistics phases

The Rust song runtime now installs the existing STATSMAN, StageStats and
PlayerStageStats surfaces. The shared current/accumulated/final stage, distinct
player and multiplayer objects, high scores, played/possible songs, steps arrays,
radars, constant policies and method argument rules follow the production control.
Stage-owned Song and multiplayer Steps objects remain distinct from current
GAMESTATE selections. Dance percentages preserve the native f32 arithmetic and
rounding captured in `native-dance-percent.json`.

`reset_stage_statistics` performs the host load-boundary reset independently of
STATSMAN:Reset(), which remains a no-op. It follows the stage's current raw player
array, preserves played/possible array identities and trailing entries, and roots
the selected Steps across calls to the current public getters. Collecting getters
can change selection or fail without invalidating the original snapshot or undoing
prior writes. The next reset can recover from a failed getter.

`feed_stage_statistics` updates the originally retained stage/player tables, even
if script replaces STATSMAN or the raw stage player array. It preserves raw-write
ordering, current versus recorded life, life-meter updates, all 11 judgment counts,
and script-owned score/max-score fields. Invalid life meters or judgment tables
preserve preceding writes and can recover on the next frame. Frame publication
does not advance clocks, option levels, health, or messages. `publish_live_players`
is deliberately separate. FailPlayer updates published player state and queues
ordered Fail writes only in live mode; `take_player_writes` drains them once.

Ten statistics tests exercise the actual legacy implementation alongside Rust,
including inventories/defaults/selectors, integer/string/non-finite conversions,
setter return values and side effects, the frozen native percentage fixture,
mutable raw fields and alternate receivers, reset/feed reentry and partial errors,
ordered live writes and collection. Host roots retain the statistics until session
retirement; saved stateful methods retain their owner, while receiver-based queries
and fresh radar/life-record factories do not retain the original player. Five
existing catalog/options/PlayerState/GAMESTATE GC tests now retire STATSMAN's added
public ownership explicitly; all their original collection assertions remain.

The native production implementation is unchanged. Four cfg-gated test-support
functions appended to live.rs expose its existing feed, player publication and
write-queue phases. `statistics-native-control-review.json` verifies the complete
518,434-byte pre-edit prefix and the formatting of the 938 appended bytes.
`statistics-edit-baseline.json` preserves all pre-edit source/document bytes.
The full live query-trace path still belongs to pending live orchestration.

The first focused build exposed six borrow-order errors in the new raw table
reads. The second run passed six tests and exposed one signed-zero conversion
difference plus test-harness defects: omitted native game-state app-data retention
and expression-evaluating Chunk::eval used for statement bodies. Those were fixed;
no assertions were removed. The next focused run passed all ten tests. Clippy's
new large-enum style warning is locally annotated because boxing the 536-byte
frame payload would allocate on every update. Final checks below use the resulting
source, including that annotation.

Validation:

- `statistics-focused-v3-song-linux-debug`: **10 passed, zero failed**,
  in 100.335 seconds; sources unchanged. Log SHA-256
  `8a8e1af02ddf47fa095fe4e901e0e2e028bb0720231b25828b24c02afbc0e8d4`.
- `statistics-adapters-song-linux-debug`: **335 passed, zero failed**,
  across all **34 Rust adapter targets**, in 410.621 seconds;
  sources unchanged. Log SHA-256
  `c61e9d74d5b4c3044e9fcbd88d0f2e3ae6c0c8690f4337325c1d3797520394de`.
- `statistics-lint-song-linux-debug`: exit 101 in 23.810 seconds;
  sources unchanged. The complete song-library diagnostics match the profile-stage
  baseline exactly: the existing cmd.rs:349 while_immutable_condition denial and
  164 warnings. Integration-test lint remains blocked by that library error; this
  is not a clean Clippy result. The log also contains a dependency warning for
  deadsync-config::set_windows_gamepad_backend, already present in this stage's
  first focused build. `statistics-lint-review.json` preserves both preludes and
  verifies the library comparison. Log SHA-256
  `01957ba4bd70d4050a8a2bf94265e82100e03acd97c0af4ba3e700d9e04bb550`.
- `statistics-format.json`: all 12 changed/new adapter/test Rust files pass
  rustfmt. The four native test-only helpers pass separately as recorded above.
- `statistics-final-review.json`, produced by `review_statistics.py`, verifies
  the complete **1094-file watched set** against the final test and lint
  source manifests, all declared Rust adapter targets, log hashes, formatting
  hashes, 12 unchanged native control files plus the preserved live.rs prefix,
  and six frozen fixture hashes. All **240 interpreter/root entries** remain
  unchanged since the 949-test core run. Since the profile stage, three watched
  files were added, 11 changed, none removed and 1,080 remained unchanged.

Remaining compatibility/UI helpers, input/generated gameplay actors, screen
transitions, full compiler capture and live orchestration still require migration,
including the live query-trace path. The normal production song path remains
legacy. Unconditional cutover, removal of legacy from normal/build dependencies,
final downstream/fixture checks, normal application build and rendered smoke,
and preparation/live/memory comparisons remain required. This is a verified
adapter stage within an incomplete goal. The interpreter and C API removal
milestones remain closed.


## Option rows and screen/theme utility globals

The Rust song host now installs CustomOptionRow, the five Conf* factories,
ThemePrefsRows and OperatorMenuOptionRows using the existing Rust option-row
specifications. Factories return independent rows, choice/value arrays and message
lists. OperatorMenuOptionRows retains its uncached dynamic fallback factories.
LoadSelections uses the captured original choice count and raw selection values,
including the existing receiver/list fallback rules. EnabledForPlayers returns a
fresh two-player array. SaveSelections keeps its original row, increments the side
effect count first, then resolves current row fields and the current PREFSMAN,
ThemePrefs or SL destination. Values and intermediate tables remain rooted across
collecting getters and setters. The MusicRate global policy, player selection,
multi-choice modifier names, first-selected/default value policy, partial writes
and return arity follow the control implementation. ThemePrefsRows.InitAll forwards
its exact argument to the current ThemePrefs.InitAll after constructing the
existing default definitions.

ScreenString, ScreenSystemLayerHelpers credit text, SelectMusicOrCourse and the
20 existing Branch routes are installed. The background-fit/Center1Player script
and Simply Love's TableToString/SM script are copied verbatim from host.rs, with
unchanged chunk names. They continue resolving current public managers, formatter,
Trace and actor methods at call time. `screen-ui-script-control.json` verifies the
1,545-byte and 3,443-byte script copies against the control source. The native
option-row and helper implementations were not edited. The pre-edit source and
document bytes are preserved in `screen-ui-edit-baseline.json`.

Eight differential tests cover all supported custom row names in original/lower/
upper case, all declared theme preference names, configuration/operator factories,
metadata and independent identities, invalid keys, all save destination families,
raw list/field behavior, sparse arrays, live method replacement, collection,
partial errors and side-effect ordering. They also cover screen routes, fit method
order, centering preferences, self-referencing table formatting and SM's formatter/
broadcast/Trace ordering. Rust GC tests verify that saved save callbacks retain
only their own row and that load/enabled/factory callbacks do not retain discarded
rows or manager tables.

The first focused run passed seven tests. Its only failure was a test assumption
that a string first argument would be skipped by LoadSelections; the native method
only skips table receivers and creates a fresh list in that case. The assertion
was corrected to require that fresh list and verify the supplied list stays
unchanged. The next focused run passed all eight tests; no coverage was removed.

Validation:

- `screen-ui-focused-v2-song-linux-debug`: **8 passed, zero failed**,
  in 16.586 seconds; sources unchanged. Log SHA-256
  `4253d7ce3a1c5385368b32d249127331a745ef750e11a4da3fdfa8727f7f8afd`.
- `screen-ui-adapters-song-linux-debug`: **343 passed, zero failed**,
  across all **35 Rust adapter targets**, in 310.848 seconds;
  sources unchanged. Log SHA-256
  `0692c4ce2a66fb36d99e19b2e0d9ae76c1699769143429b5339d8e5f9f9e54dc`.
- `screen-ui-lint-song-linux-debug`: exit 101 in 32.786 seconds;
  sources unchanged. All song-library diagnostics match the statistics-stage
  baseline byte-for-byte: the existing cmd.rs:349 while_immutable_condition denial
  and 164 warnings, with no new adapter diagnostics. Integration-test lint is
  blocked by the library error; this is not a clean Clippy result. The complete
  log differs only by an additional Cargo build-lock wait line, as recorded in
  `screen-ui-lint-review.json`. Log SHA-256
  `34a97b034af6516e606b7f608d0b1ad049ed5e247f5dd8bf0e8fece3230f2e89`.
- `screen-ui-format.json`: all five changed/new Rust files pass rustfmt.
- `screen-ui-final-review.json`, produced by `review_screen_ui.py`, verifies the
  complete **1099-file watched set** against final test/lint source manifests,
  all Rust adapter targets, log/format hashes, 14 unchanged native control files,
  six unchanged frozen fixtures, and both verbatim Lua helper copies. All **240
  interpreter/root entries** remain unchanged since the 949-test core run. The
  audit enforces the exact planned delta: five watched files added, three changed,
  none removed and 1,091 unchanged since the statistics stage.

Remaining compatibility bindings, screen/input and generated gameplay actors,
transitions, full compiler capture and live orchestration still require migration.
The inspected top-screen factory owns player/life-meter actors and dispatches
construction-time messages before song actors load; that ordering must be retained
when the screen path is ported. Production song execution remains legacy.
Unconditional cutover, removal of normal/build legacy dependencies, final
regression/fixture checks, normal application build and rendered smoke, and
preparation/live/memory comparisons remain required. This adapter stage is verified;
the migration goal is not complete. Interpreter and C API removal stay closed.

## Screen input registry and existing predicate compiler

The Rust song runtime now provides `bind_screen_input`, `dispatch_input` and the
sticky offline-compatibility check. Screen methods retain the current registry
through traced captures. Callback functions are rooted as keys in its private Lua
map; the Rust dispatch vector keeps identities and optional compiled predicates.
Registration is strict, deduplicates by function identity and returns the supplied
screen receiver. Screen replacement clears the callback map while saved methods
address the replacement registry and the offline rejection remains set.

Dispatch preserves identity order, deferred removal, additions above/below the
current cursor, nested dispatch and cursor repair after screen replacement.
Callbacks share a fresh event and DeviceInput table on each Lua dispatch. Errors
are collected in callback order; Lua truthiness stops subsequent callbacks.
An input-only compiled prefix keeps the original event-allocation bypass. Once a
Lua callback runs, all remaining callbacks use its shared mutable event, including
otherwise compilable predicates. Installed hooks force Lua execution. Binding
and dispatch use the existing rooted Rust stack API without introducing internal
calls visible to debug hooks. Resource limits unwind the dispatch; deferred roots
are then released at the idle VM boundary.

The existing seven-file finite bytecode compiler/evaluator was copied from the
legacy crate into the song adapter's `baked_program` module. Its legacy Function
dump wrappers and their scratch buffer were removed. The bytecode compiler,
schema validation, evaluator, batch reuse, shared cells and Boolean-table behavior
are unchanged. This relocates an existing production dependency; it does not add
baking capabilities. The input predicate schema and frame evaluator are preserved
with a bytecode entry point. Future song baking integration can use the same
module without depending on the legacy runtime.

`review_input_port.py` verifies all seven compiler files and the input predicate
against their original controls, allowing only the documented wrapper/import
changes and formatting. Both compiler fixture files retain every test body and
assertion, routed through test-only dumping helpers into the moved compiler.
All 31 compiler fixtures pass, covering shared and independent state, actions,
binary strings, table aliases, malformed dumps, cache identity and VM destruction.
The native input implementation and legacy compiler remain unchanged as controls
until the complete song replacement is ready for production cutover.

Twelve new input tests compare event conversions, strict registration, chaining,
deduplication, sticky offline rejection, ordered errors and truthiness, deferred
removal, additions, nested dispatch, screen replacement and saved methods. GC
checks cover retained callbacks, their captured tables, discarded screens and
release after removal/rebinding. Compiled predicates are compared both with the
legacy path and direct Lua execution. A mixed ordered sequence verifies that a
false compiled prefix enters Lua and later predicates see its mutations. Repeated
compiled-only dispatch keeps Lua table count and live heap bytes unchanged. Hook
tests verify fallback and the absence of synthetic calls during binding. A Rust
instruction-limit test verifies unwinding and deferred-removal cleanup.

The first focused run stopped at test compilation: the fixture bridge attempted
to call a crate-private legacy dumping method, and hook setup used Value instead
of NativeHook. The bridge now uses the existing public dump operation and the
hook calls use the declared enum. The next run passed 41 tests. Binding was then
moved off an internal helper closure to preserve hook visibility; two additional
regressions bring the focused suite to 43 passing tests. No failing coverage was
removed or disabled.

Validation:

- `input-focused-v3-song-linux-debug`: **43 passed, zero failed** in 111.091
  seconds; sources unchanged. Log SHA-256
  `82f821bc75d321c414741215b067afe362418fc3f95ce1f71ad1940357c4d3d9`.
- `input-adapters-song-linux-debug`: **386 passed, zero failed** across all
  **38 Rust adapter targets**, in 347.082 seconds; sources unchanged. Log SHA-256
  `886a70082e1fb2a8b6e5242bc9f1a732953d5df55e9fb936f59028775bbf92ac`.
- `input-lint-song-linux-debug`: exit 101 in 27.266 seconds; sources unchanged.
  The existing cmd.rs:349 while_immutable_condition denial still blocks Clippy.
  The 164 existing song-library warnings remain, plus two style warnings in the
  unchanged moved compiler (`manual_range_patterns` in compile.rs and
  `chunks_exact_to_as_chunks` in decode.rs). There are no input-registry warnings.
  `input-lint-review.json` verifies the prior diagnostics after excluding those
  two blocks and accounting for Clippy moving its once-per-lint default-level
  note to the decoder's earlier occurrence. This is not a clean Clippy result;
  the library error prevents integration-test lint. Log SHA-256
  `0d0336da57deb51a992301fc1cff7ff1747eca276d8387f6e5341bcf7d0fa914`.
- `input-format.json`: all **15 changed/new Rust files** pass rustfmt.
- `input-final-review.json`, produced by `review_input.py`, checks the complete
  **1112-file watched set** against final test/lint source manifests, all Rust
  targets, log and format hashes, **16 unchanged native control files**, the
  compiler relocation proof and six frozen fixtures. All **240 interpreter/root
  entries** remain unchanged since the 949-test core run. The exact watched delta
  is thirteen files added, three changed, none removed and 1,096 unchanged since
  the screen UI stage. `input-edit-baseline.json` retains the pre-edit bytes.

The production song compiler remains legacy. Generated
screen/player actors, transition orchestration, compiler capture, existing baking
integration and full live orchestration still require migration. Unconditional
cutover, normal/build legacy dependency removal, final downstream/fixture suites,
normal application build and rendered smoke, and preparation/live/memory
comparisons remain required by the unchanged objective.

## SCREENMAN publication and screen request observations

The Rust song runtime now exposes `publish_screen` for an already constructed
screen and its two player actors. Publication uses the existing Rust stack API,
roots all supplied tables before global setters can reenter Lua, and preserves
the native write order and partial writes on errors. Existing SCREENMAN tables
and authored method replacements survive publication. GetPlayerAF is recreated
with the new pair; saved older getters retain their original pair without an
extra screen capture. This phase constructs no substitute gameplay actors.

SCREENMAN implements GetTopScreen, SystemMessage, SetNewScreen, AddNewScreenToTop
and set_input_redirected. GetTopScreen reads the loading/current-screen fields
raw. AddNewScreenToTop uses ordinary global lookup and Name assignment. Valid
SetNewScreen strings queue the latest target while leaving the current screen
object unchanged. Side-effect counting retains return arity, conversion and
failure order. Direct numeric conversion avoids internal tonumber calls visible
to debug hooks.

`begin_screen_request_capture` enables ordered source observations without
discarding already captured events. Capture reads and validates the public
runtime's beat before seconds, preserves f32-derived seconds, and retains the
selected runtime across reentrant getters and collection. Duplicate authored
requests remain observable. `take_screen_requests` drains observations once and
uses native pending-request priority: a new screen is consumed first, leaving an
earlier transition for the next drain. The transition priority test seeds the
existing pending queues on both backends; it does not stand in for the unfinished
StartTransitioningScreen/OffCommand/Out actor implementation.

Twelve differential tests cover the complete manager method set, receiver/player
conversions, manager identity, saved selectors, raw versus ordinary lookup,
loading truthiness, invalid manager replacement, request multiplicity and
precision, disabled tracing, partial failures, reentry, GC, hook counts, counter
numeric boundaries and independent runtimes. Native comparisons call the actual
production publication function and output drainer. The native live.rs prefix
of **519,372 bytes** is unchanged; its only addition is a **171-byte** accessor
behind both rust-engine and test-support. Pre-edit source and document bytes are
preserved in `screen-manager-edit-baseline.json`.

The initial focused run passed ten tests. Review then replaced number conversion
through internal callbacks with direct rooted stack conversion and added two
regressions. Those exposed rejection differences for nonfinite numbers and
out-of-range numeric strings; both were fixed in the adapter. The hook fixture
also initially attempted to use the sandbox's absent debug library and was
corrected to install hooks through the existing host APIs. The final focused run
passes all twelve tests. No failing case or assertion was removed or disabled.

Validation:

- `screen-manager-focused-v4-song-linux-debug`: **12 passed, zero failed** in
  118.418 seconds; sources unchanged. Log SHA-256
  `0521d4e849754209808f8716925d42fd9eec657d08957b48291a628209188229`.
- `screen-manager-adapters-song-linux-debug`: **398 passed, zero failed** across
  all **39 Rust adapter targets**, in 354.626 seconds; sources unchanged.
  Log SHA-256 `15bf002ddb9f0344374ca98174ef52219b29e4a02fc7d1915ff2578067df31a8`.
- `screen-manager-lint-song-linux-debug`: exit 101 in 26.408 seconds; sources
  unchanged. Its log is byte-identical to the input-stage baseline: the existing
  cmd.rs:349 while_immutable_condition denial and 166 warnings, including the two
  style warnings in the unchanged relocated compiler. There are no new screen
  manager diagnostics. This is not a clean Clippy result; the library error still
  prevents integration-test lint. Log SHA-256
  `0d0336da57deb51a992301fc1cff7ff1747eca276d8387f6e5341bcf7d0fa914`.
- `screen-manager-format.json`: five changed/new Rust files pass rustfmt; the
  appended native test accessor is formatted separately, preserving its original
  file prefix byte-for-byte.
- `screen-manager-final-review.json`, produced by `review_screen_manager.py`,
  verifies the complete **1114-file watched set**, final test/lint source hashes,
  all Rust targets, log/format hashes, **15 unchanged native controls** plus the
  unchanged live.rs prefix and exact accessor, and six frozen fixtures. All **240
  interpreter/root entries** remain unchanged since the 949-test core run. The
  exact watched delta is two files added, five changed, none removed and 1,107
  unchanged since the input stage.

Generated screen/player actors, transition execution, complete compiler capture,
existing baking integration and full live orchestration remain unfinished.
Production song execution is still legacy. The unconditional cutover and all
final application, dependency, fixture and performance gates in the saved
objective remain required; the migration goal is not complete.


## Generated player constructor and life-meter bindings

Continued the existing production migration scope with ScreenGameplay's Player
constructor phase. The private Rust factory creates a PlayerActor, its unnamed
AttackDisplay ActorFrame at X=-40, and its LifeMeter, preserving initial player
positions, names, lookup/sequence aliases, actor methods and OITG zoom behavior.
The new internal actor kinds do not expand authored Def construction. The factory
is not a replacement for the complete gameplay tree: Judgment, Combo,
HoldJudgments, NoteField and screen/theme assembly remain to be ported.

Life-meter queries retain their original meter. GetLife prefers published live
player state, while IsFailing/IsHot/IsInDanger retain the native local-field
semantics. SetLife uses the supplied actor with its originally captured player
index, updates live state and retained statistics, queues the live write, updates
the meter, then synchronously broadcasts LifeChanged. Earlier mutations survive
later conversion or callback failures. Its recursion guard clears after errors
and execution-limit interruption; the side-effect counter advances only after
the notification succeeds. Statistics retain their original player objects when
authored Lua replaces the public stage array. Saved player setters and zoom
getters do not retain a retired player; saved life queries retain only the meter
as their actor receiver. No locks are held across authored callbacks.

Seven focused tests cover five native differential controls (construction,
receiver/class overrides, current/local life queries, shared state and write
ordering, partial failures, reentry and GC), plus Rust scene-release ownership
and fatal execution-limit recovery. The native control calls the unchanged
production player and meter constructors. Its only native-source addition is a
643-byte test accessor after the unchanged 85,757-byte compile.rs prefix, gated
by both rust-engine and test-support. Pre-edit bytes are recorded in
`generated-players-edit-baseline.json` and the documentation baseline.

The first build exposed two local Rust compilation errors, which were fixed.
The first runtime fixture incorrectly assumed the default style was
`dance-single`; the actual constructor uses `single`. A subsequent fixture exposed
that native typed boolean conversion follows Lua truthiness. The fixture and
Rust zoom/guard conversion were corrected to match the native implementation,
with strings, numbers, tables and false explicitly covered. The seven-test
focused run then passed. Ownership review removed an unnecessary actor capture
from saved player methods and strengthened the release test before the final
full run. No existing coverage was deleted, weakened or skipped.

Validation against the final source:

- `generated-players-adapters-song-linux-debug`: **405 passed, zero failed**
  across all **40 Rust adapter targets**, in 459.195 seconds;
  sources unchanged. Log SHA-256
  `0b3e80a5e6044f5e0a3ee42a9ed4a3e0fa3d3caf22dc6c540ee390d9a77da385`.
- `generated-players-lint-song-linux-debug`: exit 101 in 28.794 seconds;
  sources unchanged. Library diagnostics are byte-identical to the SCREENMAN
  baseline: the existing cmd.rs:349 while_immutable_condition denial and 166
  warnings. No new player-binding diagnostics appeared. This remains an
  unsuccessful Clippy run, and the library failure prevents integration-test
  lint. Log SHA-256 `5cf6ad2ed396c26a3a6c6c49f84eb673ad4ee155305abaebd800890d9edb9ae0`.
- `generated-players-format.json`: all five changed/new Rust adapter/test files
  pass rustfmt; the native accessor is checked separately, preserving the
  original native source prefix.
- `generated-players-final-review.json`, produced by
  `review_generated_players.py`, verifies the complete **1116-file** watched
  source set against the final test and lint hashes, target coverage, log/format
  hashes, **15 unchanged native controls** plus the original compile.rs prefix,
  and six frozen fixtures. All **240 interpreter/root entries** remain unchanged
  since the 949-test core run. The watched delta is two added, six changed,
  none removed and 1,108 unchanged files since the SCREENMAN stage.

The complete screen tree and transitions, compiler capture, existing baking
integration and full live orchestration remain unfinished. Production song
execution is still legacy. Unconditional cutover and all final application,
dependency, fixture and performance gates in the saved objective remain required.
This is progress within that objective; the migration goal is not complete.


## Generated NoteField, columns and spline bindings

Continued the fixed gameplay-actor migration scope with the existing NoteField
constructor. The private Rust factory creates the bundled blank Board Sprite and
eagerly constructs every style column, with independent position, rotation and
zoom handlers. Named/sequence Board aliases, parent identities, style metadata,
handler aliases and the original spline retained by GetSpline are preserved.
Preparation reads the constructor's style offsets; live construction retains the
already-registered zero pose until native-style updates. Public NoteField
inherits ActorFrame and has the same forwarding inventory, including the same
errors for entries whose existing native implementation is absent. No new
StepMania features or authored Def classes were introduced.

The spline API uses the unchanged shared gameplay solver. SetPoint copies,
pads/truncates and validates authored vectors; SetSize retains implicit zero
knots and truncates stored points/coefficients. Solve publishes the existing
coefficient shape, while evaluation preserves native f32 arithmetic, negative
fractions, non-looping clamping and loop wrapping. Numeric coercion for spline
geometry remains distinct from the handler setters' existing read_f32 policy.
Ordinary field getters/setters may reenter and collect while their selected
tables remain rooted.

Tap callbacks preserve argument conversion, current-field publication before
callbacks, synchronous reentry, ignored return values, partial failures and
side-effect timing. Column update plans run the column vector after the field
callback with its incoming delta, outside the field's hibernation and update-rate
gates. Replacing the vector invalidates the cached traversal. Retained handler
and spline queries keep only their selected Lua objects. Explicit scene release
retires column actors; a saved field query cannot create fresh actors in that
released scene and reports an error instead of accessing removed Rust ownership.
This does not replace the still-pending screen-transition lifetime orchestration.

Eleven tests cover nine native differential controls (construction across styles
and clock modes, public/bound receiver behavior, handler conversion, cubic
geometry, resizing and failures, getter/setter reentry and GC, style regeneration,
tap callbacks and update ordering), plus Rust retained-query ownership and
released-scene failure handling. The initial build caught local Option/Result
conversion errors. Runtime comparisons then exposed the preparation/live column
pose distinction, an incorrect fixture assumption that public forwarding methods
accepted detached calls, and the different native policies for numeric infinity
and the string `"inf"` in tap-column conversion. The adapters and fixture were
corrected against the actual native behavior. No existing test was removed,
weakened or skipped.

Validation:

- `notefield-focused-v4-song-linux-debug`: **11 passed, zero failed** in
  103.109 seconds; sources unchanged. Log SHA-256
  `369cc688cfc0d3ee7d4f85f794e169a9de7b2a8c4270cd220ed091f700c2fabd`.
- `notefield-adapters-song-linux-debug`: **416 passed, zero failed** across all
  **41 Rust adapter targets**, in 333.807 seconds; sources unchanged.
  Log SHA-256 `96ca74c5907a1d8096c71e188ac59ea6ba5d25f885516fa03af415478e783dbc`.
- `notefield-lint-song-linux-debug`: exit 101 in 115.117 seconds, including
  Cargo lock waiting; sources unchanged. Its library diagnostics are byte-identical
  to the generated-player baseline: the existing cmd.rs:349
  while_immutable_condition denial and 166 warnings. There are no new NoteField
  diagnostics. Clippy is still unsuccessful; its library failure prevents the
  integration-test lint. Log SHA-256
  `d1bf2c3fdaa3fa0a8ac919693e9024fafceab40d4710cfa45a8d756a969dd3fd`.
- `notefield-format.json`: seven changed/new Rust adapter/test files pass rustfmt.
  The native test accessor is checked separately; the prior **86,400-byte**
  compile.rs prefix is unchanged and only **258 bytes** were appended, behind
  both rust-engine and test-support. Pre-edit bytes are retained in
  `notefield-edit-baseline.json`.
- `notefield-final-review.json`, produced by `review_notefield.py`, verifies the
  complete **1119-file** watched source set against final test/lint hashes,
  target coverage, log/format hashes, **15 unchanged native controls** plus the
  compile.rs prefix/accessor, and six frozen fixtures. All **240 interpreter/root
  entries** remain unchanged since the 949-test core run. The exact watched delta
  is three added, seven changed, none removed and 1,109 unchanged files since the
  generated-player stage.

Judgment, Combo, HoldJudgments, remaining generated theme/screen construction,
transitions, compiler capture, existing baking integration and full live host
orchestration remain unfinished. NoteField construction and bindings are available
for that assembly; compiler spline-track extraction and the renderer-thread host
callback path still belong to the pending full compiler/live integration.
Production song execution is still legacy. The unconditional cutover and final
application, dependency, fixture and performance gates in the saved objective are
still required. The migration goal is not complete.


## Generated player Init assembly

The saved production-migration objective was reread before this continuation.
The immediately preceding reply restated scope and made no goal progress; the
current-state NoteField audit was rerun successfully before source edits. It
verified the previous 416-test source set and the closed interpreter baseline.
The migration finish line remains unchanged.

`actors/player_parts.rs` attaches the existing Player Init children to the
player's owned scene: Combo, Judgment, unnamed per-column HoldJudgment frames,
then NoteField. The previously constructed unnamed AttackDisplay stays first.
Each ordinary child has the same sequence, named lookup and parent identity;
the unnamed group forwards to its current last hold frame. NoteField uses the
already verified Board/column/spline constructor in this same scene. Internal
HoldJudgment inherits ActorFrame without opening additional authored Def classes.
Generated actors track native child registries, retaining original named aliases
when their children are renamed.

Combo calls the current GetComboFonts and ordinary SL modifier lookups, preserving
the invisible Actor fallback for hidden/empty-font cases. Otherwise it creates
the existing frame and BitmapText Number. Judgment owns the hidden,
non-animating JudgmentWithOffsets sprite and optional context texture. Hold
judgments retain raw modifier reads, the single THEME:GetPathG call per player,
string conversion/default label, path trimming, per-column sprites and initial
alpha through the current diffusealpha method. Lua callbacks and intermediate
tables stay rooted through reentry and GC. A failed partial construction releases
the whole new scene and allows subsequent construction.

The seven new tests compare the actual legacy constructors across five styles,
both clock modes and both players; cover hidden/empty-font Combo, texture lookup
with GC, duplicate child groups and renamed aliases, and the real legacy frame
runner's callback/tween order. Two Rust ownership checks cover partial-failure
cleanup and release of the full player tree while a retained spline and the
other player's scene remain usable. Native controls expose only cfg-gated test
accessors; their original byte prefixes are preserved in the edit baseline.

The first focused run had five passes and one fixture failure: the test used the
legacy callback-only traversal instead of its frame/tween runner. The corrected
second run passes all seven tests. Neither focused run is final-source evidence:
concurrent work changed nine deadsync-notefield files and added 21 separate
notefield performance sources during the first run, then changed select_music.rs
during the second. Their hashes are recorded in `player-parts-concurrent-delta.json`,
`player-parts-concurrent-added.json` and `player-parts-concurrent-v2-delta.json`;
those changes and additions were preserved. No migration adapter source changed during either
run. The complete adapter and lint runs below validate the resulting worktree.

Validation:

- `player-parts-focused-song-linux-debug`: five passes, one fixture failure in
  126.80 seconds; source guard false. The callback-only legacy runner did not
  advance tweens. Log SHA-256
  `9690ccb57e2b32a786cfcc4498fe27a4a24908a751f96672b27dc792379d2df6`.
- `player-parts-focused-v2-song-linux-debug`: seven passes, no failures in
  123.70 seconds; source guard false due to the separate music-selection edit.
  Log SHA-256 `8977cbf5c5b95f1aa46416e425c6edb32ee30139bb7d035eb684a46ab68d000b`.
- `player-parts-adapters-song-linux-debug`: **423 tests passed across all 42
  Rust adapter targets**, no failures, in 356.95 seconds. Cargo exited zero,
  but the wrapper exited one because the broad source guard observed another
  select_music.rs edit. Log SHA-256
  `d5395b050eabb1fc33bb0dc8d41bc994bebc8980f389224005cd8272ef256171`.
  `player-parts-concurrent-adapters-delta.json` records that exact change.
  `player-parts-adapter-dependencies.json` and its full cargo-tree output prove
  deadsync-theme-simply-love is absent from this target's normal, build and dev
  dependency graph. Every other watched source, including all changed migration
  adapters, matches the test/lint source snapshots. This supports the adapter
  result; it does **not** turn the broad source guard green or establish final
  application validation. Neither the wrapper nor its result was altered.
- `player-parts-lint-song-linux-debug`: exit 101 in 160.29 seconds including
  Cargo lock wait, with a stable source set. The song-library diagnostics from
  its first warning onward are byte-identical to the NoteField-stage baseline:
  the existing cmd.rs:349 while_immutable_condition denial and 166 warnings.
  A dependency warning and Checking lines were interleaved before that first
  library diagnostic; `player-parts-lint-review.json` records the comparison
  boundaries. No new library diagnostics. Integration-test lint remains prevented
  by the library error. Log SHA-256
  `c94c5470f3937bd0c75a0d3c0efe253f37ae959ac0fa94162bee29c71883dafb`.
- `player-parts-format.json`: all six changed/new Rust adapter/test files pass
  rustfmt. The two native test accessors are separately formatted; compile.rs's
  original **86,658-byte** prefix and lua_util.rs's **983,823-byte** prefix are
  unchanged, with only 486 and 525 cfg-gated test bytes appended respectively.
- `player-parts-final-review.json`, produced by `review_player_parts.py`, verifies
  all **1,142 watched paths**, the adapter source/target coverage, unchanged
  interpreter/root baseline (**240 entries**), 14 unchanged native controls plus
  the two original prefixes, six frozen fixtures, log and formatting hashes.
  It explicitly records the broad source mismatch and the changed nondependency.
  The migration-owned delta is two new files and eight changed files. The remaining
  21 additions and ten changes belong to concurrent work and are preserved;
  1,101 watched paths are unchanged from the prior stage. This is scoped adapter
  evidence, with `goal_complete` false.


The screen's later theme commands and combo milestones, remaining generated
screen/theme construction and transitions, compiler capture, existing baking
and full live host orchestration remain unfinished. Production song execution
is still legacy. The unconditional cutover and final application, dependency,
fixture and performance gates in the saved objective remain required. This
player assembly is progress toward that finish line, not completion of the goal.


## Generated screen display children

The saved objective was reread before this continuation. The preceding turn was
progress: it implemented player Init assembly and produced 423 passing adapter
tests. Current-source inspection found only the already recorded select_music.rs
change relative to that run; no validation process was still running. This turn
continues the same production migration without reopening the interpreter or C
API milestones.

`actors/screen_parts.rs` provides private screen-construction phases for Timer,
ScoreP1/P2, LyricDisplay, SongBackground, Debug, existing simple engine labels,
OniGameOver sprites and the empty Overlay/Header/Footer/SongForeground frames.
These phases create actual actors in an owned scene. Score frames retain their
named-only Sprite/PercentageDisplay/BitmapText tree and initial percentage text;
queries, update traversal and scene release include those children. Timer's
GetSeconds retains the existing ignored-argument zero result and open class
lookup. PercentageDisplay and Timer remain internal actor kinds.

LyricDisplay owns the two real BitmapText layers, Common Normal font and Changed
commands. Its initial position reads the current options of enabled players;
all-, one- and no-reverse cases retain their native command functions. The
existing inherited Actor:LyricCommand helper was copied verbatim from the native
actor class source, including current Var lookups, text/color/width handling,
shadow/stroke settings, crop direction and tween timings. Cached constructor
factories still create fresh commands for each display.

SongBackground creates its implementation frame, four border quads, and three
brightness overlays with the native 40-pixel center band. Screen dimensions and
the current BGBrightness getter preserve f32 conversion and initialization
order, including GC during preference callbacks. Its frame lookups honor the
native raw __songlua_owned_child_lookup policy: named snapshots come from the
ordered actor array. Ordinary generated score registries retain their aliases.
The existing direct-child traversal remains unchanged.

Seven new tests exercise both preparation/live constructor modes; timer/class
lookup and label defaults; both named-only score trees; lyric positioning and
retained Changed handlers; real lyric tween updates; background geometry,
duplicate groups, renames and owned lookups; and named-child updates/GC/release.
The first compile caught an incorrect use of HostCall::number as a Value
conversion; the adapter now uses the existing rooted numeric conversion helper.
The next run passed four tests and failed two fixture assumptions in the legacy
control: an exposed texture getter can return nil, and texture image dimensions
need not equal the source PNG dimensions. After correcting those assumptions,
429 tests passed. Review then found that the new nil guard also accepted an
absent getter. A separate method-availability probe reproduced two actual Rust
adapter failures: Timer and PercentageDisplay lacked the native GetTexture
fallback. That failing run is preserved in screen-parts-texture-probe-song-linux-debug.

The actor constructor now installs the existing texture query as an owner-bound
method, retaining the public class gates. Internal kinds expose the query through
their existing fallback. It requires a table receiver but reads the captured
actor, honors current class overrides, creates fresh metadata handles and keeps
retained handles stable across field changes and GC. The extra differential test
checks those behaviors for Timer and both percentage displays; existing scene
release coverage also includes the new captured closures. This is a song-host
binding fix; the interpreter and the native control implementations are unchanged.

Final verification is recorded in `screen-parts-final-review.json`:

- `screen-parts-texture-fix-song-linux-debug`: all seven focused tests passed
  after the two failing method-availability observations were fixed.
- `screen-parts-verified-adapters-song-linux-debug`: **430 tests passed across
  43 Rust adapter targets**, zero failed, in 378.401 seconds. The
  test log SHA-256 is `18f6e3c817b921d38b9996979aeb31300137cb96f6ccd148268037931c4b9f73`.
- `screen-parts-verified-lint-song-linux-debug`: Clippy stopped on the unchanged
  `cmd.rs:349` loop-condition error, with the same 166 library warnings. The
  library diagnostic body is byte-identical to the preceding player stage;
  test linting remains blocked by that library error. This is not a clean
  Clippy result. The run took 142.470 seconds including lock waiting.
- Formatting passed for all five changed/new Rust adapter files, and the
  appended test-only native accessor is separately rustfmt-checked. Its
  original 87144 byte production prefix is unchanged.
- All 1144 watched source keys and hashes match both final runs
  and the focused fix run. Both final wrappers report unchanged sources.
  The stage adds two source files and changes seven; the other
  1135 watched files match the previous stage after its
  already recorded concurrent edit is accounted for.
- All 240 interpreter/root entries, the
  15 native control files and six frozen fixtures are unchanged.
  The inherited lyric helper matches the native source verbatim.

The earlier 429-test results, initial fixture failures, two-failure regression
probe and pre-edit source snapshots are retained. No failure or coverage was
removed to obtain the passing final run.

StepsDisplay construction/setters, transition actors, the loaded underlay and
overlay theme trees and complete screen ownership/initialization still require
migration. Compiler capture, existing baking/live orchestration and the
unconditional production cutover remain unfinished. Final application rendering,
dependency/link proof, fixture suites and performance measurements remain the
same required finish-line gates. This display-construction phase does not make
the production song path use the Rust engine yet.


## StepsDisplay construction and gameplay initialization

The saved objective was reread and the preceding screen display audit rerun
against the current worktree before edits. That audit passed: the prior turn
made concrete progress with the texture-query fix and 430 passing adapter tests.
The interpreter and C API milestones remain closed; the production migration's
acceptance criteria are unchanged.

The private StepsDisplay constructor now owns its four actual children in the
same scene: blank Frame/Autogen sprites and Meter/Description bitmap text with
Common Normal fonts. Owned lookups follow the ordered child array, while the
setters preserve the native registry's original names. The internal class keeps
ActorFrame inheritance and the existing bound-method fallback.

SetFromSteps, SetFromTrail and SetFromGameState retain their supplied receiver,
current getter lookup, typed table/numeric conversion, optional values and
side-effect timing. SetInternal preserves difficulty mappings, edit descriptions,
parameter identity, immediate display/autogen visibility and both child command
passes. It captures the selected registry across the first pass and enumerates
the current direct children for the second. Temporary values, parameters and
child snapshots remain rooted across GC and callback reentry. Plain callback
tables accepted by the native methods retain a command guard without being
indexed as owned Rust actors.

The separate gameplay initialization phase uses the native metric command bodies
verbatim. It preserves player order, direct position writes, per-child On commands,
PlayerState Load parameters, frame flipping, initial hiding and current-steps
selection. Commands are fresh per initialization and shared by both players as
in the native path. Full screen assembly remains responsible for its existing
preferred-difficulty and hibernation fields.

The first focused run passed four tests and failed two. One test used the
nonexistent SetPropagateCommands name; the fixture now invokes the real
propagate method. The other found missing PLAYER_1/PLAYER_2 globals in the Rust
song host, causing the native frame-flip helper to treat both players as P2.
The manager setup now publishes the same player constants as install_basic_globals;
the tests assert those values explicitly. No interpreter code was changed.

Eight differential tests cover constructors/owned children, class overrides,
steps/trail mappings and ordered callback passes, conversion failures and partial
writes, reentrant visibility, current GAMESTATE selection, real gameplay metrics,
plain receivers, retained parameters/removed children across GC, tween updates
and explicit scene cleanup. Both context clock modes are covered where applicable;
these actor controls do not replace the still-required full compiler capture and
baking/live validation.

The initial broad regression run was explicitly stopped after 91 passing tests
when review found that a removed Autogen table was not rooted through a later
collecting IsAutogen callback. That run has exit -15, unchanged sources, and a
separate cancellation note; it is not treated as a completed or passing suite.
The focused GC probe passed on the native control and failed on the Rust adapter.
The corrected setter retains all four selected children until SetInternal returns,
matching the native handles even if callbacks remove every registry entry. The
regression now checks later child callbacks, IsAutogen and the parent Set callback,
then verifies collection after the setter returns. Direct-message traversal also
roots its current receiver and its entire child snapshot. The new unnecessary-
unwrap Clippy warning found in the first lint run was corrected.

Final verification is recorded in `steps-display-final-review.json`:

- `steps-display-focused-v4-song-linux-debug`: all eight tests passed on the
  final sources in 99.983 seconds, including the expanded removed-child
  GC regression. Earlier failures and the cancelled broad run are retained.
- `steps-display-final-adapters-song-linux-debug`: **438 tests passed across
  all 44 Rust adapter targets**, zero failed, in 363.119 seconds.
  Log SHA-256: `0c2a1004a53de5f8878afcc084e70d7ef64dfda24363ddb26e3983eb06eb21f6`.
- `steps-display-final-lint-song-linux-debug`: the new warning is gone. The
  library diagnostics match the preceding screen-display stage byte-for-byte:
  166 existing warnings and the existing `cmd.rs:349` loop-condition error.
  Clippy exits 101 and therefore still blocks test linting; it is not clean.
- Formatting passes for all six changed/new Rust adapter files. The appended
  cfg-gated native test accessor is separately formatted; the original
  984348 bytes of lua_util.rs remain unchanged.
- All 1146 watched source keys and hashes match the final full test,
  focused test and lint snapshots. All three wrappers report unchanged sources.
  The stage adds two source files and changes seven, leaving
  1137 watched files unchanged from the preceding stage.
- All 240 interpreter/root entries,
  15 native control files and six frozen fixtures are unchanged.
  The native metric command table is reproduced verbatim in the Rust host's
  cached Lua factory. No interpreter or Lua C API scope was reopened.

Transition actors, loaded underlay/overlay theme trees, full screen construction,
compiler capture and existing baking/live orchestration, unconditional cutover,
and final normal application/rendering/dependency/fixture/performance checks
remain required by the original saved objective. Production song execution is
still legacy; this phase does not complete the migration goal.


## 2026-11-05 ? In/Out/Cancel migration progress, preparation gate still failing

The saved migration objective was reread before work. The previous conversational
reply had conflated the already closed interpreter milestone with this separately
authorized migration. Neither completed milestone was reopened. The preceding
StepsDisplay review was rerun against current sources and passed before edits.
This turn made implementation progress and identified a real preparation defect;
it is neither a completion claim nor a blocked/no-progress turn.

The private Rust screen factory now constructs In, Out and Cancel with their
actual owned children, animation queues, bitmap resources and retained commands.
The virtual tween-time query uses the transition child, ignores the owner's
queue, and returns zero outside the active state. Starting dispatches the child's
StartTransitioning tree once and then changes state. The frame plan advances the
owner's queue first, gates children and its update callback while dormant or
finished, and tests completion before the final child update. Hibernation and
update rate retain their existing order. Detached plain callback trees retain
their complete child snapshot through GC; the existing StepsDisplay guard and
snapshot helpers are now shared through native_children.

The missing Center/CenterX/CenterY/FullScreen methods were added with the existing
current-global reads, typed dimension conversion and eager fallback validation.
Existing ivalues, ToEnumShortString/pname and Color helpers are now installed for
transition commands. Color retains its 21 constants, callable lookup and Alpha
behavior. The three native Lua command/stage-label bodies are copied verbatim;
all 57 bundled bitmap key/filename/source records and texture preparation policy
are retained. The legacy implementations and comparison fixtures are unchanged.

**The preparation gate fails and remains enabled.** Constructing In before
replay registration in non-live mode drains the queued Hide callback inside the
legacy capture scope. That changes observed visibility and
SL.Global.GameplayReloadCheck while capturing the future effects. The Rust
constructor currently retains a replay command instead, leaving these values
unchanged. Production creates the screen before startup/replay setup, so this is
an implementation gap, not a fixture-only discrepancy. Its evidence and required
next step are in `transitions-preparation-findings.json`. Fix constructor capture
and the subsequent replay handoff; do not merely set visibility early, waive the
comparison, or substitute live behavior for preparation. Full preparation/capture
and baking validation remains required independently of the passing actor tests.

Validation, audited by `review_transitions.py` / `transitions-review.json`:

- Focused v5: **11 passed, 1 failed**, 106.743 seconds. The passing cases cover
  live constructors, incoming styles/spooky/reload/stage labels, Init/On/Off,
  outgoing player and routine white-score writes, current geometry globals,
  helper conversions, repeated starts, owner/child timing, hibernation/rate,
  callback mutation/GC and explicit scene cleanup. The preparation constructor
  comparison is the sole failure. It also compares the shared reload flag.
- Full adapter run: **449 passed, 1 failed across all 45 targets**, 418.999
  seconds, exit 101. `checks.py` gained an optional `--no-fail-fast` argument so
  the remaining targets execute after the known failure; no coverage is skipped.
  All 438 prior adapter tests still pass. The 12 transition tests include the
  same one failing preparation test.
  Log SHA-256: `017f098a8a6698d55e3b80e9288886b2f540384f3f875c94894bf3da70cbcd55`.
- Clippy: exit 101, 31.155 seconds. The song-library diagnostic messages and
  files match the preceding stage: 166 existing warnings and the existing
  `cmd.rs:349` error. Test lint remains blocked. No new library diagnostics.
- Formatting passes for all 12 changed/new Rust files. All 1150 watched source
  keys/hashes match focused, full regression and lint snapshots, and each wrapper
  confirms unchanged sources. Four sources were added and nine changed;
  1137 watched files are unchanged from the preceding stage.
- All 240 interpreter/root entries, 16 native controls and six frozen fixtures
  are unchanged. `transitions-edit-baseline.json` preserves pre-edit bytes,
  including the validation helper and active documentation. Earlier focused
  failures remain recorded. Those exposed missing geometry/helper bindings as
  well as fixture errors (initial fullscreen position, texture-method
  availability, manager setup and engine-specific error text); correcting those
  fixtures did not remove the real preparation failure.

This stage and the full migration are **incomplete**. Required work still includes
the preparation capture defect, the five unnamed gameplay transition roles,
loaded theme trees/full screen construction, compiler capture, existing baking
and live orchestration, unconditional cutover, and final application build,
rendered smoke, dependency/link, fixture and performance validation. Song
production remains legacy. The original finish line is unchanged.


## 2026-11-05 ? Constructor capture regression resolved

The saved migration objective was reread and the preceding transition review was
rerun against current sources before editing. That review still showed 449
passing tests and the retained failing preparation constructor comparison.
The interpreter and C API removal milestones remain closed. This turn made
implementation progress toward the separately authorized migration; it does not
change the original acceptance scope or claim production cutover.

Generated screen constructors now create preparation capture state in the song
host. Sparse typed command blocks retain explicit writes, timestamps, durations,
easing and queued scopes. Empty tween states still advance capture time. Pending
commands dispatch after the active command body and keep ancestor guards through
nested drains. The dispatch target and direct-child snapshots remain rooted
through callbacks and collection. Scopes and guards are restored after dispatch.
The In constructor's queued Hide now updates visibility and shared reload state
at the original phase while preserving its captured future visibility write.

Replay promotion clears constructor capture/queues and does not execute their
already dispatched Lua callbacks again. Native non-live position/aux getters
retain their captured current values until synchronization, while the replay
owns the captured destination pose. This distinction was found by the new
handoff comparison and fixed in the adapter.

The original preparation regression is still enabled and now passes. Two new
differential tests compare every typed block for all In/Out/Cancel constructor
actors, and exercise nested queue order, shared globals, forced GC and promotion
without duplicate callback execution. The constructor action hook runs inside
the existing legacy setup closure, before its Init barrier. The initial test had
incorrectly queued these actions after that barrier, where native dispatch is
deferred; moving the actions to the actual constructor phase retained every
behavioral assertion. The old control implementation was not edited. Compile
errors in the first test helper versions and all subsequent failing runs remain
recorded. See `preparation-capture-findings.json` for those distinctions.

Final validation, audited by `review_preparation_capture.py` and
`preparation-capture-review.json`:

- Full adapter suite: **452 passed, 0 failed across all 45 targets**, 486.977
  seconds, exit 0. The command retained `--no-fail-fast`.
  Log SHA-256: `bfa2b140bcbf47b61de53fb01443fe5410ed361b5754f328b0cdb78c6e0a5f86`.
- Final focused v6: **14 passed, 0 failed**, 206.236 seconds, exit 0.
  Log SHA-256: `2ee8822cf2bd3b52af718a1684faffe7e4f8cbcca34f1c19c9dc1c2b8803aeeb`.
- Final Clippy v2: exit 101, 214.200 seconds. The 166 warnings and existing
  `cmd.rs:349` error have exactly the preceding diagnostic messages and source
  files; no new library diagnostics remain. Test lint is still blocked by that
  existing library error. The two new collapsible-if warnings from the initial
  lint run were fixed before the final runs.
- These elapsed times include compilation and, for focused/lint runs, waiting
  for Cargo's build lock. They are validation durations, not workload performance
  measurements.
- Formatting passes for all seven changed/new Rust files. All 1151 watched
  source hashes match the final focused, full and lint snapshots; every wrapper
  confirms unchanged sources. One source was added and six changed; 1144
  watched files are unchanged from the previous transition stage.
- All 240 interpreter/root entries, 16 native controls and six frozen fixtures
  are unchanged. `preparation-capture-edit-baseline.json` preserves pre-edit
  source/document bytes. Historical transition failure reports are retained;
  active architecture documentation describes the resolved constructor defect.

**The production migration remains incomplete.** Constructor capture is now
verified for these adapters. The complete compiler Init/startup barrier, frame
clocks, remaining capture channels, stop/finish/hurry and recurring scheduling,
existing baking/live orchestration, remaining screen/transition/theme trees,
unconditional cutover and final application/rendering/dependency/fixture and
performance checks are still required. Production song execution still uses the
legacy backend. No completion criterion has been added, waived or removed.


## 2026-11-05 - Constructor cancellation verified

Constructor capture now preserves Stop/Finish/Hurry behavior: retained direct
blocks, merged finish output, discarded pending callbacks, scaled capture clocks,
current-position getters and native queue-overflow handling. Spin initialization
keeps its rotation queue separate from raw destination rotation. Hibernate records
its immediate delta and clears the theme sleep flag with raw assignment.

Eight new differential tests retain the previous fourteen transition/capture
tests. They cover cancellation, zero/tiny durations, queue boundaries, self-queue
handling, parent/child behavior, detached immediate-block caches and spin getter
ordering. Constructor controls remove CompileFrames before construction to match
the production phase; native production controls remain unchanged. The initial
16-pass/3-failure run exposed missing hibernation capture and incorrect rotation
restoration without a spin queue. Those failures were corrected and retained in
the evidence. Full effect capture and compiler scheduling remain pending.

Final validation is recorded in `capture-cancellation-review.json`, generated by
`review_capture_cancellation.py`:

- All **460 adapter tests pass across 45 targets** (517.484 seconds, exit 0).
  Log SHA-256: `77f2355a46ca86308b7de933ad6edcf3ace868e270908b5edb3aba48a0ff4ce8`.
- All **22 focused tests pass** (234.050 seconds, exit 0).
  Log SHA-256: `d21f1724ad69b240b578a80b96937fe45bd0be2f19a68c42c4ee7f034aee2524`.
- Formatting passes for six changed Rust files. Clippy retains exactly the
  existing 166 warnings and `cmd.rs:349` error (227.054 seconds, exit 101).
  Its log hash is `047d0ffecdcbbfbd980aecba87e667cdaf3b4e5634af093d6486cad2e944c654`.
  Test lint remains blocked by that existing library error.
- Validation durations include compilation and Cargo lock waits; they are not
  workload performance measurements. All 1151 watched source hashes match the
  focused, full and lint snapshots. Six files changed, none were added or removed,
  and 1145 remain unchanged from the preceding stage.
- All 240 interpreter/root entries, 16 native controls and six frozen fixtures
  remain unchanged. Pre-edit source/document bytes are retained in
  `capture-cancellation-edit-baseline.json`.

The interpreter and C API removal milestones stay closed. **The separately
authorized production migration remains incomplete**, with its original criteria
unchanged: remaining compiler/startup/frame/capture and baking/live integration,
remaining screen trees, unconditional cutover and final application/rendering,
fixture, dependency/link and workload measurements. Production song execution
still uses the legacy backend. This verification completes the cancellation fix;
it does not declare the migration complete or authorize a further goal.


## 2026-11-05 - Authored preparation startup boundary verified

The saved objective and prior cancellation review were revalidated before edits.
The previous goal turn completed verification and recorded authoritative evidence;
it was progress. The interpreter and C API removal milestones remain closed.

The Rust song host now has an explicit preparation phase for authored declarations.
Root Init installs a queue barrier shared across subsequent roots and On. The
compiler can copy owned typed command blocks, block-step indices, pending tween
steps and command start times before releasing that barrier after Update(0).
Discovery runs eligible zero-time commands while retaining delayed commands and
queued messages for chronological frame replay. Released scenes are removed from
pending startup owners; callback errors restore command guards and discovery
scope. Startup visibility is immediate, and Stop/Finish clear deferred state.

Six new tests compare the actual production constructor/startup routines with the
adapter. They cover Init/Begin/On/Update(0)/queue ordering, nested commands, shared
state and GC; complete typed Init capture and pending queue metadata; delayed
commands and zero-time messages; exact callback errors and recovery; cancellation
of deferred queues; and scene retirement before collection. No existing test or
native production implementation was changed. The isolated control installs the
real game-state and preference managers and supplies a loader frame for screen
options. It exercises the production startup routine without claiming to cover
full ScreenGameplay construction or activation.

The initial full-block comparison exposed a real visibility timing mismatch,
which was corrected. Early startup comparisons also lacked required control
setup globals, including PREFSMAN for the zero-delta option update. Those setup
failures and all subsequent runs remain recorded in
`preparation-startup-findings.json`. The error test now requires its intended
callback failure so a setup error cannot satisfy it.

Final validation is audited by `review_preparation_startup.py` and recorded in
`preparation-startup-review.json`:

- **466 tests pass across all 46 adapter targets**, 458.006 seconds, exit 0.
  Log SHA-256: `14fb21a7d6da16d2cccde536c3c9d541892bd0ae941fb4f895ae25d004606663`.
- **Six focused tests pass**, 110.053 seconds, exit 0.
  Log SHA-256: `4476ac3a3bbde52ea1a613f6c3081823ba760e8b784422a1c483fe864708c8dc`.
- Formatting passes for five changed/new Rust files. Clippy reports the same
  166 warnings and existing `cmd.rs:349` error, with identical diagnostic messages
  and source files (27.368 seconds, exit 101). Test lint remains blocked by that
  existing library error.
  Log SHA-256: `0d0336da57deb51a992301fc1cff7ff1747eca276d8387f6e5341bcf7d0fa914`.
- These times include compilation and Cargo lock waits, not workload benchmarks.
  All 1152 watched source hashes match the focused, full and lint snapshots;
  every wrapper confirms unchanged sources. One test file was added, five
  watched files changed, and 1146 are unchanged from the previous stage.
- All 240 interpreter/root entries, 16 native controls and six frozen fixtures
  remain unchanged. Pre-edit bytes are retained in
  `preparation-startup-edit-baseline.json`.

**The production migration remains incomplete.** The existing compiler collects
initial state and assigns foreground parents before Init; the Rust convenience
constructor still combines construction and Init. Full startup screen activation,
shared-state snapshots/rewinding, chronological frame scheduling, remaining
capture channels, existing baking/live integration, remaining screen trees and
unconditional cutover remain required. Final application/rendering, fixture,
dependency/link and workload measurements are also outstanding. Production song
execution still uses legacy. The original finish line is unchanged.


## 2026-11-05 - Separate live construction and Init verified

The Rust song host now exposes graph construction, external parent assignment,
Init and foreground attachment as separate operations. This preserves the live
compiler's construct/parent/initial-state/Init/append order while retaining the
existing construct-and-initialize convenience method. Ordinary declaration
subscriptions and child parent links wait for Init; generated native actors keep
their constructor behavior. Names are observable before Init, then follow the
existing child-first loading-name rules. Top-level Condition is ignored, as in
MakeActor; child Conditions still exclude children.

Foreground attachment appends once, shares subtree and wrapper ownership, and
invalidates traced traversal roots. Scene-local snapshots tolerate an external
parent. Init errors retain owned partial state; retry skips finished children.
Uninitialized and attached scene cleanup is checked across collection, including
wrappers created before and after attachment.

Four new tests cover live construction, complete initial/final render states,
registration timing and native message identity order, foreground parent/child
visibility, Init errors/retry, foreign/cyclic parents and GC cleanup. The existing
six offline capture/startup tests remain. One old actor assertion incorrectly
expected a false root Condition to remove the root; it now requires the actual
production behavior with an added differential assertion.

The controls exposed an important existing mode distinction: offline Def creates
instances while scripts execute; live Def decorates declarations for later
materialization. The new construction identity comparison therefore uses the
live control. Preserving offline constructor identity, method availability and
timing in the Rust compiler remains required; these tests do not prove that work
complete. Message-order comparisons import allocator identities using the same
mechanism as the established message tests and separately check registration.

The first full suite found two regressions: raw CourseContentsList display names
were coerced too early, and SongMeter's explicit constructor parent field was not
reflected in the new deferred Rust metadata. Both were fixed in the host by
reusing raw display name handling and the existing adoption helper. Both failing
test files are unchanged. Earlier compile errors, control corrections and the
initial 468-pass/2-fail run remain recorded in `construction-init-findings.json`.

Final validation is audited by `review_construction_init.py` and recorded in
`construction-init-review.json`:

- **470 tests pass across all 46 adapter targets**, 473.586 seconds, exit 0.
  Log SHA-256: `30d82aef48bfcaa8796407632b10c779aeb68232b154878c1897fb06de8271aa`.
- **30 focused tests pass** for CourseContentsList, SongMeter and preparation,
  129.565 seconds, exit 0. Earlier actor/construction focus passed 36 tests;
  those targets also pass in the final full run.
  Final focus log SHA-256: `892232418b6be28ca4163604e7cfb7c8f163a786c265d8a15c35925627fdd68a`.
- Formatting passes for all seven changed Rust files. Final-source Clippy reports
  the same 166 warnings and existing `cmd.rs:349` error, 29.356 seconds, exit 101.
  Its log is byte-identical to the preceding stage, SHA-256
  `0d0336da57deb51a992301fc1cff7ff1747eca276d8387f6e5341bcf7d0fa914`.
  Test lint remains blocked by that existing library error.
- Times include compilation and Cargo lock waits, not workload benchmarks.
  All 1152 watched source hashes match the final focused, full and lint snapshots;
  every wrapper reports unchanged sources. Seven watched files changed and
  1145 are unchanged from the preceding stage; none were added or removed.
- All 240 interpreter/root entries, 16 native controls and six frozen fixtures
  remain unchanged. Pre-edit bytes are retained in
  `construction-init-edit-baseline.json`.

**The production migration remains incomplete.** Offline constructor semantics,
full compiler startup and state rewind, frame scheduling, remaining capture
channels, existing baking/live integration, remaining screen trees and transitions,
and unconditional production cutover remain required. Final fixture, normal
application/rendering, dependency/link and workload measurements are outstanding.
Production song execution still uses legacy. The interpreter and C API removal
milestones remain closed; the migration finish line has not changed.


## 2026-11-05 - Preparation Def ownership and cloning verified

`begin_actor_preparation` now installs the existing offline constructors. Def
initializes a supplied table in place, provides its methods/resources immediately,
and leaves Init and ordinary actor registration to the later host phase.
Untyped constructor arguments create an empty instance. Constructor method
rebinding, explicit child lists and deferred Condition filtering match the native
control. The live declaration path remains a separate mode.

The first typed child occurrence keeps its identity; later owned occurrences
clone. Reusing an owned root creates an independently owned graph. Cloning copies
mutable state, Frames and command capture, shares ordinary payloads and Lua
closures, and rebinds instance methods without rerunning resource constructors.
Concatenation initially shares child identities and composes commands. A supplied
offline SongMeter Stream stays the same object; Course Display appends once after
existing children. Scroller values and changed input text survive cloning.

Temporary constructor ownership transfers when a parent claims ordinary or native
children. Collection checks cover independent cloned scenes and claimed Stream,
Display and Graph resources. Unclaimed ordinary constructor actors remain until
VM teardown, matching the measured production control. Six new differential and
ownership tests extend the preparation suite from 10 to 16 tests.

Validation is audited by `review_offline_constructors.py`, with results in
`offline-constructor-review.json` and intermediate evidence/limitations in
`offline-constructor-findings.json`:

- **476 tests pass across all 46 adapter targets**, 536.593 seconds,
  exit 0. Log SHA-256: `8210e0e7537a6a436e46a6db8f28107cbf9e1e1f6ee733f82d7466c5fb1060c6`.
- **36 focused tests pass** across preparation, CourseContentsList and SongMeter,
  138.543 seconds, exit 0. Log SHA-256:
  `a4e5f2624c6ac559440fef44189eb1c0aacf9137ac1dd4fcb2754ea1f5c8dd98`.
- Formatting passes for eight changed/new Rust files and the isolated new native
  test bridge. Clippy has the same 166 warnings and existing `cmd.rs:349` error,
  with no new diagnostic messages/files; test lint remains blocked by that error.
  Exit 101, 30.908 seconds; log SHA-256:
  `05ac0ce4afe6697a409cc0632f8e7daadde483284c253a4a5b1e3aaccde9b015`.
- All 1153 watched source hashes match focused, full and lint snapshots; each
  wrapper confirms unchanged sources. Nine watched files changed, `offline.rs`
  was added, and 1143 files are unchanged from construction/Init validation.
- All 240 interpreter/root entries, 15 native control files and six frozen
  fixtures remain unchanged. `compile.rs` adds only a feature-gated test bridge
  to the real root ownership installer; all other bytes in that file are unchanged.
  Pre-edit bytes are retained in `offline-constructor-edit-baseline.json` and the
  earlier implementation checkpoint in `offline-constructor-v2-checkpoint.json`.
  Reported durations include compilation and lock waits; they are not benchmarks.

**The production migration remains incomplete.** Direct Def comparisons do not
prove preparation-mode file/font/virtual-theme loader behavior. The existing
virtual loader fills media paths before Def, while the native loader creates its
dummy before assigning the path; this still needs adaptation and comparison.
Full compiler startup/state rewind/frame scheduling, remaining capture channels,
existing baking/live integration, remaining screen trees/transitions and the
unconditional production song cutover remain required. Final fixture comparisons,
normal application build/render smoke, dependency/link proof and preparation/live/
memory measurements remain outstanding. The interpreter and C API removal
milestones stay closed. Native-code backends and new baking capabilities remain
outside the saved migration goal.


## 2026-11-06 - Preparation actor, font and file loaders verified

Preparation LoadActor now preserves the existing production first-result behavior,
invalid-argument dummy actors, media initialization order and file resolution.
LoadFont creates registered BitmapText instances; NullActor occurrences clone.
Saved callbacks, including tail calls, resolve paths from their source, preserve
arguments and restore loader scope after errors or instruction limits. The host
entry remains stable when authored code replaces global LoadActor. The original
noteskin source compiler remains separately bound.

Five new differential/boundary tests extend the preparation suite to 21 tests.
The original three failing cases are preserved byte-for-byte. Initial compiler
errors and a file-loader binding that shadowed the noteskin compiler were fixed;
the first full run was intentionally cancelled before tests, then rerun in full.
All intermediate evidence remains in preparation-loader-findings.json.

Final validation is audited by review_preparation_loaders.py and recorded in
preparation-loader-review.json:

- **481 tests pass across all 46 adapter targets**, exit 0, 528.018 seconds.
  Log SHA-256: `9f6e41e979beb69f4e359c055e28f78dfa292cf91bafbaa8bbcbf5ee9dd2d859`.
- **69 focused tests pass**, including unchanged noteskin coverage, exit 0,
  169.274 seconds. Log SHA-256:
  `38a975a9cdd82547c878b2ffe4445e37e82a449639ef89a38ab2158c2556deed`.
- Formatting passes for eight Rust files. Clippy retains the same 166 warnings
  and existing `cmd.rs:349` error, with unchanged diagnostic messages/files.
  Exit 101, 30.366 seconds; log SHA-256:
  `05ac0ce4afe6697a409cc0632f8e7daadde483284c253a4a5b1e3aaccde9b015`.
- All 1154 watched hashes match final focused/full/lint snapshots. Eight watched
  files changed, loading.rs was added, and 1145 are unchanged from the prior
  verified stage. All 240 interpreter/root entries, 16 native controls and six
  frozen fixtures remain unchanged. Durations include compilation and lock
  waits; they are not performance measurements.

**The production migration remains incomplete.** Full compiler startup and state
rewind, frame scheduling, remaining capture channels, existing baking/live
integration, remaining screen trees/transitions and unconditional production
cutover remain required. Final fixture, application/rendering, dependency/link
and workload measurements remain outstanding under the saved acceptance scope.
The completed interpreter and C API removal milestones stay closed. No new
capabilities or acceptance criteria are added by this stage.


## 2026-11-06 - TopScreen controls and option-row ownership verified

The Rust adapter now has the existing TopScreen control bindings and base screen
allocation. Option rows construct their named frame, title and grouped item text;
their GetName callback retains its original row name. Public actor methods keep
the existing receiver/class dispatch, while private controls retain their bound
owners. MusicWheel shares the original song/sort backing slots with GAMESTATE.
Navigation, lead-in limits, pause/row state and side-effect/error ordering match
the production host.

Transition requests record their source before descendant Off commands, select
Out through the native child registry, and commit the request after activation.
Saved independent wheel, row-constructor and constant methods do not retain a
retired screen. Rows have separate constructor ownership, preserving their
preparation command capture and registered lifetime across screen retirement.

Nine new tests compare these controls with the full production host. Intermediate
failures, corrections to new test assumptions and exact run evidence remain in
`top-screen-controls-findings.json`. All eight tests from the last failing run,
including the row-name failure, remain unchanged; the ninth extends preparation
capture/ownership coverage.

Validation is audited by `review_top_screen_controls.py` and recorded in
`top-screen-controls-review.json`:

- **490 tests pass across all 47 adapter targets**, exit 0, 558.682 seconds.
  Log SHA-256: `64e3b0f2de02b1f44901b3a674dc01e6289744458815ec972cd6d127db4a4500`.
- **50 focused tests pass** across top-screen controls, screen manager, generated
  players and transitions, exit 0, 179.502 seconds. Log SHA-256:
  `93bd92f6a9c8a7b33256da971964d01951d9cbf27789871ac71bf3298b293a48`.
- Formatting passes for five Rust files and the isolated native test bridge.
  Clippy retains the same 166 warnings and existing `cmd.rs:349` error, with no
  new diagnostic messages/files. Exit 101, 72.518 seconds; log SHA-256:
  `93b9e1e3812102ca34d47f401bd41e471c2490b85985666bb8dbf2c0ddb91139`.
- All 1156 watched hashes match the final focused/full/lint snapshots. Six watched
  files changed, two were added, and 1148 remain unchanged. All 240 interpreter/root
  entries, 15 native controls and six frozen fixtures remain unchanged. compile.rs
  adds only a feature-gated bridge to the actual production host. Recorded times
  include compilation and lock waits; they are not performance measurements.

**Full screen assembly and the production migration remain incomplete.** The
initial full-player test fixture exposed the missing GetComboFonts prerequisite.
The focused controls currently use the verified player/life-meter base constructor.
Complete that helper and remaining display prerequisites, then restore and verify
the original full-player fixture; the narrower fixture does not satisfy assembly
acceptance. The source references and this required follow-up are retained in the
findings report.

Loaded theme trees and full screen assembly, compiler startup/state rewind/frame
scheduling/capture, existing baking/live orchestration, unconditional cutover and
final fixture/application/rendering/dependency/performance checks remain required.
The saved finish line is unchanged. The interpreter and C API removal milestones
remain closed.


## 2026-11-06 - File/font bindings and complete-player fixture verified

The Rust song adapter now provides the existing FILEMAN surface and
StripSpriteHints helper. Read queries share the production path resolution,
wildcard, sorting and metadata policies. Copy/CreateDir/Remove/Unzip/FlushDirCache
retain their sandbox stub results and side-effect counting; no filesystem writes
were introduced. Saved callbacks preserve their captured song directory through
collection, and counter reads/writes preserve reentry and error behavior.

GetComboFonts runs the verbatim production Lua scanner with the same chunk name.
It searches the current theme, keeps Wendy first and Wendy (Cursed) last, and
returns an empty list without matching assets. Authored replacements of its
Lua dependencies, recursive calls, GC and failures remain observable.

The required follow-up from `top-screen-controls-findings.json` is resolved:
`top_screen_controls` again calls the complete `gameplay_player` constructor.
The nine prior screen tests remain unchanged. A tenth explicitly verifies full
Combo/Judgment/hold/NoteField trees using the real font helper. Six new file/font
tests compare behavior against the production implementation. The first run's
incorrect source-inspection test assumption and its correction remain recorded
in `file-font-helpers-findings.json`; the corrected test uses host inspection
because the production debug sandbox intentionally reports the current song path.

`review_file_font_helpers.py` audits `file-font-helpers-review.json`:

- **497 tests pass across all 48 adapter targets**, exit 0, 576.781 seconds.
  Log SHA-256: `91b7a0ef604c60307b7187847ab1a0bc8366f136fcb42cefe21e1da043a618c6`.
- **23 focused tests pass** across file/font, player-part and screen targets,
  exit 0, 45.203 seconds. Log SHA-256:
  `fd99eb4da3f6fb7bb81982fd0c9842797e664a10b96c66bba6a51f65b7e7ca2b`.
- Formatting passes for five changed/new Rust files. Clippy retains the same
  166 warnings and existing `cmd.rs:349` error, with no new diagnostic messages
  or files. Exit 101, 30.013 seconds; log SHA-256:
  `b49f32d2260a208c20d0c07d3e41e8f9a85e2bebaa5220c03ab369d08391752b`.
- All 1159 watched hashes match the focused/full/lint snapshots. Four watched
  files changed, three were added, and 1152 remain unchanged. All 240
  interpreter/root entries, 16 native controls and six frozen fixtures remain
  unchanged. Recorded run times include compilation and lock waits, and are
  not workload performance measurements.

**Complete screen assembly and the production migration remain incomplete.**
Loaded theme trees and the five unnamed transition owners, compiler startup/state
rewind/frame scheduling/capture, existing baking/live orchestration, unconditional
cutover and final fixture/application/rendering/dependency/performance checks
remain required by the saved objective. The interpreter and C API removal
milestones remain closed; this stage adds no acceptance criteria.


## 2026-11-06 - ScreenGameplay base assembly verified

The private Rust screen factory now constructs both life meters and both players
in the existing native order, attaches enabled screen children, dispatches the
initial LifeChanged messages without authored broadcast records, and preserves
stable draw order and all five unnamed transition owners. The underlay executes
verbatim production Header, background-filter, difficulty-meter, standard-life
and BPM snippets. Its required theme helpers use the existing production policies.
LifeMeter retains its bound child queries through the public Actor class.

Eight new differential tests cover preparation/live construction and enabled-player
combinations, underlay geometry/text/variants, initial meter tweens, retained BPM
and life updates across GC, scene retirement, real theme helper argument rules,
constructor reentry/raw tag writes, and failed construction cleanup with retry.
All pre-existing adapter test files remain unchanged. Failed runs and the precise
oracle corrections are recorded in `screen-assembly-findings.json`; no test was
deleted or disabled. The native oracle installs its real public method classes
after initial construction, matching the production host; explicit rebuild cases
retain those classes before construction.

`review_screen_assembly.py` audits `screen-assembly-review.json`:

- **505 tests pass across all 49 adapter targets**, exit 0, 557.035 seconds.
  Log SHA-256: `36729285581ff658216e406953b66a5a78f9d944b38ed4fb29a001be196bce69`.
- **61 focused tests pass** across screen assembly, generated players, player
  parts, screen parts, TopScreen and transitions, exit 0, 219.952 seconds.
  Log SHA-256: `3bcfc91a9de59b2751ae497cd54d695133771a1d0c2defcfb3488242396358b2`.
- Formatting passes for nine changed/new Rust files and the isolated test-only
  compiler bridge. Clippy retains the same 166 warnings and existing `cmd.rs:349`
  error, with no new diagnostic messages or files. Exit 101, 31.620 seconds.
  Log SHA-256: `2c6d61aa91102be6bcbe33e059d89c53fc12357c242f9c09a62f39bad49b8090`.
- All 1170 watched hashes match focused/full/lint snapshots: seven changed files,
  eleven additions, no removals, and 1152 unchanged entries. All 240 interpreter
  and root entries, all other native controls, and six frozen fixtures remain
  unchanged. Removing the test-only bridge restores the exact previous native
  compiler bytes. The five underlay snippets and two Lua theme helpers match
  production source verbatim. Timings include compilation and lock waits and
  are not workload performance measurements.

**The production migration remains incomplete.** Original loaded combo,
underlay, overlay, decoration and transition files; selection/reload orchestration;
full compiler/live startup, capture/output and fallback; unconditional production
cutover; and final regression/fixture/application/rendering/dependency/performance
validation remain required by the unchanged saved objective. Interpreter and C API
removal milestones remain closed.


## 2026-11-06 - Bundled combo and unnamed transition loads verified

The active production migration remains incomplete. The standalone interpreter
and C API removal milestones stay closed. This checkpoint ports two existing
host load phases; it does not change the fixed acceptance scope.

- `actors/gameplay_theme.rs` loads the original combo milestones and five unnamed
  transition children. `gameplay_theme/source.rs` compiles fresh chunks with their
  exact native source names and environments, independently of authored
  `loadstring`/`setfenv` replacements. Original bundled Lua files are unchanged.
- All loaded materialization owners join the screen scene before Init, including
  actors from failed Init calls. Milestone Init precedes Combo parenting. Both
  players receive milestones even when disabled, subject to the existing frame
  and HideComboExplosions checks. The existing bitmap policy supplies textures.
- The loader preserves native raw registry writes and the original startup queue
  boundary. It must not open the barrier before the underlay phase. The native
  oracle bridge calls the actual production phase functions and is cfg-isolated;
  removing only that bridge restores the preceding compile.rs bytes exactly.
- Eleven new comparisons exercise preparation/live behavior, real temporary font
  discovery, the fontless invisible Combo leaf, visual styles, ordering,
  environments, source names, retained commands, collection and release after
  successful or failed loading. All pre-existing adapter test files are unchanged.
- `theme-load-adapters-v1-song-linux-debug`: **516 passed, 0 failed**, all **50**
  Rust adapter targets; **647.9249878739938 s**, exit 0. Log SHA-256:
  `db9cdf1a2c51f77f5e7e77af85bc4ed7e70271eb0d9b82328aefac98c63ccb03`.
- `theme-load-lint-v1-song-linux-debug`: **27.66400612005964 s**, exit 101;
  the same **166 warnings** and existing **cmd.rs:349** error as the preceding
  checkpoint. Diagnostic messages/files match exactly; no new diagnostics.
  Log SHA-256:
  `58e736f79184a708c14e4e827f71eae24df9955525bc3d453f18ff40016ee162`.
- `review_theme_load.py --full theme-load-adapters-v1 --lint theme-load-lint-v1`
  passes. All **1173** watched hashes match both runs; **3 added, 4 changed,
  1166 unchanged, 0 removed**. The **240** generic-engine/root entries, native
  controls, frozen fixtures and previous adapter tests remain unchanged.
  Applicable formatting and the isolated oracle-bridge formatting pass.
- `theme-load-edit-baseline.json`, checkpoints v1-v4 and
  `theme-load-findings.json` retain the original files and failed-run evidence.
  The first compile failure, initially fontless fixtures and premature startup
  queue barrier were resolved without removing failing coverage. Final focused
  cases are verified inside the full regression log; no redundant rerun was used.

Next, load the original underlay, overlay and decoration files in production
order between these phases, including their real helper bindings and startup
barrier. Selection/reload orchestration, complete compiler/live output and
fallback, unconditional production song cutover, and final fixture/application/
rendering/dependency/performance checks remain required by the original objective.
No final normal-application or rendered smoke-test claim is made at this checkpoint.


## Loaded gameplay underlay and construction ownership (2026-11-06)

The Rust song adapter now executes the original bundled Danger, StepStatistics,
NoteField and SongInfoBar files in the existing underlay phase. This is a verified
adapter checkpoint within the fixed production migration, not completion of that
goal or a reopening of the interpreter/C API milestones.

The phase preserves the shared StepStatistics environment and global write
forwarding, the fresh relative NoteField environments and closed resource lists,
hidden player-option text, parenting before Init, native adoption order, the
startup queue barrier, direct Begin/On traversal and initial CurrentSongChanged
delivery. The existing PlayerNumber/OtherPlayer tables, range and other numeric,
time and color helpers are installed. GetColumnMapping and the HSV/Brightness
helper bodies match their unchanged native Lua sources exactly, including source
names and retained line layout. Actor align preserves immediate horizontal and
vertical writes without delegating through authored replacement methods.

Loading a theme phase can leave implicit Def/LoadFont templates whose callbacks
share upvalues with the loaded actors. Every object created during the phase now
joins the screen's ownership boundary, including objects left behind after a
callback fails. The focused checks verify retained callbacks after GC and release
after both successful loading and an explicit On failure, in preparation and live
modes. They also compare the full tree, layout, optional widgets, resource errors,
helper conversions and startup queue ordering.

The seven failed focused runs are retained in
[theme-underlay-findings.json](theme-underlay-findings.json). They exposed missing
existing host bindings, one incorrect Lua source extraction, two fixture
assumptions about unsupported native calls, and the implicit-template retention
bug. No prior adapter test or frozen fixture was changed, and all twelve new
cases remain enabled. The final focused v8 run passed all twelve tests on stable
sources (157.64343014196493 seconds; log SHA-256
`454bc39b40cc1b9e072970f073b2f06fc469fab9ac3795054302c4763b2cee5d`).

Final checkpoint verification:

- [Full adapter result](theme-underlay-adapters-v1-song-linux-debug/result.json):
  **528 passed, 0 failed**, all **51** Rust adapter targets, exit 0,
  626.0830330341123 seconds. Log SHA-256
  `8095d2c27464983ff6fcedb5f8367c30dbbfd496352a82862e30669baae4b89d`.
- [Clippy result](theme-underlay-lint-v1-song-linux-debug/result.json): exit 101,
  29.981704583158717 seconds. The existing 166 warnings and `cmd.rs:349` error
  have the same diagnostic messages and files as the preceding checkpoint.
  No new diagnostic was introduced. Log SHA-256
  `8ae1ac9edc444efd63d2a4bff1d3cfd41894ca51a7da489d51dbfcc37644f0df`.
- [Source audit](theme-underlay-review.json), generated by
  [review_theme_underlay.py](review_theme_underlay.py): all **1,178** watched
  source hashes match both full tests and lint; **5 added, 8 changed, 1,165
  unchanged, 0 removed** relative to the combo/transition checkpoint. All prior
  adapter tests, the native controls, six frozen fixtures and 240 interpreter/root
  entries remain unchanged. The native compiler differs only by the isolated
  test bridge. Formatting passes for eight changed Rust files and that bridge.

The normal production song compiler still uses the legacy engine. Original
loaded overlay/decorations, selection/reload, compiler and live output,
preparation/fallback/cache integration, unconditional cutover, and the final
fixture/application/rendering/dependency/performance gates remain required.
In particular, the native underlay's exclusion from the two live song output
streams must be preserved when those output streams are integrated; normal
listener delivery and authored message history remain active. This checkpoint
is not final application or rendered-gameplay validation.


## Loaded gameplay overlay and decorations (2026-11-06)

The original bundled overlay, Header and Footer files now run through the Rust
song adapter. This closes another existing host load phase within the fixed
production migration. The interpreter and C API removal milestones remain closed;
the production song compiler still uses the legacy engine.

The overlay uses its shared write-forwarding environment and closed resource
loader. Normal loading replaces the placeholder before Init, retains name,
draw order and engine-layer identity, and runs direct Begin/On in native order.
The live diagnostic capture variant keeps the wrapper and appends the probe after
those commands. Native preparation does not install the probe's actor host: that
existing error is preserved after the theme has been attached and run, with its
one-child partial tree explicitly compared. Header/Footer use the existing
ScreenString environment, merge the hibernating metric On command, replace their
placeholders, and participate in the final stable sort after their actual Init.

Required host uptime and DarkUI bindings preserve native behavior. Uptime uses
preparation f32 music time or live f64 uptime, including runtime replacement,
conversion and error rules; test clock inputs use original source lines and FIFO
values without calling authored debug replacements. Actor draw-order and
hibernation methods publish the native fields consumed by these phases, including
the hibernation countdown. Declaration provenance captures the original debug
primitive. Theme ownership covers implicit templates and failed On construction.

Nine comparisons cover preparation/live trees, both players, styles, the capture
probe, ordering, resource environments/errors, original judgment listeners,
observed clock inputs, GC retention, release and partial failure. The judgment
fixture delivers actual native gameplay messages; authored Broadcast intentionally
inserts HoldNoteScore_None and has different existing offset-tracking behavior.
No prior test or frozen fixture was changed and no case was disabled.

The two failed focused runs and their findings remain in
[theme-overlay-findings.json](theme-overlay-findings.json); original files and
intermediate states remain in the overlay baseline/checkpoint snapshots. Focused
v3 passed all nine tests on unchanged sources in 161.75173825002275 seconds,
with log SHA-256 `dc853ea9a944ff0d7e5c2c541bf2a622cb28b7531a78a87ac689713715d75e67`.
The same cases passed again as part of the full suite; no redundant focused rerun
was needed after that final source check.

Checkpoint evidence:

- [Full adapter result](theme-overlay-adapters-v1-song-linux-debug/result.json):
  **537 passed, 0 failed**, all **52** Rust adapter targets, exit 0,
  673.265517055057 seconds. Log SHA-256
  `f99824e318f52196c0765bdf8421b458f32072925cb137697cb9be8d49b4e6ac`.
- [Clippy result](theme-overlay-lint-v1-song-linux-debug/result.json): exit 101,
  30.345865006092936 seconds. The same **166 warnings** and existing
  **cmd.rs:349** error have identical diagnostic messages/files to the verified
  underlay checkpoint. Log SHA-256
  `c7d9a1775f05f8a1c278d0ba6c9640acc2b665dbcfc2980142dfb70f52b4b4cd`.
- [Source audit](theme-overlay-review.json), from
  [review_theme_overlay.py](review_theme_overlay.py): **1,181** watched source
  hashes match both runs; **3 added, 11 changed, 1,167 unchanged, 0 removed**.
  All previous adapter tests, native controls, six frozen fixtures and the
  240 generic-engine/root entries are unchanged. The native compiler differs
  only by two cfg-isolated test bridge functions; removing that bridge restores
  its prior bytes exactly. Formatting passes for eleven changed Rust files and
  the separately formatted bridge.

Selection initialization/reload, complete compiler/live output, preparation and
fallback/cache orchestration, unconditional production cutover, and the final
fixture/application/rendering/dependency/performance checks remain required.
These results do not establish normal-application cutover or rendered gameplay
validation. The fixed migration acceptance criteria have not changed.


## Retained selections and screen reconstruction (2026-11-06)

The Rust adapter now connects retained GameState selection to initial and
replacement screen construction. This is a verified boundary within the saved
production migration; the interpreter and C API milestones remain closed, and
the production song compiler still uses the legacy engine.

`managers/game_state/scene.rs` captures the original selected-slot table and known
chart identities with immutable metadata. Context projection reads chart indices
and difficulty before the potentially reentrant style query, applies selected
chart timing when available, and preserves the supplied fallback otherwise.
Initial CPU-player seeding copies the first human's retained chart directly,
without SetCurrentSteps validation, broadcasts or side-effect increments. All
four enabled-player combinations are covered.

`gameplay.rs` exposes initial construction, reconstruction and the subsequent
ordered theme load as separate operations. Initial SL state uses the selected
context; initial statistics are created after base screen publication, matching
native late-global installation. Reconstruction restarts options, removes old
message subscriptions before constructing the new tree, seeds selections,
publishes the screen and input map, resets statistics and clock, and then lets
the caller load combo/underlay/overlay/decorations/transitions in native order.
Process GameState, SL, arbitrary globals, environment and RNG state survive.
Saved input methods address the replacement callback map. Outgoing actors retain
their existing owner until the caller explicitly releases it.

Theme callback failures keep partial actors in the supplied screen scene. Tests
compare partial published state and prove cleanup after release. Repeated cycles
run the actual End walkers, assert that End sees the outgoing clock and no top
screen, reconstruct, then explicitly release and collect the outgoing Rust scene.
The full session's startup/navigation loop, automatic ownership retirement,
retained-reference policy and renderer/output plumbing remain unfinished; these
adapter methods are not a claim of complete production reload integration.

The initial focused failure exposed a real ordering defect: statistics still used
the original context after nonhuman chart seeding. Installing them at the native
late phase fixed the disabled player's possible points. Fixture corrections also
made the style test actually replace the retained style slot (SetCurrentStyle
accepts a name string), and added the required outgoing End phase to the cleanup
comparison. The unchanged native preparation control still retained the old
screen outside its traversable strong global graph after End/replacement; it has
no counterpart to explicit Rust scene release. Its lifecycle effects remain
compared, while strict Rust weak-root collection assertions are checked separately
in both modes and across two replacements. No Rust cleanup assertion was weakened.

All five failed runs, including a duplicate caused by a failed formatting-sensitive
text edit, remain in [gameplay-reload-findings.json](gameplay-reload-findings.json).
The original files and intermediate test states are retained in the baseline and
v1-v4 checkpoints. No pre-existing test or frozen fixture changed; all eight new
cases remain enabled. Focused v6 passed on unchanged sources in
43.88368859607726 seconds, log SHA-256
`bfcdf6e362d5bcff05337bb2f9abe0743a3b4228755beb19b3e1a197abec2932`.

Checkpoint verification:

- [Full adapter result](gameplay-reload-adapters-v1-song-linux-debug/result.json):
  **545 passed, 0 failed**, all **53** Rust adapter targets, exit 0,
  760.6237746179104 seconds. Log SHA-256
  `ad5dfd49863a2e4f27627155cea5f6c9c10025cb605e29b44aa08fc08635bac8`.
- [Clippy result](gameplay-reload-lint-v1-song-linux-debug/result.json): exit 101,
  32.250320069957525 seconds. The same **166 warnings** and existing
  **cmd.rs:349** error have unchanged diagnostic messages/files. Log SHA-256
  `ae3119b21d0d8a33343cfefb02cc3baa9a68b0b9202c82d11f681539ad8ee7fd`.
- [Source audit](gameplay-reload-review.json), from
  [review_gameplay_reload.py](review_gameplay_reload.py): all **1,184** watched
  source hashes match full tests and lint; **3 added, 8 changed, 1,173 unchanged,
  0 removed**. All prior adapter tests, native controls, six frozen fixtures and
  240 generic-engine/root entries are unchanged. Removing the one new cfg-isolated
  compiler bridge restores its prior bytes exactly. Formatting passes for nine
  changed Rust files and the bridge formatted separately.

The full compiler/session startup, reload/end ownership and output, existing
preparation/fallback/cache behavior, unconditional production song cutover, and
final interpreter/adapter/downstream/fixture/application/rendering/dependency/
performance validation remain required. Test elapsed times above are not workload
performance measurements. The fixed migration acceptance criteria are unchanged.

## Screen retirement, Lua reachability and VM identity (2026-11-06)

This checkpoint implements the saved migration requirement to preserve retained
Lua actors across screen replacement, then collect unreachable state. It does
not reopen the completed interpreter or C API removal milestones. The separate
production migration remains active against its existing acceptance criteria.

`retire_actor_scene` removes an outgoing scene's host ownership after the caller's
End phase while keeping saved Lua actors, parent/child references, wrappers and
sound handles operational. A generic private table attachment supplies traced
ownership without exposing or coupling actor policy to the engine. Lua field and
metatable edits cannot discard that native state. The private edge stays strong
for weak tables; unreachable cycles remain collectible. GC drops only enqueue
identity-tagged notices, and a later actor-host operation removes Rust metadata
without a Store lock held across collection or callbacks. Shared active scene
owners continue to own their actors. Explicit hard release remains available for
intentional invalidation.

The existing native registry also owns actors created through saved old actors.
The new differential control reproduced a missing late-wrapper subscriber after
GC. Such wrappers now enter the current strong message registry and remain
subscribed until its next replacement. Full screen coverage exercises that second
actual reconstruction boundary before asserting collection. Retained callbacks,
partial End errors, detached/weak authored fields, shared ownership, sound handles
and repeated cleanup are covered in preparation and live modes. Cross-runtime
scene handles previously could alias because their owner was a VM-local TableId;
they now use the existing VM-scoped constructor Root. The raw TableId embedding
contract itself is unchanged.

All eleven new retirement cases and five generic host-state/GC cases pass.
[Findings](scene-retirement-findings.json) retain the six failed focused runs,
including the cross-runtime and late-subscription defects. Incorrect new fixture
assumptions about foreign raw IDs and weak native bound-method maps were corrected
against the existing contracts and unmodified native controls. No previous test
or frozen fixture changed, and no new retirement test is ignored. Strict Rust
cleanup assertions remain; the native control's separately retained roots do not
justify claiming equivalent collection timing. The intermediate
[555-test review](scene-retirement-preliminary-review.json) is retained as
historical evidence of the earlier implementation, not final verification.

Final checkpoint results:

- [Full song adapters](scene-retirement-adapters-v2-song-linux-debug/result.json): **556 passed, 0 failed**, 731.5668242189568 seconds. Log SHA-256
  `bb9bd34ee306f51274db58f966ad46286c074369134b16f8c6e092d5000b5023`.
- [Core interpreter](scene-retirement-core-v1-core-linux-debug/result.json): **954 passed, 0 failed**, 237.49286296707578 seconds. Log SHA-256
  `ae9c328c2804df66c8882410d4da8adb4e8b8a383a3873c3cb7de669201d667b`.
- [Noteskins](scene-retirement-noteskin-v1-noteskin-linux-debug/result.json): **312 passed, 0 failed**, 49.939300707075745 seconds. Log SHA-256
  `4a9fc7d194d19db2473a347911db2c3e06c88c04fc31f883b9071ca73b8e395c`.
- [GIF definitions and component](scene-retirement-gifs-v1-gifs-linux-debug/result.json): **15 passed, 0 failed**, 189.99763834103942 seconds. Log SHA-256
  `f37c66e3ba6780f1f0a496bb98e4efbe2b7d096a1cdca898ed1cc7709a6da784`.
- [Core Clippy](scene-retirement-core-lint-v1-core-linux-debug/result.json): exit **0**, 13.42795091914013 seconds. Log SHA-256
  `888268e7e026525b0f84f2fdc9d12701277e58270c9ae34e3f555d57aa0b676c`.
- [Song Clippy](scene-retirement-lint-v2-song-linux-debug/result.json): exit **101**, 43.12625870597549 seconds. Log SHA-256
  `67c55176011a63e6e9afeaaf416533121c32a725b1a0fadbf555443c5e041b56`.

Song Clippy reports the same **166 warnings** and existing **cmd.rs:349** error;
its diagnostic messages and files match the preceding selection checkpoint.
These elapsed times measure verification runs, not workload performance.

The [source audit](scene-retirement-review.json), produced by
[review_scene_retirement.py](review_scene_retirement.py), verifies all **54**
Rust song targets and the eleven new cases within the full **556-test** result.
All **1,187** watched hashes match the final song test and lint runs. Compared
with the preceding selection checkpoint, **18 files changed, 3 were added,
1,166 are unchanged and none were removed**. Prior adapter tests, native controls,
the production compiler and frozen fixtures are unchanged. Formatting passes for
all twenty changed or added Rust files.

The core/core-lint, noteskin and GIF results precede the final late-registry fix
but their component inputs are unchanged. The audit proves that exactly five
optional song-adapter/test files differ, checks the module feature gate and uses
the [GIF dependency features](scene-retirement-gif-features.json) to confirm that
those files are excluded from the reused checks. It does not claim that all six
runs have identical whole-workspace snapshots. The 15 GIF tests include fourteen
loader cases and one gameplay component case; the initial audit's count of fourteen
was corrected to match the actual filter and source files.

The next integration work remains the full compiler/session startup and navigation
loop, automatic End/retirement, captured preparation/live output, fallback/cache
behavior and unconditional production song cutover. Final fixture comparisons,
normal application build and rendered smoke, dependency/link proof and workload
preparation/live/memory measurements are still required. This checkpoint is not
production cutover or final application evidence.

## Authored song-layer construction and compiler debug globals (2026-11-06)

This checkpoint integrates the existing compiler's authored-file and Init phase
with the Rust song runtime. `SongLayerPlan` validates entry counts, primary index,
start beats and files before Lua execution, preserving the empty-preparation
shortcut. Physical paths remain separate from raw authored source spellings,
including `./`, directory defaults and filename whitespace. The exact physical
file is read; its source alias controls provenance, directory/path context and
the existing Xero environment rule.

`initialize_song_layers` executes entries in the shared VM. Background roots stay
outside the screen's command children; foreground actors see their parent during
Init and attach afterward. Delayed activation flags use raw publication, and
post-Init names/visibility, UpperLeft wrapping/command composition and live
load-error/non-table fallback follow production behavior. Earlier globals and
file side effects survive fallback. Every new actor owner, including implicit
templates and partial failures, joins the screen. Its private traced attachment
owns the loader table; retirement preserves saved objects and later collects the
unreachable graph.

`begin_song_compilation` installs the production public debug policy once. Its
getinfo describes current script context, traceback accepts only valid UTF-8
strings, and Lua upvalue reads preserve the existing result shape/index rules.
Reloads preserve author replacements. Internal loaders and constructors keep
their captured VM primitives and raw source names. The declaration helper now
recognizes bare absolute names already accepted by production Def; existing
missing-source rejection remains covered. No generic-engine change or Lua C API
compatibility layer was added for this phase.

Seven new cases cover input validation and six differential construction cases:
preparation/live source spelling and provenance; parent/Init/attach order and
scheduled foreground behavior; live fallback side effects; Xero/nested files;
UpperLeft partial errors; compiler debug behavior; and failed-Init retirement.
The oracle executes the actual production compiler in a retained VM and stops
at its existing malformed mod_actions typed read immediately after Init. This
verifies the construction boundary, not complete startup or compiled output.

[Findings](song-layers-findings.json) retain all four failed runs and corrections:
an invalid helper call; unsupported new-fixture assumptions about SetName and an
untextured Sprite; the missing public debug table; and inherited visible lookup
and string-only traceback defects. The corrected UpperLeft case explicitly
requires the existing line-6 error and compares its earlier effects. No prior
test or frozen fixture changed; all seven new tests remain enabled.

Checkpoint verification:

- [Focused result](song-layers-focused-v5-song-linux-debug/result.json): **52 passed**
  across five targets, 166.37374414200895 seconds, unchanged sources. Log SHA-256
  `56b2dcbf10c7c9fb3907677a6a88da40b4663ed1efdb37df8f4f4fe9049b5e47`.
- [Full adapter result](song-layers-adapters-v1-song-linux-debug/result.json):
  **563 passed, 0 failed**, all **55** Rust adapter targets, exit 0,
  739.1618973810691 seconds. Log SHA-256
  `cd11b1e41671b399d1ba636b8e0f094390c489861b3698b219ceb1faa05f4145`.
- [Song Clippy result](song-layers-lint-v1-song-linux-debug/result.json): exit 101,
  28.7952301800251 seconds; the same **166 warnings** and existing **cmd.rs:349**
  error have unchanged diagnostic messages/files. Log SHA-256
  `ae3119b21d0d8a33343cfefb02cc3baa9a68b0b9202c82d11f681539ad8ee7fd`.
- [Source audit](song-layers-review.json), from
  [review_song_layers.py](review_song_layers.py): all **1,191** watched hashes match
  both final song runs. Relative to the retirement checkpoint: **8 changed,
  4 added, 1,179 unchanged, 0 removed**. Prior tests and frozen fixtures are
  unchanged. The production compiler's prior bytes remain an exact prefix; its
  sole addition is a test-support/rust-engine-gated oracle. Formatting passes for
  nine changed/new Rust files and that bridge separately.

The passing **954 core**, **312 noteskin**, **15 GIF** and core-Clippy results
from the retirement checkpoint cover unchanged component inputs. The audit checks
every changed path, normal manifest configuration, the optional Rust module gate,
the oracle's feature gate and current [GIF dependency features](song-layers-gif-features.json).
It does not claim identical whole-workspace snapshots across those earlier runs.
Verification durations above are not workload-performance measurements.

Full startup/navigation remains unfinished. Pre-Init state capture, Begin/On and
zero-update ordering, startup request consumption before queued probes, prepared/
live output and fallback/cache integration still need completion. Production song
execution remains legacy. Unconditional cutover and final fixture comparisons,
normal build, rendered smoke, dependency/link and preparation/live/memory evidence
remain required by the existing objective. The completed interpreter and C API
removal milestones remain closed.

## Pre-Init compiler state capture (2026-11-06)

The original migration objective remains unchanged. The prior conversational
acknowledgement made no code progress; this continuation reread the attachment
and verified the current construction checkpoint before implementing capture.
The interpreter and C API-removal milestones remain closed.

`initialize_song_layers` captures players and Underlay/Overlay/SongBackground/In
before executing authored entries, then each authored actor sequence before Init.
The `song_initial_states` accessor returns copied render values keyed by actor
identity while the active screen traces all captured tables through its private
host state. Lua mutation and GC cannot change captured values. Retired/released
scene handles return no capture; callers retaining table handles past that
boundary must root them. Temporary dynamic-lookup results are rooted throughout
capture. Wrappers created by Init/On still belong to the later startup boundary.

The Rust reader preserves the existing raw scalar, flag, vector and enum
conversions, numeric defaults and disabled FOV behavior. Comparing the actual
production compiler revealed that generated player dummy sprites incorrectly
inherited authored Sprite animation defaults; their default is now disabled,
as with production `create_dummy_actor`, until changed by a command.

Three added cases compare production capture/order and errors in preparation
and live modes: player/screen capture before file side effects, layer capture
before parent/child Init, later mutation and GC, raw render properties, camera
disabling, and malformed state rejection before Init. Native observations are
behind test-support/rust-engine gates in the actual compiler. All prior seven
layer cases and every earlier test/fixture remain unchanged and enabled.

Verification:

- [Focused result](song-initial-focused-v5-song-linux-debug/result.json):
  **50 passed, 0 failed** across layer, render, preparation and gameplay-theme
  targets. An earlier [35-case run](song-initial-focused-v4-song-linux-debug/result.json)
  also covers generated players, player parts and retirement. The final full run
  below includes the subsequent explicit-rooting review.
- [Full adapter result](song-initial-adapters-v1-song-linux-debug/result.json):
  **566 passed, 0 failed across all 55 targets**, exit 0, unchanged sources.
  Duration: 815.1361247890163 seconds; log SHA-256
  `f663d92dbacfdb51708dfdf29985dc50b9c0ad4d0d0794971b6aa7ff4e36a720`.
- [Song Clippy result](song-initial-lint-v1-song-linux-debug/result.json):
  the same **166 warnings and existing cmd.rs:349 error**; diagnostic messages
  and files match the prior baseline. Exit 101, 29.335212491918355 seconds;
  log SHA-256 `1f2755c017d9f7c26995e8ff90875c4ef87a24fe5b820b80ed4842175809e20f`.
- [Source audit](song-initial-review.json), produced by
  [review_song_initial.py](review_song_initial.py): **1,192** current hashes match
  both final runs; **10 changed, 1 added, 1,181 unchanged, 0 removed** relative
  to the authored-layer checkpoint. Formatting passes for all ten changed/new
  adapter/test Rust files and the new oracle definitions separately. Removing
  only the two gated observation calls and new gated observer definitions
  recovers the prior compiler bytes exactly, including its mixed line endings.
- The earlier **954 core, 312 noteskin, 15 GIF** and core-Clippy results cover
  unchanged component inputs. This continuation changes only the optional song
  adapter, its tests, and the gated compiler observers. Manifests and the earlier
  GIF feature-tree inputs are unchanged. No new application build, rendered
  smoke, or workload performance measurement is claimed here.

[Findings](song-initial-findings.json) retain the failed compile attempts, the
8-pass/2-failure capture comparison and its generated-sprite correction, the
partial edit-script assertion and final audit-matcher corrections. No failing
coverage was deleted, weakened or skipped. Suite durations are verification
costs, not preparation/live workload measurements.

Production song execution still uses the legacy backend. Full startup/navigation,
zero-update and queued capture, prepared/live output, fallback/cache integration,
unconditional cutover and the objective's final fixture/application/render/
dependency/link/performance evidence remain required. Pre-Init capture is an
integration prerequisite, not a replacement finish line.

## Song activation through the real initial update (2026-11-06)

The fixed production-migration objective remains active. The prior goal response
only acknowledged scope and made no progress; this continuation reread the
objective, inspected the pending implementation and completed concrete activation
work. The interpreter and C API removal milestones remain closed.

`activate_song_layers` now reads and retains the four typed Init globals, consumes
Init NewScreen requests at the existing boundary, and activates the loader roots
in production order: Begin, In, ScreenChanged, On, startup options, then Update(0).
Only the NewScreen request is consumed afterward; transition requests remain
pending. Deferred background/foreground startup and live initial-update settings
are preserved. The initial update follows dynamic raw children and native wrapper
storage, including child mutations, sleeping actors, pending callback selectors,
partial callback errors and collecting callbacks. Constructor scenes created by
activation are adopted by the screen even when a command fails.

The actual production compiler supplies the differential oracle. A gated observer
stops after Update(0), before any future queue probe. A separate gated first-load
marker allows an empty retained test VM to take the ordinary first-load path;
subsequent compiler calls still take the retained-VM reload path. This corrected
a harness mismatch that had retained four queued actors from a preliminary screen.
No queue entries are filtered from the comparison. Nine new cases cover activation,
GC retention, options/requests, deferred layers, errors, cleanup and queue order.

Verification on the final source:

- [Focused regressions](song-activation-focused-v5-song-linux-debug/result.json):
  **51 passed, 0 failed**, covering activation, authored layers, preparation and
  retirement. Duration 181.91177434497513 seconds; log SHA-256
  `24b8cd381f764ba533014eb59cad707b2f694de8ff2df26ae45dd8fe5ff64adf`.
- [Shared regressions](song-activation-shared-v1-song-linux-debug/result.json):
  **89 passed, 0 failed**, covering actors, wrappers, messages, transitions,
  effects and top-screen controls. Duration 99.62681544199586 seconds; log SHA-256
  `96d8c009dc96eeeceb9772730bbc79e1d9efc7fce3f5efd2b563e06126df07d9`.
- Thus **140 tests across 10 targets** were run against this source. The earlier
  566-test/55-target result is a historical full-suite checkpoint, not a new full
  run. The manifest now contains 56 Rust adapter test targets.
- [Clippy](song-activation-lint-v1-song-linux-debug/result.json) retains exactly
  the same 166 warnings and existing `cmd.rs:349` error. Exit 101; duration
  31.614393156953156 seconds; log SHA-256
  `81f4f532f8b246e217c89ceba4ce84be8170870caf1b54793d64daa37da360bd`.
- [Source audit](song-activation-review.json), generated by
  [review_song_activation.py](review_song_activation.py), verifies all **1,195**
  source hashes against the test/lint snapshots: **12 changed, 3 added, 1,180
  unchanged, 0 removed** relative to the pre-Init checkpoint. Existing tests and
  frozen fixtures are unchanged. Formatting passes for changed/new adapter/test
  Rust sources and new compiler observer definitions. Removing only the gated
  observer additions recovers the prior compiler and startup utility bytes exactly.
  The manifest change only adds the new test target. Generic engine and downstream
  component inputs are unchanged; their earlier core/noteskin/GIF results are reused.

[Findings](song-activation-findings.json) preserve all failed runs and the evidence
used to distinguish test-harness mistakes from adapter defects. The wrapper update
omission was fixed in the adapter. The cached chronological traversal was not used
for the real zero update, whose production implementation is dynamic. Existing
coverage was not deleted, weakened or skipped. Check durations above are not
preparation or playback performance measurements.

Production song execution still uses the legacy compiler. Initial-update/wrapper
and broadcast capture outputs, shared-state queued discovery/rewind, prepared/live
output, complete reload orchestration, fallback/cache integration, unconditional
cutover, and final fixture/application/render/dependency/link/performance evidence
remain required. This activation checkpoint is not the migration finish line.

## Owned startup capture output (2026-11-06)

The previous goal turn made progress: activation through Update(0) was implemented
and verified. This continuation reread the same migration objective and connected
its existing capture data to `SongActivation`. The interpreter and C API removal
milestones remain closed; production migration is still incomplete.

Activation now returns owned `SongStartupState` values for the initial update and,
in preparation without a replacement-screen request, Init/On tween captures.
Each contains the initial render state, command blocks, queue durations and names,
block-step associations, and the update ancestry's hibernation/rate path. Before
Update(0), the loader privately traces each captured actor identity; removing a
child and collecting during a callback cannot invalidate the retained snapshot.
Only a fully unchanged command-block prefix is removed from initial-update output.
Wrapper states created by On or Update(0) receive the existing compiler's default
initial state and remain distinct from the ordinary-child initial-update capture.

Broadcast capture preserves preparation/live startup rules, supplied parameter
presence, nesting and the runtime beat. Native life-loaded and Underlay construction
scopes temporarily exclude their events from the live song output while continuing
to deliver callbacks. The runtime's Lua history has_params flag and runtime event
parameter-presence flag remain distinct for an explicitly supplied nil value.

Three new differential cases compare actual compiler output for state/block/queue
capture, wrapper defaults, removed actors, changed versus unchanged prefixes,
Init/On tweens, live deferred updates and nested broadcasts. The gated observer
now stops after the compiler has taken real startup broadcasts and collected
wrapper defaults, still before future-command discovery. The existing native
`capture_startup_states` extractor supplies preparation tween comparisons from
that retained actual initial-state map; no future queue is executed by the oracle.

Verification:

- [Broad relevant regressions](song-startup-capture-focused-v3-song-linux-debug/result.json):
  **90 passed, 0 failed across seven targets** (activation, preparation, gameplay
  theme, Underlay, messages, retirement and authored layers). Duration
  282.13755659293383 seconds; log SHA-256
  `5c14fde9f5cd8d17acffc64565408024245055295cb8e9c3cb7165c8bb2938cc`.
- [Final activation recheck](song-startup-capture-focused-v4-song-linux-debug/result.json):
  **12 passed, 0 failed**, after replacing `or_insert_with(Default::default)` with
  `or_default`. Duration 141.8122159219347 seconds; log SHA-256
  `a1119e1643e15bc740a726a05ec031b9bcede5c3c0bae413aff4aad64c3f190f`.
- [Final Clippy](song-startup-capture-lint-v2-song-linux-debug/result.json):
  exactly the prior **166 warnings and existing cmd.rs:349 error**. Exit 101;
  29.637522358214483 seconds; log SHA-256
  `6f860e9526987982e666c6e954316313dc235ddd8098a4c65380ced3ad0978e7`.
- [Source audit](song-startup-capture-review.json), generated by
  [review_song_startup_capture.py](review_song_startup_capture.py): **1,196**
  watched source hashes match the final recheck and lint. The audit proves the
  default-insertion cleanup is the sole source difference from the broad run.
  Relative to activation: **13 changed, 1 added, 1,182 unchanged, 0 removed**.
  Existing tests and fixtures are preserved (the original activation test bodies
  are unchanged; insertion added one blank separator before its harness).
  Formatting passes. Removing the gated observers and extractor recovers the
  prior normal compiler/startup source. Manifests and all generic-engine and
  downstream component inputs remain unchanged; their earlier results are reused.

[Findings](song-startup-capture-findings.json) preserve the initial 10-pass/2-failure
comparison, the native theme-event suppression correction, the explicit-nil test
correction, the 69-pass intermediate run and the lint cleanup. The source audit's
first matcher rejected only the inserted blank separator; the corrected matcher
then proved the old test bodies exactly. No coverage was deleted, weakened or
skipped. Verification durations above are not preparation/live performance data.

The normal production song path still uses the legacy backend. Shared-state
snapshot/rewind around future queued-command discovery, complete prepared/live
output integration, reload orchestration, fallback/cache integration, unconditional
production cutover and final fixture/application/render/dependency/link/performance
evidence remain required. Startup captures do not replace that finish line.

## Shared script state around queued-command discovery (2026-11-06)

The previous goal turn clarified scope without implementation progress. The
separately authorized migration objective was reread and the existing source
checkpoint verified before making these changes. Its finish line remains fixed.

The optional song adapter now exposes `discover_song_startup_commands` and
`restore_song_startup_locals`. Discovery saves global bindings, root command
environments (including `__songlua_env_target`), referenced non-actor tables and
Lua closure cells. A private scene attachment traces every saved Lua handle;
restoration keeps identities, cycles and aliases. The traversal uses a bounded
native stack and passes an 8,201-table comparison. Native closures and actor
state remain outside this shared-script snapshot, matching the existing compiler.

Snapshots remain until explicit rewind before chronological replay; discovered
script mutations remain visible while declarations are collected. Live and
replaced screens discard the queue barrier without probing. Probe errors retain
partial writes and discard the snapshot. Successful rewind consumes it. Foreign
and retired scene handles are rejected, and constructor scenes created by probes
are adopted even when the probe fails. No generic engine or C API work was added.

- [Final regression run](song-shared-state-focused-v4-song-linux-debug/result.json):
  **60 passed, 0 failed**, across activation (18), preparation (21), retirement
  (11) and song layers (10). Exit 0; 211.20786385098472 seconds;
  log SHA-256 `fa963dfb88dac681991670491365103b1e520c42dedaa7818ee955963b870bef`.
- [Final Clippy](song-shared-state-lint-v2-song-linux-debug/result.json):
  **166 existing warnings and the existing cmd.rs:349 error**, no new diagnostics.
  Exit 101; 30.481977861840278 seconds;
  log SHA-256 `6f860e9526987982e666c6e954316313dc235ddd8098a4c65380ced3ad0978e7`.
- [Source audit](song-shared-state-review.json), generated by
  [review_song_shared_state.py](review_song_shared_state.py): **1,198** watched
  source hashes match both final runs. Relative to the startup-capture checkpoint:
  **7 changed, 2 added, 1,189 unchanged, 0 removed**. Formatting passes; the
  existing test bodies/fixtures are preserved. The native utility changes are
  append-only gated test-oracle helpers. Normal compiler, manifest, generic-engine
  and downstream component inputs remain unchanged, so their recorded checks
  remain reusable.

[Findings](song-shared-state-findings.json) retain the initial compile errors,
16-pass/1-failure comparison and corrected new assertion, earlier 60-pass run,
and the single new Clippy style warning fixed before the final 60-pass run.
The assertion correction preserves both engines' matching diagnostic history;
no existing coverage was deleted, weakened or skipped. These verification times
are test costs, not representative preparation/live performance measurements.

The production song compiler still uses the legacy backend. Queued actor-state
and sprite-binding capture, replay setup, complete prepared/live output,
reload/session orchestration, fallback/cache, unconditional cutover, and final
fixture/application/render/dependency/link/performance evidence remain required.
This shared-state checkpoint does not complete the migration goal.

## Queued actor and media startup captures (2026-11-06)

The previous goal turn made progress by implementing shared-script rewind. The
migration objective and source checkpoint were reread before extending that
phase; its completion criteria remain unchanged.

`discover_song_startup_commands` now removes the startup queue barrier before
capturing actor properties, including wrappers and siblings that a probe later
removes. It flushes and clears only earlier capture blocks and their immediate
block cache; tween cursors, pending steps and commands survive. After probing,
`song_startup_queued_states` returns owned initial properties, discovered blocks,
empty queued step/command arrays and the compiler's ancestry clocks. Saved actor
identities remain traced by the loader; shared script rewind still waits until
declarations have been collected. Clocks are read after probing, matching the
existing compiler.

Before queued probes, Sprite bindings are deep copied (texture, frame declarations,
state defaults, texture epoch and frame-sheet fields). BitmapText/RollingNumbers
retain raw startup text after the real Update(0). Live and replaced screens skip
future queue captures. Malformed binding keys fail before dispatch with the queue
barrier removed, retaining partial writes and allowing subsequent direct commands.

A complete media comparison found that Rust `Sprite:Load` updated state without
recording three native capture writes. Size, texture-bound state and frame-index
reset are now recorded at their successful mutation points, including explicit
writes that equal prior values. Existing Sprite/actor behavior remains covered.

- [Final regressions](song-queued-capture-focused-v4-song-linux-debug/result.json):
  **114 passed, 0 failed** across actors (26), BitmapText (8), preparation (21),
  retirement (11), activation (22), song layers (10), sprites (8) and wrappers (8).
  Exit 0; 280.5415425361134 seconds;
  log SHA-256 `bed8daf86f73b3f4fe38ac079ab9094cb6c725e5a54361830a60c2ee69cbb26a`.
- [Clippy](song-queued-capture-lint-v1-song-linux-debug/result.json): exactly the
  recorded **166 warnings and existing cmd.rs:349 error**, with no new diagnostics.
  Exit 101; 36.47073893994093 seconds;
  log SHA-256 `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source audit](song-queued-capture-review.json), generated by
  [review_song_queued_capture.py](review_song_queued_capture.py): all **1,198**
  watched hashes match both final runs. Relative to the shared-state checkpoint:
  **12 changed, 0 added, 1,186 unchanged, 0 removed**. Formatting passes. Earlier
  test bodies and fixtures are unchanged. Removing gated observer additions
  recovers the prior normal compiler/startup source. Generic-engine, downstream
  and manifest inputs remain unchanged, preserving their recorded evidence.

[Findings](song-queued-capture-findings.json) retain the missing helper compile
error, the invalid BitmapText callback fixture (corrected by putting its update
callback on the supported ActorFrame parent), and the actual missing Sprite
capture writes. The complete engine-to-engine comparisons remain; no existing
coverage was deleted, weakened or skipped. The verification durations above are
test costs rather than representative preparation/live performance measurements.

Normal production songs still use the legacy backend. Replay setup, prepared/live
output integration, session/reload orchestration, fallback/cache, unconditional
cutover and final fixture/application/render/dependency/link/performance evidence
remain required. This capture checkpoint does not complete the migration goal.


## Chronological replay setup (2026-11-06)

The migration remains active and incomplete. This separately authorized goal
keeps the completed interpreter and C API-removal milestones closed.

`RustSongRuntime::begin_song_replay` restores saved script locals, the five saved
sprite binding fields and compiler-provided zero-frame getter values, then
rebuilds the captured Init/On queues. Callback-only actors derive their initial
replay pose from immediate captured blocks. Queue destinations retain the
associated block easing and optional parameter. The existing frame walker
consumes those queues with its hibernation, update-rate and callback ordering.
The boundary rejects live/replaced screens and invalid owners or phase reuse.

Preparation getters initially expose exported frame-zero fields independently
of the queue destination. Tween advancement publishes current and destination
fields; relative movement, color writes, stop and finish preserve the native
getter rules. Color changes retain channels from the exported getter state.
All new state lives in the song adapter; no generic engine API was added.

- [Final regressions](song-replay-setup-focused-v6-song-linux-debug/result.json):
  **127 passed, 0 failed**, across actors (26), BitmapText (8), effects (10),
  preparation (21), rendering (8), retirement (11), activation (27), sprites (8)
  and wrappers (8). Exit 0; 273.4485588062089 seconds;
  log SHA-256 `29062684a6c5e7d9be3c287f46e64ac7d3cdfc5e451dc34f334b7d441d80725f`.
- [Clippy](song-replay-setup-lint-v1-song-linux-debug/result.json): exactly the
  recorded **166 warnings and existing cmd.rs:349 error**, with no new diagnostics.
  Exit 101; 34.06814994988963 seconds;
  log SHA-256 `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source audit](song-replay-setup-review.json), generated by
  [review_song_replay_setup.py](review_song_replay_setup.py): all **1,199** watched
  source hashes match both final runs. Relative to the queued-capture checkpoint:
  **8 changed, 1 added, 1,190 unchanged, 0 removed**. Formatting passes; all earlier
  tests and fixtures are preserved. Removing the new gated observer recovers
  the prior normal compiler utility source. Engine, downstream and manifest
  inputs are unchanged, so their recorded results remain reusable.

[Findings](song-replay-setup-findings.json) preserve every intermediate failure:
frame-zero destination leakage, the invalid test-target invocation, the new media
fixture's incorrect delayed-discovery/default-frame assumptions, the missing
renderer import, and relative writes using the queue tail instead of exported
getter values. The corrected media fixture explicitly installs frame properties
and proves its immediate probe ran. Five complete differential comparisons were
added; no prior test or failing comparison was deleted, weakened or skipped.
Verification durations are test costs, not representative runtime measurements.

Normal production songs still use the legacy backend. The compiler must supply
its real overlay initial states and complete chronological output capture,
including tween progress and write provenance. Tracked actors/options/media/text,
retained live/session orchestration, fallback/cache, unconditional cutover and
final fixture/application/render/dependency/link/performance evidence remain
required. This adapter boundary does not complete the migration goal.


## Tween progress and startup block output (2026-11-06)

The existing chronological actor scheduler now records per-step `[song time,
native time remaining]` samples. Song time is explicit and independent of actor
delta or script music/beat clocks. Hibernation reduces a frame's active span;
update rates scale consumed actor time without scaling that span. Wrappers use
the owner's wake remainder before its update rate. Paused running tweens add hold
samples, while unstarted steps remain empty. Repeated sampling at one song time
does not append duplicate holds.

`advance_song_replay` also adopts actors created during callbacks, including
partial failures. `capture_song_tween_replay` copies owned progress and current
values for the explicitly written tween targets; it does not retain Lua values.
`SongTweenReplay::apply_startup_progress` attaches reached samples to original
startup command blocks using their captured step associations. Cancelled steps
keep their indices, so later appended queues cannot populate their output.

Three new differential comparisons check exact progress/pose output against the
actual legacy scheduler and final startup-block attachment. They exercise parent
hibernation, child rates, wrappers, hurry, paused/duplicate samples, cancellation,
new queued steps, unchanged explicit writes, callback errors/recovery, GC and
foreign/retired handles. The positive-clock assertions also prove samples use
song time rather than an implicit zero clock.

- [Regressions](song-tween-progress-focused-v1-song-linux-debug/result.json):
  **106 passed, 0 failed** across actors (26), effects (10), preparation (21),
  retirement (11), activation (30) and wrappers (8). Exit 0;
  260.37599004106596 seconds;
  log SHA-256 `0ed4b85904cc2ee9d9f324c36605cc91d5799d30306c1a5f242d72e02b58197e`.
- [Clippy](song-tween-progress-lint-v1-song-linux-debug/result.json): exactly the
  baseline **166 warnings and existing cmd.rs:349 error**, with no new diagnostics.
  Exit 101; 35.328642670065165 seconds;
  log SHA-256 `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source audit](song-tween-progress-review.json), generated by
  [review_song_tween_progress.py](review_song_tween_progress.py): all **1,199**
  watched source hashes match both runs. Relative to replay setup: **9 changed,
  0 added, 1,190 unchanged, 0 removed**. Formatting passes, earlier tests/fixtures
  are unchanged, and native utility additions are isolated test observers.
  Generic engine, downstream and manifest sources are unchanged; their earlier
  recorded checks remain reusable.

[Findings](song-tween-progress-findings.json) record the scope and evidence.
Verification durations are test costs, not representative runtime measurements.
Normal production songs still use legacy Lua. Full overlay extraction and
chronological write/message capture, actions/tracked actors/options/media/text,
retained live/session orchestration, fallback/cache, unconditional cutover and
final fixture/application/render/dependency/link/performance validation remain
required. The migration goal remains incomplete.


## Tween message provenance (2026-11-06)

The active migration now retains ordered message writes for the existing 25 tween
properties. `begin_song_replay` installs an indexed capture owned by its song
session; `song_replay_tween_messages` exports existing `SongLuaStatefulMessageCapture`
values without Lua handles. Duplicate actor indices use the compiler's last
index. Non-overlay actors do not enter the output, and retired/foreign scenes
cannot export it. Nested broadcasts restore their prior message scope, and
same-value writes remain present. Beats are read at each setter, so a handler's
clock changes are represented. Real queue tails and hibernation determine delays;
command duration, easing and opt1 remain distinct from that remaining time.

A new differential case exposed Hurry's capture-block flush: native Hurry clears
an open block's metadata before scaling the queue. The adapter now retains this
open-block state separately, clears it on cancellation/dispatch and preserves the
scaled queue's delay when a new tween is appended. Existing motion/progress
comparisons remain passing. All changes stay in the song adapter.

- [Regressions](song-message-provenance-focused-v5-song-linux-debug/result.json):
  **112 passed, 0 failed**, covering actors (26), messages (13), preparation (21),
  rendering (8), retirement (11) and activation (33). Exit 0;
  251.96033060504124 seconds; log SHA-256
  `95809fde17e483747705ada9953af9b4bcc4ee207027f62519353977e07b8f05`.
- [Clippy](song-message-provenance-lint-v1-song-linux-debug/result.json): exit 101,
  32.03913203184493 seconds; **166 warnings and the existing cmd.rs:349 error**.
  The log is byte-identical to the previous checkpoint, SHA-256
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source audit](song-message-provenance-review.json), from
  [review_song_message_provenance.py](review_song_message_provenance.py): **1,200**
  watched sources, **11 changed, 1 added, 1,188 unchanged, 0 removed** relative to
  the tween-progress checkpoint. Both runs match the current source. Formatting
  passes; prior tests and fixtures, normal legacy compiler, generic engine,
  downstream components and manifests remain unchanged. Their recorded unchanged
  component evidence is reused.
- [Findings](song-message-provenance-findings.json) retain the three initial
  build-setup failures and the subsequent 111-pass/1-failure differential run.
  No coverage was removed, ignored or weakened. The Hurry correction is verified
  by exact native output and a strengthened scaled-tail assertion.

This checkpoint exposes tween-property message output only. Non-tween immediate
and scheduled capture, full overlay extraction, tracked player/layer indices,
late wrapper remapping, actions/options/media/text output, retained live/session
lifecycle, fallback/cache and unconditional production cutover remain required.
Final fixture/application/render/dependency/link/performance validation remains
outstanding; these verification durations are not performance measurements.
The migration goal remains active and incomplete. The interpreter and C API
removal milestones remain closed.

## Immediate property publication, unresolved queue mode (2026-11-06)

The adapter now exports owned immediate-property frame buffers and cumulative
message writes. The 31-target projection includes base transforms, alignment,
visibility/hibernation, size/texture state, effect presets, culling and framebuffer
flags. Direct values, queued publications, final destinations, resets and
same-value message writes remain distinct. Six new comparisons cover these
routes, native compound-setter order, mixed pose/immediate actor ordering, queued
precision, partial errors, GC and owned output after scene teardown.

**This work is not yet verified: one new differential regression fails.** The
resource test passes its direct setter and invalid-input comparisons, then finds
that an actor without a startup queue still uses native command capture. Legacy
runs the queued body during capture and emits a relative delay of 1 second;
Rust runs it at the next explicit frame and emits a timestamp of 2 seconds.
The missing capture-scheduler distinction affects callback/shared-state ordering
as well as update metadata. No test was removed, ignored or weakened. The next
implementation must preserve that scheduler and its queued scope; changing the
expected output or only its timestamp would not resolve the defect.

- [Regression result](song-immediate-capture-focused-v3-song-linux-debug/result.json):
  **128 passed, 1 failed**, eight targets: actors 26, effects 10, framebuffers 6,
  preparation 21, render 8, retirement 11, activation 38 passed/1 failed, sprites 8.
  Exit 101; 301.1833476400934 seconds; log SHA-256
  `4fd6a5fe0a7796a79c62b2b1674c9d7af5b227f5b143c9524d1cc7b2a8a27002`.
- [Clippy result](song-immediate-capture-lint-v1-song-linux-debug/result.json):
  exit 101; 34.219553869916126 seconds. Its log remains byte-identical to the
  previous checkpoint: 166 warnings and the existing cmd.rs:349 error; SHA-256
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source audit](song-immediate-capture-review.json), from
  [review_song_immediate_capture.py](review_song_immediate_capture.py): all 1,201
  watched source hashes match both runs; 13 changed, 1 added, 1,187 unchanged,
  0 removed relative to message provenance. Formatting passes. Earlier tests,
  fixtures, normal legacy compiler, core/downstream sources and manifests are
  unchanged. The audit explicitly reports `regressions_passed: false` and retains
  the failing test name; its successful execution is not a green test result.
- [Findings](song-immediate-capture-findings.json) retain the initial import error,
  superseded 121-pass run, current failure and scheduler source references.

The goal remains active and incomplete. Delayed non-tween capture, full overlay
extraction, tracked actor/wrapper indices, actions/options/media/text output,
retained live/session orchestration, fallback/cache and unconditional production
cutover remain required, followed by the original fixture/application/render/
dependency/link/performance gates. Test durations are not runtime measurements.
The interpreter and C API removal milestones remain closed.

## Capture queue scheduling correction (2026-11-06)

The previous immediate resource regression is fixed. Actors without startup
queue steps now retain the existing command-capture path. A queued command
outside a frame can run synchronously, and its writes carry the relative
captured start rather than an invented frame timestamp. Nested dispatch scopes
restore their parent on failure. The first frame-created queue discards old
captured durations so an earlier sibling's next command is not postponed.
Startup replay queues retain their existing chronological dispatch.

The original native/Rust equality comparison remains enabled. Its additional
Some(2) expectation was an unproven assumption; native evidence establishes None
plus a one-second relative delay. That assertion now matches native output and
is strengthened by checking that the body runs before frame advancement and
runs only once. Two additional cases compare nested partial errors and scope
restoration, and earlier/later actor ordering over the real native frame plan.
The latter exposed stale captured durations and passed after their removal.

- [Regressions](song-static-queue-focused-v3-song-linux-debug/result.json):
  **144 passed, 0 failed**, nine targets: actors 26, effects 10, framebuffers 6,
  messages 13, preparation 21, render 8, retirement 11, activation 41, sprites 8.
  Exit 0; 318.9885217009578 seconds; log SHA-256
  `9eee551ecffe5308dee82b908a06160fa76d819c98df4f5622c4aa1036a07dad`.
- [Clippy](song-static-queue-lint-v1-song-linux-debug/result.json): exit 101;
  33.86567255598493 seconds. The log is byte-identical to the prior baseline:
  166 warnings and the existing cmd.rs:349 error; SHA-256
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source audit](song-static-queue-review.json), from
  [review_song_static_queue.py](review_song_static_queue.py): 1,201 watched
  sources, 9 changed and 1,192 unchanged, no additions/removals. Final hashes
  match Clippy. Regression hashes differ only by the separately recorded
  [API documentation correction](song-static-queue-doc-comment.json); reversing
  that exact comment restores the tested hash. No executable source changed
  after the regression run. Formatting passes. The earlier 38 activation cases,
  all other tests/fixtures, normal legacy compiler, core/downstream sources and
  manifests are preserved; the failing resource case is strengthened as above.
- [Findings](song-static-queue-findings.json) preserve the initial name-shadowing
  compile error and the subsequent 100-pass/1-failure ordering run.

This fixes the recorded cases without claiming complete preparation scheduling.
Queue-clock prediction, recurring capture, delayed non-tween and pose output,
full overlay extraction, tracked actors/wrappers, actions/options/media/text,
retained live/session orchestration, fallback/cache and unconditional production
cutover remain required. The original fixture/application/render/dependency/link/
performance gates remain outstanding. Test durations are not performance results.
The migration remains active; interpreter and C API milestones remain closed.

## Recurring command capture (2026-11-06)

The separately authorized production migration remains active. Its acceptance
criteria are unchanged; the interpreter and C API removal milestones remain
completed. The previous queue-scheduling correction was progress toward that
migration. This change implements recurring commands already captured outside
frame updates, using exact command intervals alongside native f32 timer steps.

Four native/Rust comparisons cover ordering around child and update callbacks,
hibernation and update rates, conditional renewal, zero/negative deltas, the
64-run cap, Hurry/Stop/Finish, partial callback errors, GC, one-time reporting and
dispatch restoration. They compare actual native frame advancement plus complete
supported immediate/message buffers; all earlier activation cases are preserved.

- [Regressions](song-recurring-capture-focused-v2-song-linux-debug/result.json):
  **148 passed, 0 failed across nine targets**, 183.483740 seconds, unchanged
  sources. Log SHA-256:
  `4648a7c0e0153b0abc20ca1c7dfcbf7949575d165abacac2528bf3a725cdb3a4`.
- [Clippy](song-recurring-capture-lint-v1-song-linux-debug/result.json): exit 101,
  exactly the recorded 166 warnings and existing cmd.rs:349 error, no new
  diagnostics. Log SHA-256:
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source review](song-recurring-capture-review.json): formatting passes; all
  1,202 watched source hashes match both runs. Four files changed, one scheduler
  module was added, and 1,197 files remain unchanged from the prior checkpoint.
  Existing fixtures, core engine, downstream consumers and manifests are intact.
- [Findings](song-recurring-capture-findings.json) record the initial 104-pass
  check, final coverage and limits. Verification durations are not application
  performance measurements.

Recurrence first created in dynamic frame queues and future queue-clock prediction
still require integration, along with delayed property/pose output, full compiler
and live/session orchestration, fallback/cache handling and the original cutover,
fixture, application/render, dependency/link and performance validation. Production
song execution still uses legacy Lua. This check does not complete the migration.

## Scheduled renderer properties and pending destinations (2026-11-06)

Continued the fixed production migration with ten renderer targets outside the
interpolated tween pose. The shared property buffer preserves ordered direct and
scheduled writes, delays/durations/easing, repeated destinations and message
provenance. The prior immediate-only drain remains a projection of that buffer;
callers choose one drain per sampling pass. Compiler-supplied pending render
values provide starting destinations across drains and are cancelled by Stop or
merged by Finish. Finish updates render state from the merged destinations and
normalizes disabled FOV. Actor depth flags propagate one flag at a time through
children, retaining the native write order.

Three new comparisons use actual legacy capture buffers and pending-tween data.
They cover all ten targets, invalid arguments, nested messages, partial failures,
Hurry/Stop/Finish, same-frame writes superseding old pending values, FOV sentinel,
GC, scene ownership and owned output after teardown. All 45 earlier activation
cases and all other existing fixtures remain unchanged.

- [Regressions](song-render-properties-focused-v3-song-linux-debug/result.json):
  **151 passed, 0 failed across nine targets**, 328.750785 seconds; unchanged
  sources. Log SHA-256:
  `fac9e4813ca094e1a523e5fb850f4c611669e295305e26b6e743e9c5a862d5fa`.
- [Clippy](song-render-properties-lint-v1-song-linux-debug/result.json): exactly
  the existing 166 warnings and cmd.rs:349 error; no new diagnostics. Log SHA-256:
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source review](song-render-properties-review.json): formatting passes and all
  1,203 watched source hashes match both runs. Seven files changed, one module
  was added and 1,195 files remain unchanged. Core, downstream consumers,
  manifests, normal production compilation and historical fixtures are intact.
- [Findings](song-render-properties-findings.json) retain development outcomes:
  v1 passed all 79 existing selected cases but exposed two new fixture
  assumptions; v2 passed 151 checks; review then tightened Finish getter handling
  and its coverage for the final v3 run. Verification time is not an application
  performance measurement.

This proves captured renderer output without future frame-clock prediction.
Queue-progress/cross-actor advance prediction, dynamic recurrence and remaining
pose/text/sprite channels still require integration. Full compiler/live sessions,
fallback/cache, unconditional cutover and the original fixture/application/render,
dependency/link and performance validation remain required. Production song
execution still uses legacy Lua. The goal remains active and incomplete; the
completed interpreter and C API removal milestones remain closed.

## Preparation queue-clock prediction (2026-11-06)

Continued the existing production migration. The previous renderer-property
capture was progress toward the same fixed acceptance criteria. The interpreter
and C API removal milestones remain completed.

The Rust adapter now accepts a canonical double-precision frame timeline and
optional actor delta/global-span rows. Compiler-owned fallback clocks retain
queue steps, epochs, traversal order, zero-state visibility, queued dispatch
remainders and shared immutable back-tween samples. Scheduled render publications
use those predictions for delay, progress and frame advancement. The streamed
interface retains its prior path until a timeline is installed. Hurry invalidates
cached samples; Stop/Finish clear queue clocks; release, retirement and entering
live mode discard prediction state. Nonfinite/unrepresentable forecasts stop
rather than running an unbounded prediction loop.

Four new comparisons use the actual native scheduler and capture buffers. They
cover earlier/self/later targets, repeated destinations, callback dispatch,
actor delta/span rows, zero tails, partial update errors, GC, owner/mode checks,
Hurry/cancellation, owned samples after teardown and final-frame zero states.
All 48 earlier activation tests and existing fixtures are unchanged.

- [Final regressions](song-queue-clock-focused-v4-song-linux-debug/result.json):
  **155 passed, 0 failed across nine targets**, 371.768711 seconds;
  sources unchanged. Log SHA-256:
  `d5ac4e4b5073629c2c2f15f7f749e0845103da9707478cfb416cae42b7f3f4d4`.
- [Final Clippy](song-queue-clock-lint-v3-song-linux-debug/result.json): baseline
  166 warnings and existing cmd.rs:349 error; no new diagnostics. Log SHA-256:
  `e0eab6a37e5621655159412d3f0c6843980e6bf551476c7dce6b46f352028521`.
- [Source review](song-queue-clock-review.json): formatting passes; all 1,204
  watched source hashes match both final checks. Ten files changed, one module
  was added and 1,193 files remain unchanged. Core, downstream consumers,
  manifests, historical fixtures and normal production compilation are intact.
- [Findings](song-queue-clock-findings.json) retain the focused three-pass run,
  broad 155-pass runs, two new lint warnings and their correction, and the final
  live-mode cleanup. Durations include builds/Cargo waits and are not application
  performance measurements.

This checks future prediction for scheduled render output. Complete fallback and
recurring queue execution, remaining pose/text/media output, tracked/wrapper
mapping, full compiler/live sessions and fallback/cache still require integration.
Unconditional production cutover and the original fixture/application/render,
dependency/link and performance gates remain outstanding. Production songs still
use legacy Lua. The goal remains active and incomplete; its finish line is unchanged.

## Fallback command execution and recurrence handoff (2026-11-06)

Continued the existing production migration. The prior queue-clock prediction
was progress toward the same fixed acceptance criteria. The completed interpreter
and C API removal milestones remain closed.

Full-timeline preparation now retains fallback capture queues and dispatches
commands from the canonical clock. Actor update rates no longer shift those
callbacks to a different frame. Hibernation can release a pending callback at a
zero-delta wake; its publication retains the predicted timestamp and actual frame
advance. A newly self-queued recurring command receives only the saved dispatch
remainder and is not advanced twice in the same frame. Runtime queue errors
report once, preserve partial writes and permit later actor jobs. Nested inline
commands explicitly override absent timestamps and restore their caller's scope.
Queue overflow accounting reads the canonical pending steps.

Four comparisons use actual native scheduling and property buffers: rates and
hibernation; recurring handoff with partial errors; queue errors across Stop;
and an inline child without its parent's timestamp. The original three cases
first failed against the old executor and now pass without changes. All 52
previous activation tests and historical fixtures are preserved.

- [Regressions](song-fallback-execution-focused-v3-song-linux-debug/result.json):
  **159 passed, 0 failed across nine targets**, 359.178765 seconds;
  sources unchanged. Log SHA-256:
  `f31bc9e6985d0830c71b48300653501079ea09fcd21506d497b5d10450d6d04c`.
- [Clippy](song-fallback-execution-lint-v1-song-linux-debug/result.json): exactly
  the baseline 166 warnings and existing cmd.rs:349 error. No new diagnostics.
  Log SHA-256:
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source review](song-fallback-execution-review.json): formatting passes and all
  1,204 watched source hashes match both final checks. Five files changed, none
  were added/removed, and 1,199 remain unchanged. Core, downstream consumers,
  manifests and normal production compilation remain intact. The audit also
  reconstructs and hashes the three originally failing test cases unchanged.
- [Findings](song-fallback-execution-findings.json) retain the initial three
  failures, the focused three-pass run, final coverage and limits. Durations are
  verification costs, not application performance measurements.

These checks cover callback timing and the supported immediate/render buffers.
Complete fallback pose sampling and getter reconciliation, remaining pose/text/
media output, startup and retained-session orchestration, tracked/wrapper mapping,
full compiler/live consumers and fallback/cache still need integration.
Unconditional cutover and the original fixture/application/render, dependency/link
and performance gates remain outstanding. Production songs still use legacy Lua.
The migration remains active and incomplete; its finish line is unchanged.

## Fallback pose output and sampled getter handoff (2026-11-06)

This continues the fixed production migration; the interpreter and C API removal
milestones remain closed. Full-timeline fallback capture now publishes all 25
existing tween-pose targets. The new full overlay drain covers 66 targets and
shares storage with the existing 31/41-target projections. Pending destinations,
Finish/Stop, partial errors and ordered message writes remain owned Rust output.
Startup native tween queues retain their separate replay path.

The compiler handoff accepts previous-frame XYZ and sampled pose getters.
Position samples update current getters; other supported pose samples update the
destination fields read by relative setters. Direct XYZ writes update later-actor
prior reads. Stop resets the position destination to the frozen current value;
Finish merges pending destinations and newer writes. Successful queued and
recurring commands reconcile fallback current XYZ at their native boundaries.

- [Regressions](song-pose-output-focused-v5-song-linux-debug/result.json): Cargo
  exit 0, **162 passed, zero failed across nine targets**; activation has 59 cases,
  preserving the exact earlier 56-case prefix. The new comparisons include all
  25 pose targets, sampling and relative writes, timing/progress, message order,
  cancellation, GC, owner checks and owned output after retirement/VM destruction.
- [Clippy](song-pose-output-lint-v4-song-linux-debug/result.json): exit 101 with
  exactly the existing 166 warnings and cmd.rs:349 error; no new diagnostics.
  Formatting passes for all eleven modified adapter/test files and the new module.
- [Source review](song-pose-output-review.json): both final whole-worktree
  snapshots retain `sources_unchanged=false`, and the wrapper assertions failed.
  The separately captured [test dependency graph](song-pose-output-test-dependencies.json)
  proves the changing shell source is outside these checks. All watched inputs
  within that graph match the final regression and lint snapshots. Independent
  source changes and two added downstream tests are preserved and recorded;
  this is not a clean whole-worktree or final application validation claim.
- [Findings](song-pose-output-findings.json) explain the initial literal compile
  error, the corrected native Stop expectation and the source-audit limitations.
  No failing coverage was deleted or ignored. The [source diff](song-pose-output-source-diff.patch)
  and documentation baseline retain the edit boundary.

Remaining existing text/sprite/media output, full overlay/track/wrapper extraction,
production compiler and retained live/session orchestration, fallback/cache and
unconditional cutover still require integration. Original content, final-source
regression, application/render, dependency/link and performance gates remain
required. Production song execution still uses legacy Lua. The migration is open.

## Text, mask, sprite and texture preparation output (2026-11-06)

This continues the fixed production migration. The 23 remaining existing media
setter properties now enter startup capture and scheduled frame/message output.
The full overlay drain covers 89 authored setter targets; the older 31/41-target
views remain consuming projections. Actor masks, animation, loop/rate, text
layout/style flags and texture coordinates retain native setter order, including
equal writes. Sprite state selection is immediate; rectangle state clears remain
scheduled. CropTo/scaletoclipped now publish their existing size/zoom mutations.

[Regressions](song-media-output-focused-v2-song-linux-debug/result.json) pass
**199 tests across 13 targets**, including four new comparisons and the unchanged
59-case activation prefix. Coverage includes all 23 media properties, direct and
timed values, narrow drains, retained texture handles, state/geometry write order,
partial errors, pending Stop/Finish, GC, retirement and startup/queued capture.
The broader group includes actors, bitmap text, effects, framebuffers, messages,
models, preparation, render, retirement, activation, sound, sprites and wrappers.
Cargo exited 0 in 387.6866857719142 seconds; log SHA-256
`743b3802a759905ffd9082a3ae3df308a5d87ef3f90e87a2eebd99a8cad2d760`.

[Clippy](song-media-output-lint-v2-song-linux-debug/result.json) retains exactly
the existing 166 warnings and cmd.rs:349 error (exit 101), with no new diagnostics.
Formatting passes. The [source review](song-media-output-review.json) confirms
all 1,208 watched hashes match both final runs, eleven modified files and one new
module, with no other source changes from the previous reviewed checkpoint.
The [findings](song-media-output-findings.json) preserve the initial native failure:
the production class does not expose lowercase `setallstatedelays`. The test now
uses supported `SetAllStateDelays`, and the extra Rust alias was removed. No test
or assertion was dropped. Source/documentation diffs and the earlier content are
preserved in this directory.

Derived EffectTime and cached StretchRect output still belong to complete
extraction. Production compiler/live/session/navigation, automatic teardown,
fallback/cache, unconditional cutover and the original content/regression,
application/render, dependency/link and performance gates remain pending. Normal
song execution still uses legacy Lua. The migration remains open; the completed
interpreter and C API-removal milestones remain closed.
## Tracked compiler actors and manual draw snapshots (2026-11-06)

This continues the explicitly authorized production migration with its fixed
finish line. The interpreter and C API-removal milestones remain closed.

The Rust collector now reads the published top-screen registry and both players
in native order, exports Player/Judgment/Combo, SongForeground and optional
screen-layer state, and preserves aliases and missing optional entries. Missing
foreground/HUD actors use the existing actual constructors, including native
partial mutations before a later read fails. A pinned batch retains identities
before authored getters can collect them. Copied rows reject foreign VMs and
released generations. Explicit release is required on compiler/session exit;
owned captured state remains available after release and VM destruction.

Manual Draw snapshots copy raw semantic fields and mutable table data, retain
wrapper identities, and leave authored property reads at their native boundary.
Hidden HUD draws do not mark manual output; drawing a whole player does not
resurrect it. Uninitialized preparation Sprites resolve Texture/AFT at draw time.
Inferred binding is omitted from raw snapshots, while explicit texture writes
publish the native flag used by later reads. All behavior remains in the song
adapter; no core compatibility layer or interpreter milestone was reopened.

- [Regressions](song-tracked-output-focused-v8-song-linux-debug/result.json):
  **242 passed, zero failed across 17 targets**, Cargo exit 0 in
  295.5799291068688 seconds. Activation has 69 cases, preserving the exact prior
  63-case prefix. Six new comparisons cover preparation/live output and order,
  actual child fallback, aliases, manual state and getter events, partial errors,
  forced GC, retirement, foreign owners, explicit release and owned teardown data.
  Log SHA-256: `0f411e367239feb4b7ddd632e40c6a243fcafb09021cebe0b5ef0bd4f11b7a8a`.
- [Focused corrected cases](song-tracked-output-focused-v7-song-linux-debug/result.json):
  six passed before the complete group was rerun. The earlier failed runs remain
  preserved. The [findings](song-tracked-output-findings.json) explain the initial
  HostCall API compile error, invalid native fixture, omitted draw getter,
  inferred snapshot flag and missing explicit texture-state publication. No
  failing test or assertion was removed or ignored.
- [Clippy](song-tracked-output-lint-v5-song-linux-debug/result.json): exit 101,
  exactly the baseline 166 warnings and existing cmd.rs:349 error; no new
  diagnostics. Log SHA-256: `175b2b7f8d9d4ae491fce4f2236f73fcb43e91087b24c8f1b5d6db4abe6ea5ea`.
- [Source review](song-tracked-output-review.json): all 1,211 watched source
  hashes match the final regression and lint runs; both recorded unchanged
  sources. Seven adapter/test files changed and three modules were added.
  Formatting passes for those ten files. Native compile.rs changes only one
  crate-private visibility prefix to let test-support reuse the actual default
  constructor, with all other bytes retained and no bulk legacy formatting.
  Source/documentation baselines and the [source diff](song-tracked-output-source-diff.patch)
  preserve the change boundary. Durations are check costs, not application
  performance measurements.

Full overlay kind/tree/wrapper/message/action and remaining derived output,
production preparation compiler and retained live/session/navigation lifecycle,
automatic teardown and fallback/cache integration still need completion.
Unconditional song cutover, original content/final-source regressions, normal
application build and rendered smoke, dependency/link proof and matched
preparation/live/memory measurements remain required. Normal production songs
still use legacy Lua. The migration remains active and incomplete.

## Renderer-kind and resource extraction (2026-11-06)

This continues the fixed production migration. The completed interpreter and C
API-removal milestones remain closed. The renderer-kind stage now uses the Rust
host API and existing owned readers for the native actor kinds, framebuffer
settings, proxy classification, Sprite/Banner, Sound, text/input displays,
ActorMultiVertex, Model/noteskin tap models, SongMeterDisplay, GraphDisplay and
Quad. CourseContentsList retains its ActorFrame projection.

The compiler supplies its previously captured class/state and AFT name set.
Startup sprite bindings and collapsed capture names preserve resource selection.
Startup text retains its original content while still performing the native eager
authored Text lookup. Empty meshes are retained live and skipped in preparation.
Application model loaders preserve noteskin slot and ordinary layer behavior,
missing resources and native error context. A pin retains each actor through
resource loading and is released on success/error. Payload data survives teardown;
generic proxy identities require the later tree remap before actor roots release.

- [Final regressions](song-kind-output-focused-v3-song-linux-debug/result.json):
  **279 passed, zero failed across 21 targets**, Cargo exit 0 in
  489.3442595549859 seconds; sources unchanged. Log SHA-256:
  `6355e29dab2d27ff4190704085d966fc236186a9bf0125667aaff50782cae99d`. Activation now has 74 tests with the exact prior 69-case
  prefix retained. Five new comparisons cover preparation/live variants, startup
  sprite/text reads, AFT name selection against the actual native tree collector,
  model-loader errors, missing resources, recovery, GC and owned teardown output.
- [Clippy](song-kind-output-lint-v2-song-linux-debug/result.json): exactly the
  existing 166 warnings and cmd.rs:349 error, exit 101; no new diagnostics. Log
  SHA-256: `e0eab6a37e5621655159412d3f0c6843980e6bf551476c7dce6b46f352028521`. The first lint pass's two new warnings were fixed
  with a boxed payload variant and a test-oracle type alias, without suppression.
  The complete regression group was rerun afterward. Earlier results remain
  preserved in the [findings](song-kind-output-findings.json).
- [Source review](song-kind-output-review.json): formatting passes for ten
  modified files and the new song_kinds module. All 1,212 watched hashes match
  both final runs, with no other changes from the tracked-output checkpoint.
  Native production code, core, manifests and backend selection are unchanged.
  Source/documentation baselines and the [source diff](song-kind-output-source-diff.patch)
  retain the edit boundary. Reported durations are verification costs, not
  representative application performance measurements.

These checks use controlled resources and application loader hooks. Full tree/
wrapper/global-reference traversal, proxy remapping, message-command probes,
action capture and remaining derived output still require integration. Production
preparation compiler and retained live/session/navigation lifecycle, automatic
teardown, fallback/cache, unconditional song cutover, legacy normal/build removal
and all original content/final-source regression, application/render, dependency/
link and preparation/live/memory gates remain pending. Normal production songs
still use legacy Lua. The migration remains active and incomplete.

## Shared script state for command probes (2026-11-06)

This continues the fixed production migration. The completed interpreter and C
API-removal milestones remain closed. The startup snapshot implementation now
also accepts selected command functions, retaining its native policy for global
and custom environment bindings, referenced helper functions, shared upvalue
cells, table contents, aliases and cycles. Actor tables remain excluded from
that traversal; their mutable state belongs to the separate action-capture scope.

Input owner/function/source values are pinned before allocating the attachment.
The owner then traces every saved handle. The original startup source-actor
retention is preserved independently of function closure references. Test support
also pins call arguments and successful returned values across rewind, releases
its temporary roots, and checks owner attachment restoration after success/error.
Native host function roots retain their own host binding data. The native oracle
only appends a test-support delegate to the actual existing snapshot/restore code.

- [Final regressions](song-probe-state-focused-v3-song-linux-debug/result.json):
  **285 passed, zero failed across 21 targets**, Cargo exit 0 in
  502.0398706968408 seconds; sources unchanged. Log SHA-256:
  `27a36b60ee92645e198fb6d3e38ac9fa45a90b31850574cf68a710b2c638829b`. Activation has 80 tests and preserves the exact prior
  74-case prefix. Five native comparisons cover shared state and global bindings,
  custom environments, error/recovery, returned table/closure/nil values, aliases,
  GC/retirement, native function roots and a 4,200-table graph. A sixth case checks
  actor source retention without registry/global/closure references.
- [Clippy](song-probe-state-lint-v2-song-linux-debug/result.json): exactly the
  existing 166 warnings and cmd.rs:349 error, exit 101; no new diagnostics. Log
  SHA-256: `e0eab6a37e5621655159412d3f0c6843980e6bf551476c7dce6b46f352028521`.
- [Source review](song-probe-state-review.json): formatting passes for three
  modified adapter/test files and the isolated native oracle append. All earlier
  native production bytes are retained, avoiding bulk formatting. The 1,212-file
  review confirms that watched dependency inputs match both final checks. A
  later Simply Love pad_config.rs edit lies outside the [captured test graph](song-probe-state-test-dependencies.json)
  and is preserved. The [original post-review mismatch](song-probe-state-final-state-v1.json)
  remains recorded. This is not whole-application final-source proof. Core,
  manifests and backend selection are unchanged.
  The [source diff](song-probe-state-source-diff.patch) and source/documentation
  baselines preserve the edit boundary.
- [Findings](song-probe-state-findings.json) retain the five-case focused result
  and interim 284-test run. Review then restored the old actor-source lifetime
  contract and added its GC test before the complete group was rerun. No failing
  coverage was removed, weakened or ignored. Durations are verification costs,
  not representative application performance measurements.

This verifies the shared-script layer, not complete actor-aware command probing.
Mutable actor state and capture buffers, nested command-local scopes, RNG and
default message parameters still need integration. Full tree/wrapper/global
reference extraction, proxy remapping, derived output, production preparation
compiler and retained live/session/navigation lifecycle, automatic teardown,
fallback/cache and unconditional song cutover remain pending. Original content/
final-source regression, application/render, dependency/link and preparation/
live/memory gates remain required. Normal production songs still use legacy Lua.
The migration remains active and incomplete.


## Indexed actor-function capture (2026-11-06)

This continues the fixed production migration and leaves the completed interpreter
and C API removal milestones closed. `capture_song_actor_function` ports the
existing indexed compiler easing/action helper: optional scalar argument and
probe beat, caller actor order and duplicate indices, semantic state rewind,
empty post-probe capture buffers, and owned command blocks. Script globals,
upvalues and RNG intentionally remain the outer probe scope's responsibility.

Raw semantic values, wrapper membership and owned pose/media/effect/queue state
are saved separately. Resetting capture history preserves original queue steps.
Inputs and saved Lua handles stay rooted across callbacks, reentry and GC. The
first successful Lua return remains rooted through restoration and clock rewind,
then is released; extra returns are discarded. Setup, authored execution and
restoration use separate VM entries so a fatal instruction abort does not block
cleanup. Restoration invalidates traversal plans through the existing registry
root-release path. Callback errors rewind state and clock; snapshot/setup errors
retain the native clock boundary. No engine limit implementation was changed.
Effect setters now record their missing command-block deltas through the common
capture writer, retaining existing ordered frame/message publications.

- [Final regressions](song-actor-capture-regressions-v1-song-linux-debug/result.json):
  **293 passed, zero failed across 21 targets**, exit 0 in
  485.06453990307637 seconds, sources unchanged. Log SHA-256:
  `4255544798101e600e0f6746e66aba1c29c7f0f9729b1335abcb404e663bad59`.
  All original 80 activation tests are an exact prefix of the current 88 cases.
  Eight added cases cover index order/duplicates and clock, pose/media/effect/raw
  state, wrapper membership, nested queues and shared-state persistence,
  reentrant restoration with GC, callback/snapshot/foreign-value errors,
  instruction-limit recovery, preservation of preexisting tween steps, and
  first-return lifetime/release. Native comparisons delegate directly to the
  existing indexed helper; no native production implementation was modified.
- [Clippy](song-actor-capture-lint-v1-song-linux-debug/result.json): exactly the
  existing 166 warnings and cmd.rs:349 error, exit 101 in
  38.928643341874704 seconds; no new diagnostics. Log SHA-256:
  `7c3989b48c23b47996efe57a081dc364092c3cfc9ad2cd663ccdbb954ef93dd1`.
- [Source review](song-actor-capture-review.json): all 1,213 watched inputs match
  both checks; formatting passes for nine changed adapter/test files and the new
  function-capture module. The [test dependency graph](song-actor-capture-test-dependencies.json)
  is captured. Core, native production sources, manifests and backend selection
  are unchanged in this step. The [source diff](song-actor-capture-source-diff.patch)
  and edit/documentation baselines retain the exact change boundary.
- [Findings](song-actor-capture-findings.json) preserve the failed scan-lease,
  borrow-checking, effect-publication, startup-fixture and fatal-cleanup attempts,
  the eight-case focused pass, and the final group. The queue fixture now ends
  native and Rust startup discovery before probing queued commands; its original
  output/shared-state assertions remain. No failing coverage was removed,
  weakened or ignored. Durations are verification costs, not representative
  preparation/live/memory measurements.

The indexed helper is implemented, but full actor-aware action/message capture
still requires lazy touched-actor scopes, nested command-local rewind, RNG and
default message parameters. Full tree/wrapper/global-reference extraction, proxy
remapping, remaining derived output, production preparation compiler and retained
live/session/navigation lifecycle, automatic teardown, fallback/cache and
unconditional song cutover remain pending. Original content/final-source
regressions, normal application/render, dependency/link and preparation/live/
memory gates remain required. Normal production songs still use legacy Lua.
The migration remains active and incomplete.

## Command-local state inside actor probes (2026-11-06)

This continues the fixed production migration. The command-local portion of an
actor-aware action scope now preserves Lua closures first reached through
playcommand, queues, broadcasts, group/course commands and updates, including
zero-delta startup updates. Ordinary execution leaves the scope inactive.

The existing script snapshot traversal also runs in a nested host callback with
one native-stack lease. Saved cells, function identities, aliases, cycles and
referenced table contents stay rooted through the runtime registry's private
host-state chain. Locals-only capture does not shallow-copy global or custom
environment bindings; the outer shared-script snapshot retains that separate
responsibility. Native host functions have no Lua upvalue cells to rewind.
Nested scopes merge encountered functions/snapshots into their owner, keeping
locals advanced between stability attempts; the outermost scope restores in
reverse snapshot order so earliest values win. RNG restores at every boundary.
Failed snapshot construction releases its attachment. Static review also added
release of remaining saved attachments after an early restoration error.

- [Final regressions](song-command-locals-regressions-v2-song-linux-debug/result.json): **309 passed, zero failed across
  22 targets**, exit 0 in 487.2445682140533 seconds; sources unchanged.
  Log SHA-256: `cf0b7ee34b93fcac96365dcefa7ae4cae47d4753456637a2854518282279504a`.
  Activation has 93 cases with the exact prior 88-case prefix unchanged. Five
  native comparisons cover dynamically reached command closures, custom
  environment binding persistence versus table rewind, shared cells/cycles and
  detached commands with GC/retirement, callback and instruction-limit recovery,
  and nested local/RNG boundaries. The course target adds its existing 11 cases
  because its dispatch also uses the new command-call wrapper.
- [Clippy](song-command-locals-lint-v1-song-linux-debug/result.json): exactly the existing 166 warnings and
  cmd.rs:349 error, exit 101 in 34.072165836114436 seconds; no new diagnostics.
  Log SHA-256: `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source review](song-command-locals-review.json): all 1,214
  watched inputs match both final checks. Formatting passes for nine modified
  adapter/test files, the new scope module, and the isolated test-only oracle
  append. All earlier native production bytes are unchanged. The oracle delegates
  to the actual existing action-scope begin/restore functions. Core, manifests
  and backend selection are unchanged in this step. The [test dependency graph](song-command-locals-test-dependencies.json),
  [source diff](song-command-locals-source-diff.patch) and edit/documentation
  baselines preserve the exact review boundary.
- [Findings](song-command-locals-findings.json) preserve the first compile failure
  in the test string accessor, the five-case focused pass and interim 309-case
  group. The full group was rerun after static review tightened attachment
  cleanup. No failing coverage was removed, weakened or ignored. Durations are
  verification costs, not representative application performance measurements.

The command-local layer is implemented; full actor-aware probing still requires
lazy touched-actor semantic snapshots/capture-buffer reset, default message
parameters and action/message orchestration. Full tree/wrapper/global-reference
extraction, proxy remapping, remaining derived output, production preparation
compiler and retained live/session/navigation lifecycle, automatic teardown,
fallback/cache and unconditional song cutover remain pending. Original content/
final-source regressions, normal application/render, dependency/link and
preparation/live/memory gates remain required. Normal production songs still use
legacy Lua. The interpreter and C API removal milestones remain closed; this
production migration remains active and incomplete.

## Lazy touched-actor function capture (2026-11-06)

This implements the existing compiler touched-function helper within the fixed
migration scope. Actor semantic snapshots occur at the first capture-producing
write. Reads, invalid/no-op setters and settext alone do not mark actors touched.
The caller's selected order and duplicate indices survive; all touched actors
are reset/restored, including unselected actors and detached queued targets.
Preexisting semantic queue steps remain intact. Command-local and RNG rewind
follow the native boundary; the touched set is frozen before cleanup activity.

Saved rows, replaced globals and the first callback return stay rooted through
GC and authored restoration callbacks. Cleanup host callbacks are allocated
before scope activation. Fresh VM cleanup entries recover after fatal execution
limits. Native mutation boundaries are wired through actor, render, effect,
media, lifecycle, wrapper, framebuffer and mesh bindings. Shared renderer update
paths now retain their missing command deltas. SetState retains the native empty
timed rectangle-clear marker; geometry scale commands retain their pose deltas.

- [Final regressions](song-touched-capture-regressions-v1-song-linux-debug/result.json): **326 passed, zero failed across 23 targets**,
  exit 0 in 391.53915316192433 seconds; sources unchanged. Log SHA-256:
  `7aebaf719f26c95b86759e2c6d667b6b46f8b9acc2c4db0d7192fbea5ba59663`.
  Activation has 104 cases with the exact earlier 93-case prefix preserved. Eleven
  additions exercise lazy selection, write boundaries, independent direct/timed
  setter families, queue/local/RNG scopes, wrapper membership, partial errors,
  fatal-limit recovery, return/global lifetime through GC, structural sort and
  subsequent generic updates, and preexisting queues. Banner coverage also runs
  because its loader shares the sprite path.
- [Clippy](song-touched-capture-lint-v1-song-linux-debug/result.json): the exact existing 166 warnings and cmd.rs:349
  error, exit 101 in 31.544781579170376 seconds; no new diagnostics. Log SHA-256:
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source review](song-touched-capture-review.json): all 1,217 watched inputs match both final checks.
  Formatting passes for 17 modified adapter/test files, the new scope module and
  the isolated test-only native oracle append. Native production bytes are
  unchanged. Six unrelated shell edits/additions outside the captured test graph
  are preserved. Core, manifests and backend selection are unchanged here. The
  [dependency graph](song-touched-capture-test-dependencies.json),
  [source diff](song-touched-capture-source-diff.patch), and edit/documentation
  baselines preserve the review boundary.
- [Findings](song-touched-capture-findings.json) retain all focused compile/runtime failures and the final passes.
  The new traversal test's initial cached-order expectation was corrected to the
  actual native pre-replay generic traversal, which rebuilds b|a after sorting;
  native and Rust already agreed. Existing coverage remains unchanged. The v1
  sources_unchanged=false record remains intact for concurrent shell edits.
  No coverage was removed, skipped or weakened. Test durations are verification
  costs, not representative preparation/live/memory measurements.

Default message parameters and complete action/message orchestration remain
pending, followed by full tree/wrapper/global-reference extraction, proxy
remapping, remaining derived output, production preparation compiler and retained
live/session/navigation lifecycle, automatic teardown, fallback/cache and
unconditional song cutover. Original final-source/content regressions, normal
application/render, dependency/link and preparation/live/memory gates remain
required. Normal production songs still use legacy Lua. The interpreter and C API
removal milestones stay closed; this migration remains active and incomplete.

## Default parameters and guarded message probes (2026-11-06)

The compiler's six existing default parameter objects now use the Rust engine:
Judgment, EarlyHit, LifeChanged, HealthStateChanged, ExCountsChanged and
PlayerOptionsChanged. Judgment reuses the existing retained note/result methods.
LifeMeter callbacks accept ignored arguments, and ExCounts retains its original
ten zero fields. Unknown names, exact suffix/case rules and numeric defaults
match the actual native helper.

A retained message runner calls the explicitly selected function even after its
source field is replaced or removed. It owns one default parameter table reused
across attempts. Actor, command and parameter identities are traced captures;
public handles are explicit VM roots released by their caller. The unchanged
body of named-command execution is shared with the runner: recursion guards,
scoped directory, command locals, theme-error behavior and receiver/queued-target
draining retain their existing boundaries. Unrelated delayed actors stay pending.

- [Final regressions](song-message-probe-regressions-v1-song-linux-debug/result.json): **331 passed, zero failed across 23 targets**,
  exit 0 in 377.7511448059231 seconds; sources unchanged. Log SHA-256:
  `351ff15ffc98c3d28d7eaaeb0542d53e4a9401d0187cff9dd6f15ebde7c75b4e`.
  Activation has 109 cases and the exact prior 104-case prefix is unchanged.
  Five additions compare all parameter shapes and retained method results,
  handler replacement with GC and reusable parameters versus local rewind,
  guarded recursion/queue order, no-default arity, directory scope and complete
  preservation of an unrelated pending queue, callback/fatal-limit recovery,
  and collection after explicit root release. A detached handler remains callable
  after Rust scene retirement until its root is released.
- [Clippy](song-message-probe-lint-v1-song-linux-debug/result.json): the exact existing 166 warnings and cmd.rs:349
  error, exit 101 in 33.144939126912504 seconds; no new diagnostics. Log SHA-256:
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Source review](song-message-probe-review.json): all 1,218 watched inputs match both final checks.
  Formatting passes for five modified adapter/test files, the new message-probe
  module and the isolated cfg-only native oracle append. All pre-existing native
  bytes remain unchanged. The oracle delegates to the actual private native
  runner. Core, manifests and backend selection are unchanged. The
  [dependency graph](song-message-probe-test-dependencies.json),
  [source diff](song-message-probe-source-diff.patch) and edit/documentation
  baselines retain the review boundary.
- [Findings](song-message-probe-findings.json) preserve the first test-harness compile error, the three-pass/two-failure
  focused run and final five-case pass. Both engines already agreed at the new
  fixture failures: command-local snapshots rewind their referenced log table,
  and a queue created outside startup drains immediately. The corrected fixture
  additionally asserts exact order via a persistent scalar trace and leaves the
  unrelated queue pending through the actual startup/discovery path, with full
  before/after capture equality. Existing log rewind and owned-block comparisons
  remain. No prior coverage was removed, weakened or skipped. Durations are
  verification costs rather than representative application performance results.

The message-runner/default helper is implemented. Complete function-action
capture still needs its outer script/global state, note-field column/spline
snapshot, broadcast/sound suppression and collection, tracked targets,
side-effect counts and native cleanup order. Stable/deferred message extraction,
full tree/wrapper/global-reference/proxy and remaining derived output, production
preparation compiler and retained live/session/navigation lifecycle, automatic
teardown, fallback/cache and unconditional song cutover remain pending. The
original final-source/content, normal application/render, dependency/link and
preparation/live/memory gates still apply. Normal production songs still use
legacy Lua. Interpreter/C API removal milestones remain closed. The production
migration remains active and incomplete.

## Full function-action scope and SOUND binding (2026-11-06)

The Rust adapter implements the existing public compiler function-action helper.
It composes shared-script snapshots and the lazy touched-actor scope, preserving
overlay order and duplicate indices plus tracked-row order. Output collection
precedes column/spline restoration; side effects are measured after that restore
and before actor reset/rewind. Output globals and clocks restore before outer
script tables/cells. Saved identities, inputs, previous globals and the first
callback return remain rooted across all callbacks and GC. Cleanup callbacks
remain allocated before scope activation. Fatal instruction aborts use fresh VM
cleanup entries. Authored restoration errors preserve native partial state and
release private scope/attachments so subsequent probes remain usable.

Original note-field/column/handler/spline identities are retained. Native fields
receive independently cloned pre-probe values; an old point-table alias may stay
mutated while the restored spline gets its saved clone. Typed getters and the
raw/full named-child lookup boundaries match the existing native helper.

The combined comparison exposed an inventoried production gap: Rust had not
installed the existing SOUND global. Its seven methods now preserve native
side effects, captured-manager return identity, method argument offset,
string-only path input, path lookup without extra trimming, audio filtering,
resource deduplication, repeated PlayOnce calls, partial errors and runtime
queued-startup/message provenance. This adds existing host behavior only.

- [Final regressions](song-function-action-regressions-v1-song-linux-debug/result.json): **350 passed, zero failed across 24 targets**,
  exit 0 in 409.12762353615835 seconds; sources unchanged. Log SHA-256:
  `813078174c0ac185bf1bc7b3a5468758114c3dff2a9803028272629d00524315`.
  Activation has 117 cases with the exact prior 109-case prefix unchanged.
  Eight additions compare combined overlay/tracked output and broadcasts/sounds,
  script/local/RNG rewind, column membership and deep spline clones, cleanup
  ordering and first-return lifetime through GC, deferred output-error precedence,
  fatal-limit recovery, SOUND methods/path/identity behavior, sound event context
  and partial errors/GC, and failed column restoration with later recovery.
  Existing NoteField coverage adds eleven cases because these snapshots now
  participate in function actions.
- [Clippy](song-function-action-lint-v1-song-linux-debug/result.json): the exact existing 166 warnings and cmd.rs:349
  error, exit 101 in 34.5905553009361 seconds. Log SHA-256:
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
  Its output is bitwise identical to the established baseline; no new diagnostics.
- [Source review](song-function-action-review.json): all 1,221 watched inputs match both final checks.
  Formatting passes for five modified adapter/test files and three new modules.
  Native production/oracle implementations, core and manifests remain unchanged.
  Test support delegates to the actual existing public native action helper.
  The [dependency graph](song-function-action-test-dependencies.json),
  [source diff](song-function-action-source-diff.patch) and edit/documentation
  baselines retain the exact review boundary.
- [Findings](song-function-action-findings.json) preserve the borrowed-stack conversion compile failure, both new-fixture
  and missing-SOUND failures, the seven-case pass and final eight-case pass.
  The fixture's nonexistent SOUNDMAN name was corrected to native SOUND; the
  resulting missing Rust binding was implemented with all capture assertions
  retained. Static review then used direct HostCall text coercion, tightened
  raw-path input to native's string-only rule and added restoration-error coverage.
  The weak table for the returned-value check lives on an actor so outer script
  rewind cannot mask a leaked return root. No existing coverage was removed,
  weakened or skipped. Durations are verification costs, not representative
  preparation/live/memory measurements.

The public function-action helper is implemented. The internal no-table-rewind
variant still needs its callers in stable/deferred message orchestration.
Per-actor message extraction, stable/cross-actor/deferred messages, easing target
probes and full tree/wrapper/global-reference/proxy/derived-output compilation,
production preparation compiler and retained live/session/navigation lifecycle,
automatic teardown, fallback/cache and unconditional song cutover remain pending.
All original final-source/content, normal application/render, dependency/link and
preparation/live/memory gates remain required. Normal production songs still use
legacy Lua. Interpreter/C API removal milestones remain closed. The production
migration remains active and incomplete.

## Per-actor command and message extraction (2026-11-06)

The Rust song adapter now probes individual source commands and extracts their
message blocks, repeated sounds and startup sound history using the existing
compiler behavior. Source mutable/capture state, scalar globals, reached actors,
shared script/command locals and RNG rewind at the native boundary. Structural
child/handler edits remain visible. Failed commands preserve their first deferred
detail and strongly retain their actor until the deferred record is released.

- [Source diff](song-actor-messages-source.diff) and [review](song-actor-messages-review.json)
  cover ten existing files and three added modules. Native production bytes are
  unchanged; the native append only observes deferred state under test-support.
- [Findings](song-actor-messages-findings.json) retain the initial channel-type
  compile error and the seven-pass/one-failure lifetime fixture. That new fixture
  used a complete gameplay screen with its own actor owner; the isolated scene
  replacement keeps the original retention and final collection assertions.
- Focused v3 passed all eight cases. The regression group passed **358 tests in
  24 targets**, with the exact previous 117-case activation prefix preserved.
  Cases cover defaults/arity, frozen names, queues, mutable aliases, callback and
  instruction errors, malformed keys, partial cleanup, discarded returns during
  reentrant GC, startup sound history, deferred deduplication and root release.
- [Regression result](song-actor-messages-regressions-v1-song-linux-debug/result.json):
  413.94855694216676 seconds; log SHA-256
  `ce89683de50088a8193e57d8b8f3bf4ed6c60ce58476cb9c8e0e1af3fe01bfa7`.
- [Clippy result](song-actor-messages-lint-v1-song-linux-debug/result.json):
  exit 101, 35.006988420151174 seconds; exact established baseline log SHA-256
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
  The existing cmd.rs:349 failure remains recorded; no new diagnostics appeared.

Stable cross-actor/deferred retries and chronological observed marking remain to
port. Easing/tree/derived-output compilation, full production song/live/session/
fallback/cache integration, unconditional cutover and the original content,
rendered application, graph/link and preparation/live/memory gates remain open.
Production songs still use legacy Lua. The completed interpreter and C API
removal milestones remain closed; this is progress on the separately authorized
migration and does not redefine its finish line.

## Stable cross-actor and deferred message retries (2026-11-06)

The Rust song adapter now implements the existing stable cross-actor pass and
end-of-song deferred retries. Each pass retains its input actors; the stable pass
also freezes and roots every selected function before any handler runs. Two
attempts share command locals and one defaults object, then restore the outer
script snapshot. Source-only differences do not invalidate stable cross-actor
blocks. Changing cross-actor effects and failed second attempts retain the
original deferred diagnostic. Stable additions publish after the whole pass.

Deferred consumption removes and independently roots the full batch before
retrying, including retired actors. It skips missing overlay identities and
already observed/runtime-covered handlers. Stable representable retries publish
source, cross-actor and tracked commands with repeated sound paths. Broadcasts,
unrepresentable sound/music effects and changing captures retain their details.
A later missing handler preserves earlier command publications while returning
an error. Removed roots release on all completion/error paths.

Successful chronological delivery now marks the matching deferred identity as
observed only with the real preparation frame clock installed. Failing delivery
and pre-frame broadcasts remain unobserved. A successful callback can have no
actor writes and still suppress an incorrect end-of-song retry.

- [Source diff](song-message-retry-source.diff),
  [review](song-message-retry-review.json) and
  [findings](song-message-retry-findings.json) cover five existing source files
  and two added modules. Native production/oracle implementations, core engine,
  manifests and backend selection are unchanged. Test-support wrappers delegate
  directly to the existing native passes.
- Focused v1 passed all six initial comparisons. Focused v2 passed all eight,
  adding actual frame-clock observed handling and isolated retired-actor root
  release after batch consumption. Other cases cover frozen removed handlers,
  duplicate actor identities, shared-local instability, second-attempt errors,
  source/tracked effects, repeated sounds, rejected effects, runtime coverage,
  missing overlay identities and partial publication on a later hard error.
- [Regression result](song-message-retry-regressions-v1-song-linux-debug/result.json):
  **366 passed in 24 targets**, including 133 activation cases with the previous
  125-case prefix unchanged; 449.98126933304593 seconds; log SHA-256
  `ebda969cfcb7b14beb61ed7bbd41c815887ebc0bdb9187044caa9f285957169e`. Sources stayed unchanged throughout the run.
- [Clippy result](song-message-retry-lint-v1-song-linux-debug/result.json):
  exit 101, 35.743966489098966 seconds; exact prior baseline log SHA-256
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
  The recorded cmd.rs:349 error and 166 warnings remain unchanged.

The next existing native entry points include `capture_overlay_function_eases`
and the overlay tree/compiler integration. Easing/tree/wrapper/global-reference/
proxy/derived-output compilation, full song/live/session/fallback/cache migration,
unconditional production cutover and the original content, rendered application,
graph/link and preparation/live/memory checks remain required. Production songs
still use legacy Lua; this checkpoint does not complete or narrow the migration.
The interpreter and C API removal milestones remain closed.

## Compiler easing endpoints and rooted overlay tree (2026-11-06)

The Rust song adapter now implements the existing compiler easing endpoint helper
and complete overlay draw-tree collection. Easing retains function/input roots
across both endpoints, selects indexed probes for known actors and touched probes
otherwise, keeps native clock clamping and shared-state ownership, and reuses the
existing pure delta-intersection builder. Duplicate indices keep native last-write
behavior, including ignored out-of-range output indices.

The tree collector performs AFT-name and two-level raw global-reference prepasses,
then visits highest-index wrappers first, chains native parent indices and omits
wrapper children. It reads initial state before source-command probes and Name
and raw child slots afterward. Plain Actor/root Frame elision retains native
reference, draw, message, deferred and live conditions. Existing resource/kind
readers supply renderer payloads; generic proxies resolve against the final tree,
including missing targets and duplicate-identity last-write behavior.

Rust default wrappers remain in existing actor metadata; authored Lua wrapper
lists take precedence. Cursor observations are returned on the VM stack and
immediately pinned through their corresponding iteration scope. Collected rows
hold roots while the final owner table is built. Temporary traversal alone adds
no batch root; explicit batch release invalidates copied identity handles and
preserves owned output. Foreign VM release/lookup is rejected.

- [Source diff](song-overlay-compiler-source.diff),
  [review](song-overlay-compiler-review.json) and
  [findings](song-overlay-compiler-findings.json) cover four existing files and
  three added modules. Native production/oracle implementations, core engine,
  manifests and backend selection are unchanged. New test-support wrappers call
  the actual native easing and tree helpers.
- Focused v1 preserved a borrow-check failure; copying the returned VM value
  before pinning fixed it. New Rust test-literal/span spelling corrections are
  also recorded. Focused v2 passed seven cases and failed four. One required the
  collector's owned-wrapper fallback; two required removal of asymmetric
  native-only prepared_root/top-screen fixture aliases; the structural Name
  fixture now makes its intended raw edit while also exercising semantic-name
  rewind. No original assertions or prior activation coverage were removed.
- Focused v3 passed **all 13** comparisons, including endpoint metadata/clocks,
  known/unknown targets, duplicates, empty/error/recovery paths, detached function
  lifetime, wrapper order, two-level/key-only references, authored registry
  membership, forward AFT and proxy remaps, changed children/names, live elision,
  collecting errors, detached transient children and retired/foreign/released
  batch roots.
- Clippy v1 caught a new large-enum warning. Boxing only the owned state query
  record removed it. [Clippy v2](song-overlay-compiler-lint-v2-song-linux-debug/result.json)
  has the exact established 166-warning/cmd.rs:349-error log SHA-256
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`;
  exit 101; 36.02400150988251 seconds.
- [Final regression result](song-overlay-compiler-regressions-v1-song-linux-debug/result.json)
  covers the boxed final source: **379 passed across 24 targets**, including 146
  activation cases with the original 133-case prefix unchanged;
  586.3407347388566 seconds including build time. Log SHA-256:
  `7d214ab5bc5a6a7cb9a51fd036881b06e2d6778fe5ceeb84e0a3a83dcc589f48`. Sources stayed unchanged throughout the run.

The next existing compiler entry points include
`compile_overlay_compile_actor_function_action` and
`read_overlay_compile_actor_actions`. Action discovery/orchestration, remaining
derived-output/final-remap work, complete song/live/session/navigation/fallback/
cache integration, unconditional cutover and the original content, rendered
application, dependency/link and preparation/live/memory gates remain required.
These regression wall times do not measure application preparation performance.
Production songs still use legacy Lua; the migration goal remains active and
incomplete. The interpreter and C API removal milestones remain closed.

## Compiler function actions and raw action-table reader (2026-11-07)

The Rust adapter now ports `compile_overlay_compile_actor_function_action` and
`read_overlay_compile_actor_actions` as `compile_song_function_action` and
`read_song_overlay_actions`. `SongActionOutput` borrows the caller's existing
message/sound arrays, synthetic-message counter and compile diagnostics.

Successful function capture publishes repeated sound paths in native order,
parameter-free broadcasts only when all broadcasts qualify and compiled listeners
exist, then one counter-named synthetic message for direct overlay/tracked blocks.
Empty, missing-listener and side-effect-only handled results retain native behavior.
The reader pins the original table and current entry across callback reentry/GC,
reads the next current raw slot after each capture and ignores numeric __index
spoofs. It preserves malformed skips, first-nil termination, trimmed float-string
parsing (including existing string NaN/infinity behavior), finite numeric beats,
Lua truthiness and hard invalid-UTF-8 action strings. Function errors/unhandled
captures record the existing unsupported count/detail and continue later entries.
Hard parser errors retain earlier publications while returning an error.

- [Source diff](song-action-compiler-source.diff),
  [review](song-action-compiler-review.json) and
  [findings](song-action-compiler-findings.json) cover three existing adapter/test
  files and one added module. Native production/oracle implementations and the
  core interpreter are unchanged. New test facades invoke the actual native
  compiler helpers. The adapter edit changes no manifest or lockfile.
- Focused v1 passed eight native comparisons. Focused v2 passed all nine after
  adding Rust instruction-abort/continuation/recovery coverage. Cases cover
  listener/broadcast ordering, mixed parameter handling, direct/tracked effects,
  repeated sounds, counter continuation, raw/malformed entries, unsupported
  deduplication, hard-error partial output, native functions, collection during
  a cleanup error and reading the mutated next entry from a detached table.
  The quota case also retains owned output after scene/VM teardown.
- Clippy v1 found one new collapsible-match warning. Its match-guard correction
  keeps one capture per function entry. [Clippy v2](song-action-compiler-lint-v2-song-linux-debug/result.json)
  matches the exact 166-warning/cmd.rs:349-error baseline; exit 101;
  34.62723498302512 seconds; log SHA-256
  `1478af1927dc9a12e65016b830c374199056efd88e7d997c940de2a1adab7d5d`.
- [Final regressions](song-action-compiler-regressions-v1-song-linux-debug/result.json)
  passed **388 tests across 24 targets**, including 155 activation cases with the
  original 146-case prefix unchanged; 658.1998660811223 seconds including build/lock
  wait time. Log SHA-256:
  `592b8b02066f0e06e6db070831ef85f547dc65299476291dee4c21f2e0caf851`.
- The runner's whole-worktree unchanged assertion failed on concurrent
  `pad_config.rs`/`pad_config/paging.rs` edits outside the authoritative song test
  graph. These flags and both focused/lint histories remain unchanged. Scoped
  review proves song dependency inputs stable across both final checks; it also
  confirms their exact current lockfile and resolved graph. The final 1,231-file
  membership/hash snapshot matches its PowerShell audit. Formatting passes.
- Concurrent shell and Simply Love screen work, including the new paging module,
  is preserved. The shared lock changed before the final checks from
  `f35f7b8b95dc258b18af4dbab142e6d31cfc5137d232ee4c9499a7ec81bc5b1f`
  to `041a21d229719d5b3947a984fd757f205b6f19e9963ca09c9d318738f0a24558`.
  [Resolved graph diff](song-action-compiler-dependency-changes.diff) records one
  new edge to the existing smallvec v1.16.4 dependency. Final validation uses this
  updated shared state; it is excluded from the adapter implementation diff.

Per-table action compilation is available; production orchestration which chooses
prefix/global/runtime tables still uses legacy. Existing next entry points include
`read_eases_with_function_capture` and `probe_function_ease_target`, along with
remaining column/derived-output/final-remap processing. Complete song/live/session/
navigation/fallback/cache migration, unconditional cutover and original content,
rendered application, dependency/link and preparation/live/memory gates remain
required. These regression wall times are not application preparation benchmarks.
The migration remains active and incomplete. Interpreter/C API removal milestones
remain closed.


### Easing-table reader and retained easing names (2026-11-07)

The Rust song adapter now has `read_song_eases_with_function_capture` and a
compiler-owned `SongEasingNames` registry. These replace the existing generic
raw-sequence reader and loaded `ease`/`xero` name registration without a legacy
handle facade. The native reader, native registration and test-support source
are unchanged.

- The reader preserves the first-nil boundary, raw entry fields, malformed-entry
  skips, both span layouts, float/string and player conversions, hard UTF-8
  errors, callback metadata and decisions, ordered owned outputs, diagnostics
  and the native statistics merge (including callback output counts).
- Original outer tables, selected entries and callback functions remain pinned
  through mutation, nested readers, reentry and GC; all temporary pins release
  on hard errors and instruction aborts. The callback owns any surrounding
  compiler snapshot scope, as on the native path.
- Registration uses ordinary lookup in the actual global table, then original
  namespace tables in native name order. Last aliases win; table annotations
  are raw writes; earlier registrations survive a later getter error. A private
  traced table owns registered functions/tables through preparation. Explicit
  release is idempotent and rejects a foreign VM. Released owners reject use.
- The original **155 activation tests remain unchanged**, with nine new cases.
  Eight compare directly with the existing native reader/registration, including
  actual overlay endpoint capture. They cover GC retention, nested reads,
  namespace mutation, partial errors, native functions, owned results after
  scene/VM teardown, and instruction-abort cleanup.
- [Focused v1](song-easing-reader-focus-v1-song-linux-debug/result.json) records
  test-code compilation errors (incorrect enum/type names and command-block
  construction). They were fixed without changing existing coverage.
  [Focused v2](song-easing-reader-focus-v2-song-linux-debug/result.json) passed
  all nine cases, 170.15181736904196 seconds; SHA-256
  `d954de3040f2c06ca55bb64b3da902b6865e10480e6cdb6570be97d2d5efb446`.
  Two GC fixtures were then strengthened to remove the function from its entry
  before nested collection and clear its original namespace during registration.
- [Clippy v1](song-easing-reader-lint-v1-song-linux-debug/result.json) matches the
  recorded 166-warning/cmd.rs-error diagnostics; exit 101, 41.80235600005835
  seconds; log SHA-256
  `7c3989b48c23b47996efe57a081dc364092c3cfc9ad2cd663ccdbb954ef93dd1`.
  The whole watched source map was unchanged during both completed checks.
- [Final-source adapter regressions](song-easing-reader-regressions-v1-song-linux-debug/result.json)
  passed **397 tests across 24 targets**, including all 164 activation tests;
  491.12635413510725 seconds including build time. Log SHA-256:
  `c51de069610e34f0a5c4380bd0c06f0381647c663299e32d007282f389743e00`.
  The strengthened GC fixtures passed. The whole watched source map remained
  unchanged during the final regression and Clippy runs.
- [Source review](song-easing-reader-review.json) checks original test-prefix
  preservation, unchanged native reader/test-support sources, formatting, exact
  baseline Clippy diagnostics, current dependency graph and source hashes.
  [Source diff](song-easing-reader-source-diff.diff) SHA-256:
  `928478bdad1c264e4467ed6cc57a1ab2bd0ddf11e1dc71e95316f161c001c4b4`.
  [Final PowerShell audit](song-easing-reader-final-state.json) records independent
  membership/hash verification of the reviewed sources. Shared work is preserved.

The actual automatic function-ease pipeline still requires target probing and
note-column endpoint sampling/restoration before its overlay fallback can be
wired into the production compiler. The callback-based reader alone does not
complete that pipeline or cut over production songs. Remaining derived output,
compiler/live/session/navigation/fallback/cache migration, unconditional cutover
and the original content, rendered application, dependency/link and
preparation/live/memory gates remain required. Regression build wall time is
not a preparation benchmark. The migration remains incomplete; interpreter and
C API removal milestones remain closed.


### Note-column function-ease endpoints (2026-11-07)

The Rust adapter now provides `read_song_note_column_transform_samples`, its
explicit-field variant and `capture_song_note_column_function_eases`. The
endpoint helper uses the existing rooted column snapshot/restore implementation
through a column-only session. No native implementation, engine source or
manifest was changed by this task.

- Samples retain native raw point/axis reads, ordinary handler/metadata lookup,
  numeric/string conversion, mode/baseline rules, finite filtering, two-point
  position splines, uniform translation/zoom/rotation, witness validation and
  scan/write order. A Y mismatch still reads later points and reports later
  malformed values. No new geometric modes or baking capabilities are added.
- Global fields are collected before sampling. A private traced queue retains
  future fields and releases each processed field before the next getter,
  matching native Vec consumption. Columns, handlers, splines and points remain
  rooted through authored getters/setters and collection.
- Endpoints preserve authored span metadata and native beat clocks for both
  time units. Column restoration precedes clock restoration; body errors take
  priority over sampling errors after successful cleanup. Snapshot and restore
  errors preserve native partial state. The first return remains rooted through
  sampling/restoration; other return values can collect. Script cells, RNG and
  non-column actor writes retain their native changes.
- All **164 earlier activation tests remain unchanged**. Twelve new cases
  include eleven native comparisons and a Rust instruction-abort/recovery case.
  They exercise actual spline methods, callback-driven GC, transient fields,
  restoration/exception ordering and first-result lifetime. Reader integration
  publishes real column windows and retains owned rows after scene/VM teardown.
- [Focused v1](song-column-eases-focus-v1-song-linux-debug/result.json) passed
  ten cases, 178.39526241808198 seconds; log SHA-256
  `1c1ed68e1797c58595e04c65b244ee0d44f6b1a24cb6a651f9db7bf1a4b1304c`.
  Review then refined processed-field release and added the lifetime/reader cases.
  [Focused v2](song-column-eases-focus-v2-song-linux-debug/result.json) passed
  all twelve, 175.22353713191114 seconds; SHA-256
  `96223604e01c1708f6784f1563d6a920b56085925c6e7d09c24783a3a1060f85`.
  Its whole-map assertion failed during concurrent workspace/shell manifest and
  pad-config edits; both the test result and audit failure are preserved.
- [Clippy v1](song-column-eases-lint-v1-song-linux-debug/result.json) stopped at
  Cargo's transient `--locked` mismatch before Clippy ran. The concurrent lock
  update then landed. [Clippy v2](song-column-eases-lint-v2-song-linux-debug/result.json)
  matches the exact existing 166-warning/cmd.rs-error diagnostics; exit 101,
  43.270726040005684 seconds; log SHA-256
  `e94fce9f0744d4eb6cc5cd745f9e2a95ffa834d24b39213a31b7450ab5644b6a`.
  Concurrent shell/input-backend/pad-config source edits caused its whole-map
  unchanged flag to be false; these packages are outside the song test graph.
- [Concurrent workspace evidence](song-column-eases-concurrent-workspace.json)
  preserves the unrelated `deadsync-reflex` addition and lock update. Removing
  its single workspace-member line exactly reproduces the previous manifest
  hash. The resolved song graph remains unchanged at 342 packages (stdout SHA
  `684526a07919b632da56d9698cf6088f9a7b37287834b5b3bb43b2e3ab797435`).
  Final review requires the exact current root manifest and lock in both checks.
- [Final-source adapter regressions](song-column-eases-regressions-v1-song-linux-debug/result.json)
  passed **409 tests across 24 targets**, including all 176 activation cases;
  656.3119201189838 seconds including build time. Log SHA-256:
  `4570299bd5f18ee0f33085ea49469b5f9695f6f0984bc96904c7cff7b6cbb4a6`.
  Cargo test exited 0; the wrapper exited 1 on its whole-map unchanged assertion
  during concurrent shell/reflex/input and pad-config work. No failing or ignored
  test was removed, weakened or skipped to obtain this result.
- [Source review](song-column-eases-review.json) verifies that all watched song
  dependency inputs match both final checks, the exact root manifest/lock match
  both checks, and the resolved song graph matches its record. All differences
  since those checks are in packages outside that graph. The native helpers and
  test-support source are unchanged. Formatting passes; the original 164-case
  activation prefix and exact baseline Clippy diagnostics are preserved.
  [Source diff](song-column-eases-source-diff.diff) SHA-256:
  `6ebc738125ed6f2aed37c9094c7b86cea47921ef6e64c99c623222ae29179f3e`.
  [Final PowerShell audit](song-column-eases-final-state.json) independently checks
  the reviewed 1,236-file watched membership/hash map. It records four further
  shell edits after source review, all outside the song dependency graph, and
  preserves `full_watched_snapshot_matches=false`. Watched dependency hashes
  still match; [the final source map](song-column-eases-final-sources.json) records
  the current 1,236 entries. Concurrent workspace,
  shell/reflex/input and Simply Love pad-config work is preserved. The original
  whole-map failure flags remain intact; scoped validation does not rewrite them.

This ports the existing static function-ease column endpoint branch. Automatic
setter-target probing, its overlay fallback orchestration, remaining derived
column/frame output, production compiler/live/session/navigation/fallback/cache
migration, unconditional cutover and the original content/rendered application/
dependency/link/preparation/live/memory gates remain required. Regression wall
time includes compilation and is not a preparation benchmark. The migration is
incomplete; the interpreter and C API removal milestones remain closed.

### Automatic function-ease probing and capture selection (2026-11-07)

`probe_song_function_ease_target` now records the native setter and actor order,
including empty tween ownership. `read_song_eases_for_overlay_actors` runs the
existing column-endpoint, player-target and overlay-fallback branches in their
original order. Temporary globals, RNG, clocks, partial errors and retained
values follow existing native behavior; the outer compiler still owns shared
script and actor rewind. The test-support wrapper delegates to the unchanged
native reader. No engine, native production implementation or manifest was
changed by this adapter work.

All 176 earlier activation cases are preserved. Seventeen added cases include
fifteen native comparisons and two Rust lifetime/instruction-abort cases.
[Focused v1](song-ease-probes-focus-v1-song-linux-debug/result.json) exposed
missing empty-tween actor ownership and a fixture syntax error; both were fixed.
The [v2 interruption](song-ease-probes-focus-v2-interruption.json) records an I/O
failure before test execution and ENOSPC while saving metadata. Its missing
result is preserved. An [existing-binary diagnostic retry](song-ease-probes-existing-binary-retry.json)
passed 16 cases and rejected one fixture's assumption that a player starts at
X=0. The fixture now compares actual initial state. The genuine
[Cargo focused v3 run](song-ease-probes-focus-v3-song-linux-debug/result.json)
passes all 17 with unchanged watched sources.

C: storage pressure also interrupted an initial source write; the affected file
was restored byte-for-byte from its phase baseline before reapplying edits.
[Recovery evidence](song-ease-probes-write-recovery.json) records that restoration,
the rejected cache-deletion action (no deletion occurred), and incomplete hash
verification after cache compression. Broader checks use a task-owned cache on
D:. The [first Clippy attempt there](song-ease-probes-lint-v1-song-linux-debug/result.json)
failed on mount ownership before checking the song crate. Ownership was corrected;
[Clippy v2](song-ease-probes-lint-v2-song-linux-debug/result.json) matches the exact
existing 166-warning/cmd.rs-error baseline, with unchanged watched sources.
The runner's [only option change](song-ease-probes-runner-change.json) is optional
`--jobs`, retaining its default of four and all existing checks. Initial checks
used one job. The [serial regression attempt](song-ease-probes-regressions-v1-interruption.json)
was deliberately stopped during executable linking, before tests, and restarted
with four jobs using its completed cache artifacts. Its incomplete result is
preserved; test selection and assertions are unchanged.

[Final-source regressions](song-ease-probes-regressions-v2-song-linux-debug/result.json)
pass **426 tests across 24 targets**, including all 193 activation cases. Cargo
and the wrapper exited 0; watched sources were unchanged. The run took
1164.4088282061275 seconds including build/cache time, which is not a preparation
or live-performance benchmark. Log SHA-256:
`75740f7e55d46958f8095e70d2e5fecfa426b0673d1df052e6a22bd319638a09`.
[Source review](song-ease-probes-review.json) verifies formatting, the unchanged
176-case prefix, exact established Clippy diagnostics, matching current
manifests/lock and the unchanged 342-package resolved graph. All 1,238 watched
source hashes match both completed checks. The unrelated shell edit is preserved.
The [source diff](song-ease-probes-source-diff.diff) has SHA-256
`01fa02e44dc074012cc3153094455db5584b9e18fde529218ec82c35369160e3`.
The independent [PowerShell audit](song-ease-probes-final-state.json) records final
source membership/hashes, the preserved test prefix and the unchanged objective;
its [source map](song-ease-probes-final-sources.json) retains the final state.

The production migration remains incomplete. Its acceptance scope is still
[objective.txt](objective.txt), SHA-256
`86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34`.
Production songs still use legacy Lua. Compiler/live/session/fallback/cache
cutover and the original content, rendered application, dependency/link and
preparation/live/memory checks remain open. These are the existing finish line;
the interpreter and C API removal milestones remain closed.

### Callback-table discovery and existing static mod formats (2026-11-07)

The Rust adapter now implements direct and nested update-function table discovery,
global-function nested-table discovery and the serialized-chunk global-name
predicate. These retain ordinary authored debug lookups, fixed upvalue counts,
depth-first first-seen order, recurring handlers, raw child iteration and the
native predicate's SongMeter stream handling and error suppression boundary.
`SongCallbackTables` keeps discovered identities rooted until explicit release;
namespace/getter/function values remain retained through authored mutation and GC.
Those inputs are retained during discovery; the returned batch owns the result
table identities until release.
Foreign or released batches reject access without releasing another VM's roots.

`read_song_mod_windows` retains the raw first-nil sequence and existing conversion
and span rules. `read_song_runtime_mod_eases` preserves named easing lookup,
bitwise first-unique entry order, per-player accumulated state, first-enabled
player static-overlay selection, target mapping and sustain calculation. The
engine, native production readers, manifests and actor dispatch are unchanged.
The existing Rust easing reader only exposes two private helpers for reuse.

Seven comparison cases were added; all **193 earlier activation cases remain
unchanged**. [Focused v1](song-callback-tables-focus-v1-song-linux-debug/result.json)
passed the two raw readers and failed five fixtures: the native VM had not yet
installed the song host's debug table, and a Quad stream incorrectly attempted
to call the ActorFrame-only update method. The fixtures now initialize the native
song host and use an ActorFrame stream. No adapter code changed between runs.
The [fixture correction audit](song-callback-tables-fixture-correction.json)
confirms that only the new test source changed between the two runs.
The predicate coverage also checks its early return and suppression of a
recurring-handler lookup error. [Focused v2](song-callback-tables-focus-v2-song-linux-debug/result.json)
passes all seven, with unchanged watched sources. The failed result is preserved.

[Clippy](song-callback-tables-lint-v1-song-linux-debug/result.json) matches the
entire established log byte-for-byte: 166 warnings and the pre-existing cmd.rs
error, exit 101. Watched sources were unchanged. The
[resolved dependency graph](song-callback-tables-test-dependencies.json) is still
342 packages with the same stdout hash as the previous checkpoint.

[Final-source activation regressions](song-callback-tables-regressions-v1-song-linux-debug/result.json)
pass **200 tests**, including all 193 original cases and the seven new comparisons;
Cargo and the wrapper exited 0. Watched source hashes were unchanged. This is one
activation target, rather than a final application or whole-inventory validation.
The 353.07617099094205-second wall time includes cache/build overhead and is not
a preparation or live-performance benchmark. Log SHA-256:
`729b3ab92fb0df8c651b530032cfb355e8e00932f453678de0570341ee086031`.
[Source review](song-callback-tables-review.json) proves formatting, preserved test
prefix, unchanged actor dispatch, helper-visibility-only changes in the existing
easing reader, matching manifests/lock/graph and all 1,241 watched input hashes
matching both completed checks. The [source diff](song-callback-tables-source-diff.diff)
has SHA-256 `0719bfc87831bff399be55e47096e46ac58a013a953bbf49403eb4679b4fabd4`.
The independent [PowerShell audit](song-callback-tables-final-state.json) checks
final membership/hashes, the original test prefix and unchanged objective, and
retains the [final source map](song-callback-tables-final-sources.json).

These are compiler adapters; production orchestration still needs to select and
consume the tables in its existing sequence, including Xero and sampled readers.
Production songs remain on legacy Lua. The [original objective](objective.txt),
compiler/live/session/fallback/cache cutover and all original final validation
gates remain unchanged. The interpreter and C API removal milestones stay closed.

### Static song schedule assembly (2026-11-07)

`compile_song_static_schedule` now assembles the existing prefix, global mods,
time mods, global eases, Xero and action stages in native order. It retains typed
ordinary lookups, the optional perframe lookup error boundary, callback-table
identity checks and initialized-global ownership. `SongStaticTables` roots lexical
inputs and the selected perframe tables until explicit release, with foreign-VM
and repeated-release checks. Xero parsing retains first normalized node ownership,
sparse pair handling, per-player state, bitwise capture deduplication, the existing
probe/overlay branches and sustain calculation. Parsed tables and unused functions
release before captures; future callback functions remain rooted across GC.

`read_song_preparation_overlay_actors` implements the native static preparation
snapshot/restore around overlay collection. It copies the six schedule globals
in order, recursively cloning raw keys and values into plain tables without
preserving metatables or table aliases. It restores bindings before propagating
collector errors, retaining native restore-error priority. The live collector is
unchanged. No generic engine or actor dispatch changes were required. The native
compiler only gains an explicitly enabled test-support observer and startup seed;
source review reconstructs its original bytes exactly after removing those two
test-only blocks.

The [first focused run](song-static-schedule-focus-v1-song-linux-debug/result.json)
passed four cases and failed three. It exposed the missing preparation copy and
fixtures that expected metatables on tables the native compiler copies. The copy
is now implemented. Dynamic lookup/error fixtures use getters on `_G`, which the
native preparation preserves; an additional comparison explicitly checks that the
table copy removes metatables and separates shared identities.
[Focused v2](song-static-schedule-focus-v2-song-linux-debug/result.json) passes all
seven cases with unchanged watched sources. The original failed result remains
intact. The earlier 200 activation tests are unchanged.

[Final-source activation regressions](song-static-schedule-regressions-v1-song-linux-debug/result.json)
pass **207 tests across one target**, exit 0, with unchanged watched sources.
[Formatting and source review](song-static-schedule-review.json) verify the test
prefix, original native bytes, helper-visibility-only changes to the existing mod
reader, matching manifests/lock and the unchanged 342-package dependency graph.
[Clippy](song-static-schedule-lint-v1-song-linux-debug/result.json) retains the
same ordered 166 warnings and pre-existing cmd.rs error, exit 101; it introduces
no new diagnostic. Its whole-log hash differs from the prior checkpoint because
the test-only insertion shifts native source locations. The independent
[PowerShell audit](song-static-schedule-final-state.json) retains the
[final source map](song-static-schedule-final-sources.json). Check wall times include
build/cache overhead and are not preparation or live-performance measurements.
The early library typecheck is diagnostic evidence, not final application proof.

This is progress within [the original objective](objective.txt). Production songs
still use legacy Lua. Sampled perframe and derived output, full compiler/live/
session/fallback/cache integration, unconditional production cutover, existing
content comparisons, rendered application smoke tests, dependency/link proof and
preparation/live/memory measurements remain required. The interpreter and C API
removal milestones remain closed.


## Shared chronological frame-track compiler (2026-11-07)

The existing renderer-track algorithms now live in `src/frame_capture.rs`, with
owned values and a small domain-specific capture source interface. The native
compiler uses that shared code, and the Rust replay feeds its publications to
`SongOverlayFrameCapture`. The two former scheduled-update structures alias one
owned schema. No interpreter or C API work was reopened.

The shared state machine retains completion order, cancellation, immediate/final
writes, back-tween retargeting, message synchronization, gaps, native interpolation,
progress clocks and stable endpoint coalescing. Its sampler calls a supplied
getter handoff in the original order. This is a component of chronological
compilation; complete driver orchestration, pose publication and the full Rust
getter handoff remain required.

The Rust source roots actor inputs for each call and releases them on every
ordinary result path. Player metadata uses the existing raw typed lookup rules.
Publication batches are owned outside the actor-store mutex while metadata is
read. A metadata error restores the entire ordered batch for retry, matching the
native visit-before-clear behavior. Full frame consumption includes empty touched
rows; existing filtered drain APIs retain their previous behavior.

[The initial comparisons](song-frame-capture-focus-v1-song-linux-debug/result.json)
reported five passes and three failures in new fixtures. They used `fov` on a
Quad, reused the message-modified pose as its own prior frame, and expected base
alpha to interpolate as a tween-pose field. Fixtures now use an ActorFrame for
`fov`, separate prior/message states, and assert both immediate base alpha and
interpolated X/Y. The elastic case uses the existing authored capture-easing field
instead of an unsupported actor method. All eight cases remain, including their
output assertions; the [retry](song-frame-capture-focus-v2-song-linux-debug/result.json)
passes all eight. Coverage includes compound writes, pending schedules, easing
clocks, cancellation/finish, message changes, typed errors/retry, unclocked
merging, GC, foreign/retired scenes and output after VM destruction.

The [first combined gate](song-frame-capture-regressions-v1-song-linux-debug/result.json)
found that the moved sample's `opt1` field was still private to its new module.
The original library test was preserved and the field now shares the same crate
visibility as the other moved sample fields. No scheduling algorithm changed.
The [final combined gate](song-frame-capture-regressions-v2-song-linux-debug/result.json)
passes **1,096 tests**, with zero failures, ignored cases or filters: all **849
song library tests** and **247 tests across five integration targets**. These
include both original perframe golden tests, all 212 activation cases, the
existing column/static-output/PlayerOptions cases and the eight new comparisons.
Watched sources were unchanged during the run. The 27 library-test harness
warnings reference the existing test/performance support code.

[Clippy](song-frame-capture-lint-v1-song-linux-debug/result.json) retains 166 warnings
and the pre-existing `cmd.rs:349` error, exit 101. Diagnostic message/path counts
match the baseline after mapping the extracted module's locations back to
`perframe.rs`; no new diagnostics were accepted. Formatting passes.

The [reproducible extraction](extract_frame_capture.py) and its
[proof](song-frame-capture-extraction-proof.json) compare the saved algorithms,
native wrappers and type aliases with the current source. [Source review](song-frame-capture-review.json)
also verifies preserved host behavior, unchanged existing tests and engine,
manifest-only test registration, the unchanged runner and the unchanged
342-package dependency graph. The independent [PowerShell audit](song-frame-capture-final-state.json)
retains the [final watched source map](song-frame-capture-final-sources.json).
Build/check wall times are not preparation/live performance measurements.

The [original objective](objective.txt) remains unchanged and active. Production
song execution still uses legacy Lua. Complete chronological compiler/live/
session/fallback/cache integration, unconditional cutover, final content and
rendered-application checks, dependency/link evidence and preparation/live/memory
measurements remain required. The interpreter and C API removal milestones stay
closed.

### Standalone sampled perframe compiler (2026-11-07)

`compile_song_static_perframes` consumes the static stage's two selected perframe
tables and retains their functions through sampling. It preserves raw first-nil
rows, valid span selection, prefix/global ordering, midpoint window membership,
the original sample iterator, gap holds and adjacent-snapshot curve output.
Scheduled messages retain stable equal-beat order, queued-block filtering,
message replacement and completion, and the existing effect-field and motion-clock
rules. Callback clocks restore with native error priority; script and actor
writes remain available to the following compiler stage. Plain Lua message
targets are supported without requiring Rust actor metadata.

[Focused v1](song-perframes-focus-v1-song-linux-debug/result.json) passed four cases
and failed the overlap comparison: the new reader emitted no player curves
because it read only Lua fields while setters retained their values in Rust
metadata. The correction records transform-field presence in the existing
capture state. Presence survives block resets and participates in the existing
capture snapshots/restoration, preserving the distinction between an untouched
property and an explicitly written neutral value. The block-writing algorithm
and public capture output are unchanged. Player observations combine typed
ordinary field reads with the retained metadata.
[Focused v2](song-perframes-focus-v2-song-linux-debug/result.json) validates that
correction and the strengthened plain-table/motion-clock comparisons. The
original failure remains recorded; no earlier activation test was changed.

[Final-source regressions](song-perframes-regressions-v1-song-linux-debug/result.json)
pass **445 tests across 24 targets**, including all **212 activation cases**.
The prior 207-case prefix is unchanged. This broader run covers the existing
actor/capture families affected by the new private field-presence bookkeeping.
The GC comparison also checks release of consumed source tables during the
first callback and release of the retained functions after the pass.
[Clippy](song-perframes-lint-v2-song-linux-debug/result.json) retains the established
166 warnings and pre-existing cmd.rs error; it introduces no new diagnostic.
The first Clippy run had the same baseline; final v2 rechecked the formatted
iterator signature. Formatting passes. The early library check was a diagnostic while tests were
being authored, not final-source application evidence.

[Source review](song-perframes-review.json) checks unchanged native sampler bytes
after removing its iterator visibility annotation and signature formatting, unchanged original
capture block writing and actor/effect dispatch, the preserved test prefix,
current manifests/lock and the unchanged 342-package resolved graph.
The independent [PowerShell audit](song-perframes-final-state.json) retains the
[final source map](song-perframes-final-sources.json). Check wall times include
build/cache overhead and are not preparation/live performance measurements.

The [fixed objective](objective.txt) remains open: production songs still execute
legacy Lua. Remaining derived output and chronological frame compilation, full
compiler/live/session/fallback/cache integration, unconditional cutover, existing
content comparisons, rendered application validation, dependency/link proof and
preparation/live/memory measurements are still required. The interpreter and
C API removal milestones remain closed.

### Column spline tracks and zoom-hide windows (2026-11-07)

`capture_song_column_splines` and `SongColumnSplineCapture` now produce the
existing position/zoom tracks as owned gameplay data. The reader retains typed
ordinary metadata lookups, raw sparse points, unsigned indices, finite-point and
beat-conversion validation, the 65,536-point bound, bitwise geometry comparison,
preceding-frame and one-recent-buffer reuse, and the native solver. Coalescing
retains non-reflexive NaN coefficients and the original logical 128 MiB per-layer
track limit. Output remains usable after the Lua VM is destroyed.

`read_song_note_column_zoom_hides` preserves outer-player indexing, raw threshold
conversions, mode/subtract-beat rules, sparse points and sorted windows. The two
readers retain their distinct native lifetimes: spline capture gathers both
fields before processing; hides retain each player while processing its columns
and query the next global afterward. Authored getters can detach values, mutate
later globals, reenter bindings and collect without a metadata/capture mutex
being held. Capture state is restored before reporting ordinary reader errors.

[The eight focused native comparisons](song-column-output-focus-v1-song-linux-debug/result.json)
pass with unchanged watched sources. They compare coefficient bits and sharing
patterns, real spline bindings, in-place edits, signed zero, sparse points,
metadata changes, reenabled geometry, GC of getter-created values and consumed
fields, typed errors/recovery, hide-window rules and both named limits. The track
limit case retains 42 shared-geometry frames and rejects the 43rd logical 3 MiB
frame, matching the actual production accumulator. The early library check was
diagnostic evidence; the final regressions include the later player-lifetime
implementation and test-target registration.

The new target is registered with the same `rust-engine,test-support` requirements
as the existing Rust/native comparisons. No production dependency or backend
selection changed. The native accumulator only shares its type/method visibility;
the old Rust column reader only shares private helpers and formats their
signatures. Native algorithms and existing actor dispatch remain unchanged.

[Final-source regressions](song-column-output-regressions-v1-song-linux-debug/result.json)
pass **220 tests across two targets**: all eight new comparisons and all
**212 byte-unchanged activation cases**. Watched sources were unchanged.
[Clippy](song-column-output-lint-v1-song-linux-debug/result.json) matches the
entire established log: 166 warnings and the pre-existing cmd.rs error, exit 101.
Formatting passes. [Source review](song-column-output-review.json) verifies the
native bytes, helper changes, unchanged activation source, manifest-only test
registration and the unchanged 342-package graph. The independent
[PowerShell audit](song-column-output-final-state.json) retains the
[final source map](song-column-output-final-sources.json). Check wall times include
build/cache overhead and are not preparation/live performance measurements.
The earlier 445-case run is historical evidence for its earlier source, not a
claim of a new broad run for this checkpoint.

The [original migration objective](objective.txt) remains active. Production songs
still use legacy Lua. Chronological frame compilation, complete compiler/live/
session/fallback/cache integration, unconditional cutover, existing content and
rendered application validation, dependency/link proof and preparation/live/memory
measurements remain required. The interpreter and C API removal milestones stay
closed.

## Rust static update compiler output (2026-11-07)

`compile_song_static_update_output` ports the existing update compiler's static
branch. It retains startup PlayerOptions targets and approach speeds, the native
clock origin/rate/tail rules, column transforms and initial spline tracks. The
caller supplies the zero-second spline capture taken before tree-eligibility
getters. This preserves phase order and leaves the chronological branch required.
The large mod-window algorithm is shared with the existing compiler rather than
copied. Native compiler changes only expose six existing declarations to the
adapter and its test oracle; restoring their visibility restores the saved bytes.

The option reader pins both original tables for a compilation, including when
authored getters replace their globals. Compact sorted snapshots intern names
in Rust, preserve raw state/speed traversal and typed numeric conversions, and
remain independent of Lua string identities. State and traversal scratch storage
is taken out of its mutex before calling Lua. Cached child callbacks each acquire
one native stack lease. Ordinary errors restore scratch state; final cleanup
releases option roots on success and failure. Foreign VMs reject the retained root.

The [initial focused run](song-static-update-focus-v1-song-linux-debug/result.json)
reported one pass and seven failures. It exposed the reader's attempt to acquire
multiple native stack leases in one callback. Two new fixtures also incorrectly
chained PlayerOptions methods that return numbers, and used a literal that did
not produce negative zero. Traversals now run in cached child callbacks; the
fixtures invoke each setter and produce negative zero at runtime. All eight
cases and their output assertions remain. The [retry](song-static-update-focus-v2-song-linux-debug/result.json)
passes all eight comparisons, including real public setters and spline bindings,
bitwise float/coefficients, raw values, numeric/string names, typed errors and
recovery, lookup order, GC, wrong-VM rejection and owned output after VM teardown.

[Final-source regressions](song-static-update-regressions-v1-song-linux-debug/result.json)
pass **239 tests across four targets**: 212 activation cases, eight column-output
cases, 11 PlayerOptions cases and eight new static-output cases. Existing test
files are byte-unchanged. Watched sources were unchanged during both the passing
focused run and regressions. [Clippy](song-static-update-lint-v1-song-linux-debug/result.json)
retains the ordered baseline diagnostics: 166 warnings and the existing
`cmd.rs:349` immutable-loop-condition error, exit 101. The log bytes differ; the
ordered diagnostic messages and source paths match, with no new diagnostics.

[Source review](song-static-update-review.json) proves formatting, the scoped
native changes, unchanged engine/tests/runner, manifest-only test registration
and the unchanged 342-package test dependency graph. That graph still includes
the legacy production song dependency; it is not cutover evidence. The independent
[PowerShell audit](song-static-update-final-state.json) records the final
[watched source map](song-static-update-final-sources.json). Check wall times
include build/cache overhead and are not preparation/live performance results.

The [original migration objective](objective.txt) is unchanged and active.
Production song execution still uses legacy Lua. Chronological frame output,
complete compiler/live/session/fallback/cache integration, unconditional cutover,
content comparisons, rendered application validation, dependency/link proof and
preparation/live/memory measurements remain required. The interpreter and C API
removal milestones remain closed.

## Chronological getter handoff and native pose publication (2026-11-07)

`sample_song_overlay_schedule` now uses the complete preparation getter writer.
The writer preserves scalar/boolean/vector aliases, player destination
suppression, ordinary assignment hooks, reentry, GC roots and partial-error
ordering. The scalar field routing and pose metadata updates reuse the existing
implementations; the native writer's other bytes are unchanged.

`publish_song_tween_frame` samples the native queue set and publishes current
poses in the existing target order, including empty touched rows. Getter-only
fallback state is excluded. The new frame reset preserves native queue ownership;
the existing command-probe reset remains unchanged. This prevents frame resets
from switching startup queues into fallback capture.

[All eight focused comparisons](song-frame-handoff-focus-v1-song-linux-debug/result.json)
pass on the first run. They exercise the real schedule-to-callback handoff,
native startup queues across eight frames, fallback separation, typed metadata,
plain-table targets, alias identity, errors/recovery, reentry, GC and scene
ownership. The previous eight frame-capture test bodies are unchanged.

[Final-source regressions](song-frame-handoff-regressions-v1-song-linux-debug/result.json)
pass **1,104 tests across six targets**, with zero failures, ignored cases or
filters: all 849 library cases, 212 activation cases, 11 PlayerOptions cases,
eight column-output cases, eight static-update cases and all 16 frame-capture
cases. Watched sources were unchanged. [Clippy](song-frame-handoff-lint-v1-song-linux-debug/result.json)
retains the baseline's complete diagnostic multiset: 166 warnings and the
pre-existing `cmd.rs:349` immutable-loop-condition error, exit 101. There are no
new diagnostic messages or source paths.

[The reproducible extraction](prepare_frame_handoff.py) and its
[proof](song-frame-handoff-extraction-proof.json) preserve the scalar routing,
native writer bytes outside that routing, and the old pose metadata transition.
[Source review](song-frame-handoff-review.json) checks those changes, the unchanged
existing test bodies and probe/fast-getter semantics, formatting, unchanged
engine/manifests/runner, and the unchanged 342-package test dependency graph.
That graph still includes the legacy production dependency and does not prove
cutover. The independent [PowerShell audit](song-frame-handoff-final-state.json)
records the [1,257-source map](song-frame-handoff-final-sources.json). Check wall
times include build/cache overhead; they are not preparation/live performance
measurements.

This is part of the [unchanged migration objective](objective.txt). Production
songs still use legacy Lua; full chronological orchestration, compiler/live/
session/fallback/cache integration, unconditional cutover and final application,
content, dependency/link and performance validation remain required.

## Complete chronological Rust driver (2026-11-07)

`compile_song_update_functions` connects the existing Rust preparation adapters
into the production update compiler's chronological algorithm. It preserves the
static eligibility/initial spline pass, startup rewind, ordered message replay,
queue schedules and getter handoff, actual actor/draw callbacks, native pose
publication, cancellations, retargeting and held frames. It emits player/mod/
column windows, spline tracks, NoteField wrapper frames, appended wrapper actors,
text samples, stateful message writes, broadcasts and media drains as owned data.
The caller still supplies the preceding compiler phases and owns session cleanup.

Runtime wrappers receive native queue ownership at the same creation/capture
boundary, join capture indices in order and retain the existing parent chain and
output remapping. Wrapper cursors read the next slot after authored getters and
GC; they do not prefetch later rows. Motion observations retain the distinct
frame and field-capture read order. Zero-delta startup only exposes a motion clock
under the native conditions. Full frame getter restoration exports fields without
rewriting a native tween destination. The existing standalone perframe writer is
unchanged in behavior.

The first type check found six Rust integration errors. The first comparison build
then exposed inaccessible owned native startup fields; only their declaration
visibility was widened. The next build caught the harness's two-value clock rows,
which require a third actor-delta value. Neither build ran tests. The
[initial executing comparison](song-update-driver-focus-v3-song-linux-debug/result.json)
passed eight of nine cases and found a real queue mismatch: message getter
restoration rewrote the last queued destination, making a later relative setter
use an old interpolated value. The separate frame writer fixes this while keeping
all output assertions. [All nine cases then passed](song-update-driver-focus-v4-song-linux-debug/result.json),
and [289 affected adapter cases passed](song-update-driver-regressions-v1-song-linux-debug/result.json).

Clippy identified seven new structural warnings. Named capture/sample types, a
cached replay reader, borrowed arguments and fixed-array enumeration remove them.
The cleanup's first build caught three stale reader references; those were fixed.
A tenth comparison covers exact song timing, BPM changes, origin/rate and GC during
callbacks. [All ten complete-driver cases pass](song-update-driver-focus-v6-song-linux-debug/result.json),
including startup recurrence/cancellation, getter feedback, player options and
columns, runtime wrappers, NoteField motion/cursor reads, message provenance,
static output and typed frame errors. The earlier nine case bodies remain intact.

[Final-source regressions](song-update-driver-regressions-v2-song-linux-debug/result.json)
pass **290 tests across nine targets**, with no failures, ignored cases or filters:
212 activation, 16 frame capture, 11 PlayerOptions, 10 effects, 10 complete driver,
eight each for column output/static update/wrappers, and seven model cases.
Watched sources were unchanged. [Clippy](song-update-driver-lint-v3-song-linux-debug/result.json)
retains exactly the previous diagnostic multiset: 166 warnings and the existing
`cmd.rs:349` error, exit 101. There are no new diagnostic messages or source paths.

[Source review](song-update-driver-review.json) proves formatting, unchanged
existing tests and engine sources, the preserved native algorithms (only seven
visibility declarations change), and the unchanged 342-package test dependency
graph. [The copied pure window finalizer](prepare_update_samples.py) matches the
saved native implementation with explicit borrowed-input adaptations. The
independent [PowerShell audit](song-update-driver-final-state.json) records the
[1,261-source map](song-update-driver-final-sources.json). The test graph still
includes the legacy production dependency; it is not cutover evidence.

Checks now use `/var/tmp/deadsync-song-rust-production-20261107` in WSL. The 6.2 GiB
cache was copied from the existing network cache without deleting it. Windows
copying completed with no failed files; Linux ownership and executable permissions
were restored for the copied cache. Check wall times include build/cache work and
are not preparation/live/memory performance measurements.

The [original migration objective](objective.txt) is unchanged and active.
Production song entry points still use legacy Lua. Full compiler/live/session/
fallback/cache integration, compiler timing diagnostics, unconditional cutover,
final content/rendered application validation, dependency/link proof and matched
preparation/live/memory measurements remain required. The interpreter and C API
removal milestones remain closed.


## Existing factory branch and compiler update traversal (2026-11-07)

`RustSongRuntime::compile_song_update_stage` joins hit-command preparation,
model-material installation and the existing analytic/chronological choice.
`compile_song_multitap_update_overlays` reads the existing descriptor tables and
uses the shared pure curve builders. It preserves numeric/string conversion,
single/double difficulty selection and fallback, sequence holes, suppressed peak
errors, text-count boundaries, noteskin selection, visual descendants, partial
output and the precision guard that requires chronological replay.

Hit commands retain the raw Judgment/Dim/grade order, actor/subtree order,
state/local/RNG restoration, default parameters and temporary-field cleanup on
failure. Model clocks use compiled initial texture state. The compiler plan owns
its Lua identities through a registry table, keeps the active plan rooted during
callback invalidation/GC and reuses its frame-delta buffer without holding a mutex
across Lua. Retirement and the existing invalidation operations release its roots.

The combined-stage comparison exposed a real integration defect in the earlier
Rust driver: advancing the screen's ordinary child tree omitted separate
background entries. The driver now uses the compiler's ordered traversal for
screen queue/wrappers, theme owners, players, authored roots, transitions, columns
and the final screen callback. The existing queue, tween, effect and command
implementations remain shared with the actor adapter. Original background motion
and model sample assertions are retained and pass. The loaded theme's rejection
of analytic preparation has its own comparison; isolated factory cases exercise
the analytic branch. Initial runs also exposed a native-stack lease error and a
truthiness mismatch in the new readers. The failed logs remain preserved.

[The broader regression run](song-multitap-regressions-v1-song-linux-debug/result.json)
passes **325 tests across 12 targets**, with zero failures, ignored cases or
filters: 212 activation, 16 frame capture, 14 factory/update-stage, 11 each for
PlayerOptions and scene retirement, ten each for effects/layers/complete driver,
eight each for column/static-update/wrappers, and seven models. Only two
`rustfmt` changes follow that run: module declaration ordering and wrapping the
newly visible field-reader signature. Their exact before/after bytes are checked
by [source review](review_multitap.py), with
[24 final focused cases](song-multitap-focus-v11-song-linux-debug/result.json)
against the formatted source. All watched sources remain fixed during each run.

[Final Clippy](song-multitap-lint-v3-song-linux-debug/result.json) retains the
existing 166 warnings and `cmd.rs:349` error (exit 101); it is not a green lint
check. The diagnostic multiset matches the previous phase. The review and
[independent PowerShell audit](song-multitap-final-state.json) verify the watched
source map, objective and logs. Interpreter sources and all pre-existing tests
remain byte unchanged. The legacy algorithm bodies are unchanged; only the
visibility of the shared visible-start helper and test-oracle dummy factory
changes. The generic compiler field reader also gains adapter-internal visibility.

The [original objective](objective.txt) remains active and unchanged. Production
song entry points still use legacy Lua. Full compiler output/session/live,
fallback/cache and timing-diagnostic integration, unconditional cutover, final
content/rendered application validation, dependency/link proof and matched
preparation/live/memory measurements remain required. The closed interpreter
and C API removal milestones are not reopened. Test/check wall times are not
preparation, live-playback or memory performance measurements.

## Finite song compiler assembly (2026-11-07)

`rust_engine::compile_prepared_song_lua_layers` now assembles the existing finite
preparation pipeline: layer planning and startup reloads in one VM, activation,
queued discovery, startup tween baking, declaration and actor collection, static
schedules/perframes, the analytic/chronological update branch, deferred messages,
media/material/draw finalization, tracked output and layer splitting. It returns
the existing owned production payload. Live contexts are explicitly rejected;
normal production song entry points still use the legacy compiler.

The startup queue state is now an owned shared type. Native compilation shares
that type and three newly visible pure helpers; its algorithm bodies are preserved.
The new path does not execute a legacy VM. The engine and all pre-existing test
bodies remain unchanged in this phase. The GC-stopped startup control is honored.
Existing outer stage and per-frame timing diagnostics are emitted; build/test
wall times below are not preparation/live/memory performance measurements.

Full-output comparisons exposed and fixed missing startup and replay media
capture, the reload End traversal's assumption that the authored loader was an
owned actor, metadata-only deferred queue eligibility, and the loader's identity
parent in custom draw output. Offline preparation starts capture after initialized-global reads;
constructor, Init and outgoing End events are excluded from that capture.
A new fixture initially assumed On broadcasts were captured; native suppresses
that stream during startup queues. Its corrected assertion retains the exclusion
checks and requires the actual initial Update(0) broadcast. All failed runs remain
preserved. No existing test was removed or relaxed.

The twelve compiler comparisons cover complete production output for static and
chronological songs, background/theme screen motion, separate layer ownership,
startup reload globals/RNG/End, custom draw/proxy/text/sound resources, scalar and
empty entries, unsupported screen requests, capture boundaries, queued/message
sounds, framebuffer allocation and deferred sprite kinds. Setter observation
fields enabled only by `test-support` are explicitly not yet populated by this
facade. Generic adapter diagnostics also need separation/formatting into the
native compiler's callback-error records before cutover; source inspection shows
that copying every diagnostic into `unsupported_perframes` is too broad.

[Final Rust adapter regressions](song-compiler-facade-regressions-v2-song-linux-debug/result.json)
pass **386 tests across 18 targets**, with zero failures, ignored cases or filters.
These include all 212 activation cases and the twelve full-output compiler cases,
plus the existing effects, draw, framebuffer, model, PlayerOptions, retirement,
column/message/frame capture, layers, factory, static/update-driver, sound,
sprite and wrapper coverage. The separate [final compiler run](song-compiler-facade-focus-v6-song-linux-debug/result.json)
passes all **12 cases** with the timing environment flag, recording six matching
pairs of native/Rust per-frame summaries. All watched sources remained unchanged.

The [earlier combined run](song-compiler-facade-regressions-v1-song-linux-debug/result.json)
passed all **849 native library tests** and exposed one existing activation test
failure: clearing broadcast history unconditionally removed live Init events.
The reset now applies only in offline mode. The original failing assertion and
all 212 activation cases pass in the final run. Only that Rust adapter guard and
a transparent model-reader type alias follow the library run; source review
verifies those exact two changes. The earlier failed log is retained. The native
library code was not changed by either correction.

[Final Clippy](song-compiler-facade-lint-v2-song-linux-debug/result.json) retains
exactly the prior diagnostic multiset: 166 warnings and the pre-existing
`cmd.rs:349` immutable-loop-condition error (exit 101). The initial new model-reader
type-complexity warning was fixed with the alias. Formatting passes.
[Source review](song-compiler-facade-review.json) verifies the 1,268-file watched
snapshot, unchanged engine and existing tests, native algorithm bodies, extracted
startup type, final checks and the two final Rust-only corrections. The
[independent PowerShell audit](song-compiler-facade-final-state.json) rechecks Git
membership, hashes, objective, logs and documentation. The source changes remain
uncommitted; no production entry-point or default backend changed in this phase.

Full live/session/fallback/cache integration, setter/callback-error observations,
unconditional normal production cutover, final content and rendered-application
validation, dependency/link proof and matched preparation/live/memory measurements
remain required by the unchanged original objective. This is an implementation
step, not a completed migration or a replacement finish line.

## Song callback errors and recurrence eligibility (2026-11-07)

The finite Rust compiler now consumes a dedicated callback-error stream instead
of counting every actor diagnostic as an unsupported perframe. General host
notices retain their existing standalone diagnostic view. Update functions,
queued commands, recurring commands and draw callbacks use separate Lua-visible
report-once flags, read and written ordinarily before reading the clock and actor
label. Reads may reenter Lua or collect; actor identities are rooted and no store
mutex is held across those reads. Report-once suppression follows Lua truthiness,
and authored flag resets reenable reporting. Callback and queue failures on the
same actor no longer suppress one another. Partial state and draw output survive
recoverable errors. Fatal engine errors continue to propagate.

Compiler draw failures are caught at the authored loader root, preserving the
native preparation boundary. The lower-level draw API still returns traversal
errors. The generic lifecycle, activation, chronological callback, queue and
recurring paths share the reporting helper. Native compiler bodies, engine
sources and all pre-existing test bodies are unchanged.

A retained native-rejected fixture exposed a separate real defect: recurrence
stored only in Rust metadata was invisible to eligibility and plan construction.
The compiler selected static output and never executed the startup queue. Both
readers now fall back to the owned recurring-command metadata. This preserves
chronological output for successful startup cycles and rejects a failing startup
tween command, while runtime-started recurring errors remain recoverable as in
the native compiler. Temporary tracing used to locate the skipped replay was
removed before final checks; its source and failed logs remain archived.

The new diagnostic comparisons check every production payload field. Callback
kind, actor label, clock, ordering, count, partial output and the authored error
cause are checked against the native compiler. Backend-specific error-body
and traceback rendering is normalized only in these new tests; each engine retains its
own diagnostic rendering. Existing assertions are unchanged. The first cycle
fixture failed in native preparation before reaching the protected recurring
branch, so its original Lua source is retained as a native/Rust rejection test.
A runtime-started cycle separately covers recoverable recurring failures.

[Final regressions](song-callback-errors-regressions-v1-song-linux-debug/result.json)
pass **445 tests across 21 targets**, with zero failures, ignored cases or filters.
They include all **212 activation**, **26 general actor**, **22 transition** and
**11 new diagnostic** cases, plus the existing compiler, draw, effects, model,
PlayerOptions, retirement, frame/column/message, layer, factory, static/update,
sound, sprite and wrapper checks. The diagnostic cases cover initial and positive
updates, independent callback/queue failures, runtime recurrence, partial draws,
authored flag resets, framebuffer notices, startup rejection, reentrant flag/label
GC, truthy suppression and successful startup-cycle output. The separate
[focused run](song-callback-errors-focus-v3-song-linux-debug/result.json) passes
**37 cases across three targets**. All watched sources remained unchanged.

[Final Clippy](song-callback-errors-lint-v3-song-linux-debug/result.json) matches
the previous diagnostic multiset exactly: 166 warnings and the pre-existing
`cmd.rs:349` error, exit 101. A new collapsible-if warning was fixed. Formatting
passes. Intermediate failed builds and comparisons are preserved, including the
explicit Cow type required when the command label became owned.

[Source review](song-callback-errors-review.json) verifies the unchanged native
algorithms, interpreter and existing tests, new test registration without other
manifest changes, preserved rejected fixture, removed tracing, and final checks.
The [independent PowerShell audit](song-callback-errors-final-state.json) rechecks
the 1,270-source Git inventory, hashes, objective, logs and documentation. The
source changes remain uncommitted. Normal production song entry points still use
the legacy backend; these are preparation-adapter checks, not cutover or rendered
application evidence. Build/test durations are not matched preparation/live/memory
performance measurements.

The original migration goal remains unchanged and incomplete. Reference-only
setter observations, full live/session/fallback/cache integration, unconditional
production cutover, final content/downstream and rendered-application validation,
dependency/link proof and matched preparation/live/memory measurements are still
required. The interpreter and C API removal milestones remain closed.


## Song compiler setter observations (2026-11-07)

The finite Rust compiler now populates the five existing test-support fields:
`boolean_writes`, `speed_writes`, `noteskin_writes`, `state_calls` and
`overlay_writes`. This closes the comparison-wiring gap recorded in the prior
compiler/callback sections. It does not switch the application's backend.

The four option/manager streams share Rust-owned observation state through
existing binding payloads. The actor stream retains setter arguments separately
from interpolated rendering tracks, including equal writes, same-frame overwrite
rules, queued-command suppression and the `UpdateCommand` exception. Startup,
reload, discovery, chronological replay and deferred probes keep the native
capture boundaries. `FromString` includes its existing speed-history record;
queued immediate setters preserve their last same-frame value. All capture code
is gated by `test-support`, with no added Lua globals or shared-engine changes.

`tests/rust_song_observations.rs` adds 14 cases covering the full
production payload and all five reference fields, with separate rejection and
diagnostic controls. Cases include all option levels, dynamically republished
owner identity, accepted/rejected noteskins, partial speed failures, parsed
options, nested broadcasts, chart metadata, reloads, GC, multi-layer index
remapping and queued/direct/message setter ordering.

The first run exposed incorrect assumptions in newly authored fixtures:
non-frame actors cannot install update functions, Init screen requests are
discarded, and the published option owner is the Song level. Corrected positive
fixtures retain every original Lua source in explicit controls. The original
nil-parameter message probe still checks its diagnostic count, command label and
authored cause, and compares all remaining output; only the engine-specific
rendering of that one error is normalized. No pre-existing test body was edited,
removed, ignored or filtered. Earlier failed check directories remain preserved.

| Check | Evidence directory | Result |
| --- | --- | --- |
| Broad adapter regression | `song-observations-regressions-v1-song-linux-debug` | 525 passed; 0 failed |
| Focused compiler regression | `song-observations-focus-v5-song-linux-debug` | 37 passed; 0 failed |
| Clippy (baseline defect remains) | `song-observations-lint-v2-song-linux-debug` | 166 warnings and existing `cmd.rs:349` immutable-loop error; exact prior diagnostic set |
| Adapter without observation code | `song-observations-shipping-cfg-v2` | `cargo check --locked -p deadsync-song-lua --features rust-engine --lib` passed |

The broad run covers 28 targets with 525 passing tests,
including all 212 activation controls. Source snapshots match across the final
checks. Formatting passes. Clippy's diagnostic set matches the preceding
callback phase, including its recorded pre-existing error; no new lint failure
is accepted. The adapter check without test support is not an application build.

`review_song_observations.py` checks all 1272 watched source hashes,
saved pre-edit bytes, unchanged existing tests/native algorithms/shared engine,
test-only manifest additions, fixture retention, formatting, exact result
snapshots and baseline lint diagnostics. `song-observations-review.json` and
`song-observations-reviewed-sources.json` record that review;
`audit_song_observations.ps1` independently verifies current Windows file hashes
and the original objective before writing `song-observations-final-state.json`.

The migration remains active. Live/session/fallback/cache integration,
unconditional song routing, legacy normal/build dependency removal, final
downstream/content regressions, application/render smoke and matched preparation,
live and memory measurements are still required by the unchanged objective.
The interpreter and C API removal milestones remain closed.


## Live frame host phases (2026-11-07)

`rust_engine::SongLiveFrames` now owns the existing live GameState/player host
phases. It retains the statistics tables selected through the actual Lua methods
and the native Current option owners in a rooted private batch. Scene-level
construction selects owners before resetting health publication, matching the
legacy constructor. Life meters bind to those retained statistics. Released and
foreign-runtime owners reject further use without consuming another VM's roots.

Frame preparation preserves statistics-before-clock ordering, independent uptime,
global Song timing versus each selected Steps timing, f64/f32 conversion points,
ordinary versus raw accesses and partial mutation boundaries. Attack activation
uses the prior PlayerState audio position and strict endpoints, with retained
schedule identity and the existing outro gate. Options then advance with the
supplied frame delta or independent Song/P1/P2 deltas. Preparing before synchronous
Player callbacks is idempotent; resuming after them refreshes feedback and
statistics without reapplying uptime, attacks or option advancement.

The post-actor health phase reads current life and fail settings for each enabled
player, publishes a changed status before notifying listeners, and preserves
cross-player life writes and partial failures. Callback error reporting remains
the responsibility of the full live-scene coordinator. Health publication resets
for a new owner while the retained VM and its process globals survive.

`tests/rust_live_frames.rs` adds 17 cases against the actual legacy
live scene's prepare/update methods and its unchanged private health function.
They compare clock/position fields, all relevant option levels, statistics, meter
life, controller/health queries and notification order using exact scalar values.
Cases include rate/timing separation, frozen music, prepared-frame feedback,
attack replacement, retained tables through global replacement and GC, nested
life writes, health thresholds/fail-off, failed option clocks and recovery,
constructor health reset, release and foreign VM rejection.

New test harness mistakes were corrected without changing existing coverage:
both engines now execute the same literal chunk (`call`, without the native
`eval` expression rewrite), with separate explicit-return checks. The nested
life-write fixture uses Player's existing `SetLife` method; its original invalid
LifeMeter call remains an error/partial-state control. Both enabled-player and
disabled-player cases remain. Earlier failed runs and pre-edit test files are
preserved. The fail-off case additionally verifies zero life remains Alive.

| Check | Evidence directory | Result |
| --- | --- | --- |
| Affected adapter suites | `song-live-frames-regressions-v1-song-linux-debug` | 157 passed; 0 failed |
| Native live-phase comparisons | `song-live-frames-focus-v4-song-linux-debug` | 17 passed; 0 failed |
| Clippy (baseline defect remains) | `song-live-frames-lint-v1-song-linux-debug` | 166 warnings and existing `cmd.rs:349` error; exact prior diagnostic set |
| Adapter without test support | `song-live-frames-shipping-cfg` | `cargo check --locked -p deadsync-song-lua --features rust-engine --lib` passed |

The regression run covers 14 targets and 157 passing tests,
with no failed, ignored or filtered tests. It includes the unchanged clock,
statistics, player, song-option, game-state, screen-manager, message, player-part,
generated-player, underlay, reload, retirement and compiler-observation controls.
Source snapshots match the final checks. Formatting passes; Clippy has no new
diagnostic relative to the recorded baseline. These are adapter checks, not a
normal application build or before/after performance measurement.

`review_live_frames.py` checks all 1275 watched source hashes,
saved pre-edit bytes, existing test preservation, test-only manifest additions,
exact native-source prefix preservation, formatting, final result snapshots and
baseline lint diagnostics. The only edit to native `live.rs` is an appended
test-only delegate to its original health helper. The shared engine is unchanged.
`song-live-frames-review.json` records the review; `audit_live_frames.ps1`
independently verifies Windows file hashes and the unchanged objective.

The full production live-scene coordinator remains pending: layer entry/update
ordering, actor/native-feedback phases, draw and output assembly, synchronous
gameplay callback bridges, retained-session ownership, fallback and cache wiring.
The load boundary must clear prior live-player feedback before new Init callbacks;
the native scene constructor resets health publication later. Authored selection
of statistics owners must also govern live player writes. These remain existing
production semantics required by the fixed objective. Normal routing, legacy
normal/build dependency removal, final content/downstream/application/render
checks and matched preparation/live/memory measurements remain required.
The interpreter and C API removal milestones remain closed.


## Live actor traversal and player feedback (2026-11-07)

The Rust live actor phase now reuses the complete chronological job executor:
top screen, screen layers, player subtrees, authored song layers, transitions and
the final top-screen callback. Live-only jobs apply hold animation, judgment
animation and NoteField Mini zoom after each player's subtree, before song
callbacks. Offline compiler plans remain free of live feedback. Switching the
cached plan's mode invalidates its traced root through the existing mechanism.

`RustSongRuntime::advance_song_live_actors` requires an active, activated live
screen. It roots the current job plan and optional entry callback across author
callbacks, cache invalidation and GC; new actors are adopted even when a callback
fails. The entry callback receives each actor before inactivity, own-delta and
inherited-delta selection. This is the actor phase used by the pending scene
coordinator; it does not schedule layers, publish host clocks/health or assemble
live output by itself.

Native feedback preserves hold/judgment timestamp deduplication, MissedHold's
existing queue, 6/7/12/14-state judgment selection, FA+/autoplay distinctions,
early/late frames, tilt and actual Sprite tween methods. The three feedback
helpers re-read their respective inputs in order, so hold callbacks can change
later judgment/option inputs. Raw modifier/child tables and the shared hold color
remain rooted across methods that replace owners and collect. Mini uses the owned
Current options table, writes X/Y before reading OITG mode, and preserves Z in
OITG mode. Feedback runs even when the player's actor delta is inactive.

`tests/rust_live_actors.rs` adds 16 cases against the unchanged native
live scene and production render reader. These compare exact scalar getter and
queue values, authored callback order, and current rendered judgment/hold poses.
They also cover wrappers, rates, hibernation, note-column input delta, prepared
feedback, runtime wrapper creation/cache invalidation, report-once actor errors,
entry-hook timing/recovery, partial feedback errors and foreign/retired owners.

The first new fixtures incorrectly called frame setters on declaration tables,
Combo/Debug/non-frame note columns, or wrote Mini into a level replaced by the
next option update. Positive fixtures now retain initialized roots in Init,
instrument the existing native callback fields where setters are absent, and
set all option levels when testing a persistent Mini value. Explicit rejection
controls retain the invalid non-frame setters. The original new fixture file and
all failed runs remain archived; every previously existing test is byte unchanged.
Rendered-pose assertions strengthen the animation comparisons.

| Check | Evidence directory | Result |
| --- | --- | --- |
| Affected adapter suites | `song-live-actors-regressions-v1-song-linux-debug` | 204 passed; 0 failed |
| Native live actor comparisons | `song-live-actors-focus-v4-song-linux-debug` | 16 passed; 0 failed |
| Clippy (baseline defect remains) | `song-live-actors-lint-v1-song-linux-debug` | 166 warnings and existing `cmd.rs:349` error; exact prior log |
| Adapter without test support | `song-live-actors-shipping-cfg` | `cargo check --locked -p deadsync-song-lua --features rust-engine --lib` passed |

The regression run covers 16 targets with no failed, ignored or filtered
tests. Shared chronological/compiler output, observations, diagnostics, generic
actors, player parts, generated players, NoteFields, wrappers, transitions,
statistics, retirement and prior live-frame controls pass against the final
source. Formatting passes. Clippy's log is byte identical to the prior live-frame
baseline; its existing unrelated error is not suppressed. These are adapter
checks, not a normal application build or final performance measurements.

`review_live_actors.py` checks all 1278 watched source hashes,
saved pre-edit bytes, existing test preservation, the test-only manifest addition,
formatting, final-source result snapshots and lint diagnostics.
`song-live-actors-review.json` records the result. Native implementations and the
shared engine are byte unchanged. The immediately previous goal turn only
acknowledged scope and made no progress; this turn implements and verifies the
live actor phase. The fixed migration goal remains active and incomplete.

Still required: background/foreground scheduling and entry activation in the
full live-scene coordinator; draw/output assembly, outro behavior and synchronous
gameplay callbacks; session ownership, fallback/cache wiring and unconditional
production routing; removal of legacy normal/build dependencies; final content,
downstream, application/render, graph/link and matched preparation/live/memory
validation. The new load boundary must clear prior live players before Init;
selected statistics owners must also govern script live writes. The interpreter
and C API removal milestones remain closed. These are the existing migration
criteria, not new completion requirements.


## Live background and foreground scheduling (2026-11-07)

`SongLiveLayers` retains the existing background and foreground owners through
traced Rust host callbacks and a private rooted batch. Rust activation cursors
hold no untraced Lua handles. Background preparation runs after host clocks and
options, before synchronous Player callbacks. It preserves two layers, stable
activation ties, current/fading order, LoseFocus/transition/reset/On/GainFocus
ordering, rate wrappers and plan invalidation after traversal reordering.

Foregrounds activate through the live executor's actor-entry hook after Player
feedback and before incoming delta selection. The phase preserves strict start
and stop boundaries, the crossed-start music delta, visibility-triggered On
restarts, the rate query after On, and retained selected option objects across
replacement/GC. The caller supplies the last completed music time and commits it
after its outro phase; layer phases preserve partial cursors and authored writes
when they fail. New constructor owners are adopted on both success and failure.
Release validates VM ownership before consuming the retained root.

The native background reset clears Actor pose/effect/queue fields while keeping
resources, dimensions, authored data, commands, callbacks, wrappers and the three
existing preserved render flags. Render comparisons caught an initial Rust reset
that also discarded dimensions; that adapter defect is fixed. The original
state-reader path remains token identical after removing the new explicit reset
branch, and the startup walker body is unchanged. No native algorithm or shared
engine source changed.

`rust_live_layers.rs` adds 16 comparisons against the actual native live
scene and production render reader. They include two-layer/tie ordering, rates,
fades and reactivation, foreground entry/expiry/restarts, prepared frames, Model
depth/wrap defaults, Sprite animation reset, GC during ordinary method lookup,
partial failures, cleanup and foreign ownership. Positive updates reject
unexpected callback errors and compare complete local render states. The
original invalid preference assertion now has an explicit error control: the
option phase reads this preference even when no layers are scheduled. A no-layer
case verifies exactly that one read. The rejected screen-clock rewind also
remains checked before recovery with monotonic screen time. All earlier test
files and failed runs are preserved; previously existing tests are byte unchanged.

| Check | Evidence directory | Result |
| --- | --- | --- |
| Affected adapter suites | `song-live-layers-regressions-v1-song-linux-debug` | 483 passed; 0 failed |
| Native live layer comparisons | `song-live-layers-focus-v4-song-linux-debug` | 16 passed; 0 failed |
| Clippy (baseline defect remains) | `song-live-layers-lint-v1-song-linux-debug` | Exact prior log: 166 warnings and existing `cmd.rs:349` error |
| Without test support | `song-live-layers-shipping-cfg` | `cargo check --locked -p deadsync-song-lua --features rust-engine --lib` passed |

The 24-target regression run has no failed, ignored or filtered tests. It
includes prior live phases, compilation and chronology, activation, observations,
diagnostics, render/effects, Sprite/Model/text, player parts, wrappers, transitions,
statistics and retirement. Formatting passes. `review_live_layers.py` verifies
all 1281 watched source hashes, preserved pre-edit bytes, final-source
check snapshots, unchanged existing reader/startup behavior and test preservation.
`song-live-layers-review.json` records the review; `audit_live_layers.ps1`
independently verifies Windows hashes and the unchanged objective. These checks
do not claim a normal application build, final rendering smoke or matched
performance measurements.

The previous goal turn was verified progress on live actor traversal and player
feedback; this turn implements and verifies live layer scheduling. The migration
remains active. Full live-scene assembly still needs draw/output, outro and
synchronous gameplay callback coordination, retained-session/fallback/cache
wiring, unconditional production routing and legacy dependency removal, followed
by final content/downstream/application/render/graph/link and representative
preparation/live/memory checks. The load boundary must clear prior live players
before Init; selected statistics owners must govern script live writes; failed
background preparation must not leave the outer frame marked prepared. These
are existing production semantics to preserve while assembling the coordinator.
The interpreter and C API removal milestones remain closed.


## Retained live-scene coordination (2026-11-07)

The staged `rust_engine::SongLiveScene` owns its Rust VM and rooted loader,
overlay/tracked actor lists, selected statistics, frame phases and layer phases.
It coordinates preparation, actor updates, foreground/background scheduling,
success/failure outro ordering, current render states, post-state health,
input, custom Draw, startup output and End. `begin_live_song_load` clears old
live-player feedback before Init; selected statistics become authoritative for
SetLife/AddLife/RecordLife writes. A failed background preparation clears the
prepared mark so a retry repeats the host phase. Health failures retain the
existing report-once boundary. Current-state reads use owned tween/render fields,
including when authored raw pose fields have invalid types.

Explicit broadcasts capture their source parameters after the history write and
before listeners can mutate them. The snapshot retains omitted/explicit nil,
function provenance, tables, actor identity gaps, cycle/depth/entry limits,
unsupported value gaps, path normalization and theme-source eligibility. This
stream observes delivery; it does not replace message delivery. Live draw
collection now mirrors the existing boundary: immediate submissions are drained,
while normal draw frames remain internal until the next Draw traversal. The
generic preparation draw collector remains available with its original behavior.
End preserves compiled Sound owner identity and prevents duplicate retirement
stops during the explicit handoff back to the retained runtime.

Validation against the final source:

- 561 passed, zero failures across 30 test targets;
  19 new live-scene cases pass with no ignored/filtered tests.
  The tests compare actual legacy compiler/live scenes with Rust assembly,
  scalar observations, complete current overlay/tracked poses, output streams,
  normalized Sound owner identities and draw frames. They include explicit GC
  between retained phases and after End, background retry, selected statistics,
  health report-once, gameplay-noted outro and handoff cleanup.
- Rustfmt and `cargo check --locked -p deadsync-song-lua --features rust-engine
  --lib` pass. The Clippy command still exits 101 at the unchanged
  `src/cmd.rs:349` immutable-loop error; its entire log is byte-identical to the
  previous layer-phase baseline. No new Clippy diagnostic was introduced.
- Existing tests, native algorithms and `deadlib-lua` source are byte-unchanged
  from the preserved pre-coordinator snapshot. Only the staged Rust adapter,
  its test target and this migration evidence changed.

Focused final run: `song-live-scene-focus-v10`, 83.454 seconds, log SHA-256
`09949f9d4169dde9b254e24b44f8cedd7a2cb6fc2d822b7eb6997afd8c49336a`. Broader run: `song-live-scene-regressions-v1`,
803.681 seconds, log SHA-256 `5e64e6fdaf33dd584289521f35de966369a21fb527dae2626f01401674e50575`.
These are validation durations, not the required final performance comparison.

Development runs remain archived in `song-live-scene-focus-v1` through
`song-live-scene-focus-v9`. Failures identified compiler mistakes, a missing
appended screen actor in the harness, invalid fixture syntax, a request issued
during Update(0), and the live draw-drain defect. Positive cases now require
the real fixture actors; malformed syntax has its own dummy-fallback control.
The original unconditional startup request remains a separate native 128-reload
rejection control. It does not establish the Rust live facade's reload behavior.
The handoff fixture originally expected an End stop for a Sound without a file;
the native compiler excludes that owner. That exact fileless source remains an
explicit native comparison, and the positive handoff case supplies a real WAV.
The shared test builder extraction preserved every existing comparison.

Evidence: `song-live-scene-edit-baseline.json`,
`song-live-scene-before-sources.json`, `review_live_scene.py`,
`song-live-scene-review.json`, `song-live-scene-reviewed-sources.json`,
`song-live-scene-shipping-cfg/result.json` and final Windows hash audit.

The migration remains active. This coordinator is a staging API: the normal song
compiler and application still use legacy. Production live compilation/routing,
same-VM synchronous gameplay hooks, post-health ArrowEffects cache updates,
presentation target/content publication, complete next-load/session reset,
reference command-probe eligibility and imported message-identity coverage gaps,
connections for the existing compiled input/interaction snapshot entry points,
fallback/cache cutover and removal of production legacy dependencies remain.
Final downstream/content checks, normal application build and rendered smoke,
dependency/link verification and matched preparation/live/memory measurements
also remain. The interpreter/C API removal milestones stay closed; no new
language parity, C API or baking work was added.


## Rust song compiler and retained live reload (2026-11-07)

`rust_engine::compile_song_lua_layers` now handles both existing preparation and
live contexts. Its live path stops after startup Update(0), reads the actual
assets/current poses and late sprite/proxy targets, appends ScreenGameplay,
collects tracked actors and transfers the VM into `SongLiveScene`. It uses the
existing layer splitting, wrapper ordering, parent remapping, live indices and
primary-layer ownership rules. Empty live sessions retain their screen. Failed
live files retain the existing dummy-actor fallback and prior Lua side effects.
No future callbacks are executed to extract live assets.

The shared payload is now `CompiledSongLuaData<Actor, Scene>`. The existing
`CompiledSongLua<Actor>` alias retains its legacy scene owner until normal
application routing is ready. Fields, defaults and Debug representation are
preserved. The finite Rust compiler body and native algorithms are unchanged;
only their payload scene type was generalized. The old finite Rust entry point
remains available, and the unified entry point uses the same preparation body.

`rust_engine::reload_song_lua` runs outgoing End, reads the selected context,
releases scene-owned roots and reconstructs in the same Rust VM. Globals,
functions, saved actor identities and the RNG survive. Outgoing output precedes
new startup output. The load boundary clears live player feedback and cached
execution/replay plans before replacement Init. Startup requests preserve the
native unsupported-screen rejection and 128-reconstruction limit. Startup End
does not add the later live scene's compiled Sound stop pass; runtime End keeps
that pass and avoids duplicate retirement stops.

The differential cases exposed the native epoch behavior of old actor handles:
after playback state is cleared, ordinary scalar getters read their retained Lua
backing fields. Rust now marks the old epoch, preserves those reads and publishes
subsequent scalar writes to that backing. Initial live scalar fields and rotation
getter fields retain their native publication boundaries. Cached base alignment
getters fall back to their Lua defaults after reload. This is verified for the
tested scalar methods; complete saved-handle content/reattachment behavior remains
part of the pending presentation/session audit. Startup test-only observation
histories are consumed and discarded on the native live branch; the Rust live
payload follows that rule while finite preparation retains its histories.

Final-source validation:

- 575 passed across 31 regression targets, with zero
  failures, ignored cases or filtered cases. All existing test files are
  byte-unchanged. The 14 new compiler cases compare every compiled
  payload field through the preserved Debug schema, complete current scene poses,
  startup/live output, source rows, Sound owner identities, reloads and rejection
  controls. Cases include multiple layers/wrappers/proxies/AFTs, empty sessions,
  fallback side effects, preserved globals/RNG/End order, stale player feedback,
  saved scalar handles across GC, the 128-reload bound and an actual loading-worker
  to playback-thread VM handoff.
- 10 library tests passed with the explicit `compile::` filter.
  The whole library test target compiled; the filter runs the relevant existing
  native compile/finalization/reload coverage rather than unrelated unit families.
- Rustfmt passes for changed Rust modules and the new shared declarations.
  `cargo check --locked -p deadsync-song-lua --features rust-engine --lib` passes
  without test-support. Clippy retains the recorded immutable-loop error at
  `src/cmd.rs:349` and its prior diagnostics. Shared declaration line shifts are
  mapped to the old locations for comparison; no new diagnostic is accepted.
- `review_live_compiler.py` checks all watched sources, the exact shared/native
  type transformations, the unchanged finite compiler body, payload schema/Debug,
  manifest addition, old test preservation and interpreter preservation. The
  independent Windows hash audit records the final state and documentation.

Focused final run: `song-live-compiler-focus-v5`, 275.932 seconds, log SHA-256
`c1dddbe8b05eca2a6a2f86be4e780263bb88bae01bfeb25dedad732424c3bb59`. Broad run: `song-live-compiler-regressions-v1`,
915.016 seconds, log SHA-256 `1181c9c374c0373a7f7b7b8a630672df9299e177f914eca044ff285785aa2e52`.
These durations are validation costs, not the final preparation/live/memory
performance comparison required by the objective.

Development runs and original fixtures remain archived. The first live payload
comparison caught incorrectly retained reference histories. A saved-pose fixture
initially assumed 77 survived native replay reset; the direct native comparison
showed its Lua fallback was 0, and the Rust epoch defect was fixed. The statistics
fixture initially called unsupported `PlayerStageStats:GetLife`; its supported
calls now use `GetCurrentLife` and `LifeMeter:GetLife`, with an explicit control
preserving the unsupported-name rejection. No failing coverage was removed.

The migration remains active. The normal application song facade still uses
legacy. Same-VM synchronous gameplay hooks, post-health ArrowEffects updates,
presentation/content publication, existing compiled input/interaction snapshots,
remaining reference probes and retained-handle/session cleanup behavior still
need connection and verification. Normal routing, fallback/cache cutover and
legacy dependency isolation/removal remain, followed by final content/downstream
checks, a normal application build and rendered smoke, dependency/link proof and
matched preparation/live/memory measurements. The interpreter/C API removal
milestones remain closed; the objective and its finish line are unchanged.

## Rust retained gameplay callbacks (2026-11-07)

The Rust live scene now installs the five existing synchronous gameplay hooks:
DidTapNote, Step, tap-row Judgment, hold Judgment and avoided-mine Judgment.
Gameplay retains closures sharing the scene's one Rust VM, selected statistics
and timing. Scene reloads retain that VM even while old hooks still exist.
The scene/runtime accessors now return a scoped VM guard. Lua host functions use
HostCall for nested Lua execution; gameplay writes are drained and applied after
releasing the VM guard. Dropped hook roots are queued for release on the next
idle VM access, and the final owner destroys the VM without a HostFn/VM cycle.

Event callbacks refresh selected statistics, the owned current music rate,
song beat/music time and live players without advancing uptime, option groups,
attacks or the outer prepared frame. Native messages bypass authored Judgment
normalization. Row payloads include current notes and preceding active hold
heads, with retained note/result methods and routine participant ownership.
DidTapNote retains its selected field/callback across GC, preserves independent
optional column/score/brightness returns, recomputes brightness with the mine
exception and preserves the existing once-per-owner error flags. Player writes
before an error still apply. Pending/output writes and Song FailType precedence
also use the existing gameplay methods. New actors created by event callbacks
are adopted by the current screen even when the callback reports an error.

Final-source evidence:

- 586 tests passed across 32 regression targets, with
  zero failures, ignored cases or filtered cases. This retains the preceding 31
  compiler/scene/actor/binding targets and adds 11 gameplay cases.
  They use real GameplayRuntimeState instances, both installed closure calls and
  the gameplay judgment pipeline, native payload/return/output comparisons,
  nested Lua messages, malformed-rate and callback errors, prepared-frame reuse,
  GC, old hooks across reload, worker/playback thread transfer and final VM
  destruction after the last retained hook drops.
- 150 library tests passed under the explicit `live::` filter; the
  entire library test target compiled. Shipping cfg without test-support also
  compiles. Applicable Rustfmt checks pass. Clippy matches the preceding baseline,
  including the unrelated immutable-loop error at src/cmd.rs:349; no new
  diagnostic is accepted.
- [Source review](song-live-gameplay-review.json) verifies the 1292
  watched sources, the archived pre-edit bytes, the manifest addition, the
  unchanged generic interpreter and native algorithms. The only changes to
  existing test files bind or release the new scoped runtime guard; their
  assertions and coverage are preserved. Independent final hashes are in
  [the final audit](song-live-gameplay-final-state.json).

The initial test compile used the wrong public write-type path and omitted the
native reload factory argument; both fixture declarations were corrected. All
development logs remain archived. The tap test exposed precise closure capture
of a copied root ID instead of its owner; the closure now calls through the
retained owner, keeping its root alive until callback destruction. This adapter
lifetime defect was fixed without changing the generic VM. Run durations are validation costs, not the
matched preparation/live/memory measurements still required by the objective.

This is progress on the [unchanged production migration objective](objective.txt),
not completion. The normal song facade still uses legacy. Post-health
ArrowEffects, presentation/content publication, compiled input snapshots,
remaining reference probes and retained-handle/session auditing remain pending.
Production routing, fallback/cache cutover, legacy isolation/removal and the
final content/downstream, normal-build, rendered-smoke, graph/link and matched
performance/memory gates remain required. The interpreter and C API removal
milestones stay closed.

## Rust ArrowEffects and post-health cache (2026-11-07)

The song adapter now installs the existing ArrowEffects functions on the Rust
VM: YOffset/YPos/XPos/ZPos, RotationX/Y/Z, Zoom, Alpha, Glow and explicit Update.
This ports the supported native implementation, including its zero-valued
Z/RotationY/Glow and the existing column-zero RotationX alias. It adds no new
StepMania effects. Native X/Y/zoom/alpha/rotation expressions are transferred
with protected host reads; `port_song_arrow_math.py --check` verifies that
transfer against the unchanged native source. Scoped neg_multiply allowances
retain the captured float expressions and nonfinite far-edge results. Integer
side partitioning uses equivalent div_ceil arithmetic.

Bindings borrow the runtime-owned catalog through a private weak-value table,
avoiding ownership of unrelated catalog entries or unselected option levels.
The last selected option owner lives in a traced private batch. Every getter preserves
GetCurrentPlayerOptions selection, normal method lookup, numeric conversion,
error and return behavior. The last selected options remain rooted across GC;
cached Tipsy uses that selection's timer/cosecant while activation, speed and
offset come from the current player's owned Current options. Beat factors,
Tipsy, bounds and inversion caches commit only after a successful update. No
cache mutex spans Lua execution. Binding functions are rooted until publication;
the public API table can be collected after its global and saved references drop,
while the private host updater remains available.

The live scene refreshes the private cache after frozen actor states and health
callbacks, matching the existing host phase. It bypasses an authored replacement
of the public ArrowEffects.Update function. Raw frame clocks are consumed only
on success; retries preserve them. Reload keeps the same binding table, VM and
saved option identities. The diagnostic clock tape now distinguishes domains
and reads stack provenance in a child host call, so repeated query clocks do not
attempt a second native-stack lease on their caller. Shipping code retains no
diagnostic tape state.

Final-source evidence:

- **658 tests passed across 38 targets**, with no failures,
  ignored cases or filtered cases. This preserves the prior 32 targets and adds
  the arrow target, existing clock target and four existing catalog/game-state/
  player-option/player-state targets. The 14 arrow tests compare
  all **246 existing captured native X/Y/zoom cases**, alpha/nonfinite boundaries,
  per-player rotations, raw clocks, atomic cache failure, reentrant getters,
  selected-option and public-table GC, health ordering, error/retry clock consumption and reload.
  Preparation and live timing also compare the existing pause/speed-ramp corpus
  across music rates and player/nil inputs. The original regression files and
  reference inputs are unchanged.
- 1 existing native host test passed with the explicit
  `host::tests::arrow_` filter; the full library test target compiled. Shipping
  cfg without test-support compiles. Applicable Rustfmt checks pass. Clippy
  matches its prior diagnostics, including the pre-existing src/cmd.rs:349
  immutable-loop error. These validation durations are not performance results.
- [Source review](song-arrows-review.json) checks 1295 watched sources,
  the five archived edit boundaries, native/interpreter/test preservation,
  exact math transfer, reference hashes, formatting and final-source logs.
  [The independent final audit](song-arrows-final-state.json) records final
  source, objective and documentation hashes.

Development logs remain archived. The initial standalone oracle omitted player
constants, and a health fixture initially did not cause a health-state change;
both setups were corrected without dropping their assertions. The first adapter
run exposed repeated native-stack leases during numeric reads. Protected value
conversion and isolated diagnostic stack reads fixed that adapter error without
changing deadlib-lua.

The first broad run passed 606 tests and failed two unchanged statistics GC
tests. Arrow functions had captured the complete catalog, retaining stage
statistics after retirement. Capturing the required player-option levels
fixed those two tests, but the unchanged isolated option-lifetime test then
showed that this still retained unselected levels. Borrowing through a private
weak catalog reference corrected both ownership errors; the runtime retains its
original catalog root and the last selected options stay strongly rooted. The
final focused run includes all seven statistics/arrow/clock/catalog/game-state/
player-option/player-state targets, and the full 38-target run includes their
unchanged lifetime checks. The failed logs, pre-correction sources and source
hashes remain archived; only arrows.rs changed between those failures and the
final source.

The [production migration objective](objective.txt) remains active. Normal song
routing still uses legacy. Presentation/content publication, existing compiled
input snapshots, remaining reference probes and saved-handle/session auditing,
production fallback/cache cutover and legacy isolation/removal remain required.
The final content/downstream suites, normal application build and rendered
song/gameplay/noteskin smoke, graph/link proof and matched preparation/live/memory
measurements are still pending. Interpreter and C API removal milestones remain
closed; this phase does not change the finish line.

## Rust live visual and content publication (2026-11-08)

The Rust live scene now supplies bind_targets and write_visuals for the
existing prepared visual slots. Frozen actor poses publish in binding order,
followed by tracked player/judgment/combo, song-foreground and screen-layer
poses and hidden-player flags. Mesh vertices and texture paths, text and
attributes/stroke, and sprite paths/frame arrays retain the native field order.
Earlier fields and slots stay published if a later read fails. Proxy targets
resolve current actor identity to a local slot in the proxy's own layer;
cross-layer and missing targets retain the native sentinel. Invalid global
bindings leave the previous binding map intact; duplicate identities use the
last binding. Test-support visual paths and draw bindings are also available.

Preparation and live publication share the existing text-attribute and sprite
frame loops through visitors. Their read/conversion/error order is preserved;
the source review reconstructs both refactors from the archived input and
compares the complete formatted files. Live arrays decode once, preserve
bitwise float meaning and keep their Arc when values are unchanged, including
in-place edits, shortened arrays and NaNs. Retained render snapshots keep their
original immutable values. Each protected read owns its request and partial
result; no store or output mutex spans authored getters, collection or nested
actor calls. Lua actor/registry/helper values are traced captures.

Lua debug inspection can obtain a host caller. Publication callbacks therefore
have an atomic single-use guard: recursive and saved calls return a Lua error
without taking the active request or trying to consume an absent payload. The
dedicated test recovers the original getinfo from Def.Actor's captured upvalues
(the public compiler debug wrapper hides raw frames), obtains the real caller, calls it recursively
and after publication, forces collection, and verifies a fresh publication still
works. The initial 683-test source/review remains archived; only the new content
module and the appended test changed for this correction.
The initial reentry fixture inspected the synthetic public debug wrapper; its
failed run and diagnostic probe are retained. It now inspects the captured VM
primitive and keeps the recursive/saved-call error assertions.

Final-source evidence:

- **684 tests passed across 41 targets**, with no failures,
  ignored cases or filtered cases. The previous 38 targets remain present; the
  new live-visual target and existing mesh/proxy targets extend that run.
  The 10 new tests compare real native and Rust compiled live scenes,
  complete prepared visual payloads, collecting getter order, frozen poses,
  tracked visibility, Arc identity, partial errors, layer-local proxy retargeting,
  invalid/missing bindings and immutable snapshots after session teardown.
  The focused five-target run passed 42 tests.
- 11 native publication control tests passed with the explicit
  `publication` filter; the full library test target compiled. Rustfmt and the
  shipping configuration without test-support pass. Clippy matches its prior
  diagnostics, including the existing src/cmd.rs:349 immutable-loop error.
  Validation durations are not matched application performance measurements.
- [Source review](song-live-visuals-review.json) verifies 1298 watched
  sources, the five archived edit boundaries, the two reader refactors and
  final-source logs. The interpreter, native implementations, ArrowEffects and
  original regression files are unchanged. The initial compile caught a
  required mutable VM guard for object identities; the correction and failed
  build log are retained. [Final audit](song-live-visuals-final-state.json)
  records independent source, objective and document hashes.

The [unchanged production migration objective](objective.txt) remains active.
Normal production song routing still uses legacy. Existing input snapshot
integration, reference/eligibility/coverage probes and retained-handle/session
auditing, production fallback/cache cutover and legacy isolation/removal remain
required. Final interpreter/adapter/downstream/content checks, the normal
application build and rendered song/gameplay/noteskin smoke, graph/link proof
and matched preparation/live/memory measurements remain pending. The completed
interpreter and C API removal milestones remain closed.

## Rust actor setter identities and lifetimes (2026-11-08)

The three existing snapshot-eligible setters now have authenticated Rust
forwarders and per-actor bound functions. Copying a bound setter preserves its
target and chained return. Declaration attributes are replaced during native
installation, fresh template clones receive their own setters, and public/raw
overrides, class removal, assignment hooks and second-argument rules are tested
against the existing native controls in both clock modes. Identity records are
private weak Rust data. Retired scenes retain method tables through their traced
Lua owner, including weak actors; saved setters keep their target alive until
released. Sound initialization merges its methods into the existing table.

- **998 tests passed across all 73 existing Rust adapter targets**,
  with no failures, ignored cases or filtered cases. The focused setter target
  passes 10 tests. The identity/GC library test passes with 849 other tests
  explicitly filtered; this does not claim a full library run.
- Formatting and the shipping configuration without test support pass. Clippy
  retains exactly the prior 166 warnings and src/cmd.rs:349 immutable-loop error;
  its complete log hash is unchanged. These validation durations are not matched
  application performance measurements.
- [Source review](song-actor-setters-review.json) checks all 1300 watched sources,
  five archived existing-file boundaries and two new source files. The interpreter,
  native implementations and pre-existing test bodies are byte-identical. Earlier
  failures and interrupted checks remain preserved: missing preparation fields,
  weak retirement ownership, constructor attributes and non-setter fallback were
  corrected without dropping tests. Source review additionally identified Sound's
  table replacement; its focused differential case now passes.

Input snapshot runtime-owner and inspection/compiler drafts are staged as `.txt`
artifacts outside watched production source. They have not been compiled or wired
into the scene. The input-only and combined factories, frozen-plan eligibility and
VM-free teardown controls are the next required migration path.

The [fixed objective](objective.txt) remains active. Normal production song routing
still uses legacy. Remaining reference/eligibility/coverage and saved-handle/session
audits, unconditional routing, fallback/cache cutover and legacy isolation remain,
followed by all final downstream/content, application/rendering, dependency/link
and matched preparation/live/memory gates. The interpreter milestone stays closed.

## Rust input snapshot migration in progress (2026-11-08)

The snapshot drafts described above have now been wired into the Rust live
scene. Input-only owners and combined input/update owners retain the existing
finite bytecode-program schema, shared cells, action order and hierarchical
delta arithmetic. They own no Lua values or VM. The combined factory reads the
retained lifecycle plan; it does not execute authored callback bodies to decide
eligibility. The cached plan now handles the loader's plain synthetic root.

The focused Linux run passed 54 tests: nine new direct engine comparisons,
26 existing actor tests and 19 existing live-scene tests. A second run passed
all 17 original snapshot-contract tests against both engines and the nine
direct comparisons (43 tests total). The Rust contract mirror changes only type
aliases and compiler selection; a formatting-aware source comparison preserves
every original assertion and fixture body. The Windows-only rendering target
ran zero tests under Linux and does not provide rendering evidence.

The second recorder stalled in WSL's shared filesystem after test execution;
its final result.json is unavailable. The original log is retained as
interrupted verification. An [independent Windows source check](actor-input-snapshots-contract-v1-host-source-proof.json)
confirmed all 1307 watched sources were unchanged and retained the log hash.
The source/formatting review passed independently on Windows. SIGKILL is pending
only for the owned stalled recorder; WSL and unrelated work were not restarted.
A separate Windows run is checking the same frozen source. This phase is not
yet fully verified.

[Source review notes](actor-input-snapshots-review-notes-v1.json) record a pending
correction: static model materials must not create a texture update job merely
because their vector is nonempty. They also identify the existing Gameplay
transition's next_screen_name accessor as a remaining Rust scene API gap.
Neither finding adds acceptance scope. Source remains frozen during the checks.

The [original production migration objective](objective.txt) remains active and
incomplete. Normal song routing still uses legacy. Final source regressions,
unconditional routing and legacy isolation, application/rendering smoke,
dependency/link evidence and matched measurements remain required. The completed
interpreter and C API-removal milestones remain closed; completing this goal
does not authorize another phase.

## Rust snapshot validation recorded (2026-11-08)

The static-material eligibility correction is implemented: only an actual
material clock creates the frozen texture job. The Windows behavioral run
passed **78 tests across five targets**: the full 17-case existing contract
with Rust aliases, nine direct engine comparisons, 26 existing actor tests,
19 live-scene tests and seven paired model tests. There were no failed,
ignored or filtered cases. The preceding Windows source passed 71 tests.

The subsequent cleanup removed a blank line after a doc comment and added
three scoped Clippy attributes documenting the existing schema/traversal
argument boundaries. The [audit](actor-input-snapshots-final-state.json)
checks complete-file equivalence after removing only those metadata changes;
it does not describe a repeated 78-test run against different file hashes.
Final formatting/source review and the native Windows build without test
support pass. Clippy reports the prior 166 warnings and the unchanged
cmd.rs:349 immutable-loop error; no diagnostics remain in the new snapshot
files. Earlier failed, interrupted and pre-cleanup checks remain preserved.

The Windows checks avoid the stalled WSL shared filesystem. Its interrupted
Linux recorder and two owned diagnostic reads received SIGKILL; those signals
remain pending until the filesystem calls return. No WSL restart or unrelated
process termination was performed. This is adapter validation, with no claim
of normal migrated application or rendering validation.

Production facade/fallback/cache cutover and legacy isolation are the next
implementation work. The existing Gameplay next_screen_name API gap remains
recorded for that cutover. The final application build and rendered smoke,
dependency/link evidence, final regression suites and matched measurements
remain required by the [unchanged objective](objective.txt). The production
migration is incomplete; the interpreter and C API-removal milestones stay closed.
