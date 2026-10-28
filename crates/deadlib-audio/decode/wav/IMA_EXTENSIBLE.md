# Extensible IMA descriptors and validation

The IMA block codec now recognizes the WAVEFORMATEXTENSIBLE GUID corresponding
to tag 0x0011. The Samples union supplies the block's frame count; it is not a
PCM precision value. Three-bit and four-bit IMA retain their existing code widths,
word packing, arithmetic and geometry validation. Container parsing remains
independent of the codec and preserves the complete raw descriptor.

[Microsoft's WAVEFORMATEXTENSIBLE description](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatextensible)
defines the compressed samples-per-block union, channel mask and tag/GUID mapping.
[WAVEFORMATEX](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex)
describes format-specific data following an extensible header. Applying those
general fields to IMA's existing block geometry is an interpretation; this does
not establish acceptance by every Windows audio API or certify every compressed
extensible profile. The nibble-width file mapping has direct 4-bit reader evidence
below. SDL also implements the IMA union mapping, while
[explicitly declining Microsoft ADPCM's unspecified coefficient placement](https://github.com/libsdl-org/SDL/blob/main/src/audio/SDL_wave.c).
An authoritative Microsoft coefficient layout was not located during this audit;
that remains unresolved, rather than defining a new private layout here.

`AdpcmSpec::format_extensible(endian, channel_mask, output)` emits a 40-byte IMA
descriptor into caller storage. It supports 3/4-bit geometry, both byte orders,
matching speaker masks, zero/unspecified masks and SPEAKER_ALL. Invalid arguments
leave the output unchanged. Use the descriptor with `Adpcm::encode_block` and
the codec-independent Muxer when a specific speaker mask is needed.

`AdpcmWriter` now honors `WriteOptions::force_extensible` for IMA, using an
unspecified mask. Both seekable and known-length output use this descriptor;
encoded block bytes and lossy quantizer decisions remain unchanged. The flag
previously had no effect in this writer. Requesting it for Microsoft ADPCM now
returns Unsupported before sink I/O. Ordinary Microsoft descriptors and their
coefficient tables retain the existing block/reader/writer paths. Format.valid_bits
is documented as the raw Samples word for an extensible block codec; native PCM
precision semantics remain unchanged.

## Evidence

Four tests add independently packed golden frames, all reader APIs, exact
descriptor bytes, writer parity, mask/union/GUID/geometry failures, opaque trailing
format data and no-I/O failure for the unresolved Microsoft mapping. The full
78-test suite passes on x64 debug/release and i686 debug, with denied-warning
Clippy. `validation/ima-extensible-checks*` records exact source hashes and logs.

The independent Python corpus has 320 fixture cases: RIFF/RIFX/RF64/BW64, 3/4-bit
IMA, 1/2/6/33 channels, masks, empty/trimmed/cross-block audio, complete short final
blocks, and leading/trailing fact. A separate 96-case encoder bank covers both
finalization paths, constant/ramp/noise input and 1/129-frame lengths. Header
bytes are checked independently, and the scalar oracle deinterleaves channel
words into integer bitstreams before reconstructing samples. It does not call
the Rust codec or any reference decoder to produce expected samples.

All 416 gates and 2784 borrowed/seekable/forward/range checks pass. Forty
predeclared nonempty RIFF mono/stereo 4-bit cases match dr_wav directly. Forty
paired legacy-header 3-bit cases match FFmpeg's separately specified combined
rounding; those paired cases validate packing/arithmetic, not extensible-header
acceptance. Both encoder finalization paths produce identical encoded blocks.
The recorded lossy error measurements do not imply an encoder-quality improvement.

| Direct extensible input, 208 cases per width | 3-bit successful exits | 3-bit exact spec prefixes | 4-bit successful exits | 4-bit exact spec prefixes |
| --- | ---: | ---: | ---: | ---: |
| dr_wav | 96 | 28 | 96 | 64 |
| Symphonia | 0 | 0 | 0 | 0 |
| libsndfile | 0 | 0 | 0 | 0 |
| FFmpeg | 0 | 0 | 0 | 0 |

Each report retains full lengths/hashes, exit codes, fact-limited prefixes,
first differences and errors. Eight of the dr_wav exact prefixes at each width
are empty audio. Its other 3-bit coincidences contain only the initial frame;
they do not demonstrate general 3-bit decoding. Nonempty exact 4-bit cases here
are mono/stereo RIFF and RF64. No direct external parity is claimed for 3-bit
extensible RIFX or for more than two channels. Symphonia rejects non-byte-aligned
extensible coded widths (or the container probe); libsndfile reports an
unimplemented format. FFmpeg reports a required channel-layout error even on the
explicit stereo-mask fixture. These are observed outcomes, not inferred proof
that every reader or specification rejects the mapping.

`ima-extensible-references-final.*` records 256 valid leading-fact inputs rejected
by the exact previous forward-driver binary, checked against the retained
`bw64-dummy-adpcm-forward.json` binary hash. Its raw tool snapshot is preserved
as `ima-extensible-initial-reference-tool.py.txt`. The initial run before a
formatting-only driver change is also retained with its exact driver snapshot.
`ima-extensible-references-repro.*` repeats the complete matrix with the current
tool and no dependency on that prior executable. Every fixture and encoded output
is archived with hashes; no third-party codec source is embedded.

```sh
python crates/deadlib-audio/decode/wav/tests/reference_ima_extensible.py --output target/wav/new-ima-extensible.json
```

The runner uses the pinned validation-only references described in ADPCM.md and
refuses existing output destinations. All 432 earlier forward ADPCM outputs and
204 earlier file/range checks pass unchanged. All 24 physical sparse-file probes
pass with the newly effective extensible flag, including real offsets above
16 GiB on x64/i686; LARGE_FILES.md
records geometry and whole-process memory scope. Two million checked mutations,
1490 prior corpus inputs and 100000 fresh ASan executions pass; FUZZING.md records
limits and the complete preserved corpus. Broader comparative ADPCM performance,
additional extensible mappings and forward compressed wavl routing remain open.
Borrowed and file-backed segmented IMA now work, including this extensible
mapping; WAVL.md records their independent fixture and cache/seek evidence.
