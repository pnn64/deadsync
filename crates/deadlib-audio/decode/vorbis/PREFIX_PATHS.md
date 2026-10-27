# Bounded Huffman prefix path

The real-packet [codebook traces](CODEBOOK_PROFILE.md) place 87–92% of scalar
calls inside VQ decoding on the two songs, with 84–86% of calls resolving a
prefix-table leaf. The 2026-10-27 update makes that bounded path available for
inlining: `Huffman::decode` and `Codebook::decode_vector` use `inline(always)`,
while the tree continuation and bit-cache refill use `inline(never)`.

The tree loop moved into a private helper without changing its operations.
Refill still uses the existing eight-byte cache load and bounded tail loop.
VQ decoding retains its public vector validation. Table widths, setup memory
budgets, bit consumption, errors, float operations and public APIs are unchanged.
This is a measured combination of call boundaries; the results do not isolate
the contribution of each attribute.

## Paired pipeline measurements

Baseline `995ddca57` and current production sources used identical benchmark
adapters, Cargo artifact profiles and 17 registry package identities. Seven
shuffled trials cover five inputs, four engines and full/fragmented/startup
phases: 420 timing rows. The baseline source archive and all three measured
executables were hashed and saved in ignored storage before later builds.
The machine was shared and unlocked. These are warm process measurements,
including setup and final trimming, rather than cold OS-cache startup.

| Input / phase | Baseline median, ms | Current median, ms | Baseline / current | Current range, ms |
| --- | ---: | ---: | ---: | ---: |
| `in_two`, full | 312.983 | 298.919 | 1.047x | 281.554–301.410 |
| `Goin' Under`, full | 261.823 | 248.939 | 1.052x | 243.137–257.016 |
| `in_two`, 4,096-byte reads | 295.735 | 294.318 | 1.005x | 270.901–308.581 |
| `Goin' Under`, 4,096-byte reads | 265.356 | 257.096 | 1.032x | 245.343–287.099 |
| `in_two`, first PCM | 0.736 | 0.763 | 0.965x | 0.742–0.785 |
| `Goin' Under`, first PCM | 1.099 | 1.056 | 1.041x | 1.034–1.184 |

Native full-decode medians on mono/stereo/eight-channel tone fixtures move
0.655→0.638, 1.621→1.632 and 1.412→1.390 ms. Fragmented mono/stereo medians
regress 0.595→0.632 and 1.327→1.367 ms. Startup and small-file movements are
mixed. The largest native/baseline relative median absolute deviation is 6.12%.
All trial ranges and reference results are retained; this is not a universal
throughput or latency improvement.
Within-trial baseline/current ratio medians are 1.071x/1.075x for the songs'
full decode and 1.026x/1.042x for fragmented reads. Individual fragmented
trials include regressions; the table above consistently uses ratios of the
separate timing medians. [All paired ratios](validation/2026-10-27-split-prefix-paired-ratios.json)
remain available alongside the raw times.

| Full decode | Current native, ms | Symphonia 0.6.1, ms | libvorbis 1.3.7, ms |
| --- | ---: | ---: | ---: |
| `in_two` | 298.919 | 213.868 | 344.401 |
| `Goin' Under` | 248.939 | 171.313 | 225.630 |

Native throughput still trails Symphonia on both songs. libvorbis uses the
separate C worker; compiler/runtime and adapter differences remain documented
in [PERFORMANCE.md](PERFORMANCE.md).

The exact measured Rust pipeline executable's PE `.text` virtual size falls
from 761,968 to 748,848 bytes, a 13,120-byte reduction. This executable includes
the harness and Symphonia: the difference is not an isolated library size.
A separate matching-profile assembly emission shows four out-of-line VQ calls
and two direct Huffman calls inside the baseline packet decoder. Forcing only
VQ inlining removes the VQ calls but leaves nine Huffman calls. The retained
combination removes both public call boundaries; the packet decoder contains
seven private tree-helper and 21 refill call sites. These are static call sites,
not execution counts, and this assembly is not disassembly of the timed binary.

## Resources, seeking and correctness

The five-file resource run retains 480 rows and 280 shuffled indexed-open/seek
timing rows, plus 500 independently checked libvorbisfile PCM windows. All nine
paired native counters match the baseline in every group: allocation operations,
requested bytes, peak additional/retained heap, read calls/bytes, seeks, tells
and API-exposed sample counts. Warmed native seek batches still allocate zero
bytes. Process-memory measurements remain descriptive shared-machine data.

Each seek batch has 100 fixed requests. Song medians move 13.713→12.730 ms and
11.159→11.099 ms (1.077x / 1.005x). Their current ranges are 12.654–13.117 and
10.680–11.284 ms. Indexed-open medians move 7.087→7.095 and 5.834→5.757 ms.
The eager complete-file index remains; this change does not implement sparse
or index-free codec seeking. See [RESOURCES.md](RESOURCES.md) for the API and
workload differences between native and reference seeking.

Post-change codebook instrumentation checks 42,738,816 raw channel-sample bit
patterns across eight cases against production. PCM digests, packet geometry,
per-book/per-entry frequencies, codeword lengths and prefix/fallback counters
also match the prior trace byte for byte. The cases include both songs,
FFmpeg's legacy `coyote` and six-channel `test6`, and the original floor-zero
sample under its explicit zero-span-granule compatibility policy. Instrumented
time and memory are not used as production measurements.

The full release gate passes 119 tests with all external/reference tests enabled;
offline extracted crates pass 92 with 28 external tests ignored. Formatting,
all-target Clippy with default/all features, generated-profile release Clippy,
two fuzz-oracle unit tests and fuzz Clippy pass. The existing conformance gate
rechecks 38 repository assets, 528 upstream coded samples, the 13-entry external
manifest, 144 native encoding cases and historical/legacy reports. Numerical
tolerances and strict/compatibility policies remain unchanged.

The residue scatter unit test now spans 1,620 constructed combinations of
dimensions, channel bundles, starts, partition sizes and truncated bit input.
It checks all plane/sentinel values, errors and consumed positions against a
direct interleaving oracle. The production residue implementation is unchanged.

Three five-minute AddressSanitizer campaigns (headers, packets and encoding)
complete 109,712 executions. The initial streaming campaign stops after 31
executions on its original `external-2-255` seed at the 20-second input limit.
The retained 52,302-byte artifact has SHA-256
`4e9f90f7dbe6a58f233a64e33a0f2de06a3414a42c499075b6a32c79451e1e6d`.
Fixed-input replay succeeds on both the baseline and changed decoder under a
120-second limit, taking 43.6 and 47.3 seconds respectively in those runs.
The separate streaming recheck preserves all 44 initial seeds and uses that
larger per-input budget. Its results and any slow-unit diagnostics are retained
alongside the original failed campaign. It completes 292 executions in 305
seconds with exit zero and 395 MiB reported peak RSS; slow-unit reports are
diagnostics rather than failures. These smoke runs do not establish
complete fuzz coverage or an execution-time guarantee.

## Rejected experiments

Three earlier candidates each received the same 420-row pipeline comparison
against `995ddca57`. Their patches, source snapshots, manifests, binaries and
trial distributions are retained. Their runtime changes are absent from
production, except that VQ inlining participates in the final combined policy.
Full conformance and sanitizer gates apply to the retained combination, not
retroactively to these rejected candidates.

| Candidate | `in_two` full, before→after ms | `Goin' Under` full, before→after ms | Decision |
| --- | ---: | ---: | --- |
| Direct VQ slice without repeated public validation | 276.343→281.173 | 287.567→281.019 | Inconsistent full/fragmented benefit; retain checks |
| Pair-specialized stereo residue scatter | 313.469→313.676 | 327.698→330.116 | No consistent benefit; retain generic cursor |
| Forced VQ inlining alone | 279.207→280.506 | 288.910→289.778 | Moves the call boundary; no consistent benefit alone |

The different absolute baseline times show run-to-run machine drift. Compare
each candidate with its own interleaved baseline, not with other experiments'
raw times. The residue geometry test remains useful independently of the
rejected pair specialization.
The direct-VQ patch uses zero context (`git apply --unidiff-zero`); its applied
source hashes match the original patch, as recorded in the normalization proof.

## Evidence and reproduction

```powershell
python crates/deadlib-audio/decode/scripts/benchmark.py --baseline-revision 995ddca57 --repeats 7
python crates/deadlib-audio/decode/scripts/benchmark_resources.py --baseline-revision 995ddca57 --repeats 7
python crates/deadlib-audio/decode/scripts/profile_codebooks.py
```

[Checks and provenance](validation/2026-10-27-split-prefix-checks.json),
[pipeline manifest](validation/2026-10-27-split-prefix-pipeline.json),
[trials](validation/2026-10-27-split-prefix-pipeline-trials.tsv),
[summary](validation/2026-10-27-split-prefix-pipeline-summary.tsv),
[resources](validation/2026-10-27-split-prefix-resources.tsv),
[seek summary](validation/2026-10-27-split-prefix-seek-summary.tsv),
[post-change trace](validation/2026-10-27-split-prefix-profile-main-manifest.json)
and [assembly inspection](validation/2026-10-27-split-prefix-asm-inspection.json)
retain hashes, raw trials and validation commands. Unchanged frequency tables
remain in the preceding [codebook diagnostic](CODEBOOK_PROFILE.md#evidence).
All rejected candidates and the initial fuzz timeout are included in the checks'
artifact map. Executables and downloaded audio stay in ignored storage.

Broader official/real-song conformance coverage, competitive full decoding,
cold startup, sparse codec seeking and encoder perceptual quality/rate control
remain open. Neither crate is integrated into DeadSync's application pipeline.
