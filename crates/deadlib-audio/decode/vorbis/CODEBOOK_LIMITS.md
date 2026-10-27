# Codebook declaration extent parity

The 2026-10-27 follow-up applies libvorbis's declaration extent guard to every
codebook before reading lengths or allocating entry storage. Previously only
empty books received the guard, after reading their lengths. A populated unused
book with dimension 65,535 and 256 declared entries was accepted under ordinary
native defaults; the reference rejects it immediately after the 64-bit descriptor.

`Limits::max_codebook_extent_bits` bounds
`ilog(dimensions) + ilog(entries)`. Its default is **24**, matching pinned
libvorbis 1.3.7. Rejection returns `LimitExceeded("codebook extent")` at bit 64
without exhausting the reader, before entry/memory limits and length-table reads.
The guard now applies equally to populated, sparse, empty and mapped books.

This is a configurable application policy, alongside the existing entry, value,
packet and memory caps. Set it to **40** to allow the full 16-/24-bit wire-field
extent, subject to those other limits. This keeps larger declarations available
without presenting libvorbis's implementation guard as a Vorbis I format rule.
`ParseOptions::allow_empty_codebooks` still independently controls empty Huffman
acceptance. Ogg/indexed parsing receives the same `Limits`, including during
selected setup reparsing and reset.

The reference guard is in
[pinned codebook.c](https://github.com/xiph/vorbis/blob/0657aee69dec8508a0011f47f3b69d7538e9d262/lib/codebook.c).
The specification's codebook description is in
[Vorbis I section 3](https://xiph.org/vorbis/doc/Vorbis_I_spec.html).
Neither production crate gains a reference-codec dependency or application
integration. Performance is outside this milestone; the original PCM tolerances
and exact behavior/count requirements are unchanged.

## Independent declarations and setup checks

`tests/support/book_extents.rs` writes independent wire declarations around the
bit-width transitions. Dimensions cover zero through 65,535; entries include zero
through 16,777,215. Styles include a single first or last active entry, empty sparse
books and complete power-of-two books. Lookup types one and two also appear on
both sides of the extent bound, with actual packed multiplicands. A 32-bit sentinel
after each complete book checks that successful reads leave following bits intact.

The retained **475** C decisions include **289 accepted** and **186 rejected**
declarations, with exact consumed-bit positions. Other native caps are raised
where necessary to isolate the extent guard; this does not claim that the default
entry cap accepts every declaration in this matrix. Empty books are explicitly
enabled for these comparisons.

The new test-only `bookheaders` command calls unmodified
`vorbis_staticbook_unpack` and reports its result and `oggpack_bits`. It never
initializes or evaluates VQ tables. Its complete output is retained in the
4,291-byte `codebook-extents-v1.libvorbis-headers` golden, SHA-256:
`996c82e467390409834115f78a924112e70100c758aedacc5b37bdfc8da36053`.
Normal tests use that golden without an installed reference library.

Separate checks embed all 475 declarations as an unused book in complete setup
headers and construct both synthesis contexts. All parse/initialization decisions
match. The batch command also matches **475 fresh-process header commands**.
Header rejection and synthesis initialization are checked separately, so the
reference's deferred Huffman validation cannot masquerade as accepted synthesis.
The matrix deliberately uses valid singleton/complete trees or explicitly enabled
empty books, not arbitrary malformed trees.

A standalone build of preceding commit `4b46420d6` and a current-source build
receive the same complete **46-byte** ordinary-default regression. Before: accepted,
reader at bit **331**. After: rejected at bit **64**, matching C, with the reader
unexhausted. The earlier 467-case golden also remains an identical prefix of the
final matrix's records; eight mapped-book cases were subsequently appended.

## Streaming, seeking and reference-helper checks

Four native streams test caps 14, 24 and 25, rejection one bit below each required
extent, pull, one-byte/17-byte/whole-input push, reset and **104** repeated/boundary
seeks through both pre-roll strategies. One setup spans pages. The fourth stream
intentionally exceeds the default C guard and is available only with a raised
native limit; it is not counted as reference acceptance.

Three streams accepted by the default reference guard match C raw PCM under the
original bounds, with exact 368-frame counts. Independent libvorbisfile checks
those streams and **33** seeks against its raw PCM suffixes byte for byte.
Fifteen earlier C `pcm-cases` responses, including nonfinite and residue-layout
regressions, remain byte-identical to the preserved preceding helper binary.
All 475 whole-header initializations also agree between the old and new helper.
Only the helper's new command and usage string changed; pinned codec/container
library sources remain unmodified.

Three 300-second AddressSanitizer campaigns complete **245,956 executions**:
231,982 header, 10,269 raw-packet and 3,705 streaming/seeking executions, with no
failure artifacts or slow units. **170** targeted seeds cover both sides of the
guard and the explicit full-field override, empty/nonempty and zero-dimensional
books, and a page-spanning setup. Header control bit four and stream bit six
select extent 40 instead of 24; other entry/value/memory caps remain active.
Fuzzing checks bounded state consistency separately from independent C parity.

The final source passes **155 release tests**, **113 extracted offline tests**
(47 external/manual tests ignored there), formatting, default/all-feature Clippy,
and fuzz-harness unit/lint checks. Existing configuration, prefix, finite bit-
mutation, empty-book and residue geometry suites retain their counts, including
332,832 geometry cases / 1,836,059,120 PCM values. The 38 repository assets,
528 upstream coded vectors, pinned historical/legacy corpus and 144 native
encoding cases remain enabled. The diagnostic decoder copy is generated/linted
for API compatibility only; no timing run is performed.

## Reproduction and evidence

With the pinned oracle configured:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --test book_limits --test book_limit_streams -- --include-ignored --skip regenerate_codebook_extent_golden --nocapture
```

Regenerate the golden only by explicitly selecting
`regenerate_codebook_extent_golden -- --ignored`. Inputs are procedural and are
independent of the native encoder.

The [checks/provenance](validation/2026-10-27-codebook-extents-checks.json),
[reference probe](validation/2026-10-27-codebook-extents-reference-probe.py),
[before/after probe](validation/2026-10-27-codebook-extents-before-after-probe.py)
and [full validation probe](validation/2026-10-27-codebook-extents-validation-probe.py)
retain source/input/reference/binary hashes, decisions, exact reader positions,
regression logs and sanitizer evidence. Large generated requests and evolving
fuzz corpora remain under ignored `target/`.

This resolves the extent guard difference. It does not prove complete acceptance
or synthesis parity for every legal or malformed setup. The broader conformance,
corpus and named reference differences in the [parity ledger](PARITY.md) remain
active work. Performance results cannot close those gaps.
