# BMP decoder optimization pass 2 — no production changes retained

This pass keeps the original BMP production implementation. The measured format
specializations made their target inputs faster, but the final reduced candidate
still regressed unchanged formats. Those regressions failed the selection gate.
All production files were restored to the exact frozen baseline; three useful
regression tests and the complete experiment evidence remain.

No application loading code or dependencies were changed. These results do not
establish an application stutter improvement. Nothing from this pass was staged
or committed.

## Final decision and what remains

The pass began at `945da378a907bb7c4fec1f432d02d320c9d2a462`. The original
production source is the final source, so **zero BMP production optimizations**
are retained from this pass. The retained tests independently exercise every
RGBA4444 word and mask permutations, explicit RGBA byte-order source boundaries,
and implicit-alpha handling around header gaps and trailing bytes.

The guarded restoration checked all 22 frozen candidate source/configuration
hashes before writing only `src/decode.rs`. All four production files then
matched baseline exactly. The decoder SHA-256 is
`84591b2faaf4455f788ad7b475693fa93d7b4aa2793dda43b901cd66a463454e`.
The only intentional code difference is the additional regression coverage.
See [the restoration proof](evidence/restored-baseline-source.json).

The final restored source passes 37 crate integration tests, one independent
alpha-control test, one doctest, formatting, and strict all-target codec Clippy.
All three final commands returned zero, with direct native stdout/stderr capture.
All 22 source/configuration identities match before and after that gate. See
[the final validation record](evidence/final-restored-validation-summary.json).

## Measured target gains in the rejected reduced candidate

Candidate 5 contains only exact RGBA4444 nibble extraction and direct copying of
explicit RGBA-byte-order input after the existing exact reserve. It restores the
original post-conversion implicit-alpha scan and repair. A source proof shows
that the two specializations are its only production differences from baseline.

The following are measurements of that **rejected candidate**, not retained
performance improvements. Each target has eight cases, covering four sizes and
both orientations, and 4,410,162 pixels in total.

| Target | Operation | Sum of median cycles before | Sum after | MPix/s before | MPix/s after | Geometric after/before | Cases faster |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| RGBA4444 | Decode | 33,108,782.50 | 25,565,149.37 | 290.82 | 376.20 | 0.807019 | 6/8 |
| RGBA4444 | Consuming RGBA | 34,061,026.31 | 25,518,030.82 | 283.25 | 377.37 | 0.751982 | 8/8 |
| RGBA4444 | Borrowed RGBA | 46,335,372.11 | 37,771,016.51 | 208.23 | 255.55 | 0.815898 | 7/8 |
| Direct RGBA | Decode | 15,454,557.01 | 13,860,266.80 | 624.17 | 696.66 | 0.773703 | 8/8 |
| Direct RGBA | Consuming RGBA | 15,624,566.08 | 14,302,892.40 | 618.03 | 674.14 | 0.768916 | 8/8 |
| Direct RGBA | Borrowed RGBA | 28,194,139.40 | 27,656,301.73 | 342.10 | 348.40 | 0.846333 | 8/8 |

Cycle sums add one per-input median for each case. MPix/s is total pixels divided
by the sum of per-input median elapsed times. Geometric ratios weight every case
equally. Consequently, the direct-RGBA consuming geometric reduction of 23.1%
corresponds to an 8.5% reduction in summed cycles; they are different
aggregations, not contradictory measurements. Neither estimates an application's
input mix.

The unchanged image target controls have consuming geometric ratios 0.984883
for RGBA4444 and 0.977650 for direct RGBA. Both 1x1 RGBA4444 decode cases regress
(9.18% and 2.96%); the bottom-up borrowed 1x1 case regresses 0.62%, with
overlapping sampled ranges. Those losses are retained in the
[full per-case comparison](evidence/candidate5-baseline-matrix-comparison.md).
The [target table](evidence/candidate5-target-table.json) includes exact values.

## Why the reduced source was rejected

The unchanged RGB555, RGB565, and RGB333 decode groups have geometric ratios
1.065486, 1.065651, and 1.087146: roughly 6–9% more cycles, with all eight cases
slower in each group. Unchanged decode controls collectively have ratio
1.046324. The consuming pipeline reduces some earlier collateral losses but
does not clear the remaining control regressions.

| Candidate 5 comparison | Native geometric after/before | Sum-of-cycles ratio | Unchanged native control ratio | Unchanged image ratio, all exact cases |
| --- | ---: | ---: | ---: | ---: |
| Decode | 1.007878 | 0.994943 | 1.046324 | Not applicable |
| Consuming RGBA | 0.973141 | 0.981750 | 1.010783 | 0.991458 |
| Borrowed RGBA | 0.988248 | 0.978405 | 1.014956 | 0.988447 |
| Repository consuming | 0.982861 | 0.986780 | 0.982861 | 1.010022 |

For the 82 unchanged inputs with exact reference counterparts, consuming native
and image changes are 1.008548 and 0.993460; their relative ratio is 1.015187.
Borrowed changes are 1.015540 and 0.990052, relative ratio 1.025744. The larger
104-case native-control set also includes documented native/image policy
differences. Reference drift is reported rather than subtracted from every
measurement as if it completely explained the results.

Specific remaining consuming controls include indexed8 at 1.060209 versus image
1.014612 and BGRX32 at 1.035943 versus 1.013849. The complete report preserves
all 94 decode, 76 consuming, 64 borrowed, and three repository native losses.
Faster target groups and continued wins over image do not justify overlooking
these regressions.

The alpha corpus remains an unchanged-path control after the rebase. All-zero
consuming cases have geometric ratio 1.019483 versus image 0.991254. The
511x257 first-alpha cases again exhibit opposite process regimes: bottom-up
decode/consuming ratios 0.277768/3.729028, top-down 2.086987/0.524566. Earlier
isolated ABBA/BAAB runs did not reproduce some large medium-size parent/candidate
effects. That demonstrates context sensitivity without identifying its cause;
it does not authorize deleting losses. All alpha values and sample ranges are
in the [alpha comparison](evidence/candidate5-baseline-alpha-comparison.md).

## Reference comparisons for the restored original implementation

The original frozen executable was run again immediately before/after
candidate 5 in the final comparison. Because the final production and harness
inputs are restored to those original bytes, the baseline side is the applicable
measurement of the retained implementation; no new performance claim is made
from restoring it.

| Original implementation, final paired baseline | Exact comparisons won | Geometric image/native cycles |
| --- | ---: | ---: |
| Consuming RGBA, synthetic | 98/98 | 2.407360 |
| Borrowed RGBA, synthetic | 97/98 | 2.137505 |
| Consuming RGBA, repository | 12/12 | 2.624307 |

The final paired baseline's borrowed loss is `indexed1-511x257-bottom`:
1,061,771.75 native versus 719,918.89 image cycles. The initial baseline run had
a different borrowed exception, `indexed1-511x257-top`, at 833,864.80 versus
794,934.95 cycles. Both runs and both exceptions remain available.

The rejected candidate still wins 98/98 consuming and 12/12 repository cases.
It wins 96/98 borrowed cases, losing both medium indexed1 orientations:
bottom 1,068,169.29 native / 704,924.23 image cycles; top 1,045,752.85 /
724,915.92. The top-down native/reference changes oppose one another
(2.024809 / 0.660664), another retained context-specific result.

## Experiment decisions

| Stage | Outcome |
| --- | --- |
| Encoded-order source alpha scan | Rejected. It changes short-circuit position for bottom-up input; lone-first-alpha decode regressed 25.97%, 94.86%, and 39.50% across the three control sizes. |
| Logical-order source alpha scan | Correctness passed and all-zero targets improved, but it remained conditional. Removed when the combined source failed the wider control gate. |
| RGBA4444 extraction | Meaningful focused gains, with tiny decode losses and collateral-control concerns. Retested in the reduced rebase; not retained. |
| Direct RGBA copy | All target cases improved in its focused screen. Broader control losses required isolated diagnostics and the reduced rebase; not retained. |
| Alpha-dispatch relocation | Correctness passed. It improved RGB555 decode but did not establish broad control recovery; rejected as a remedy. |
| Two-layout rebase | Strong target gains but persistent unchanged-format losses; rejected. Original production restored. |

Every parent/source/executable identity, adverse result, sample range, and
isolated repeat is retained in the [experiment ledger](evidence/EXPERIMENTS.md).
No additional candidate or optional timing followed the final rejection.

## Method, corpus, and validation boundaries

The host was an Intel Xeon E5-2696 v4, 22 cores/44 logical processors, Windows
10.0.26100, Rust 1.100.0 (0ef1bc386; LLVM 23.1.5), MSVC, with the repository's
static CRT configuration. Timing is pinned to logical CPU 2 and uses
`QueryThreadCycleTime`, five median batches, three untimed warmups, rotating
modes, output destruction, and no timed input generation or file I/O. Cycle and
elapsed-time medians are selected independently. Final full matrices use 20 ms
per batch; final alpha controls use 80 ms. Focus screens used 40 or 80 ms, as
recorded in each sidecar.

The final full and alpha runs have 115 and 30 clear strict-watcher checks.
All twelve commands returned zero. Earlier phases used recorded boundary checks
or the older watcher; their weaker process coverage is not presented as the
final watcher's coverage. No compiler, tests, or other codec benchmark ran in
these serialized timing leases. A process-name watcher cannot establish absence
of every possible background activity.

The synthetic matrix has 120 native cases and 98 exact image comparisons against
`image 0.25.10` with only its BMP feature. Twenty-two native/reference policy
differences remain explicit: eight RGB555, six RGB565, and eight mixed
implicit-BGRA cases. Native decode-only may retain indexed or RGB storage, so it
is compared only with native before/after decode. Consuming and borrowed RGBA
both produce owned RGBA results and are reported separately.

The repository corpus contains twelve valid files and one intentionally invalid
unused-palette-entry file. These valid fixtures total only 700 pixels; they do
not represent a realistic large-artwork distribution. The alpha corpus adds
18 generated files, six exact reference matches and twelve mixed-alpha policy
differences. Independent generated expected pixels and the frozen native
behavior snapshot establish native preservation; external-file native output
alone is not an independent native oracle.

All candidates pass the exact 22,570-record snapshot covering outputs, metadata,
errors, limits, and encoder serialization. Its 42,819,895 bytes have SHA-256
`076e78cae341e782b60f72fa3f60c0ff85142759e4821069f1012063f37bf0df`.
Every candidate also preserves all 622 paired allocation rows. These include
reference rows and are not 622 independent native inputs. Counters run in a
separate executable with timing compiled out and measure requested allocation
bytes, not RSS or peak live memory. No candidate introduced unsafe code,
a production dependency, or a public API.

Candidate 5 passed 37 crate integration tests, one independent alpha-control
test, one doctest, formatting, strict all-target codec Clippy, and strict
all-target/all-feature benchmark Clippy. Its 19 commands have direct durable
stdout/stderr capture and checked native exit codes. Earlier PowerShell
transcripts omitted native Cargo output; the ledger records that limitation.
The final restored-source gate is captured directly as well. No new benchmark,
snapshot build, allocation run, or timing was needed after exact restoration.
An independent analysis script also recomputed all 622 final before/after TSV
record pairs, with 7,221 numeric checks covering medians, ranges, geometric
ratios, sums, throughput, and cohort counts; see
[the summary audit](evidence/candidate5-summary-audit.json).

## Evidence and reproduction

See [REPRODUCE.md](REPRODUCE.md), the [original source identity](evidence/baseline/identity.json),
the [candidate 5 complete matrix](evidence/candidate5-baseline-matrix-summary.json),
and [manifest.json](manifest.json). The compact archive contains raw TSVs,
commands, logs, source patches, hashes, validation records, and every rejected
experiment. Executables, PDBs, the large snapshot, generated BMP payloads, build
caches, and full disassembly are excluded; their identities and reconstruction
instructions remain.
