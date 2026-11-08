# MP3 decoder status

The native decoder implements the documented MPEG-1/2 Layer III and MPEG-2.5
scope, including framed PCM, blocking streams, bounded non-seekable gapless
output, indexed files and sample-accurate seeking. It has no production codec
dependency and remains independently buildable. Existing DeadSync playback
integration is unchanged. Native encoding and performance work are separate
roadmap items outside the current decoding goal.

## Completion audit

| Decoder requirement | Evidence |
| --- | --- |
| Headers, all rates/modes/bitrates, CRC, free format, reservoir and metadata | [Foundation coverage](COVERAGE.md); exact libmad structural comparison on 84 LAME files; checked bounds and malformed inputs. |
| Entropy/scalefactors, escapes/signs/count1, SCFSI and LSF partitions | [Huffman audit](HUFFMAN.md), 175,680 native pair configurations, 4,096 quadruple configurations, 22,476 independent hybrid vectors; exhaustive compression/shape parsing. |
| Requantization, reorder, all windows, stereo and synthesis history | [Raw PCM](RAW_PCM.md), [synthesis](SYNTHESIS.md), [stereo equations](STEREO.md), [MPEG-2.5 profile](MPEG25.md); explicit reference disagreements checked with direct equations. |
| Mixed-block implicit Huffman boundaries | [Region correction](REGIONS.md); failing-before regression, distinct books, signed coefficients, both book orders, all nine rates; 1,368,576 independent expected PCM values pass. |
| Raw/file/stream framing, recovery, truncation, tiny/Interrupted reads and resets | [Streaming contract](STREAMING.md), [historical damaged music](HISTORICAL.md) and the current Rust/fuzz gates. Errors and concealment remain explicit. |
| Delay/padding, Xing/Info/LAME, exact audible/raw coordinates | [Indexed decoding](SEEKING.md), [bounded gapless streams](GAPLESS_STREAMING.md); 152 files/756 read schedules exactly match validated file PCM. All 152 files are rechecked after the region correction. |
| Seeking with reservoir, hybrid and synthesis dependencies | Every-frame/boundary/random/repeated seek audits require bit-identical native linear PCM. Independent mpg123 seeks are checked with eight reference preframes. Historical tests alone add 52,526 valid and 50,588 recovered seeks. |
| Encoder interoperability and numerical accuracy | 120 generated LAME files and 30 FFmpeg/LAME, Shine and Windows Media Foundation files; nine sample rates, mono/stereo, CBR/VBR, CRC/free format where supported; two complete historical music files. [Encoder details](ENCODER_COMPATIBILITY.md). |
| Supplied vectors, adversarial inputs and independent package | 18 pinned legacy bitstream/PCM pairs; Windows/Linux Rust tests, formatting, all-target Clippy, extracted-manifest offline tests, and ASan fuzz campaigns. [Final correction checks](tests/validation/2026-10-28-regions/validation.json). |
| Published editions, corrigendum and source provenance | [Source audit](SOURCES.md) distinguishes drafts, published previews, factual numeric sources and unofficial compatibility rules. Formal ISO certification is not asserted. |

The final package suite has **48 integration tests**, all passing on Windows
and Linux. All-target Clippy and the independently extracted offline package
tests pass. The final 84-file LAME grid and 30-file encoder PCM reruns use the
corrected executable and the unchanged RMS `2e-6` / maximum `2e-5` limits.
Counts, rates and channels must match exactly; no fitted gain, resampling or
alignment search is used. All 18 legacy vectors retain their explicitly
documented 16-bit-reference bounds and lead-in/tail rules.

## Scope and compatibility limits

Passing these checks is evidence for this implementation and corpus, not a
proof covering every possible bitstream. Formal full ISO conformance would
also require the complete published normative texts and official compliance
material; the accessed short previews and legacy 16-bit mirrors do not provide
that certification. The source audit records this distinction explicitly.

MPEG-2.5 is unofficial. Constructed mixed blocks and intensity edge cases differ
among libmad, mpg123 and Symphonia. The selected rules are documented and pass
independent mathematical expectations; universal bit-for-bit decoder agreement
is not claimed. Symphonia 0.6.1 rejects free format, so both C references cover
those files. Encoder configurations producing no audio or inconsistent Xing
counts are preserved as exclusions/rejections rather than passing inputs.

The API emits unclipped synthesis PCM, exposes header emphasis and leaves
playback filtering/output conversion to the caller. It accepts elementary MP3
streams, not Layer I/II or MPEG systems/multichannel formats. ID3 compressed or
encrypted payloads remain opaque. Tagged format changes, impossible trim and
inconsistent declared counts produce explicit errors. Strict and recovering
policies intentionally differ on damaged audio.

Earlier milestone documents and their archived failed surveys retain their
historical scope and counts. This audit, [SOURCES.md](SOURCES.md) and the final
region validation supersede their statements about unfinished decoder stages.
