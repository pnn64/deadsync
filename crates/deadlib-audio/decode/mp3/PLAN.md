# MP3 implementation and evidence ledger

The complete requested end state is a native, independently reusable MPEG-1 and
MPEG-2 Layer III decoder and encoder, with documented MPEG-2.5 compatibility.
Only workspace registration is authorized. Existing codec integrations,
playback and backends remain unchanged. Commit every verified milestone.

1. Foundation: package registration; checked bit IO; frame sync and headers;
   MPEG CRC; side information; bounded reservoir state; ID3, Xing/Info/VBRI and
   LAME metadata framing. Exercise every version/rate/channel/bitrate and
   truncation boundary, malformed fields, free-format padding and CRC coverage.
2. Spectral decoding: normative Huffman pairs/quads and linbits; scalefactors,
   SCFSI, MPEG-2 LSF partitions and intensity parameters; requantization,
   reordering, mid/side and intensity stereo; alias reduction. Prove tables and
   bit consumption independently, including mixed/short/start/stop blocks.
3. Stateful PCM reconstruction: IMDCT, overlap-add, frequency inversion and
   polyphase synthesis, then raw frame decoding and reset. Differential PCM
   comparisons with mpg123, libmad and Symphonia; establish justified numerical
   criteria before optimizing. Valid input counts/positions/channels are exact.
4. Native file and streaming decoding: CBR/VBR/free format, synchronization and
   recovery policy, arbitrary IO splits, metadata, decoder delay, encoder
   delay/padding, gapless trimming, format changes and truncated input behavior.
5. Sample-accurate seeking: raw and audible coordinates, reservoir dependencies,
   hybrid/synthesis history, VBR/Xing/VBRI hints and exact reference positions.
   Every seek must reproduce corresponding linear PCM within the documented
   tolerance, without timing drift. Include first, repeated and distant seeks.
6. Conformance and adversarial validation: independent MPEG fixture corpus,
   all three reference decoders, malformed/truncated streams, reservoir edges,
   resets, fuzzing and sanitizer campaigns, reproducible hashes and compiler
   flags. Published-standard edition/corrigendum reconciliation is required.
7. Native encoding after decoding correctness: analysis filterbank, forward
   MDCT and block switching; psychoacoustic analysis; quantization and bit
   allocation; Huffman selection/coding and reservoir scheduling; bitrate and
   quality modes; CBR/VBR/free-format writing and gapless finalization. Decode
   native files independently and compare quality/size/throughput against LAME.
8. Measured optimization: bit parsing, Huffman, requantization, IMDCT, synthesis
   and full pipelines; SIMD only after correctness and demonstrated gains.
   Benchmark whole files, streaming, startup, cold/warm seeks, encoding quality,
   CPU cycles, allocations and memory against optimized reference builds.
9. Independent package/extracted-manifest build, tests and benchmarks. Document
   coverage, numeric parity, compatibility, perf results and every remaining
   limitation. Audit every item against authoritative current evidence before
   declaring the complete goal achieved.

Passing a primitive test does not establish completion of its parent item.
The goal remains active until decoding, encoding and their complete validation
and performance requirements are implemented and verified.

Current checkpoint: foundation implemented; eight integration tests, all-target
Clippy and 84 LAME/libmad structural comparisons (732 frames) pass. Evidence and
explicit limitations are in COVERAGE.md. Scalefactor parsing now passes all
compression/shape cases and the generated-file part2 bounds gate. Huffman pairs,
escapes and quadruples now pass exact native tests and 22,476 independent
libmad/analytical hybrid vectors; see HUFFMAN.md. Hybrid and polyphase kernels
now pass a 2,880-frame all-frequency libmad comparison; see SYNTHESIS.md.
Remaining spectral processing and complete framed PCM composition are next.

## Source material

Implementation derives from the MPEG syntax and mathematical algorithms, not
from translation of another codec. Normative numeric tables will have explicit
provenance and exhaustive prefix/roundtrip checks. Local source PDFs and external
media stay in ignored `target/deadlib-mp3-reference/` storage.

- ISO/IEC 11172-3 MPEG-1: [public DIS text mirror](https://ossrs.net/lts/zh-cn/assets/files/ISO_IEC_11172-3-MP3-1993-7e218e3ec360c25149d36152c4e02f08.pdf).
  This accessed copy is a draft, despite its filename. Reconcile the published
  edition and independent conformance vectors before claiming conformance.
- ISO/IEC 13818-3 MPEG-2 LSF: [1997 text mirror](https://ossrs.net/lts/zh-cn/assets/files/ISO_IEC_13818-3-MP3-1997-8bbd47f7cd4e0325f23b9473f6932fa1.pdf).
- [ID3v2.4 structure](https://id3.org/id3v2.4.0-structure), plus corresponding
  [v2.3](https://id3.org/id3v2.3.0) and [v2.2](https://id3.org/id3v2-00) specifications.
- [LAME technical FAQ](https://lame.sourceforge.io/tech-FAQ.txt) and its tag
  format are the source for encoder-delay metadata compatibility.
- [mpg123 compliance description](https://www.mpg123.org/) and
  [Underbit compliance methodology](https://www.underbit.com/resources/mpeg/audio/compliance/)
  identify independent numerical/conformance fixtures; no corpus completion
  claim is made until the actual fixtures and outputs are checked and pinned.
