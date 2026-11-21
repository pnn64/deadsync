# BMP decoder pass 2 — experiment ledger

This pass starts at `945da378a907bb7c4fec1f432d02d320c9d2a462` and changes only the BMP codec. It does not integrate the codec into application loading. No changes are staged or committed.

Final production decision: zero BMP optimizations retained from this pass. Candidate 5's two target specializations improve their intended formats but still fail the unchanged-format performance gate. All BMP production files are restored to exact original baseline bytes; only useful added regression tests remain. Final restored-source tests, formatting, and strict all-target codec Clippy passed. The sections below preserve the complete experiment chronology, including rejected identities, favorable target measurements, and every adverse result.

## Frozen baseline

`baseline/identity.json` records 22 source/config files and 13 repository inputs. `baseline/executables.json` identifies separate timing/allocation/snapshot binaries. The behavior snapshot contains 22,570 records / 42,819,895 bytes and has SHA-256 `076e78cae341e782b60f72fa3f60c0ff85142759e4821069f1012063f37bf0df`, identical to the historical snapshot.

Timing uses CPU 2, QueryThreadCycleTime, five median batches, owned output destruction, and no timed I/O. Baseline full matrices used 20 ms per batch; alpha-position controls use 80 ms. Allocation instrumentation runs only in its separate binary. Initial allocation sidecars include ignored batch settings; later sidecars explicitly identify counter-only runs. The allocation binary compiles out all timing and emits zero timing fields.

The baseline has 120 synthetic native cases, 98 exact image comparisons, and 12 valid repository files. Twenty-two synthetic output-policy differences and one intentionally invalid repository file remain explicit in the logs. Consuming RGBA wins 98/98 exact synthetic comparisons (image/native cycle geometric mean 2.404854) and 12/12 repository comparisons (2.634531). Borrowed RGBA wins 97/98 (2.167182); the retained loss is `indexed1-511x257-top`: 833,864.80 native versus 794,934.95 image cycles. See `baseline-summary.json` and complete reference reports.

## Candidate 1: source alpha scan — rejected in its original form

Identity: `candidate1-alpha/identity.json`, parent `baseline`. The change determines implicit BGRA opacity from the validated source and writes final alpha during conversion, removing the later complete output rewrite. Production diff: 12 lines added / 12 removed. A focused test checks ignored gaps/suffixes across all five Windows headers and both orientations.

Validation passed: 35 crate integration tests, one task-local test checking every pixel in 18 independent alpha-position inputs, one doctest, formatting, and the complete 22,570-record snapshot. All 622 paired allocation rows are unchanged; their timing fields are zero. See `candidate1-alpha-preparation-summary.json` and the build/snapshot logs.

The new controls have all-zero alpha or a lone nonzero alpha at the first/last logical pixel, both orientations, and 17×9, 511×257, and 1920×1080 dimensions. Their generator and full source/expected-output hashes are retained. Six all-zero inputs admit exact image comparisons; twelve mixed-alpha cases have an explicitly different image alpha policy.

The original source scan follows encoded rows. This changes short-circuit position for bottom-up images. Paired 80 ms runs confirmed a material regression for lone-first logical alpha in all three bottom-up sizes:

| Size | Decode after/before | Consuming RGBA after/before |
| --- | ---: | ---: |
| 17×9 | 1.259732 | 1.346392 |
| 511×257 | 1.948613 | 1.238163 |
| 1920×1080 | 1.394961 | 1.255962 |

All-zero consuming RGBA improves on all six controls, with reductions of 10.2–26.5%. The six unchanged image controls have a geometric after/before ratio of 1.005789. Those gains do not justify retaining the sparse-alpha regressions. A conspicuous medium top-down decode-only baseline result produces an apparent 4× gain; it is retained in the data and is not used as evidence of an algorithmic gain.

Raw records: `candidate1-controls-{before,after}-{decode,owned}.tsv`; full cases and every loss: `candidate1-controls-{decode,owned}-comparison.md`; machine-readable details: `candidate1-controls-summary.json`. The pair order was baseline→candidate decode and candidate→baseline consuming RGBA. Both process boundaries were clean. No full candidate matrix was run after this rejection.

## Candidate 1 refinement: logical source row order — initially provisional, not retained

Identity: `candidate1-logical/identity.json`, parent `candidate1-alpha`; `parent-delta.patch` records the isolated refinement. Top-down input retains a flat scan. Bottom-up input scans encoded rows in reverse while preserving pixel order inside each row, matching the original logical output scan's early exit. The complete alpha rewrite remains removed.

The refinement is built and frozen. The exact snapshot, 35 crate integration tests, the task-local full-pixel alpha-control test, one doctest, and formatting all pass. All 622 allocation rows remain unchanged. Source/config and both input manifests were verified before and after compilation. The timing executable SHA-256 is `2eb3bf981a599b27364dd7184d2e4f029cdbbb516726572cb694f2e17b3f32ee`; see `candidate1-logical-preparation-summary.json`.

Fresh baseline/refinement controls ran at five median batches of 80 ms on CPU 2, decode AB and consuming RGBA BA. All six zero-alpha consuming cases improved: after/before ratios 0.725668–0.896266 (10.4–27.4% fewer cycles). The unchanged image controls have geometric ratio 0.974722. Mixed-alpha tiny consuming cases retain losses of 0.3–4.4%; large cases range from 0.987138 to 1.013926. Complete per-case data and every loss are in `candidate1-logical-controls-summary.json` and both comparison reports.

The 511×257 lone-first cases showed incompatible large swings between modes: bottom-up decode ratio 0.240534 versus consuming ratio 4.093657, with a reverse top-down swing. These are preserved, not counted as algorithmic gains. A granted follow-up isolated each of the two input files in fresh processes, using baseline/logical/logical/baseline for decode and logical/baseline/baseline/logical for consuming RGBA. Both medians per binary were retained; no fastest result was selected. Geometric logical/baseline ratios were bottom decode 0.978891, bottom consuming 0.996324, top decode 1.010243, and top consuming 0.989210. Same-binary spreads were 0.10–3.47%. Both implementations share the roughly 680k-cycle top-down decode versus 192k-cycle top-down consuming regime. The earlier fourfold parent/candidate effect therefore did not reproduce under isolated invocation. The exact cause of the workload/allocator/code-layout sensitivity has not been identified; it is not evidence of a fourfold decoder change. See all 16 medians in `alpha-isolated-summary.json`.

Historical decision at this stage: provisionally retain the logical-alpha refinement for its consistent zero-alpha benefit, pending the full matrix. It was later removed in candidate 5 because the broader bundle did not clear the control gate. Tiny mixed-alpha losses and medium-size process sensitivity remain in the record.

## Candidate 2: fixed-mask RGBA4444 expansion — measured, not retained

Identity: `candidate2-rgba4444/identity.json`, parent `candidate1-logical`; `parent-delta.patch` isolates this change. For 16-bit input with explicit alpha and exact ordered masks `[0x0f00, 0x00f0, 0x000f, 0xf000]`, the row converter extracts the four little-endian nibbles and multiplies each by 17. This replaces four generic channel mask/shift/replication operations. Other masks retain the original generic conversion. No allocation, unsafe code, or public API is added.

The independent test covers all 65,536 input words plus 256 repeated words, odd width 257 with nonzero row padding, headers 56/108/124, both row orientations, and three noncanonical mask permutations. Expected values use the existing independent bit-repetition oracle. A separate read-only review found no concrete correctness defect.

Build session 54998 completed successfully. The full 22,570-record snapshot, 36 crate integration tests, the task-local full-pixel alpha-control test, one doctest, and formatting all pass. All 622 allocation rows remain unchanged. Source/config and both input manifests were verified before and after compilation. The timing executable SHA-256 is `9c72b410f3ca18861fe953c70c775dfe7ac4b5a404276042cc4b36b07c6516a8`; see `candidate2-rgba4444-preparation-summary.json`.

The conditional parent/child screen covered RGB555, RGB565, generic RGB333, and RGBA4444, four sizes and both orientations, five median batches of 80 ms on CPU 2. Decode ran parent then child; consuming RGBA ran child then parent. RGBA4444 improved 6/8 target cases in both modes, with geometric after/before ratios 0.795700 decode and 0.798354 consuming. The unchanged image reference for the target had ratio 0.990463. Target improvements are 19.6–20.4% at 17×9, 25.0–25.4% decode / 13.3–13.9% consuming at 1920×1080, and larger medium-size gains whose magnitude remains subject to the observed process sensitivity.

Both 1×1 target cases regressed: decode ratios 1.043152/1.055835 bottom/top; consuming ratios 1.080727/1.048871. Unchanged native 16-bit controls had geometric ratios 1.030660 decode and 1.031293 consuming, while the available unchanged image references had ratio 1.017110. In particular, RGB333 consuming controls had ratio 1.105306 versus their image control ratio 1.048357; their worst case was 511×257 bottom-up at 1.447604. The complete results retain all these losses and do not attribute them all to host drift. See `candidate2-4444-focus-summary.json` and complete per-mode comparison reports.

Decision: provisionally retain the measured roughly 20% target gain. Full-matrix results must assess the native control regressions before final retention. If later assembly shows that the larger masked_row match changed inlining, a minimal measured inlining experiment may be warranted; this is currently an unverified hypothesis, not a new implementation plan.

A subsequent narrow check of the frozen candidate 2 binary and its matching PDB found no separate masked_row, map_row, or decode_with_limits function. Its 1,325 PDB symbols contain eight BMP functions; the decoder is one 11,842-byte decode body, with Channel::new and masks_format as the other separate decode-module functions. The matching symbol data and MSVC disassembly are retained in `assembly-candidate2`. This evidence does not support a lost-masked_row-inlining explanation, so no inline/layout variant was introduced. It does not by itself explain the measured control regressions.

Candidate 2 remains conditional on the logical-alpha parent. If that parent is rejected, candidate 2 must be rebuilt and revalidated without it before its own final comparison. All frozen ancestors and adverse measurements remain intact.

## Timing process checks

The three timing phases completed successfully, with 21, 88, and 48 process-watcher checks respectively and no matching external compute processes. The logical-alpha before/after boundaries were also clean. The watcher is a frozen copy of the JPEG task helper, SHA-256 `4866d91135259e1914465567804f08cf1b8ffbdfbd29328ada7187f3165196a1`; command arrays, raw TSV/log/identity sidecars, and JSONL process records are retained. These checks do not establish absence of every possible background workload. No builds or tests ran during the timing lease. All BMP timing jobs stopped before releasing the lease to PNG.

## Candidate 3: direct RGBA-byte-order input — measured, not retained

The broad truecolor initialization rewrite was narrowed to explicit 32-bit RGBA-byte-order input. The source proposal `candidate3-direct-rgba.patch` was applied and frozen as `candidate3-direct-rgba`, parent `candidate2-rgba4444`. It reserves exactly as before and appends initialized source bytes once, using a contiguous copy for top-down input and reversed rows for bottom-up. The obsolete identity-copy arm is removed. General mapped-output zeroing remains unchanged. Production change: 23 added lines, three removed; no unsafe code, extra allocation, or public API.

Build session 33716 and the preparation checker completed successfully. All 22,570 snapshot records match exactly; 37 crate integration tests, the task-local every-pixel alpha-control test, one doctest, and formatting pass. All 622 allocation records remain unchanged. The 22 source/config files, 13 repository inputs, and 18 alpha controls were checked before and after compilation. The frozen timing binary SHA-256 is `762925d31da68079aca897395e08faa913f56884021b62633c7887fb44ad9fc4`. Independent read-only review found no source-boundary, allocation/error-order, alpha-policy, or fallback defect.

`next-timing-plan.json` was captured before the candidate 3 focus and whole matrix ran. Its original prepared status is retained as historical provenance. A separate strict watcher extends the original process list to every bench/timing/snapshot/rssp name and additional compiler/build tools; both its parent and new hashes are recorded. A rejected candidate 3 must not be presented as the final source.

The 40 ms focus completed in session 92527 with 94 clear process checks. The direct RGBA target improved all eight cases in both modes: geometric after/parent ratios 0.771168 decode and 0.793971 consuming; target image control 0.999482. Unchanged 511x257 RGB333/555/565 decode cases regressed 49–63% in both orientations; BGRX bottom-up was 2.293x. All results and sample ranges remain in `candidate3-focus-summary.json` and `candidate3-focus-comparison.md`.

A granted isolated RGB333-511x257-bottom ABBA follow-up ran each parent/candidate invocation in its own process (session 61422, 24 clear process checks). Candidate/parent ratios were 1.007268 decode and 1.011232 consuming; image control 0.998374. Both binaries used the roughly 1.15M-cycle decode regime, so the earlier 50% implementation effect did not repeat under isolated invocation. Every median/range is retained in `candidate3-isolated-rgb333-summary.json`. This does not clear the other non-target losses.

The granted original-baseline/candidate3 whole matrix then completed (session 72105, 115 clear checks), followed by all 18 alpha-position controls (session 45185, 30 clear checks). Complete JSON records and every native loss are in `baseline-final-matrix-{summary.json,comparison.md}` and `baseline-final-alpha-{summary.json,comparison.md}`. These filenames identify that comparison run; they do not establish final source retention.

| Whole matrix | Native after/baseline geometric cycles | Ratio of summed native cycles | Image control ratio |
| --- | ---: | ---: | ---: |
| Decode | 1.010951 | 0.975779 | Not applicable |
| Consuming RGBA | 0.990422 | 0.987771 | 1.009828 |
| Borrowed RGBA | 0.983523 | 0.975802 | 0.992434 |
| Repository consuming | 1.019757 | 1.014298 | 1.019868 |

The consuming target ratios are RGBA4444 0.798367, direct RGBA 0.796456, and zero-alpha 0.876082. Their image controls are 0.996837, 0.996597, and 1.031261. RGB555/565/333 decode each regresses about 10%; consuming ratios are 1.062542/1.054900/1.057076. RGB333's image control is 1.028241; only two RGB565 cases admit exact reference comparison. Canonical RGBA32 consuming is 1.122839 versus image 1.056576. These are material unresolved losses, not all attributed to host drift.

Consuming still wins 98/98 equivalent synthetic cases and repository 12/12. Borrowed wins 96/98, losing both 511x257 indexed1 orientations. Top-down borrowed also shows a large opposing native/reference process-regime swing; the complete native and reference records remain available. Geometric means weight each case equally; summed-cycle ratios weight one invocation of every native case. Neither predicts an application's input mix.

All six zero-alpha consuming controls improve 7.5–25.5%, with image control 1.008770. Mixed-alpha small cases lose 2.6–7.3%, large cases 1.6–5.4%. The medium lone-first flips recur (bottom decode/consuming ratios 0.262/3.761; top 2.051/0.561), so their aggregate is not treated as an algorithmic result. Selection remains withheld: beating image does not justify accepting the non-target native regressions.

## Candidate 4: confine opaque-alpha selection to canonical BGRA — measured, remedy rejected

Parent: `candidate3-direct-rgba`. The only production change removes the guarded opaque branch ahead of every masked row, passes the existing decision into `masked_row`, and chooses the existing constant-255/stored-alpha conversion inside the canonical 32-bit BGRA arm. The parser enables opacity only for BI_RGB32 and fixes exactly those masks, so other 16-bit/mask paths never need the flag. Source scan order, all allocations/error order, RGBA4444, and direct RGBA remain unchanged. This is a code-generation hypothesis to measure, not an asserted explanation of the earlier losses.

The parent source/configuration and all inputs were verified before editing. The parent timing executable and matching PDB were also preserved for later static inspection. Preparation initially stopped before any native command because the optional PDB copy used a hyphenated basename; the actual filename uses underscores. The error and the precise no-replay recovery are recorded in `candidate4-alpha-dispatch-preparation-failure.json`.

Session 83061 completed with all 18 directly captured commands returning exit zero. Native stdout/stderr is now stored directly in `candidate4-alpha-dispatch-validation/*.log`, with commands, timestamps, and checked native exit codes in `commands.jsonl`. The exact 22,570-record snapshot, 37 crate integration tests, one independent alpha-control test, one doctest, formatting, strict all-target codec Clippy, and strict all-target/all-feature benchmark Clippy pass. All 622 allocation rows are unchanged. The 22 source/configuration files, 13 repository inputs, and 18 alpha inputs match before and after compilation.

The cumulative patch SHA-256 is `a1ca3865a1e681793307e4e7823a69c03d4fc3ac5236896187b4124b560e0e8e`; decoder source SHA-256 is `c5e96ff4dc644ff6b76eac336385caf2ab6b271593198cbcb1bed8bb36583934`. Timing executable SHA-256 is `fab8fef6e02991e32db9a20ce1204b3f78810fcd81072d344376707332a2ab57`. See the frozen stage identity, parent delta, executable manifest, preparation summary, and validation summary.

`candidate4-timing-plan.json` prepares a parent/candidate focus including all 32-bit paths, unchanged RGB333/555/565, and the RGBA4444 target, followed conditionally by the original-baseline full matrix and all alpha controls. This is the final selection discriminator. If material non-target losses remain, recommend the smallest defensible retained subset and one rebase/validation gate; do not retain all three merely to reach an optimization count.

The granted focus completed in session 21845: all 20 native commands returned zero, and 112 strict-watcher checks were clear. Each binary emitted 72 decode cases and 72 consuming native cases plus 50 exact image controls. No candidate 4 baseline/full-alpha timing ran. Complete records, all 26 decode and 36 consuming losses, sample ranges, metadata, and the same-binary parent repeat are in `candidate4-focus-summary.json` and `candidate4-focus-comparison.md`.

RGB555 decode improves to a candidate/parent ratio of 0.941207, while RGB333 and RGB565 are 0.991973 and 0.993479. Their consuming ratios are 0.979871, 0.983772, and 0.998316. All unchanged consuming controls are 0.993950 versus unchanged image 0.993880, so this focus does not establish broad recovery beyond reference drift. Direct RGBA consuming loses all eight cases, with geometric ratio 1.026648 versus image 1.003770. Medium direct/canonical RGBA decode losses remain large and context-sensitive; all are retained rather than assigned a speculative cause.

Comparing the same candidate 3 binary across the two identical 40 ms focus invocations shows several 3–6% shifts. RGB333 consuming and its reference both shift about 5.3%. This is evidence of run/context sensitivity, not permission to discard losses. The recommendation to reject candidate 4 as the remedy and remove the broader logical-alpha transformation was accepted. One final correctness/allocation/full-matrix gate will assess only the two exact-layout specializations. That reduction does not prove alpha caused the collateral regressions.

## Candidate 5: two exact-layout specializations — reduced rebase rejected

Parent: original `baseline`. The source contains only the frozen RGBA4444 nibble expansion and explicit RGBA-byte-order direct copy, including removal of the latter's obsolete identity row arm. Original post-conversion implicit-alpha scanning and repair is restored verbatim. All three useful added regression tests remain. No new optimization is introduced to meet a count.

`candidate5-two-layouts-source-proof.json` records exact text equivalence to the original production source plus only those two frozen sections, and verifies every other BMP production source against baseline. The current decoder SHA-256 is `be772c50798d8e90e196c75ee42434aca65a28135a65cf183b2eef5a887e599e`. Independent read-only review found no concrete source-boundary, allocation/error-order, alpha-policy, or fallback defect.

Session 67820 completed exit zero. All 19 directly captured commands returned zero: the exact 22,570-record snapshot, 37 crate integration tests, one independent alpha-control test, one doctest, formatting, strict all-target codec Clippy, and strict all-target/all-feature benchmark Clippy pass. All 622 paired allocation rows remain unchanged. All 22 source/configuration files, 13 repository inputs, and 18 alpha controls match before and after compilation. See `candidate5-two-layouts-validation-summary.json`, `candidate5-two-layouts-preparation-summary.json`, and the direct logs in `candidate5-two-layouts-validation`.

The cumulative patch contains 44 production lines added and 12 removed, plus 126 retained regression-test lines. Patch SHA-256 is `a05fdfaa951c1f5e88099fa7280f1bfaefb107018edeafd7f2069a73680a2d45`. The new timing executable SHA-256 is `c6b467d7cad9654ecbf44e797792c8cba4eff50a75a4b73c2ac34d297f14b351`; allocation binary is `fa5bbb6b6e5b5e5f70648fa240cd4fe8caf97dfc38859aabb40f31b635d5da31`. No BMP jobs remain active.

The granted original-baseline full matrix completed in session 77414 (eight jobs, five 20 ms batches; 115 clear strict-watcher checks), followed by alpha controls in session 74793 (four jobs, five 80 ms batches; 30 clear checks). All twelve commands returned zero and emitted the expected rows. The immutable timing-plan file retains its pre-run prepared status; completed raw sidecars and summaries establish what actually ran.

The consuming RGBA4444 group improves from 34,061,026.31 to 25,518,030.82 summed per-input median cycles and 283.25 to 377.37 MPix/s; geometric ratio 0.751982, eight of eight wins, image control 0.984883. Direct RGBA improves from 15,624,566.08 to 14,302,892.40 cycles and 618.03 to 674.14 MPix/s; geometric ratio 0.768916, eight of eight wins, image 0.977650. The whole native geometric ratios are decode 1.007878, consuming 0.973141, and borrowed 0.988248. Cycle sums and geometric means have different input weighting; neither is an application workload estimate.

Unchanged decode controls remain 1.046324 overall, with RGB555/565/333 at 1.065486/1.065651/1.087146 and all eight cases slower in each group. Matched consuming controls (82 exact-reference cases) are native 1.008548 versus image 0.993460; borrowed controls are 1.015540 versus 0.990052. Indexed8 consuming remains 1.060209 versus image 1.014612. These remaining losses fail the selection gate despite favorable target results. All 94 decode, 76 consuming, 64 borrowed, and three repository native losses remain in `candidate5-baseline-matrix-comparison.md` and its complete JSON.

Candidate 5 still wins 98/98 consuming and 12/12 repository comparisons against image, while borrowed wins 96/98 and loses both medium indexed1 orientations. Restored alpha paths retain the medium first-alpha regime flips (bottom decode/consuming 0.277768/3.729028; top 2.086987/0.524566). Zero-alpha consuming is 1.019483 versus image 0.991254. All alpha cases and ranges remain in `candidate5-baseline-alpha-{summary.json,comparison.md}`. No causal explanation is asserted and no unfavorable case is removed.

The authorized final decision is to reject candidate 5 and stop this tuning loop. `restore-baseline-production.py` guarded all 22 current candidate source/configuration hashes, then wrote only the decoder from its frozen original bytes. All four production files now match baseline; 21 of the 22 source/configuration files match baseline, with only the retained regression-test file differing. The proof is `restored-baseline-source.json`; restored decoder SHA-256 is `84591b2faaf4455f788ad7b475693fa93d7b4aa2793dda43b901cd66a463454e`. No new benchmark or snapshot build is necessary because production and harness/compiler inputs match the original baseline.

Final restored-source session 78920 completed exit zero. Its three directly captured commands passed formatting, all 37 crate integration tests, one independent alpha-control test, one doctest, and strict all-target codec Clippy. All 22 source/configuration identities match before and after. `final-restored-validation-summary.json` records exact commands and exits, and `retained-regression-tests.patch` records the only retained code change. An independent raw-data audit verified all 622 final matrix/alpha record pairs with 7,221 numeric checks; see `candidate5-summary-audit.json`. No more BMP benchmark, snapshot build, allocation run, or timing followed restoration.

## Provisional validation log limitation

The earlier completed Core command outputs showed the test counts and success reported above, and each preparation script checked native exit status. However, the historical PowerShell Start-Transcript build logs omit native Cargo stdout/stderr; they are not complete native test logs. Candidate 4 resolves this for its own source with the direct Python captures described above, including strict Clippy. If a different subset is finally retained, its rebase must receive the same durable validation. No historical test output was reconstructed, and the earlier logs remain intact.
