# Vorbis decoding completion audit

The requested decoding work is complete. All final gates below pass. Performance
is excluded from this audit. The implementation uses native Rust; Vorbis's only
production dependency is the sibling native Ogg crate. Application playback
integration is a separate task.

## Scope and evidence

| Parent work-plan item | Completion evidence |
| --- | --- |
| Ogg framing, packet transport and container seeking | Independent libogg page/packet/flag/granule comparisons, CRC and sequence rejection, arbitrary IO boundaries, recovery, continued packets, chains and multiplexing. The additional 32-song pack checks 39,616 pages and 465,274 packets. |
| Complete Vorbis I decoding stages | Both floors, all residue types, codebooks/Huffman/VQ, submaps, reverse coupling, all legal transforms/windows and overlap. Direct C component tests and procedural full-packet matrices cover mode counts 1..64, channel counts through 255, degenerate books, truncation and recovery. |
| Mapping, timing and lifecycle | Exact counts/positions, initial/final trim, broadcast offsets, pull/push/reset, independent logical streams, chains, packet/page reads and repeated seeks. Default seeks return sequential PCM suffixes; explicit reference seeks reproduce C scheduling, including all 172 formerly recorded differences. |
| Independent accuracy and difficult inputs | Pinned libogg 1.3.6, libvorbis/libvorbisfile 1.3.7 and Symphonia 0.6.1; all 38 repository assets, 528 upstream coded vectors, historical floor-zero/aoTuV/legacy media, two shipped ITGmania songs and 32 additional chart-linked StepMania songs. Full differential matrices and bounded sanitizer campaigns supplement these media. |
| Existing native encoder validation | The 144-case independent decoder matrix, exact encoded lengths, reset/chunk determinism and lossy reconstruction checks still pass. Competitive encoding quality/rate control is separate from decoding completion. |
| Performance | Existing measurements are preserved. No speed threshold, new performance experiment or optimization is required by the current request. |
| Independent packages and extracted workspace | Package tests, formatting, default/all-feature lint, fuzz-harness tests/lint, extracted offline package tests and dependency trees, and offline compilation of all extracted benchmark targets. |

The [coverage ledger](COVERAGE.md) retains exact numerical criteria. Floor-one
PCM uses peak/RMS bounds `3e-6` / `5e-7` times `max(1, reference peak)`;
floor-zero uses `5e-5` / `3e-6`. Counts, positions and channel identity are exact.
No frame shifting, resampling, gain fitting or tolerance widening is used.

The added [song-pack checks](SONGPACKS.md) compare 393,205,888 raw and
393,176,540 retained channel samples per independent decoder. Peak error versus
libvorbis is at most `8.344650269e-7`, with aggregate RMS below `7.224e-8`.
Symphonia peak error is at most `5.960464478e-7`, RMS below `4.245e-8`.
All 1,600 retained seeks and 544 reference seeks pass. The eight song-pack gates
were rerun after the final mapping fix, with unchanged tested-source hashes.

## Acceptance and reference differences

The audit keeps published behavior and implementation-specific behavior explicit:

* Codebook exponents/array bounds, empty books, floor-zero widths/Bark rounding,
  floor-one integer/EOF decisions, residue tails/layout and nonfinite PCM have
  documented default/reference policies and independent component, packet and
  Ogg/reset/seek checks. See [the parity ledger](PARITY.md) and its linked reports.
* Historical zero span granules and decreasing EOS endpoints remain opt-in
  container/mapping compatibility cases. Declared and emitted durations are
  separate, including the shipped Mecha song. [EOS](EOS_CLAMPING.md) and
  [reference seeking](REFERENCE_SEEKING.md) compare the actual reference output.
* [Empty EOS pages without a timestamp](UNTIMED_EOS.md) are valid framing and
  now decode successfully. The default inherits the preceding landmark; reference
  mode preserves the reference's zero declared duration while returning PCM.
  Negative/unknown granules on completed audio pages remain rejected.
* Symphonia 0.6.1's dependent-coupling order and negative-initial-timestamp trim
  disagreements have named regressions. Native output follows the specification
  and libvorbis there. These are not treated as PCM tolerance exceptions.
* CRC failures, packet holes, invalid framing and resource-limit violations
  remain errors in the strict codec APIs. Reference mode does not promise to
  reproduce every permissive recovery action on an arbitrary corrupt file.

The earlier high-order/dynamic-range floor-zero follow-up is covered by
[direct backend and full-packet checks](FLOOR0_PARAMETERS.md): orders through
255, all amplitude widths, odd/even and partial LSP vectors, extreme Bark grids,
finite curve/PCM bounds and per-position nonfinite classes. Related codebook
exponent and geometry matrices exercise additional dynamic-range extremes.

## Final gates

The full release suite passes **226 tests, zero failures and zero ignored**;
only 17 manual golden-regeneration utilities are filtered out. The extracted
offline workspace passes **147 tests** with 96 external/generator tests
ignored there; the full release run supplies their external reference/corpus
coverage. Formatting, default/all-feature Clippy, extracted benchmark-target
compilation, 4 fuzz-harness tests and fuzz/seed lint all pass.

[Final commands, logs and source/input/oracle hashes](validation/2026-10-28-completion-checks.json)
and [the post-fix eight-gate song-pack receipt](validation/2026-10-28-completion-songpacks.json)
retain the results. The driver rechecks every recorded source hash after the
complete run; the exporter independently rechecks sources, media and oracle
binaries before writing these receipts. No test fixture is regenerated.

The final current-source AddressSanitizer streaming/seek campaign completes
6,085 executions in 301 seconds, with 61 initial seeds, 399 new corpus units,
424 MiB peak sanitizer RSS and no failure artifacts. It checks native bounded
behavior and state consistency; independent C accuracy is established by the
differential suites. Earlier stage-specific campaigns remain linked in their
milestone reports.

## Continuing development

This closes the decoding acceptance audit; it is not exhaustive certification
of every possible bitstream or platform. Additional media and longer fuzz
campaigns remain useful maintenance. Sparse/index-free codec seek strategies,
competitive encoding quality/rate control, performance work, additional
platforms and application integration are future tasks outside this request.
The existing indexed seek API is complete for its documented contract.

Historical milestone reports describe the work remaining at their own dates;
this audit and its final source-hashed receipts supersede their open-work lists
for the current decoding goal.
