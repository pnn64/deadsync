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

Codec-independent segmented output is now implemented as WavlMuxer. It preserves
raw descriptors and data children, accepts explicit decoded counts and silence,
supports known-length Write-only output or Write+Seek patching, and enforces
native alignment, count/size limits and I/O poisoning. Native WavlWriter uses
the same container implementation. Five tests bring the total to 83, passing
on x64 debug/release and i686 debug with Clippy. The zero-allocation probe checks
both public muxer constructors across all containers. All 708 retained native
gates, 4484 descriptor/read/range checks and 1912 encoded outputs retain their
expected hashes under the existing BW64 dummy-byte rule. Two million checked
mutations, 1580 prior corpus inputs and 100000 fresh ASan executions pass.

PERFORMANCE.md retains matching native output benchmarks, exact source snapshots
and the rejected inlining experiment. The native object size and requested heap
counts remain unchanged, but the final paired runs show slower construction and
segmented output. This regression is an explicit optimization target. Generic
encoded serialization does not complete compressed wavl decoding/seeking/forward
routing; at that milestone those paths, broader profiles and comparative performance/RSS remained open.

Borrowed compressed wavl decoding is now implemented as AdpcmWavl. It borrows
Wave's checked inventory, adds compact 16-byte bounds and reads exact ranges with
caller scratch. It decodes held frames on direct silence seeks without expanding
virtual duration. Microsoft and legacy/extensible 3/4-bit IMA pass 216 independent
fixture gates/1296 full/range checks across all containers. Ten paired flat block
component gates pass; all four references reject direct wavl input. Global fact
trimming cannot describe hidden interior per-child padding; WAVL.md makes that
producer obligation and the natural-order/playlist policy explicit.

All 87 tests pass x64 debug/release and i686 debug, plus Clippy, two million checked
mutations, 1581 earlier corpus inputs, 100000 fresh ASan executions and the retained
708-case native sample/byte replay. The new 1827-file corpus is archived. The
eight-child index probe requests one allocation/128 bytes and reads allocate
nothing; flat AdpcmData state/zero-allocation creation is preserved. Resident
Microsoft/IMA indexing, full-read and seek timings are recorded in PERFORMANCE.md
with their narrow low-entropy scope and corrected benchmark-fixture failure.
At that milestone, compressed file-backed/forward routing, cached segmented reads, broader profile
validation, the native writer slowdown and comparative performance/RSS remain open.

File-backed compressed wavl now works through AdpcmReader. It initializes frame
offsets/holds in the consumed generic inventory, adding no second index, and keys
its one-block cache by physical offset. Exact data/silence seeks, short tails,
metadata child access, poisoning and recovery pass the expanded scalar matrix
and two direct cache/virtual-offset tests. All 89 tests pass x64 debug/release and
i686 debug with Clippy; 216 independent fixtures pass 2592 borrowed/file/range
checks. All 204 prior real-file ADPCM ranges, 708 native gates and 1912 complete
writer outputs retain their hashes. Two million checked mutations, 1827 prior
corpus inputs and 100000 ASan executions pass; the new 1857-file corpus is archived.

The wrapper adds only its existing two block-buffer allocations (52 requested
bytes in the eight-child stereo probe), and later reads/seeks allocate nothing.
Standalone before/current source archives and paired flat/resident wavl timings
are recorded in PERFORMANCE.md. MS stereo full-read and several 3-bit flat rows
show regressions and remain optimization targets; overall speed parity is not
claimed. A validation-driver shared executable race was corrected with private
binary snapshots and the interrupted corpus retained. At that milestone forward segmented ADPCM,
broader profiles/fuzzing, competitive broad-format performance/RSS and both the
reader and earlier native-writer regression candidates remain unfinished.

Forward compressed wavl now works through AdpcmForwardReader. A codec-neutral
private child iterator supplies raw extents/silence counts; the wrapper validates
block geometry, accumulates capacity, holds its previous decoded frame and uses
the existing known-duration/trailing-fact contract. Metadata callbacks inspect
interleaved children, and finish drains unread physical data while skipping
virtual silence without expansion. Unrequested block headers remain undecoded;
bad geometry and inconsistent duration still fail finalization. No extra index
or held-frame allocation is added; inline x64 state grows 24 bytes and the block
allocation probe remains two calls/52 requested bytes for its stereo geometry.

All 92 tests pass x64 debug/release and i686 debug with Clippy. The independent
216-case matrix passes 3024 borrowed/file/forward/range checks, and all 432 earlier
real-file forward outputs retain their hashes. Two million checked mutations,
1857 prior corpus inputs, 100000 ASan executions and the 708-case native replay
pass. PERFORMANCE.md records archived standalone paired flat controls and current
segmented streaming/finish measurements, including run variance and no overall
speed claim. The remaining work is the full requirement audit, broader comparative
performance/RSS and profile/format coverage, and measured optimization targets;
this milestone does not prove completion of the requested WAV implementation.

The requirement audit is now in AUDIT.md. Native performance evidence expands
from PCM16 to a fixed 52-case matrix with all six widths, 1/2/6 channels,
RIFF/RIFX/RF64/BW64, reduced precision, metadata and unaligned input. Independent
truth checks precede measurement; 25 supported Hound cases match the same output.
Both before/after runs pass 34 fresh-process memory probes and 22 real-file stream
measurements. File allocation/attributes and all 64 fixture hashes are verified.
Reconstructable fixture/source archives and raw trials preserve exact inputs.

The matrix exposes a redundant in-place PCM32 integer reconstruction. Reusing
the existing native i32 read/byte-swap path removes it with no allocation or new
unsafe code. Two further alternating paired repetitions confirm 16–20-times
little-endian and 4.1–4.5-times RIFX streaming gains in the recorded cases.
All 92 tests pass x64 debug/release and i686 debug with Clippy, 708 retained native
gates/1912 writer outputs, two million checked mutations, 1839 prior corpus inputs
and 100000 ASan executions. NATIVE_PERFORMANCE.md retains Hound comparisons,
process memory and opening/encoding tradeoffs. Remaining work includes the
earlier regression candidates, wider reference/profile assessment and a final
requirement-by-requirement completion audit.


Pinned regression investigation now separates the native wavl wrapper's roughly
6-10 ns short-operation overhead from the overlapping full-stream trials. Three
attempts at reducing it have mixed costs and are reverted, with all sources and
trials retained. The shared codec-independent writer remains, with its measured
construction tradeoff documented rather than a parity claim.

The flat ADPCM control confirms a Microsoft read regression. Caching the
immutable validated block geometry removes two descriptor parses per uncached
block, retains custom coefficient borrowing, and reduces observed x64 state
344 to 336 bytes without new allocations/unsafe code. Paired flat/segmented
measurements improve Microsoft full/stream reading 5-9%; a fresh historical
control is also faster for these reads. Opening/cached-seek variance remains
explicit. A custom-predictor test brings the suite to 93, passing x64
debug/release and i686 debug with Clippy. All 216 compressed fixture gates/3024
three-API checks, 708 native gates/1912 writer outputs, two million checked
mutations, 1869 prior corpus inputs and 100000 ASan executions pass. The new
1837-file corpus is archived. PERFORMANCE.md and FUZZING.md retain details,
including the corrected native-replay executable race and failed build logs.

Next: finish the reference/profile assessment and the final requirement audit.
The measured writer construction cost is an explicit architectural tradeoff;
it is not an untested throughput optimization candidate. No completion or
platform-wide speed certification is claimed by this milestone.


The wider Rust/reference assessment now has an isolated Symphonia 0.6.1 benchmark
with its SIMD feature enabled. All 89 independently packed native/compressed
cases pass native complete/stream and 128-range gates. Symphonia matches 38
full/stream cases and 34 native range sets; compressed rounding/probe/seek errors
remain explicit. There are 48 fresh-process memory probes, 32 real cached-file
runs and a ten-case i686 subset with 24/16 memory/file runs. Published dependency
source and fixture/storage verification are retained. No production source changed.

REFERENCE_PERFORMANCE.md records native wins and compressed shortcomings. The
physical compressed-file test finds a concrete bug in performance: a seek per
uncached block discards BufReader read-ahead. A counter below that buffer records
127.94-times reads on 64 MiB encoded files. Next work is to preserve known source
position for sequential compressed reads, then address the measured i686
Microsoft arithmetic and IMA decoding costs and finish the requirements audit.


The buffered compressed-file bottleneck is now fixed. AdpcmReader tracks a known
form-relative source cursor, invalidates it before fallible audio I/O, and
retains it only after successful reads/seeks or restored metadata access. Cache hits never
pretend to consume bytes. The generic container checks full read bounds on the
positioned path too. The extra u64 restores observed x64 state 336 to 344 bytes;
block allocations and the generic Reader layout are unchanged.

The exact File probe drops 64 MiB audio reads from 8585774834 bytes/131072 seeks
to 67108864 bytes/one seek. Full reference/performance and i686-smoke reruns
retain every fixture/gate outcome; cached Microsoft and matching IMA4 files
improve 2.57/2.26 times. All 94 tests, Clippy, 216 compressed gates/3024 checks,
708 retained native gates/1912 writer outputs, two million checked mutations,
1837 prior corpus files and 100000 ASan executions pass. The new 1846-file corpus
is archived. Next work is measured compressed arithmetic optimization (especially
i686 Microsoft), assessment of native wavl file I/O, and the final requirements
audit. This milestone does not establish overall compressed-codec superiority.


The i686 Microsoft arithmetic bottleneck now has a bounded i32 loop with checked
i64 fallback. A separate i128 oracle preserves sample values, division, errors
and partial output across 2304 blocks. Paired low-entropy complete reads improve
3.08/3.11 times (mono/stereo); paired random stereo improves 2.65 times, with
2.62-times streaming improvement. The wide loop remains on x64, where the
all-target candidate showed no useful Microsoft gain. Rejected/intermediate
experiments and all control results remain archived.

All 95 tests and both-target Clippy pass. Full 89/48/32 and i686 10/24/16
resident/memory/file campaigns retain all fixture/gate outcomes; 105/18 files
pass complete hash/storage checks. All 216 compressed gates/3024 checks, 708
native gates/1912 writer outputs, two million checked mutations, 1846 prior
inputs and 100000 ASan executions pass. The verified new corpus has 1868 files.
MS_ARITHMETIC.md records details and the random-reference IMA control slowdown.
Next: assess IMA arithmetic including that control regression, assess native
wavl file I/O, and finish the original requirements audit.


The user subsequently replaced the original broad objective with completion of
WAV decoding, passing tests and accurate interoperability, explicitly removing
performance as a criterion. That goal is complete. DECODING_VALIDATION.md and
validation/decoding-final-audit.json bind the final implementation to all 95
passing tests in x64 debug/release and i686 debug, both-target Clippy, current
native/compressed replays and fuzz evidence. A fresh direct pass also passes
460 native fixture gates, all 24 four-reference core gates, 40 ADPCM arithmetic
cases and 144 encoded-file decoding cases. Known external rounding/support
differences remain explicit. No further performance work is scheduled by this
goal; earlier next-step paragraphs are historical.
