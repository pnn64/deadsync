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
   audio convenience is now implemented for native formats and supported ADPCM.
   Known-length nonseekable output is now implemented
   for generic encoded data, native samples and ADPCM; see STREAMING.md. RIFF and
   64-bit output, extensible valid bits/masks, metadata and exact roundtrips.
   Native wavl/slnt sequences now work through borrowed, seekable and forward
   frame APIs plus incremental/known-length WavlWriter; see WAVL.md.
3. Microsoft and 3/4-bit WAV IMA ADPCM block encoding/decoding, incremental encoding,
   borrowed range access and file-backed cached seeking are implemented. See
   ADPCM.md for 40 decode, 144 encoder and 204 file/range cases, including explicit
   reference rounding disagreements. Forward-only reading is implemented with exact
   known-duration trimming; see STREAMING.md for trailing-fact handling. Extensible
   compressed mappings remain. Additional encoded formats must remain possible
   without coupling container parsing to codecs; shared Muxer supports that boundary.
4. Rich metadata views and preservation: INFO, BEXT, cue/adtl, smpl, RF64 markers,
   ADM XML/CHNA, unknown chunks. Unusual ordering, large/sparse files and multichannel.
   Borrowed views and caller-buffer serialization are implemented (41 total tests,
   allocator probe and external metadata fixtures); see METADATA.md. Full broadcast
   profile/ADM semantics, XML compression and broader metadata benchmarks remain open.
   A resident cue/INFO comparison with dr_wav is recorded in PERFORMANCE.md;
   open+walk wins in the measured cases, retained-view walks remain slower.
   A measured native-word NUL scan reduces large open+walk time by 33–40% and
   retained walks by 41–49% across two paired runs; all 44 tests pass on x64
   debug/release and i686 debug. Retained dr_wav walks still win in the repeat.
5. Differential validation against libsndfile, dr_wav, Symphonia; independent
   fixtures, malformed corpus, streaming split matrix and bounded fuzz campaigns.
   Four million checked deterministic mutations pass across x64/i686, plus two
   million x64 libFuzzer/AddressSanitizer executions. The shared harness checks
   container/stream/seek agreement and metadata/codec state properties. See
   FUZZING.md for hashes, limits and remaining coverage; this is not exhaustive.
   After the metadata optimization, the checked campaigns are repeated, all 1474
   retained corpus files replay, and 500000 fresh ASan executions pass. Both
   coverage-generated corpora are archived with verified manifests.
6. Reproducible comparative benchmarks: open/metadata/full read/stream/seek/encode,
   CPU cycles, allocations, peak memory; optimize measured bottlenecks with parity.
   Initial resident PCM16 Hound baseline and scoped heap/Windows cycle counts are
   implemented; see PERFORMANCE.md. Wider formats, references, RSS, disks and
   platforms remain outstanding. Physical NTFS RF64/BW64 files over 16 GiB now
   pass x64/i686 random-seek checks with whole-process working-set snapshots;
   see LARGE_FILES.md. Comparative RSS and sequential disk throughput remain open.
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

File-backed cached ADPCM reading passes 204 real-file sequential/range cases.
Forward-only native/ADPCM reading and known-length Write-only output are now
implemented; see STREAMING.md for exact ordering/duration/finalization contracts.
All 56 integration tests pass on x64 debug/release and i686 debug, and all 432
forward real-file reference cases pass. The latest checked mutation and ASan
campaigns also cover these new paths, with archived corpus bytes and hashes.

Native PCM/float differential coverage now passes 460 independent fixture gates,
2748 descriptor/decode/range checks and 920 typed encoder cases. The predeclared
24 basic nonempty RIFF cases agree with dr_wav, libsndfile, Symphonia and FFmpeg.
Canonical/compatibility profiles, reference errors and bit differences, and a
corrected optional-metadata adapter are retained in PCM.md and raw archives.
Physical sparse files over 16 GiB and 2^32 decoded frames pass all 24 x64/i686
checks, including actual byte/frame boundary markers and bounded process working
sets. LARGE_FILES.md records the exact scope; no comparative RSS claim is made.

Three-bit WAV IMA now shares all block, reader and writer APIs. Its 122 independent
fixture/encoder gates and 732 decoder/range checks pass, as do 44 predeclared
independent FFmpeg packing checks under its different reconstruction rounding.
ADPCM.md retains every reference outcome and lossy quality limits. All 60 tests,
Clippy, two million checked mutations, earlier corpus replay and 100000 fresh
ASan executions pass. The 204 earlier Microsoft/4-bit IMA range checks also pass.

Native wavl/slnt and multiple data children now pass 248 independent fixture/layout
gates, 1736 descriptor/decode/range checks and 992 writer cases. All four reference
drivers reject the segmented corpus directly; 24 paired flat RIFF gates match
all four, and every direct failure is retained. All 69 tests pass on x64
debug/release and i686 debug; Clippy, two million checked mutations, 100000 fresh
ASan executions and 2507 unique retained corpus files pass. The earlier 460 native
and 204 ADPCM file/range regressions also pass. WAVL.md records u64 silence/physical
offset evidence, the Option-based contiguous accessors, metadata and limits.

The compact PCM view halves x64 state from 128 to 64 bytes and reduces retained
view construction time about 29% in two paired runs. Full-format queries cost
23–25 ns instead of 10 ns; PERFORMANCE.md retains that tradeoff, every trial and
the reverted shared-budget experiment. All 708 retained native fixture gates,
4484 descriptor/read/range checks and 1912 exact encoded outputs pass. All 69
tests pass on x64 debug/release and i686 debug; Clippy, two million checked
mutations, 2507 earlier corpus inputs and 100000 fresh ASan executions pass too.

Required work still includes unresolved extensible compressed mappings, compressed wavl routing,
representative comparative benchmarks, comparative RSS and sequential
disk evidence, and broader native/reference fixtures. Metadata profile/semantic certification
and broader fuzz coverage remain unproven. Forward PCM16 streaming currently
allocates one 16-byte format buffer; the benchmark records that boundary.

RF64 forward duration handling now respects the ordinary fact count over unused
ds64 sampleCount. Native wavl queries wait for fact or the observed boundary;
explicit ADPCM counts defer unknown fact validation to finish. The inferred
ADPCM constructor still offers a provisional ds64 fallback and rejects a later
conflicting fact. Two regressions cover leading/trailing/sentinel counts, ignored
raw values and exact output. The new 40-seed duration matrix joins the shared
property harness. STREAMING.md records these API contracts.
All 71 tests pass on x64 debug/release and i686 debug with Clippy. All 708 native
fixture gates and 432 retained forward ADPCM outputs pass. Two million checked
mutations, 1443 prior corpus inputs and 100000 fresh ASan executions pass; the
1414-file generated corpus and all hashes/logs are preserved in validation.

The BW64 dummy-field correction is now implemented across readers and writers.
Raw words remain inspectable, but never determine decoded duration. All output
zeros them; compatibility fact counts above u32::MAX are rejected, while native
contiguous PCM keeps u64 frame counts. BW64.md records the standard distinction,
remaining profile requirements and historical compatibility boundaries. Three
regressions fail on the previous commit and pass now; all 74 tests pass on x64
debug/release and i686 debug with Clippy. All 708 native gates, 4484 descriptor/
read/range checks, 1912 writer outputs under the exact dummy-byte transformation,
432 retained forward ADPCM outputs and 24 updated physical sparse-file probes
pass. Two million checked mutations, 1414 prior corpus files and 100000 fresh
ASan executions pass. Normative BW64 profile/ADM validation remains open.

The extensible IMA GUID/samples-per-block mapping is now implemented for 3/4-bit
blocks across all reader APIs, with a caller-buffer descriptor serializer and
effective force_extensible output. Four tests bring the total to 78, passing
on x64 debug/release and i686 debug with Clippy. All 416 independent fixture/
encoder gates and 2784 decoder/range checks pass, including 40 direct dr_wav
4-bit RIFF gates and 40 paired legacy FFmpeg 3-bit packing gates. The exact prior
driver rejects 256 newly supported leading-fact cases. All 432 earlier forward
outputs, 204 earlier file/range checks, 24 updated physical sparse-file probes,
two million checked mutations, 1490 prior corpus inputs and 100000 fresh ASan
executions pass.

IMA_EXTENSIBLE.md records the generic Microsoft field rules used for this
interpretation and the narrow direct-reference support. The Microsoft ADPCM
coefficient placement in an extensible header was not established; forcing it
now fails explicitly before I/O instead of silently emitting a legacy header.
Unknown raw descriptors remain available through the independent container.
Compressed wavl routing, broader extensible variants/profile validation and the
comparative performance/memory requirements remain unfinished.
