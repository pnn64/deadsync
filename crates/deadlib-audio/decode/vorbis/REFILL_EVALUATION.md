# Bit-reader refill experiments

Two safe eight-byte refill alternatives were rejected after paired full-pipeline
measurements. The production reader remains identical to `85f0f9849`. This
milestone retains independent wire-bit tests and a fuzz oracle, plus the
unsuccessful experiment sources and measurements. It makes no throughput claim.

The existing [component diagnostic](SYNTHESIS_PROFILE.md) and
[residue follow-up](RESIDUE_PERFORMANCE.md) identify residue as a major song
cost. Huffman codewords use the shared bit reader, whose baseline refill appends
bytes individually unless the cache is empty. That motivated these experiments;
the stage profile does not establish how much time refill itself consumes.

## Rejected alternatives

Both patches affect only `vorbis/src/bitpack.rs`, preserve the public API and
reader fields, and use safe Rust slice/array operations. Their source snapshots
and measured executable hashes are retained with each run.

* **Masked append:** load eight available input bytes, mask to the complete
  bytes that fit the cache, shift/OR them above the retained bits, and advance
  only the consumed input bytes. Short tails keep the scalar path.
* **Reload current window:** derive the byte containing the current bit
  position, load eight bytes there, and shift away the already consumed low
  bits. This reloads any retained input instead of merging it. When a complete
  window is unavailable, keep the scalar tail path.

Both pass the native unit/primitive checks in isolated default-debug workspaces,
including mixed speculative lookahead, cache carries and packet tails. The
pipeline workers also require matching decoded sample counts. These bounded
checks do not establish exhaustive correctness of either rejected alternative.
Only the unchanged production reader receives the complete validation below.

## Paired measurements, Windows x86-64, 2026-10-27

Each experiment uses `benchmark.py --baseline-revision 85f0f9849 --repeats 7`
with the usual five files, 150 ms calibration target, resident inputs, fresh
decoder contexts and three full/4 KiB/first-PCM phases. The shuffled workers are
native candidate, native baseline, Symphonia 0.6.1 and pinned libvorbisfile 1.3.7.
Release/bench profiles and registry artifact IDs match within each comparison;
no baseline adapter adjustment is required. There are **420 trials per
experiment**, **840 total**.

Median milliseconds, baseline to candidate:

| Experiment / input | Full decode | 4 KiB reads | First PCM |
| --- | ---: | ---: | ---: |
| Masked append / `in_two` | 307.137 -> 333.826 | 344.788 -> 333.492 | 0.867 -> 0.893 |
| Masked append / `Goin' Under` | 292.268 -> 309.908 | 270.576 -> 283.504 | 1.062 -> 1.111 |
| Reload window / `in_two` | 346.213 -> 352.615 | 346.346 -> 352.728 | 0.915 -> 0.938 |
| Reload window / `Goin' Under` | 327.412 -> 328.520 | 324.772 -> 324.413 | 1.264 -> 1.265 |

Masked-append song full-decode medians regress 8.7% / 6.0%. Its native full-file
ranges overlap the baseline: 305.961-347.169 vs 292.515-329.106 ms for `in_two`,
and 259.655-344.214 vs 268.279-327.025 ms for Goin' Under. Reload-window song
medians regress 1.8% / 0.3%; its Goin' Under fragmented result is effectively
unchanged. Small fixtures are mixed; reload-window stereo full decode rises
1.245 -> 1.322 ms and first PCM rises 1.152 -> 1.229 ms.

Maximum relative MAD across all groups is 7.49% for masked append and 5.98% for
reload window. These are shared, unlocked desktop measurements. Baseline and
reference timings move between the two runs, so compare each candidate only
with its own interleaved baseline. First PCM is warm-process decoder startup,
not a cold process or storage benchmark. Neither alternative demonstrates a
consistent pipeline improvement, so neither is shipped. Allocation, memory,
seek and encoder performance comparisons were not pursued for these rejected
implementations.

## Retained wire-bit validation

The new primitive test checks 81 packet lengths and 32 mixed-width schedules:
**2,592 streams**. It derives values directly from wire-bit indices, independently
of the cache and writer, and checks speculative lookahead, invalid lookahead
width, exact consumed/remaining bits, sticky exhaustion and zero-width behavior.
It complements the existing all-width/alignment, deep Huffman and tail checks.

The `vorbis_headers` fuzz target now exercises every bounded input through an
independent bit-index oracle before header parsing, even when the bytes cannot
form a valid header. Up to 128 mixed operations check values, lookahead and read
errors, invalid width 33, zero-width reads, positions, remaining bits and sticky
end-of-packet state. A separate fuzz-library test exercises empty inputs and
all prefixes through 128 bytes.

The complete native/reference suite (119 passed), extracted offline workspace
(92 passed), formatting, default/all-feature Clippy and two fuzz-library tests
are retained in
[checks](validation/2026-10-27-refill-checks.json). The fresh 300-second
AddressSanitizer `vorbis_headers` campaign includes the prior floor-zero fixture
and the newly pinned coyote/aoTuV seed files: **142,674 executions**, no failure
or artifact. Source/input/binary hashes are checked before retaining the evidence.
No production decoder or application integration changes are included.

[Masked-append manifest](validation/2026-10-27-refill-masked-pipeline.json),
[trials](validation/2026-10-27-refill-masked-trials.tsv),
[summary](validation/2026-10-27-refill-masked-summary.tsv),
[patch](validation/2026-10-27-refill-masked.patch), and
[reload-window manifest](validation/2026-10-27-refill-window-pipeline.json),
[trials](validation/2026-10-27-refill-window-trials.tsv),
[summary](validation/2026-10-27-refill-window-summary.tsv),
[patch](validation/2026-10-27-refill-window.patch) preserve the rejected work.
The measurements do not isolate instruction costs or prove that refill is
inexpensive. Real codebook-frequency and VQ-return costs remain useful profiling
targets; the broader conformance/performance/encoding goal remains open.
