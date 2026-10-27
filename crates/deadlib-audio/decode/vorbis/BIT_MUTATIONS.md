# Bit-mutation parity

The 2026-10-27 follow-up to [the prefix/recovery work](PARITY.md) compares every
audio-packet bit in the four generated fixtures and fourteen independently
selected historical floor-zero packets against pinned libvorbis 1.3.7.
Behavior/counts remain exact and the original PCM tolerances are unchanged.
No performance comparison was used for this milestone.

## Corrected behavior

Three differences were reproduced and fixed:

* Floor-zero reconstruction previously used wider LSP/polynomial intermediates
  throughout. Sharp resonances reached by bit mutations amplified differences
  beyond the existing PCM bounds. The main calculation now follows the
  reference's f32 coefficient, amplitude, angle-step and product boundaries,
  followed by its double square root/exponential. Multiplication grouping is
  explicit, since reassociation changes rounding. The previous wide-range
  calculation remains a fallback when the f32 calculation is nonfinite; a
  254th-order regression checks that this preserves finite high-order gains.
* An invalid floor-zero book selector previously returned a packet error.
  libvorbis treats that channel's floor as absent and continues reading other
  channels. Native decoding now does the same without exhausting the remaining
  bits. A normal unit test covers 34 invalid selectors across eight legal book
  counts, checking untouched output, exact consumption and following bits.
* VQ sequence reconstruction previously carried an unrounded f64 intermediate
  into the next component. The offset must use the returned f32 scalar.
  A synthetic book with minimum one, delta `2^-24` and unit multiplicands
  exposes the difference: reference vectors are exactly `[1, 2, 3]`; carrying
  the hidden intermediate makes the third scalar one f32 ULP too large.
  Independent libvorbis probes and a normal unit test cover lookup types one
  and two. This also removes a legacy mutated-packet RMS failure.

Reference implementations inspected:
[floor-zero decoding](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/floor0.c),
[LSP reconstruction](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/lsp.c),
[VQ unquantization](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/sharedbook.c).
Production remains independent pure Rust with no reference-codec dependency.

## Matrix and reference limitations

Each mutation has up to two original pre-roll and two recovery packets. Both
decoders restart between cases. Reference output is processed in batches of
at most 64 cases to bound memory. The legacy nonzero packet selection comes
from libvorbis, so a native silence regression cannot change the tested inputs.
All finite-reference packet decisions, sizes, counts, positions and PCM are
checked, including recovery.

| Input | Mutations | Finite-reference cases | PCM channel samples compared |
| --- | ---: | ---: | ---: |
| Mono | 504 | 504 | 316,416 |
| Stereo | 7,072 | 7,072 | 38,756,224 |
| Eight-channel silence | 48 | 48 | 586,240 |
| Eight-channel dependent coupling | 24,496 | 24,496 | 501,570,048 |
| Historical floor zero | 23,736 | 23,725 | 121,890,432 |
| Total | 55,856 | 55,845 | 663,119,360 |

The finite-reference cases check **236,529 packet decisions**. The previous
7,014 byte-prefix cases / 83,072,768 PCM values also pass again.

Eleven additional legacy mutations make libvorbis emit nonfinite PCM. They
are **excluded from PCM-parity totals**, with named inputs and outcomes retained.
For example, packet 27/bit 16 produces 1,600 nonfinite values across two
reference blocks. Native finite-output rejection/finite fallback and explicit
reset recovery are tested separately. This is a documented behavior divergence,
not a widened tolerance or a passing PCM comparison. It remains part of the
parity assessment.

## Validation and evidence

The final source passes **128 release tests**, **97 extracted offline tests**
(32 external/manual tests ignored there), formatting and default/all-feature
Clippy. Existing corpus, timing/count, seeking and native encoding reference
gates remain enabled. Three five-minute AddressSanitizer campaigns complete
**142,789 executions**. Ten new raw/stream seeds reproduce the observed rounding,
selector, sequence, discard and nonfinite boundaries. No failure artifacts were
produced. Sanitizer smoke runs do not establish reference equivalence.

[Checks/provenance](validation/2026-10-27-bit-mutations-checks.json) retain exact
source/input/reference/binary hashes, matrix totals, failures and final checks.
[Sequence-book proof](validation/2026-10-27-bit-mutations-sequence-books-reference.json)
and [nonfinite reference evidence](validation/2026-10-27-bit-mutations-nonfinite-case-reference.json)
retain independent C results. External audio and derived audio-bearing seeds
remain in ignored storage.

Complete parity remains unproven. Broader legal setup/mode/codebook combinations,
reference acceptance differences, malformed-input classes, official material
and real-song coverage remain required. The eleven nonfinite cases are not
silently counted toward completion.
