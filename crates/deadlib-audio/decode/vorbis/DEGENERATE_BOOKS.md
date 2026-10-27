# Degenerate codebooks and IEEE reference propagation

The 2026-10-27 follow-up adds explicit libvorbis behavior for empty codebooks
and nonfinite synthesis. It also accepts nonempty zero-dimensional scalar books
with lookup type zero or two. Packet decisions, sample counts, positions and
recovery remain exact; the original finite PCM tolerances are unchanged.
Performance is outside this milestone, and neither crate is integrated into
DeadSync.

## Behavior and API

`Setup::parse_with_options` and `Codebook::read_with_options` accept
`ParseOptions { allow_empty_codebooks: true, ..Default::default() }`.
The default parser still rejects zero-active-entry Huffman trees: the published
underpopulated-tree rule provides a one-entry exception, without an empty-tree
exception. Enabling empty books preserves all other tree, index, resource and
context checks. Ordered zero-entry and unordered sparse declarations work with
lookup types zero, one and two. Packed lookup values are consumed even when
no decoded vector can be returned.

| Empty book consumer | Pinned libvorbis behavior now available |
| --- | --- |
| Unreferenced book | Accept setup; no synthesis effect |
| Floor-one master or subclass | Conceal only that channel's floor; consume no scalar bits |
| Floor-zero VQ | Keep zero LSP coefficients; consume no VQ bits |
| Residue classbook | Stop this residue; preserve following submap bits |
| Residue stage VQ | Skip vector additions and reads; preserve earlier and later stages |

Empty residue VQ books can have dimension zero, or a dimension that does not
divide the partition. Nonempty VQ books retain the dimension/partition check.
Floor-zero and residue-class books still require positive dimensions; residue
classbooks also require enough declared entries for the classification domain.
The cardinality comparison now correctly rejects zero entries even for a
single classification. A direct scalar read from an accepted empty book returns
`EndOfPacket` without advancing or exhausting the bit reader; consumers apply
the context-specific behavior above.

Nonempty dimension-zero books are useful as scalar books: Huffman codewords
remain meaningful without vectors. Lookup-one with positive active entries and
dimension zero remains rejected because its lattice reconstruction is undefined;
the pinned reference can fail to terminate during decoder initialization. Such
inputs are excluded from C synthesis probes. Empty descriptors also retain the
reference's `ilog(dimensions) + ilog(entries) <= 24` declaration bound.

`DecodeOptions { allow_nonfinite: true, ..Default::default() }` selects the
reference's IEEE propagation. It returns floor-zero's f32 product results
without the default wide-range fallback, permits NaN/infinity through synthesis
and overlap, and preserves earlier channel floors when a later floor reaches
EOF. That last distinction matters when a zero spectrum multiplies an infinite
floor. The default continues to require finite PCM and applies the published
all-channel floor-EOF concealment rule. Both options are copied into `OggOptions`
and `SeekOptions`, survive reset, and apply during selected setup reparsing and
seek pre-roll. They compose with `Residue2Layout::Libvorbis`.

Reference implementation sources:
[codebook parsing/consumers](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/codebook.c),
[book initialization](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/sharedbook.c),
[floor zero](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/floor0.c),
[floor one](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/floor1.c),
[residue zero/one/two](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/res0.c),
[mapping synthesis](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/mapping0.c).

## Independent comparisons

The procedural builder in `tests/support/degenerate_books.rs` writes independent
wire vectors, with uniform codewords assigned separately from the native Huffman
implementation. Two active submaps and mixed residue stages keep subsequent
bit consumption observable. The matrix varies one through seventeen channels,
coupling, all three residue types, all consuming roles, dimensions zero through
four, declared entries zero/one/four, lookup types and both zero-entry length
forms. Each prefix case has two pre-roll and two recovery packets.

| Matrix | Setups | Cases | Packet decisions | Finite PCM values | Matching nonfinite classes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Empty books | 276 | 35,080 | 175,400 | 75,280,336 | 1,141,760 |
| Nonempty dimension-zero scalar books | 36 | 36 | 180 | 26,496 | 0 |
| Previously recorded floor-zero mutations | 1 | 11 | 55 | 40,704 | 16,256 |
| Later-floor EOF regression prefixes | 1 | 83 | 415 | 31,056 | 22,800 |
| Retained golden cases | 11 | 12 | 60 | 10,400 | 640 |

The empty-book matrix includes 1,380 discarded packets. Its largest absolute
finite error is `1.430511475e-6`; every batch meets the original scaled bounds.
All eleven previously documented [nonfinite-reference cases](BIT_MUTATIONS.md)
now match decisions, counts, positions and NaN/infinity classes under explicit
IEEE propagation. Their finite subset has peak/RMS error below `6.71e-8` /
`4.53e-9`. The original default finite-output guard remains separately tested.
NaN payload bits are unspecified and are not a parity requirement; infinity signs
and finite versus nonfinite classification are checked at every output position.

The 44,932-byte `degenerate-books-v1.libvorbis-pcm` golden retains eleven setup
combinations and twelve cases, including a later-floor EOF and maximum
16-bit dimensions. Its SHA-256 is
`6b4615db3c606000482307cc37e03e945a116dfb9d68d99ba1c7f142fb6850cf`.
Independent C recapture must match its bytes exactly. Normal tests read it without
an installed reference codec. Separate native checks exercise page-spanning setup,
pull, one-byte/17-byte/whole-input push, reset, and **286** repeated/boundary seeks
through both pre-roll strategies. These require exact PCM bits except NaN payloads.

## Validation

The complete final-source gates pass **149 release tests**, **110 extracted
offline tests** (43 external/manual tests ignored there), formatting, default/
all-feature Clippy and fuzz-harness unit/lint checks. Earlier configuration,
prefix, finite bit-mutation and residue geometry suites retain their counts,
including 332,832 geometry cases / 1,836,059,120 PCM values. The 38 repository
assets, 528 upstream coded vectors, pinned historical/legacy corpus and 144
native encoding cases remain enabled.

The independent C reset-protocol selfcheck covers **1,691 fresh-process cases**
from 1,803 generated matrix requests. C libvorbisfile checks all eleven procedural
Ogg streams at exactly 368 frames per channel and verifies **121** boundary seeks
against its raw packet PCM. Finite values and infinity signs agree exactly within
this C selfcheck; only NaN payload bits are normalized for container comparisons.
The retained C golden is recaptured byte for byte.

Three 300-second AddressSanitizer campaigns complete **240,279 executions**:
225,252 headers, 10,680 raw packets and 4,347 streaming/seeking executions.
No failure artifacts or slow-unit reports are produced. The **114** targeted seeds
include strict/empty/IEEE/combined options, the later-floor EOF, maximum dimensions
and the eleven historical mutations. Fuzz summaries require finite samples under
the default policy; the explicit IEEE mode canonicalizes NaN payloads while
retaining finite bits, infinity signs and exact counts/positions. These bounded
state-consistency checks supplement independent reference comparisons.

## Reproduction and scope

With the pinned oracle and floor-zero fixture configured:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --test codebook_compatibility -- --include-ignored --skip regenerate_degenerate_book_golden --nocapture
```

Regenerate the golden only by explicitly selecting
`regenerate_degenerate_book_golden -- --ignored`. The golden's inputs are procedural;
external historical audio and its derived requests remain under ignored `target/`.

The [validation probe](validation/2026-10-27-degenerate-books-validation-probe.py),
[reference selfcheck](validation/2026-10-27-degenerate-books-reference-probe.py) and
[checks/provenance](validation/2026-10-27-degenerate-books-checks.json) preserve
commands, exact source/input/reference/binary hashes, baseline failures, reports
and sanitizer evidence. The generated diagnostic decoder copy is compiled/linted
for compatibility only; no timing run is performed.

This closes the named empty-book and eleven nonfinite cases under explicit
reference policies. It does not establish complete parity for every legal or
malformed stream. The remaining corpus, conformance and named acceptance/count
differences in the [parity ledger](PARITY.md) still require independent work.

The subsequent [extent-limit follow-up](CODEBOOK_LIMITS.md) applies the reference
guard before lengths for every book and exposes it through
`Limits::max_codebook_extent_bits` (default 24, configurable independently).
