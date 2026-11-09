# Production migration completion audit

The [verbatim objective](objective.txt) defines this audit. The goal is active;
completion is unproven. Completed interpreter/C API removal milestones remain
closed. The remaining gates below are from the original request. No additional
StepMania features, language parity, baking features, native backends or
exhaustive allocation-failure campaigns are required.

| Original requirement | Evidence and remaining verification |
| --- | --- |
| Fixed dependency, raw-access, handle and downstream inventory | [Site review](production-inventory-audit-v1/README.md) accounts for all original sites and three direct consumers. |
| Recorded supported behavior, fixtures, commands and workloads | `inventory.json`, frozen sources, baseline copies and preserved native application/measurement executables remain intact. |
| Generic engine/domain adapter ownership | Unconditional song facade, noteskin compiler and GIF capture use `deadlib-lua`; domain bindings remain in the adapters. |
| Song construction, lifecycle, managers, timing/RNG, input/messages, media, compilation and live callbacks | [Full run review](production-final-full-song-review-v1.json): 2,190 passes including 18 byte-identical launch-only retries; sole assertion is the frozen no-Lua mesh alpha defect. The formatter repair passes its original screen UI and global-helper tests. The subsequent downstream replay-hook repair is under focused native comparison. |
| Noteskin callbacks, bundled/regression content and GIF definitions | Final noteskin suite: 321 passes, including bundled/regression comparisons and cache transition. Final GIF probe: 112 native comparisons over 28 cases. Neither consumer's implementation changed in the replay-hook repair. Downstream theme regressions remain pending. |
| Ordering, shared state, errors, files/modules, sandbox and limits | Full song/native controls, 262 final core embedding/GC/limit/loading/cache/thread checks and bounded GIF controls pass. Original application replay harness exposed four missing test-support methods and a receiver mismatch; repair verification is active. |
| Rooted values, GC, cleanup, reentry and thread/session ownership | [Source/test review](production-cache-and-session-source-review-v1.json) traces the owned VM, release queue, worker transfer and retained callbacks. Original live/retirement/preparation checks pass; affected live/reload native comparisons are being rerun after the test-support repair. |
| Preparation, baking, live execution and Rust fallback semantics | Existing finite/live strategies and public routing pass the full song checks. No new baking capability is required. Final downstream validation remains pending. |
| Cache correctness | Noteskin identity 29 to 30 invalidates incompatible artifacts while preserving old files and testing rebuild/reuse. Song preparation and GIF catalog hold in-memory data. Noteskin and generic cache regressions pass. |
| Unconditional cutover; no normal legacy dependency, engine switch or fallback | `production-lua-graph-v2`: both normal/build graphs contain 399 unique package names and zero legacy runtimes; all 81 manifest/config inputs remain unchanged. Normal release build and inspected MSVC map pass, with 1,671 Rust engine symbol/object rows and no legacy objects/C API symbols. Refresh normal build after the replay-hook edit. |
| Relevant final interpreter, adapter and downstream regressions | Core 262 and noteskin 321 pass. Earlier Linux 954-check core suite has identical normalized core source. Full song result above precedes the test-support repair; six affected live/reload targets are active. Original eight-package downstream run retains its five compile errors; rerun after repair. |
| Normal application song loading, gameplay and noteskin rendering | [Application review](production-final-application-review-v1.json): normal release build and strict rendered DREAMING-ING!!/cel/OpenGL/Wasapi smoke pass; screenshot visually inspected. This is functional evidence, not synchronized pixel equality or controlled timing. Refresh final normal build/smoke after replay hooks. |
| Formatting and Clippy | Read-only 1,163-file audit: migration formatting fixed, core clean, 114 baseline/unrelated failures attributed. Core Clippy passes. Song Clippy retains the byte-identical `cmd.rs:349` immutable-condition error; no blanket allow. Downstream first Clippy attempt was stopped before completion. Final noteskin/downstream Clippy and changed-file formatting remain to record. |
| Representative preparation, live execution and memory measurements | Frozen and Rust release tools/25 DREAMING-ING!! inputs preserved. Functional current/native runs pass. Native raw visual hashes also vary across identical native processes; [bounded review](production-final-live-native-variation-review-v1.json) retains the mismatch and explains allocation-identity ordering without weakening tests. Controlled matched release runs and report remain pending; no LuaJIT target. |
| Active architecture, manifests, profiles, tooling and validation documentation | Active docs describe unconditional ownership/cutover; historical reports preserved. Final status/evidence updates remain pending. |

The known mesh alpha assertion remains enabled and constructs its mesh without
executing Lua. The known song Clippy error is byte-identical to the frozen
source. New failures must be explained or fixed; no failing coverage is deleted,
weakened or skipped to claim completion.
