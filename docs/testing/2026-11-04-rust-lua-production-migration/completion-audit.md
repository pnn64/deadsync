# Production migration completion audit

The original [objective](objective.txt) is satisfied within its fixed inventory.
The standalone interpreter and C API removal milestones remain closed. This
migration adds no language, baking, JIT, assembly or allocation-failure campaign.

## Production result

DeadSync unconditionally uses the independent Rust `deadlib-lua` engine for song
compilation/preparation, live callbacks, reloads and fallback, noteskin
preparation, and Simply Love's Step Stats GIF definitions. Generic language and
embedding facilities remain in `deadlib-lua`; song and noteskin behavior stays
in its respective adapters. Native Lua is isolated to explicit development/test
oracles. A normal build requires no engine-selection feature or legacy fallback.

| Original requirement | Evidence |
| --- | --- |
| Fixed dependency, raw-access, retained-handle and consumer inventory | [Site accounting](production-inventory-audit-v1/README.md) covers the original sites. The original inventory, baseline sources, native executables and historical reports are preserved. |
| Existing song behavior and callback lifecycle | The [88-target incremental rollup](production-original-88-target-rollup-v1.json) records 2,211 passes, two explained red assertions and 43 original ignored tests. Full coverage ran at window 11; seven affected loading/compiler targets reran at final window 13 with 109 passes. This is not an assertion that all 88 executables were rebuilt after that final narrow repair. All original assertions and ignore attributes remain intact. |
| Existing application consumers | The original semantic target passed 306 checks with one explained setter-history assertion and 94 original ignored tests. At final source, [all 72 gameplay checks](production-loader-gameplay-v3-downstream-windows-debug/result.json) and the [original Multitap regression](production-loader-multitap-v3-downstream-windows-debug/result.json) pass. The full shell suite passed 424 checks with zero failures and 16 original ignored tests after the existing background-fit helpers were restored. Final rendered gameplay also passes. |
| Raw declaration and production loader semantics | [109 affected checks](production-loader-profile-v1-song-windows-debug/result.json) pass. Bare tail-called declaration loads retain upstream's missing-source rejection. Production host loaders retain their existing file-directory fallback. No process-directory fallback was added. The development input-frame example also [type-checks](production-input-frame-example-check-v1/result.json) after its shared-type module import was repaired. |
| Core embedding, GC, limits, loading, cache and threads | 262 accepted core controls pass. All 238 current core source files are [byte-identical](production-final-unchanged-consumer-sources-v1.json) to that accepted snapshot. Earlier Linux language evidence remains source-equivalent; the interpreter milestone was not reopened. |
| Noteskin callbacks, bundled/regression content and cache | 321 accepted checks pass; all 56 current noteskin source files are byte-identical to that snapshot. The final [26-case paired run](production-loader-noteskin-matched-v1/summary.json) matches every complete bundled dance/pump output. Version 30 invalidates version-29 compiler artifacts; only that header identity is normalized for comparison. |
| Remaining GIF consumer | The bounded GIF probe passes 112 native comparisons across 28 cases. Its adapter, all 238 engine inputs and fixture files are [unchanged](production-final-gif-source-identity-v1.json). This is bounded capture/quota evidence, not exhaustive allocation-failure validation. |
| Rooting, lifetime, reentry, cleanup and session ownership | [Ownership/cache review](production-cache-and-session-source-review-v1.json), retained-session controls and the 300-actor complete native/GC comparison cover the replacements. Six actor capture batches use traced Lua tables without changing the generic 255-capture limit. |
| Preparation, live execution, fallback and cache policy | Existing compiled-first and input-specific live behavior remains. The original Sharkmode direct/texture draw regression and full shell suite pass. All nine previously failing microfixture production payloads match native, as do the final conflicting-speed-setter payload and texture/background-fit controls. There is no native-runtime fallback. |
| Normal graph and link | [Normal/instrumented normal-build graphs](production-lua-graph-v3/result.json) each contain 399 packages and no legacy runtime. Manifests are unchanged since that audit. The [final normal release build](production-loader-final-v5-application-windows-release/result.json) passes, with both full-workspace and fixed-migration source guards true. Its map contains Rust engine symbols and no forbidden legacy symbols. |
| Actual application loading/gameplay/rendering | The [final smoke](production-loader-rendered-release-v2/process.json) completes 600 frames, including 120 warmup and 480 measured frames, with exit 0, no warnings/errors and unchanged sources. The [screenshot review](production-loader-rendered-release-v2/visual-review.json) confirms gameplay HUD, cel receptors, autoplay and background video. This is functional rendering evidence, not synchronized native pixel equality. |
| Formatting and Clippy | All [33 changed Rust files](production-adapter-fixes-format-v6.json) pass formatting. Core and noteskin Clippy pass in their unchanged-source snapshots. Final song Clippy has only the frozen `cmd.rs:349` denied lint, with its source guard true. Downstream all-target Clippy retains the separately documented frozen export_gameplay constructor-arity errors. No blanket allow was added. The earlier workspace format audit retains its 114 baseline/unrelated failures; it is not a claim that the whole shared workspace is formatter-clean today. |
| Representative preparation/live cost and memory | Final release tools built with full-workspace and migration guards true. [Recorded costs](measurement-summary.md) include paired preparation/live wall time, thread cycles and process RSS/commit, plus bundled noteskin timing. Background compilation qualifies the live readings; a strict retry refused to run. No external process was stopped for timing, and no performance target was introduced. |
| Accurate ownership and validation documentation | Active architecture, engine, matrix and song documentation describe the unconditional Rust production engine and link to this audit. Earlier reports and the documentation before this completion update are retained. |

## Retained red checks and independent findings

These assertions remain enabled and are not counted as passing.

| Finding | Attribution and limit |
| --- | --- |
| Textured mesh alpha in playback | The frozen test constructs its mesh without running Lua and observes alpha 1 instead of 0.5. The [baseline audit](song-after-noteskin-song-windows-release/glow-baseline-audit.json) retains the failure and source proof. No renderer repair was added to the Lua goal. |
| Unranked coverage diagnostic order | The two missing-identity messages can reverse allocation-identity order. Known subscriber delivery/state remains checked. Same-binary isolated/full repeats and [source review](production-order-diagnostics-review-v1.json) retain the evidence. No runtime/test sorting or normalization was added. |
| Intermediate speed-setter history | The original 54-call assertion passes; 32 intermediate beat-zero field snapshots differ for overlapping setters visited through `pairs`. `speed_writes` exists only under test-support. The original fixture's final explicit XMod(4) produces identical complete production payloads. The [bounded classification](production-order-diagnostics-review-v1.json) does not promise arbitrary scripts relying on table iteration order are equivalent, and does not introduce C table-layout compatibility. |
| Four architecture assertions | [Source review](production-downstream-architecture-review-v1.json) distinguishes two frozen static mismatches from separate benchmark/input-backend changes. Tests and allowlists were preserved. |
| Workshop model/material relocation | [Causal source review](production-downstream-workshop-source-review-v1.json) identifies the independent committed material-path resolver and missing relocated sibling INI. Lua's literal model declarations and bundled compiler payloads match. No independent custom-pack native execution or pass of that original assertion is claimed. |
| Fifteen theme UI assertions | [Test source review](production-downstream-theme-source-review-v1.json) retains the formatting, allocation, storage/layout and debug-representation failures; all failing test sources and the 64 relevant browser/shared module files match the baseline. These are separate UI/input assertions, not GIF definition differences. Source identity alone is not presented as proof that the entire theme suite was executed on the baseline. The GIF adapter and payload comparisons have separate direct evidence. |
| Workspace and tooling movement | Independent notefield and audio work was preserved. Raw failed workspace guards remain in intermediate reports; [notefield review](production-concurrent-notefield-source-review-v1/review.json) records its exact pure identity change. Final application and measurement-tool builds have clean full-workspace guards. The accidental broader example-build failure is preserved; the example is repaired and the original 88 test targets all ran. |

The original broad downstream run and all transient failures remain in the
[failure index](production-downstream-failure-index-v1.json). No blanket claim
that every test in the shared worktree is green is made.

## Platform and measurement limits

Final application/rendered verification is Windows x64 MSVC, OpenGL, windowed
1280x720, WASAPI shared, using the fixed original gameplay case. A fresh Linux
application run, Vulkan pixel readback and manual interactive testing were not
performed for this final adapter snapshot. Earlier core/Linux results and their
source identity are recorded separately. The original ignored platform/long-song
tests remain unchanged. The native live visual hash also varies across repeated
native processes; [bounded analysis](production-final-live-native-variation-review-v1.json)
retains those hashes without forcing equality or introducing pointer-layout work.

The fixed migration finish line is reached. No subsequent feature or performance
work is part of this completed milestone.
