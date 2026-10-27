# Residue ranges, partial vectors and EOF parity

The goal is exact behavior, counts, positions and channel order with the original
PCM tolerances. This milestone closes reference differences in residue setup
acceptance, vector tails and reader state. Complete Ogg/Vorbis parity remains
unfinished; see the [parity ledger](PARITY.md).

## Published and reference policies

`ParseOptions::residue_behavior` chooses `ResidueBehavior::Vorbis` (default) or
`ResidueBehavior::Libvorbis`. `Setup` retains this choice for raw decoding and
reset. The same field in `OggOptions` and `SeekOptions` survives streaming reset,
index construction and every selected setup reparse.

The default requires ordered ranges and a partition size divisible by every
selected nonempty VQ dimension. The explicit reference policy accepts partitions
that end partway through a VQ vector and treats inverted ranges as empty. It
preserves the original begin/end/partition metadata. Endpoint clipping to the
current block still happens before computing the number of complete partitions.

| Consumer | Explicit libvorbis behavior |
| --- | --- |
| Type zero | Read floor(size / dimension) words and add dimension times that many values in transposed order; leave the final gap untouched. A partition shorter than one vector consumes no VQ words. |
| Type one | Read ceil(size / dimension) words; clip the last vector to the partition end. |
| Type two | With the separate reference layout, round both endpoints to whole channel groups; clip the last vector at the rounded end. |
| Empty VQ book | No additions and no word consumption, including dimension zero. |
| Inverted or clipped-empty range | No classification words or vector additions; later submaps retain their input. |
| Truncated Huffman word | Consume its available prefix without creating sticky EOF; preserve EOF that was already set on entry. |

Type zero retains complete-partition commit: an unfinished gather adds nothing
from that partition, while previously completed partitions and passes survive.
Type one and two retain completed vector components when a later word fails.
Later submaps share the same reader, so codeword counts and EOF state matter even
when the truncated residue itself becomes silent.

The [published sections 8.3/8.4](https://xiph.org/vorbis/doc/Vorbis_I_spec.html)
require whole VQ vectors per partition. The pinned reference's
[residue parser/inverse](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/res0.c)
does not impose that divisibility guard or reject inverted ranges. Its
[vector-add and Huffman routines](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/codebook.c)
define the tail and EOF behavior above.

Nonempty zero-dimensional stage VQ books remain rejected in both policies. Their
reference callbacks have undefined division or nonterminating iteration, so they
cannot supply a defined parity target. Empty zero-dimensional VQ books are safe
no-ops and are independently tested. Other resource and index guards remain in
force.

`Residue2Layout`, codebook float/byte guards, empty-book admission, floor-zero
behavior and IEEE output remain independent options. Selecting the residue
policy does not select them automatically. Reference type-two geometry uses both
`residue_behavior: ResidueBehavior::Libvorbis` while parsing and
`residue2_layout: Residue2Layout::Libvorbis` while synthesizing.

## Independent evidence

The C helper calls unmodified `vorbis_book_decodevs_add`, `decodev_add` and
`decodevv_add` callbacks. The direct matrix covers 38 books, dimensions
0/1/2/3/4/5/8/31/255/65535, empty/populated books, sequence accumulation, all three
consumers, multiple channel counts, noncontiguous/reversed native bundles,
unaligned offsets, zero/partial/whole lengths, three starting bit alignments,
truncated payloads and preexisting EOF. It checks 119,320 cases and 10,535,488
float values bit for bit, including 8,186 failed-word cases and 1,368 cases with
sticky EOF output. Positions match exactly whenever the C reader is not in its
internal exhausted sentinel state. Sticky state itself matches exactly; the
public native position retains its documented last-successful-bit meaning.
Front/back guards and unselected native channel planes must stay untouched.

A separate classword test checks fresh versus preexisting EOF under both
policies. The offline callback golden retains 355 cases across seven books,
including empty dimension zero and populated dimension 65535. It stores complete
requests and independent C responses, so regenerating procedural inputs cannot
silently alter the fixture interpretation.

The full packet matrix covers 324 setups, residue types zero/one/two, partition
sizes 1/2/3/5/7/11/12/13/17 and 2^24, inverted/equal/maximum-field ranges,
classword dimensions one/two/three, mixed cascades, 1/3/8 channels, coupling and
one/two/four submaps. Every byte prefix is checked for the smaller channel
counts; larger cases use boundary prefixes. Recovery follows every candidate.
The matrix checks 17,500 cases, 87,500 packet decisions, 648 discards and
20,010,336 finite PCM values. All decisions, block/frame counts and cumulative
positions agree exactly. Per-block peak/RMS bounds remain 3e-6 / 5e-7 times
max(1, reference peak).

Six retained full-PCM regressions cover raw reset, pull decoding, one-byte,
17-byte and whole-input push/reset, plus 168 repeated native boundary/backward
seeks across both pre-roll strategies. These state checks compare complete native
PCM bits. Independent C raw/Ogg/seek checks and unchanged older procedural
requests supplement the differential packet matrix.

## Reproduction

Set `DEADLIB_LIBVORBIS_ORACLE` to the pinned helper, then run:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --lib residue::reference_tests -- --include-ignored --skip regenerate_residue_tail_vector_golden --nocapture
cargo test -p deadlib-audio-decode-vorbis --release --test residue_tails -- --include-ignored --skip regenerate_residue_tail_pcm_golden --nocapture
```

Normal tests read the retained callback and PCM fixtures. Explicit manual
generators require `--ignored` and the helper. Procedural headers/audio come from
independent test builders, rather than the native encoder.

Reference sources remain libogg 1.3.6 at
`be05b13e98b048f0b5a0f5fa8ce514d56db5f822` and libvorbis 1.3.7 at
`0657aee69dec8508a0011f47f3b69d7538e9d262`. Production depends only on the native
Ogg crate. This milestone includes no application integration or performance
measurement.

Final validation passes 183 release tests and 128 extracted offline tests
(66 external/manual tests ignored offline), formatting, default/all-feature
Clippy, two fuzz-library tests and fuzz Clippy. Three 300-second AddressSanitizer
campaigns complete 298,138 executions with no failure artifacts or slow units.
The 420 targeted seeds derive from 42 independently C-checked packet sequences.
Fuzzing checks bounded behavior and state consistency, rather than proving full
reference equivalence.

Fresh C processes recapture all 944 callback batches and both new goldens.
Another 1,530 single-record probes cover each observed book/type/alignment/tail/
EOF/result category without preceding callback state. Six C Ogg decodes and
84 repeated C seeks match raw synthesis exactly. The helper also preserves
28 older PCM responses byte for byte. All 17,370 requests from the preceding
procedural matrices remain byte-identical after extending their test builder.

A fresh build of committed `e8d4fca809e6bd8946f11c139f06c0336e9f3db5` accepts
three of nine setup witnesses; the explicit residue policy accepts all nine,
agreeing with both the preserved and current C helpers. Default guards remain
separately tested.

[Checks and provenance](validation/2026-10-27-residue-tails-checks.json) retain
commands, source/corpus hashes, callback/packet counts, sanitizer evidence and
artifact hashes. [Independent reference checks](validation/2026-10-27-residue-tails-independent-reference.json)
record complete recaptures and unchanged request bytes. The
[compiled before/after witness](validation/2026-10-27-residue-tails-before-after.json)
retains source, driver and executable hashes. External audio and generated
corpora stay in ignored storage.
