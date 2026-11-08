# Microsoft ADPCM arithmetic, 2026-10-28

The measured i686 Microsoft ADPCM bottleneck was checked 64-bit arithmetic in
the per-sample loop. The retained implementation uses a bounded i32 loop on
32-bit targets, followed by the original checked i64 loop when necessary.
64-bit targets retain the wide loop; the initial all-target experiment did not
show a useful x64 Microsoft decoding gain. No allocation, lookup table, unsafe
code, production dependency or public representation is added.

## Arithmetic bounds and observable behavior

Header histories and coefficients are signed i16, and subsequent histories are
clamped to i16. Except for the coefficient pair (-32768, -32768), the sum of
absolute coefficients is at most 65535. Therefore the absolute dot product is
at most 32768 * 65535 = 2147450880, which fits i32. The exceptional pair starts
in the wide loop. Each individual product also fits i32.

The narrow loop runs only while the nonnegative delta is at most
floor(i32::MAX / 768) = 2796202. The largest adaptation multiplier is 768, so
its product is at most 2147483136. The absolute code correction is at most
8 * 2796202 = 22369616; the absolute prediction after division is at most
8388480. Their sum is less than 31 million. Every narrow multiplication,
addition and adaptation thus fits signed i32 before clamping. All divisions
retain Rust's truncation toward zero, including negative predictions.

After delta exceeds the bound, the remainder of that channel uses checked i64
arithmetic. There is no re-entry into the narrow loop. Negative initial delta,
invalid predictor selection, overflowing adaptation and the caller's partially
written output retain their prior behavior. In particular, a sample is written
only after its adaptation succeeds; the failing sample remains untouched.

The independent i128 test oracle in `tests/adpcm.rs` has no narrow threshold or
fallback. It checks 2304 complete mono/stereo blocks across both byte orders,
nine coefficient pairs (including signed extremes), four initial deltas, four
history pairs and four code patterns. These cover bounded codes, random codes,
delta growth followed by decay, and growth through checked overflow. Comparisons
include the result/error text, all output words, the error prefix and sentinel
suffix. The existing negative-division regression remains separate.

## Paired resident measurements

`validation/ms-narrow-final-paired-{i686,x64}.*` compares retained source with
commit `25f0ec72e`. Both standalone source copies use identical explicit opt-3,
LTO, debug-1 bench profiles, the workspace's static MSVC runtime configuration,
the same benchmark and caller-owned buffers. Baseline/current/baseline/current
processes each run three warmed trials. The benchmark thread is pinned to the
highest available affinity bit (31 on i686, 43 on x64); frequency and other
processes are uncontrolled. Input is low-entropy flat resident Cursor data.
These are six-trial medians, not confidence intervals or cold-file results.

| i686 operation | Before | Retained | Before / retained |
| --- | ---: | ---: | ---: |
| Microsoft mono full | 19.769 ms | 6.412 ms | 3.083 |
| Microsoft stereo full | 19.287 ms | 6.210 ms | 3.106 |
| Microsoft mono stream, 127 samples | 20.245 ms | 6.968 ms | 2.905 |
| Microsoft stereo stream, 127 samples | 19.451 ms | 6.375 ms | 3.051 |
| Microsoft mono 62-sample seek/read | 9.790 us | 3.145 us | 3.113 |
| Microsoft stereo 62-sample seek/read | 9.468 us | 3.082 us | 3.072 |

Opening has no arithmetic change: before/retained ratios are 0.983 and 1.027.
IMA4 control full/stream ratios are 0.992/0.988; IMA3 controls are 1.000/1.035.
The retained x64 Microsoft full ratios are 0.972/1.008 (mono/stereo), and stream
ratios are 0.967/1.007; no x64 speedup is claimed. All raw ranges, CPU-cycle
trials, control timings, source archives and executable hashes are retained.

`ms-narrow-summary.json` verifies the paired stdout hashes and records every
trial, median and range. Its generator is retained as `ms-narrow-summary.py.txt`.
The unqualified `ms-narrow-paired-*` files describe the rejected all-target
candidate. `ms-narrow-32-paired-*` describes an intermediate 32-bit-only variant
that initialized wide state before narrowing. `ms-narrow-final-*` identifies
the retained variant, which reads narrow header state directly on 32-bit targets.
These experiments are preserved without treating their timings as final results.

For an extracted paired source ZIP outside this repository, copy the
`context/worktree/.cargo/config.toml` from the final full reference source ZIP
to `.cargo/config.toml` at the extracted project root. The paired report records
that configuration's SHA-256; within this repository it is already inherited.

## Random-code reference comparison and controls

`ms-narrow-paired-reference-i686.*` alternates the previously source-pinned
`adpcm-cursor-i686` and final reference binaries against identical fixture bytes.
Both use the same opt-3/LTO/debug-1 profile, reference source, i16 output contract
and affinity bit 31. Executable hashes are checked before and after; all 496
published files of the 21 locked reference dependencies are verified before and
after. The retained driver is `ms-narrow-paired-reference-i686.py.txt`.

| Random Microsoft stereo operation | Before | Retained | Before / retained |
| --- | ---: | ---: | ---: |
| Native complete read | 19.487 ms | 7.367 ms | 2.645 |
| Native 4096-sample stream | 18.715 ms | 7.146 ms | 2.619 |
| Native 128 exact seek/read ranges | 2.404 ms | 0.913 ms | 2.632 |
| Symphonia complete control | 6.421 ms | 5.887 ms | 1.091 |
| Symphonia streaming control | 6.272 ms | 6.067 ms | 1.034 |

The improvement is not a universal three-times gain, and the retained native
complete operation remains about 25% slower than Symphonia in this paired case.
Native sign-only IMA control complete/stream medians move from 7.481/7.429 ms to
7.857/7.863 ms; its 128-range median moves from 0.928 to 1.034 ms. No IMA
algorithm changes here. These roughly 5.0%/5.8%/11.4% time increases are retained
as an open performance issue; their cause is not established by these timings.
The corresponding Symphonia full/stream controls are 6.391/6.076 ms before and
6.290/6.114 ms after. No IMA improvement is claimed.

The independent full final-source x64 campaign passes 89 resident cases, 48
fresh-process memory probes and 32 cached-file runs. The final i686 subset
passes 10/24/16. All fixture hashes and exact/error/mismatch gate outcomes match
the preceding campaigns; all 105/18 distinct files pass complete hashes and
dense/nonsparse/noncompressed storage checks. Source/fixture/raw archives have
verified entry hashes. The full matrix timings are separate unpaired observations;
the alternating run above is the arithmetic comparison. Unknown frequency,
other processes and code generation effects limit causal timing claims.

## Final validation

`ms-narrow-checks.*` records all 95 tests passing x64 debug/release and i686
debug, denied-warning Clippy on both architectures and the native benchmark's
Cargo test entry. The i128 oracle runs on both architectures; the narrow branch
is exercised on i686. `ms-narrow-wavl.*` passes 216 compressed fixture gates,
3024 borrowed/seekable/forward/range checks and ten independent flat-component
reference gates. `ms-narrow-native.*` preserves all 708 native gates, 4484
descriptor/read/range checks and 1912 exact writer outputs.

The two checked mutation campaigns each pass one million iterations, including
the narrow branch on i686. All 1846 prior archived corpus inputs replay. The
x64 ASan campaign passes 100000 executions without a failure artifact; it
exercises the wide branch, not the i686-only narrow branch. Its verified final
1868-file corpus has SHA-256
`a90cc0190ebd586c7bff85ee0b108a42bfee4aed2370b469c2255f1142ff0512`.
FUZZING.md records campaign scopes. All reports preserve unchanged-input checks;
production source bytes match the final measured source archives.
