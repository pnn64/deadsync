# Native Layer III entropy primitives

The codec now decodes all ISO pair codebooks (including table zero and linbits
aliases 16..31), both count1 quadruple books, escape magnitudes and nonzero
signs. Reserved books 4 and 14 are rejected when a coded pair requests them.
Failed pair/quad reads leave the bit cursor unchanged, including an incomplete
count1 symbol at the part2_3 boundary. The later granule parser must distinguish
this count1 stuffing condition from an invalid/truncated big-values region.

Book 1's complete codeword and signs fit in five bits. A 32-entry packed lookup
returns both signed values and the total consumed length, omitting its unused
tree. Book 2 handles its one-bit zero pair directly and uses a 128-entry lookup
for the remaining code/sign suffixes. Book 3 uses a complete eight-bit lookup.
Packed entries contain signed three-bit values and their consumed length. Other pair books
through 23 use a native flat prefix tree. Escape books 24..31 share a
five-bit prefix lookup, followed by a residual tree walk for longer words.
The generated forest retains 1,318 two-child nodes with high-bit leaf tags;
unreachable parents and duplicate escape-family roots are omitted. Short tails
peek only their available bits and zero-pad the lookup index. The stored code
length must still fit before it is consumed, preserving truncation errors.
The small tree walker stays inlined to avoid a per-pair helper call. A shared
bounded five-bit peek loads one or two bytes directly when five bits remain;
short tails retain explicit zero padding and never consume lookahead bits.
Single-bit sign flags use a checked byte-and-mask read. The sign and flag
helpers stay inlined so larger pair paths do not introduce per-sign calls
in the measured quantizer.
The eight-bit peek loads a two-byte window where available, pads a missing
second byte, and masks bits beyond the reader's explicit end.

Count1 table A's one-bit all-zero code returns directly. Its other codes start
with zero and have at most five further bits, resolved by a 32-entry lookup.
Table A's compact layout specifies the sign count and output offset for each
magnitude mask. One sign-field read selects an eight-byte record from the 81
possible signed quadruples. Table B recognizes its four-bit zero code directly; its other code/sign words
use a complete eight-bit lookup to select one of those same records with a
single checked skip. The immutable
tables occupy 7,568 bytes of elements and need
no runtime construction, allocation or pointer chasing through heap nodes.

## Numeric data provenance

The fixed wire-format codewords are ISO's numeric Layer III tables, consumed
from `dist10/lsf/encoder/tables/huffcode` in the ISO MPEG Audio Subgroup Software
Simulation Group distribution 10 (1996/1997). The
[MIT MPEG resource index](https://sound.media.mit.edu/resources/mpeg4/audio/ftp/)
identifies this distribution as ISO/IEC 13818-5 reference software. A preserved
[public archive mirror](https://nekofiles.irixnet.org/Sound/dist10.tar.gz) supplies
the exact data file. Only numeric table definitions are used; reference
implementation source is neither translated nor linked into this crate.

Archive SHA-256:
`3c22fa4dac080d03519843ac4fcc42c0a925cc6e744b5b4ffec33b3eff4397c4`.
Raw numeric `huffcode` SHA-256:
`ffe34a6725227e4649eb6104b772e4a96203da2b8a08c5a9912aef403f53cdb7`.
The original archive and raw table remain in ignored scratch storage.

`scripts/generate_huffman.py` validates all 34 table descriptors, references,
dimensions, lengths, codewords and completeness, rejecting duplicate leaves,
prefix collisions and incomplete trees. It generates an original packed trie
and a normalized CSV fixture containing 1,410 symbols. The CSV and table
descriptors are retained under `tests/data/`; no source archive is needed at
build time or runtime. Regeneration is byte-reproducible.

```sh
python3 crates/deadlib-audio/decode/mp3/scripts/generate_huffman.py --input /path/to/original/huffcode
```

The full published-standard/corrigendum and independent-conformance reconciliation
in PLAN.md remains required; the previously accessed MPEG-1 prose PDF is a draft
and omits Annex B. This data provenance is explicit rather than claiming that
missing pages were inspected.

## Validation

All 72 crate tests pass. Huffman tests exercise 175,680 pair
configurations across every codeword, both sign bits, all eight initial bit
offsets and independent zero/maximum escape extensions for x and y. All 4,096
quadruple/code/sign/offset configurations pass. Truncation at every bit of
every pair codeword is also checked at offsets 0/3/7 with positive/negative
signs and maximum escape extensions. Incomplete pair and quadruple reads leave
the input cursor unchanged. The exact-output benchmark trace includes another
1,179,648 bounded Huffman result/cursor observations. Performance comparisons
and rejected variants are recorded in the
[seventh optimization pass](../../../../docs/performance/2026-10-28-mp3-round7.md)
and [eighth optimization pass](../../../../docs/performance/2026-10-28-mp3-round8.md).
The padded-prefix helper also passes 5,701,633 independent pattern/bound checks,
including empty input and every two-byte value, offset and remaining length
through five bits. The [fourteenth optimization pass](../../../../docs/performance/2026-10-29-mp3-round14.md)
records the direct code/sign lookups and their measured tradeoffs.
The eight-bit helper additionally passes 10,027,008
independent two-byte pattern, offset and end-bound checks, plus empty input.

An independent gate embeds every codebook and sign/escape boundary in valid
MPEG-1 frames, with two/four isolated coefficients in subband zero and a zero
second granule to clear overlap. libmad's unmodified public frame API decodes
22,476 vectors. Its public hybrid samples agree with the analytical f64
long-window IMDCT equation: largest absolute error `3.063e-8`. The gates fixed
normalized hybrid RMS `2e-7` and maximum error `1e-6` before execution. This
checks codeword meaning through an independent decoder as well as the native
trie's exact integer/bit-consumption tests; it is not complete native PCM parity.

```sh
cc -O2 crates/deadlib-audio/decode/mp3/tests/oracle/hybrid.c -lmad -o /path/to/hybrid-reference
python3 crates/deadlib-audio/decode/mp3/tests/reference_huffman.py --oracle /path/to/hybrid-reference --output /path/to/new-huffman-report
```

The [validation record](tests/validation/2026-10-27-huffman/validation.json)
retains all test/Clippy commands, source/table hashes, per-book maxima, vector
and full hybrid-trace hashes, reference binary/version and regeneration checks.
The full deterministic MPEG input and reference trace remain in ignored scratch
storage and can be reproduced with the command above. Requantization across
scalefactor bands, reordering, stereo, alias reduction, stateful synthesis,
whole-stream parity, seeking, encoding and performance remain unfinished.
