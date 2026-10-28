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

Strict structural errors: duplicate fmt/fact, missing fmt/data, partial PCM frames,
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

Remaining work: forward-only audio convenience, metadata profile/semantic validation,
wavl/slnt and multiple-data handling, oracle comparisons, broader fuzzing, sparse >4 GiB
evidence and broad performance coverage (the first PCM16 baseline is in
[PERFORMANCE.md](PERFORMANCE.md)); forward-only audio convenience APIs,
and extensible compressed formats. BW64 shares 64-bit container
mechanics; ADM profile conformance is not yet claimed. RIFX extensible GUID byte
order follows the container but needs external-reference validation.
