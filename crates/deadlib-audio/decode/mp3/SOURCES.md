# Standards and reference-source audit

The implementation is an elementary-stream MP3 decoder. It supports Layer III
MPEG-1 and MPEG-2 LSF, plus a separately documented MPEG-2.5 compatibility
profile. It is not a full ISO MPEG Audio product supporting Layers I/II,
multichannel extensions, systems demultiplexing, or every optional transport.
Its API returns normalized, unclipped synthesis PCM. Header emphasis is exposed
as metadata; playback de-emphasis and output-format conversion belong to the
caller. No formal ISO certification is asserted.

## Published editions and corrections

| Publication | Audited evidence | Consequence |
| --- | --- | --- |
| ISO/IEC 11172-3:1993, edition 1 | [ISO catalogue](https://www.iso.org/standard/22412.html), [published cover/foreword preview](https://assets.vde-verlag.de/iec-normen/preview-pdf/info_isoiec11172-3%7Bed1.0%7Den.pdf) | MPEG-1 reference edition is 1993. Annexes A/B are normative; H is an informative patent-holder list. |
| ISO/IEC 11172-3:1993/Cor.1:1996 | [IEC publication description](https://webstore.iec.ch/en/publication/9588) | Adds a table to Annex H. It supplies no decoding-equation or numeric-table correction. |
| ISO/IEC 13818-3:1998, edition 2 | [ISO catalogue](https://www.iso.org/standard/26797.html), [published preview](https://assets.vde-verlag.de/iec-normen/preview-pdf/info_isoiec13818-3%7Bed2.0%7Den.pdf), introduction/foreword | Replaces 1995. The listed technical changes concern multichannel prediction/crosstalk, surround filtering and LFE syntax. The implementation uses the LSF mono/stereo scope. |
| ISO/IEC 14496-3:2001/Amd.3:2005 | [Published amendment preview](https://cdn.standards.iteh.ai/samples/39584/696c43b397e24f5ea9b6f80f8b193464/ISO-IEC-14496-3-2001-Amd-3-2005.pdf), clause 9.5.5 | Corroborates the selected intensity top-band clarification. It is a later MPEG-4 integration text, not a replacement edition of 11172-3. |

The two VDE previews' hashes and access scope are archived with the final source
audit. Their short previews do not contain all normative decoder pages. The
publisher catalogue/preview checks reconcile edition identities and the stated
changes; they do not establish a line-by-line audit of the full published texts.

## Drafts and compatibility material

The OSSRS MPEG-1 PDF has a 1993 filename but contains CD/DIS-era 1991 text and
omits the normative table annex. It must not be cited as an inspected complete
published 1993 edition. Its SHA-256 is
`a91dff423d0958667d72d6646219f4a54c0d1f0094e73860b29503bd5b2c7cec`.

The OSSRS MPEG-2 PDF is WG11 N1519, dated 20 February 1997, before publication
of the second edition in 1998. Its SHA-256 is
`25ebf438988fced761b79adcb108c0a59acc68a0f38be36017c334abb8582df5`.
The LSF syntax/partition and table inspections recorded in earlier milestones
refer to that accessed document, not a complete 1998 published copy.

The [Sieler/Sperschneider Layer3 description, issue 1.2, 4 March 1997](https://mp3guessenc.sourceforge.io/MPEG%20Layer3%20Bitstream%20Syntax%20and%20Decoding.pdf)
provides an explicit combined interpretation of MPEG-1/2 and the unofficial
MPEG-2.5 extension. Its own scope gives the ISO texts precedence. Ambiguous
mixed geometry, Huffman regions and stereo edges are therefore identified as
specific selected rules and independently tested; see [REGIONS.md](REGIONS.md),
[MPEG25.md](MPEG25.md) and [STEREO.md](STEREO.md). Decoder disagreement is
preserved rather than resolved by majority vote.

## Numeric tables, algorithms and independent references

`tests/data/{huffman,bands,dsp}.json` pin the factual numeric sources and hashes.
The implementation was written from syntax, numeric data and mathematical
algorithms; public decoder APIs supply independent outputs. The Rust package
has no production codec dependency. Symphonia is a pinned development reference;
the C wrappers call libmad/mpg123 without translating their implementations.

The 18 legacy bitstream/16-bit PCM pairs are hash-pinned in
`tests/data/iso-vectors.json`; their mirror is not represented as an ISO-issued
certification bundle. [Underbit's compliance methodology](https://www.underbit.com/resources/mpeg/audio/compliance/)
distinguishes accuracy classes and the supplied high-precision sine sweep.
Our legacy PCM gate separately reports the full-accuracy RMS comparison and a
functional bound accommodating a truncated 16-bit reference. Some vectors fail
that stricter RMS comparison. They all pass the declared functional gate.
The original 24-bit compliance sweep has not been substituted or invented.

The independent floating-PCM gate fixes RMS `2e-6`, maximum `2e-5`, and exact
frame/rate/channel/sample counts. It permits no fitted gain, resampling, or
alignment search. Direct-equation cases supply additional evidence where
reference implementations disagree. Historical failures, excluded malformed
files, unsupported reference configurations and all thresholds remain visible
in their original reports. This establishes the documented decoder checks;
formal complete standards certification would require the full normative
texts and official high-precision compliance material.
