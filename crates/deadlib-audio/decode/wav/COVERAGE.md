# Standards and validation coverage

Authoritative sources consulted 2026-10-28:

- [Microsoft RIFF structure](https://learn.microsoft.com/en-us/windows/win32/xaudio2/resource-interchange-file-format--riff-)
- [Microsoft WAVEFORMATEXTENSIBLE](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible)
- [Microsoft extensible format descriptors](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/extensible-wave-format-descriptors)
- [EBU Tech 3306 v1.1, RF64](https://tech.ebu.ch/files/live/sites/tech/files/shared/tech/tech3306v1_1.pdf)
- [ITU-R BS.2088 current recommendation](https://www.itu.int/rec/R-REC-BS.2088/en)

Foundation implements checked RIFF/RIFX boundaries and odd padding, RF64/BW64
ds64 parsing and duplicate-ID size table consumption, arbitrary nonzero channel
counts within block/byte-rate field limits, PCM 8/16/24/32 and IEEE float 32/64,
extensible GUID dispatch, masks and valid-bit depth. Encoded format tags and
vendor GUIDs are retained without requiring their decoders. Unknown chunks remain
borrowed and exact original bytes are available. No whole-audio allocation.

Strict structural errors: duplicate fmt/fact, missing fmt/audio, partial PCM frames,
invalid block/byte rates, invalid valid-bit depth, mask count mismatch, truncated
payloads/padding, missing ds64 table entries. Nonzero pad bytes are accepted and
retained. Trailing bytes outside the RIFF form are deliberately ignored.

Foundation validation (Windows x86_64, rustc 1.99.1, 2026-10-28):
`cargo test -p deadlib-audio-decode-wav --target-dir target/wav` passes seven
integration tests. Fixtures are assembled independently of the production parser;
they cover integer extrema in both byte orders, float NaN payloads (including
signaling patterns), signed zeros/subnormals, 20-in-24 extensible and 1000-channel
formats, unusual chunk order, exact unknown/padding retention, same-ID ds64
entries, every truncation prefix of RIFF/RF64/BW64 fixtures, header mutations,
resource limits, frame ranges, whole-frame caller buffers and direct-pointer
identity across alignment offsets. `cargo clippy -p deadlib-audio-decode-wav
--all-targets --target-dir target/wav -- -D warnings` passes.

## Native I/O and encoding milestone

`Stream` implements forward-only chunk payload reads, bounded size tables and
strict chunk boundaries. `Reader` scans seekable inputs without reading audio,
exposes metadata descriptors and sample-accurate native PCM/float seeks, and reads
directly into caller bytes or numeric storage. Packed integer expansion reuses
the output allocation. Writer supports native raw/typed samples and explicit
integer packing, metadata payloads, extensible formats, RIFX, explicit RF64/BW64,
and automatic RIFF-to-RF64 promotion. Finalization patches ds64/data/fact/RIFF
sizes. Low unused valid-bit padding is retained rather than canonicalized.

Validation 2026-10-28: 14 integration tests pass in debug and release profiles
on Windows x86_64; Clippy with `-D warnings` also passes. Commands are the foundation
commands above plus `cargo test -p deadlib-audio-decode-wav --release --target-dir
target/wav`. New tests cover all container/integer combinations, exact float bits,
typed I/O, odd data/metadata pads, writing at a nonzero base offset, 4000-channel
frames larger than stack scratch, full truncation matrices, short and interrupted
reads (maximum read sizes 1 through 32), short writes, I/O failure recovery/poisoning,
and a sparse virtual source crossing 4 GiB with a nonzero final sample. That last
test proves 64-bit arithmetic and seeking without large memory/disk use; it does
not establish real-filesystem or physical large-file throughput.

Microsoft and 4-bit IMA ADPCM block decoding and borrowed exact range access are
implemented and differentially exercised; see [ADPCM.md](ADPCM.md) and the retained
JSON reports. Total integration tests: 20. This does not complete compressed
encoding or all standard variants.

Codec-independent `Muxer` now handles arbitrary encoded descriptors and decoded
frame counts; native Writer shares it. Three added tests prove opaque descriptor
retention, decoded-count RF64 promotion and count overflow rejection, as well as
native byte/frame consistency. Total tests: 23; debug and Clippy pass. The retained
`validation/pcm16-generic-mux.*` benchmark has median native encoding 0.296 ms
and zero scoped heap allocation, within prior run variation. No encoding-speed
improvement is claimed from this architecture change.

Microsoft and 4-bit IMA encoding now support allocation-free block calls and a
bounded-buffer streaming writer. All 144 encoder differential fixture sets match
the specified-arithmetic references and exact fact lengths; each other reference
disagreement is retained. See ADPCM.md for scope and lossy quality limitations.
Total crate integration tests: 27.

File-backed `AdpcmReader` now adds cached block decoding and exact 64-bit frame
seeks. Four integration tests exercise arbitrary ranges, metadata, order, errors,
short blocks and >4 GiB virtual sources; 204 hash-checked real-file sequential/
range cases match retained oracle evidence. All 31 integration tests pass on
Windows x86_64 debug/release and i686 debug, and Clippy passes. Normal Cargo edges
still show no production dependencies. See ADPCM.md for commands and limitations.

Explicit metadata views and caller-buffer serializers now cover LIST/INFO/adtl,
cue, smpl, BEXT v0/v1/v2, RF64 markers and BW64 CHNA/XML envelopes. Ten tests add
independent layouts, malformed/truncated counts and zero-heap allocation probes;
all 41 debug tests and Clippy pass. The metadata oracle matches independent field
truth for all 12 fixtures and dr_wav for nine, with trailing-RF64 differences
retained. See [METADATA.md](METADATA.md) for precise scope and limitations.

Bounded fuzzing now adds four million checked deterministic mutations on x64/i686
and two million x64 libFuzzer/AddressSanitizer executions with no failures. All
42 integration tests pass in debug; the new mutation test also passes in release
and i686, supplementing the 41-test release/i686 runs. See FUZZING.md for exact
properties, seed provenance, hashes, raw logs and resource/coverage limits.

A safe native-word NUL scan is retained after component benchmarks and two paired
metadata runs. Large open+walk time falls 33–40%, retained walks 41–49%; dr_wav
retained walking still wins in the final repeat. All 44 tests pass on x64
debug/release and i686 debug, and Clippy passes. PERFORMANCE.md records raw
trials, requested heap counts, CPU cycles and the precise scope of those results.
The optimized source also passes repeated checked mutation campaigns, all 1474
retained corpus inputs, 500000 fresh ASan executions, and the metadata oracle
campaign. Verified corpus archives and raw reports are preserved; see FUZZING.md.

Known-length Write-only output now shares the native/generic muxer and ADPCM
encoder paths. Five tests add native bit/layout parity, compressed final padding,
declared-count validation, short/failing I/O and 64-bit preflight boundaries.
All 49 integration tests pass in x64 release and i686 debug, and Clippy passes.
Two million checked mutations cover the new known-length output comparisons.
STREAMING.md documents the API and PERFORMANCE.md records scoped heap/cycle data.

Forward-only native/ADPCM readers now add bounded ancillary callbacks, exact known
duration trimming, large-frame buffers, raw encoded access and explicit final
container validation. All 56 tests pass in x64 debug/release and i686 debug, and
Clippy passes. All 432 Read-only real-file ADPCM cases match the retained reference
truth. Two million checked mutations, 1474 earlier corpus inputs and 100000 fresh
ASan executions pass with the new forward state properties. STREAMING.md and
FUZZING.md document ordering/duration requirements, source hashes and evidence.

Native sample differential validation now passes 460 independent fixture gates
across RIFF/RIFX/RF64/BW64, six native formats, 1/2/6/33 channels, valid-bit layouts,
odd metadata and explicit compatibility cases. All 2748 descriptor/decode/range
checks and 920 typed encoder cases pass. All four references agree on the 24
predeclared basic nonempty RIFF gates; every wider disagreement is retained.
libsndfile independently matches RIFX extensible sample bits and valid-bit layouts;
FFmpeg adds 104 exact BW64 cases. See [PCM.md](PCM.md) for canonical padding,
reference limitations, raw reports/corpora and the corrected optional-width adapter.

Physical NTFS sparse RF64/BW64 files now exercise real offsets above 16 GiB and
decoded counts above 2^32 on x64 and i686. All 24 fresh-process checks pass;
independent Python marker checks and whole-process working-set evidence accompany
the source/binary hashes. See [LARGE_FILES.md](LARGE_FILES.md) for geometry,
reproduction, memory scope and remaining disk/platform limitations.

Standard 3-bit WAV IMA encoding/decoding now works through every block, borrowed,
seekable, forward and known-length writer API. All 122 independent fixture/encoder
gates and 732 decode/range checks pass; 44 predeclared FFmpeg packing checks agree
under its distinct reconstruction rounding. The full 60-test suite passes on
x64 debug/release and i686 debug, along with Clippy, two million checked mutations,
1474 earlier corpus inputs and 100000 fresh ASan executions. All 204 earlier
Microsoft/4-bit IMA file/range cases retain their decoded hashes. See ADPCM.md
for reference errors, exact scope and lossy encoder quality limitations.

Native wavl/slnt now works through borrowed, seekable and forward frame APIs and
incremental/known-length WavlWriter. All 248 independent fixture/layout gates,
1736 descriptor/read/range checks and 992 writer cases pass. Direct reference
drivers reject wavl here; all 24 predeclared paired flat RIFF gates agree with
all four readers. WAVL.md distinguishes that indirect evidence from direct parity.
Nine tests add native raw truth, NaN/held-frame precision, metadata, malformed
boundaries, I/O failures, huge virtual silence and >4 GiB physical-offset seeks.
All 69 tests pass on x64 debug/release and i686 debug, with Clippy, two million
checked mutations, 100000 fresh ASan executions and 2507 earlier corpus files.
All 460 ordinary native fixtures and 204 ADPCM file/range regressions also pass.

The compact native view retains identical established native truth: the 708-case
flat/wavl replay passes 4484 descriptor/read/range checks and 1912 exact encoded
outputs against immutable earlier reports and fixture archives. The full 69-test
suite passes on x64 debug/release and i686 debug; Clippy, two million checked
mutations, 2507 prior corpus inputs and 100000 fresh ASan executions pass.
`validation/compact-*` records source/binary hashes, logs and the new corpus.
PERFORMANCE.md records the paired size/construction measurements and slower
on-demand full-format queries. External references were not rerun for this replay.

RF64 forward duration handling now distinguishes an ordinary fact from an unused
raw ds64 sampleCount. Two regressions first fail against `76a01fa6d`, then pass
with the correction; `validation/rf64-duration-before*` preserves those failures
and test hashes. Leading/trailing fact, sentinel replacement, arbitrary raw
values, decoded trimming, observed native length and final mismatch rejection
are covered. The full 71-test suite passes on x64 debug/release and i686 debug,
and Clippy passes. The 708 native gates/4484 descriptor-read-range checks/1912
exact writer outputs and all 432 retained forward ADPCM outputs pass too.
Two million checked mutations over 358 seeds, 1443 earlier corpus files and
100000 fresh ASan executions pass (`validation/rf64-count-*`). BW64 cases in this
matrix retain the existing RF64-style compatibility behavior; PLAN.md records
the separately identified normative BW64 dummy-field gap.

BW64 dummy words now remain raw inspection data and never supply decoded counts;
all writers emit zero there. Three regressions fail on `158ea60ea` and pass now.
The 476 archived prior BW64 outputs match immutable earlier hashes before the
independent eight-byte zeroing rule is applied. All 1912 complete writer outputs,
708 native fixture gates and 4484 descriptor/read/range checks pass under that
rule, with every other byte/container unchanged. All 74 tests pass on x64
debug/release and i686 debug, with Clippy, 432 forward ADPCM outputs, two million
checked mutations, 1414 prior corpus files and 100000 fresh ASan executions.
All 24 updated physical sparse-file probes pass with zero BW64 dummy words;
the BW64 IMA large probe uses literal u32::MAX fact rather than the former
RF64-only count extension. See BW64.md for exact compatibility/profile scope
and `validation/bw64-dummy-*` for hashes, reports and raw logs.

Remaining work: metadata profile/semantic validation, compressed wavl routing,
broader fuzzing and representative comparative performance/RSS coverage
(the first PCM16 baseline is in [PERFORMANCE.md](PERFORMANCE.md)); extensible
compressed formats. BW64 shares 64-bit container mechanics; ADM profile
conformance is not yet claimed. These fixture results do not certify every
standard variant or workload.
