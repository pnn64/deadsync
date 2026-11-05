# Combo milestones and golf lookup bounds — 2026-11-04

Baseline: `aab4306bef34ec7c27c05fa4cbba9af050d9a5bb`. Three safe Rust optimizations reduce scoring work:

1. Successful combo updates compute the hundred crossing first. A thousand crossing is evaluated only when a hundred was crossed, skipping two constant-divisor quotients on ordinary rows.
2. Golf's two timing APIs already normalize sign before lookup. The private helper is named `strokes_from_abs_ms` and checks its upper bound directly, removing the redundant lower-bound comparison. Public signed-input behavior is unchanged.
3. Accepted lookup values are at most 103.5 ms, so their integer index is 0–103. Converting through u32 before usize avoids the wider float-to-unsigned conversion on the measured 64-bit target. The final safe array lookup remains bounds-checked.

One combo body and one renamed private golf helper change; two public golf callers only update the helper name. Public signatures, types, fields, score weights, lookup bytes and saturation are unchanged. Production line count adds one explanatory comment, plus four lines registering the tests. There are no new dependencies, heap operations or unsafe blocks. Life, hold and scroll experiments are restored exactly.

## Final measurements

Windows x86-64, Intel64 Family 6 Model 79 Stepping 1, rustc 1.99.1 / LLVM 23.1.1. Release kernels use opt-level 3, fat LTO and the static CRT. Five fresh processes alternate original/current samples on CPU 2 for twelve rounds, discard the first and retain the median of eleven. Each sample performs 10,000 fixtures. Inputs are prepared outside timing; arguments and function pointers are black-boxed. Public state types are shared with the frozen original to preserve the measured ABI.

Cycle columns are independent process medians. Throughput is the median of paired original/current cycle ratios and can differ from the ratio of those columns. Raw TSVs also retain nanoseconds. These are scoring-kernel and small caller measurements, not whole-frame gains.

| Workload / 64 inputs per fixture | Original cycles | Current cycles | Paired throughput |
| --- | ---: | ---: | ---: |
| `combo/large/64` | 1457.17 | 1534.48 | 0.954× |
| `combo/single/64` | 1429.82 | 1319.39 | 1.056× |
| `combo/small/64` | 1468.52 | 1348.13 | 1.089× |
| `combo/zero/64` | 1416.61 | 1343.41 | 1.073× |
| `golf/core_mixed/64` | 610.80 | 526.58 | 1.160× |
| `golf/core_valid/64` | 687.39 | 521.71 | 1.363× |
| `golf/public_ms/64` | 722.59 | 533.67 | 1.323× |
| `golf/record/64` | 896.64 | 717.02 | 1.276× |
| `golf/seconds/64` | 780.06 | 525.68 | 1.440× |

Ordinary combo fixtures improve 1.056×–1.089×, with the zero-increment fixture at 1.073×. The 1,001-increment fixture is a retained tradeoff: 0.954× throughput, or **4.8% more paired CPU**. Its independent median difference is about 1.2 cycles per call. That fixture always crosses a hundred and still needs the thousand calculation; the extra eligibility check helps the ordinary cases instead.

Combined golf changes improve the millisecond API 1.323×, seconds API 1.440×, and complete score-recording caller 1.276×. Original/current score-recording fixtures include both saturating counters.

Identical-source primary-control medians span **0.995×–1.025×**, with individual process pairs 0.924×–1.148×. Small changes near that spread are inconclusive. Unchanged life, hold, combo-reset and scroll fixtures expose placement/sampling variation; no gain is claimed for them.

## Separate checks of the two golf changes

For `bounds_*`, the reference already uses u32 indexing but retains the original two-sided range check. For `index_*`, the reference already uses the upper-only normalized check but retains usize conversion. Each final variant changes only the indicated operation. The seconds cases include the public timing conversion and sign normalization.

| Isolated comparison / 64 inputs | Reference cycles | Final cycles | Paired throughput |
| --- | ---: | ---: | ---: |
| `ablate/bounds_core/64` | 530.86 | 475.28 | 1.119× |
| `ablate/bounds_seconds/64` | 564.51 | 534.55 | 1.053× |
| `ablate/index_core/64` | 632.53 | 486.41 | 1.378× |
| `ablate/index_seconds/64` | 718.51 | 542.38 | 1.368× |

The lower-bound removal therefore remains beneficial after narrowing the index: 1.119× in the helper and 1.053× in the seconds API. Narrowing the index independently gives 1.378× / 1.368×. These four cases run in a separate ignored benchmark and have their own logs. They are not included in the identical-source control, which covers the twenty-four primary fixtures.

All nine changed-path fixtures and all four separate comparisons have zero allocation, reallocation and free events. The complete final benchmark has twenty-seven zero-allocation fixtures and one unchanged error-string control. Its six counters match exactly: `{'allocs': 64, 'reallocs': 0, 'frees': 64, 'allocated_bytes': 4464, 'freed_bytes': 4464, 'peak_bytes': 94}`. No allocation reduction is claimed.

## Exact behavior

Combo is updated with saturating addition, so its new value cannot be below its old value. Every multiple of 1,000 is also a multiple of 100. Crossing a thousand therefore implies crossing a hundred, including large jumps and saturation. If the hundred flag is false, the original thousand flag must also be false. Grade updates, miss-combo clearing and all other state writes remain in their original order.

The only two production callers of the golf helper use `f64::from(ms).abs()` or `offset.abs() * 1000.0`. Reached inputs are nonnegative, positive infinity or NaN. The unchanged upper comparison accepts the same normalized finite values through 103.5 and rejects infinity/NaN; None still scores a miss. The helper's name documents this private input contract. Arbitrary negative input to the old private helper is not its new contract; every public signed-input case remains supported and compared against the original.

For accepted normalized values, truncation to u32 and truncation to usize both produce exactly 0–103. Zero and signed-zero normalization, precision of both timing units, cutoff inclusion and the lookup table are unchanged. The cast also preserves values on supported 32-bit targets, though speed was measured only on x86-64.

## Correctness and provenance

- Five focused tests pass in release and debug. They check 402,916 life transitions with every state/result field compared as bits; 500,048 combo transitions with all fields and milestone flags; 201,907 f64 inputs across helper/seconds variants; 200,000 f32/None inputs and score-counter states; 34,576 scroll strings including Unicode, NaNs, error bytes and capacities; and 2,400,000 generated hold cases plus 56 explicit edge cases. Additional comparisons verify both separate golf-reference variants against the original results.
- Final release trace: **123,038,320 bytes**, SHA-256 `d9ba040ce1e3b3384413e9941b512da7334f8382e58059736b10e8e7529f66d1`. Final debug trace: the same byte count, SHA-256 `1c38377199af1a5fc639957f17ca837da94c8418f0f236d2c93a1343850f1abf`. Every old/current comparison and trace matches within its profile.
- The two profiles differ in twenty serialized words, all NaN payloads in unchanged life fields. This is existing profile-specific floating-point behavior: life source is restored to the baseline, and the frozen/current implementation matches bit for bit within each profile. Combo, golf, scroll and hold trace sections match across profiles. The profile-difference report records offsets and examples.
- The final rules release suite passes **142 tests**, with eleven benchmark/trace tests ignored. A normal release library check passes with source fingerprints for both production files. Focused Clippy passes with warnings denied against the final source context.
- The audit checks twenty-two kernel stages, complete generated compiler context, exact source scope, the frozen originals and receiver/type bridges, separate-comparison transformations, binary/log/trace hashes, every raw timing row and all six allocation counters. All measured sources, including the runner, match `checked-final`. Rust formatting, Python compilation and whitespace checks pass.

Full testing reuses the verified isolated dependency workspace from earlier batches, overlays current source and tests only the rules package. Rules tests use inline values, but the reused preparer also snapshots the comment-cited `tests/fixtures/itgmania-actors/displayed-speed.json` (the metadata preserves the comment's trailing period). Full-driver reproduction needs that existing file through the checkout or `DEADSYNC_NOTEFIELD_FIXTURE_ROOT`; the unit tests themselves do not read it. Compilation uses D: temporary storage; tests retain the host temporary directory. The main checkout is read only.

## Discarded variants and reproduction

Guarded and complementary life-lock arithmetic showed no useful gain. A remainder-based milestone formula was slower than the quotient guard. Integer golf range classification slowed a valid-helper fixture to 0.878× and did not improve its callers, so it was restored. Unconditional miss-combo addition slowed zero/mixed resets; both zero-delta branching and the rest of that function are restored. Byte and character-pattern scroll dispatch showed no consistent benefit and was restored. Moving the hold clamp past its early return also showed no benefit and was restored.

The corpus grew while exploring these candidates; each stage retains its own source snapshot and trace hash. The first completed final run found two unused test imports under Clippy. They were removed, followed by fresh `checked-final`, `checked-control`, `lint-debug` and full-package validation. Earlier runs remain immutable and are not the reported final timings.

```powershell
$env:DEADSYNC_SCORING_STATE_ARCHIVE = 'D:\deadsync-scoring-state-repro'
python scripts/benchmark_scoring_state.py control --baseline aab4306bef34ec7c27c05fa4cbba9af050d9a5bb --reference control --runs 5
python scripts/benchmark_scoring_state.py final --reference control --runs 5
python scripts/benchmark_scoring_state.py baseline-debug --baseline aab4306bef34ec7c27c05fa4cbba9af050d9a5bb --debug
python scripts/benchmark_scoring_state.py checked-debug --reference baseline-debug --debug
python scripts/benchmark_scoring_state.py full-release --full
```

Snapshots, binaries, raw run/ablation logs and traces are retained under `D:\deadsync-scoring-state-20261104`. Checked-in manifests, TSVs and checks JSON identify their hashes and sources.
