# WAV implementation and evidence ledger

Scope: comprehensive independent decoding/encoding, zero production codec
dependencies, exact native samples, bounded malformed-input handling, reusable
caller buffers, zero-copy compatible PCM, streaming and sample-accurate seeking.
Only workspace registration is authorized. Existing audio decode, playback and
backends stay unchanged. Preserve unrelated work and commit each verified milestone.

1. Container and native PCM foundation (implemented, seven integration tests;
   see COVERAGE.md): RIFF/RIFX, RF64/BW64 sizes and tables,
   extensible format, native integers/floats, unknown chunk retention, boundaries.
2. Read/seek, chunk streaming and native PCM/float encoding implemented (14 total
   integration tests, debug/release and Clippy; see COVERAGE.md). Forward-only
   audio convenience and known-length nonseekable output remain. RIFF and
   64-bit output, extensible valid bits/masks, metadata and exact roundtrips.
3. Microsoft and 4-bit WAV IMA ADPCM block encoding/decoding, incremental encoding,
   borrowed range access and file-backed cached seeking are implemented. See
   ADPCM.md for 40 decode, 144 encoder and 204 file/range cases, including explicit
   reference rounding disagreements. Forward-only audio convenience, 3-bit IMA and
   extensible mappings remain. Additional encoded formats must remain possible
   without coupling container parsing to codecs; shared Muxer supports that boundary.
4. Rich metadata views and preservation: INFO, BEXT, cue/adtl, smpl, RF64 markers,
   ADM XML/CHNA, unknown chunks. Unusual ordering, large/sparse files and multichannel.
5. Differential validation against libsndfile, dr_wav, Symphonia; independent
   fixtures, malformed corpus, streaming split matrix and bounded fuzz campaigns.
6. Reproducible comparative benchmarks: open/metadata/full read/stream/seek/encode,
   CPU cycles, allocations, peak memory; optimize measured bottlenecks with parity.
   Initial resident PCM16 Hound baseline and scoped heap/Windows cycle counts are
   implemented; see PERFORMANCE.md. Wider formats, references, RSS, disks and
   platforms remain outstanding.
   Native borrowed byte-copy specialization retained after two measured runs and
   x86_64/i686 correctness checks; full PCM16 read time decreased 6.6–12.1% in
   the recorded runs. See PERFORMANCE.md for scope and raw results.
7. Completion audit against every requirement, standards coverage, performance
   results and explicit remaining limitations. Green unit tests alone do not prove
   standards coverage or competitive performance.

Record commands, source provenance and results with each implementation milestone.

Codec-independent encoded output muxing is implemented; native Writer shares the
same path. Opaque descriptors, 64-bit decoded counts and bounded size accounting
have dedicated coverage; 23 tests and a repeated PCM benchmark pass. ADPCM
encoding and compressed readers were the next implementation work at this point.

Microsoft and 4-bit IMA encoding are now implemented: block buffers, standard
format generation and incremental writer with fact-trimmed final padding. Tests
and the four-reference encoder campaign are recorded in ADPCM.md. Compressed
reader work followed below; standard variants and remaining validation/performance
work stay open.

File-backed cached ADPCM reading is now implemented and validated against the
retained corpus. All 31 integration tests pass on x86_64 (debug/release) and i686
(debug); all 204 real-file sequential/range cases pass. Forward-only convenience,
known-length nonseekable output, metadata, 3-bit/extensible ADPCM, PCM differential
coverage, fuzzing and broad performance/RSS evidence remain required.
