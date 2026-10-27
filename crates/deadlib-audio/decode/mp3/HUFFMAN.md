# Native Layer III entropy primitives

The codec now decodes all ISO pair codebooks (including table zero and linbits
aliases 16..31), both count1 quadruple books, escape magnitudes and nonzero
signs. Reserved books 4 and 14 are rejected when a coded pair requests them.
Failed pair/quad reads leave the bit cursor unchanged, including an incomplete
count1 symbol at the part2_3 boundary. The later granule parser must distinguish
this count1 stuffing condition from an invalid/truncated big-values region.

The implementation is a native flat prefix tree: 1,393 two-child nodes with a
high-bit leaf tag, plus root and linbits arrays. These immutable tables occupy
5,672 bytes of elements and need no runtime construction, allocation or pointer
chasing through heap nodes. No throughput claim is made before full PCM parity.

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

Fourteen integration tests now pass. Huffman tests exercise 175,680 pair
configurations across every codeword, both sign bits, all eight initial bit
offsets and independent zero/maximum escape extensions for x and y. All 4,096
quadruple/code/sign/offset configurations pass. Truncation at every bit of
selected longest/escape symbols leaves the input cursor unchanged.

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
