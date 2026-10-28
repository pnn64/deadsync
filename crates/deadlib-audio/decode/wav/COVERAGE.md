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

Remaining work: encoding, streaming/read+seek APIs, ADPCM, structured metadata,
wavl/slnt and multiple-data handling, oracle comparisons, fuzzing, sparse >4 GiB
evidence and all performance measurements. BW64 currently shares 64-bit container
mechanics; ADM profile conformance is not yet claimed. RIFX extensible GUID byte
order follows the container but needs external-reference validation.
