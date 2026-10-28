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

Remaining work: ADPCM encoding and compressed I/O convenience, structured metadata,
wavl/slnt and multiple-data handling, oracle comparisons, fuzzing, sparse >4 GiB
evidence and all performance measurements; forward-only audio convenience APIs,
known-length nonseekable encoding, and extensible compressed formats. BW64 shares 64-bit container
mechanics; ADM profile conformance is not yet claimed. RIFX extensible GUID byte
order follows the container but needs external-reference validation.
