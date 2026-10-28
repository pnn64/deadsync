# Generated, redistributable fixtures

`truncated-first-long-v1.ogg`, `.bin` and `.out` retain the procedural 33-mode,
long-first residue-zero stream, 17 repeated seek requests and the unmodified
libvorbisfile response for failed first-packet track-only preroll. They are
generated tones, not third-party media. `reference_seeking.rs` regenerates the
case and recaptures the C response when `DEADLIB_VORBIS_REFERENCE_SEEK_INPUTS`
names an ignored scratch directory. Ordinary offline tests only read these
fixtures. The milestone validation manifest records their hashes and the C
oracle/source provenance; see [reference seeking](../../REFERENCE_SEEKING.md).

These files contain locally generated tones/silence, not third-party music.
The original four Ogg fixtures were generated with FFmpeg's libvorbis encoder. They exercise setup parsing and
provide independently decodable fixtures for the PCM differential suite.
Ogg stream serials are arbitrary and may change when regenerated.

```powershell
ffmpeg -f lavfi -i 'aevalsrc=0.25*sin(2*PI*440*t)|0.125*sin(2*PI*997*t):s=48000:d=0.08' -c:a libvorbis -q:a 3 -metadata TITLE=NativeVorbisHeaderFixture -y stereo.ogg
ffmpeg -f lavfi -i 'anullsrc=r=44100:cl=7.1' -t 0.02 -c:a libvorbis -q:a 1 -y eight-channel.ogg
ffmpeg -f lavfi -i 'aevalsrc=0.1*sin(2*PI*211*t)|0.1*sin(2*PI*337*t)|0.1*sin(2*PI*463*t)|0.1*sin(2*PI*71*t)|0.1*sin(2*PI*587*t)|0.1*sin(2*PI*709*t)|0.1*sin(2*PI*829*t)|0.1*sin(2*PI*947*t):c=7.1:s=48000:d=0.06' -c:a libvorbis -q:a 3 -y surround.ogg
ffmpeg -f lavfi -i 'aevalsrc=0.3*sin(2*PI*997*t):s=8000:d=0.08' -c:a libvorbis -q:a 0 -y mono.ogg
```

`surround.libvorbis-pcm` is a raw-packet PCM protocol captured from the pinned
libvorbis 1.3.7/libogg 1.3.6 oracle, with no granule/EOS trimming. It contains the
channel count followed by `P`/block-size/frame-count/planar-f32 records, matching
the `pcm` mode of `tests/oracle/oracle.c`. SHA-256:
`8da9dd588870c8fd09c670b4c08bc6fc7e76db45afb706178fb233d3ddcb107e`.
This golden makes the dependent eight-channel coupling case testable without C
tools; Symphonia 0.6.1 has an explicitly tested forward-coupling discrepancy.

`discarded-window-v1.libvorbis-pcm` captures the pinned oracle's `pcm` mode for
`stereo.ogg` with its second audio packet (raw packet index four) replaced by
an empty packet. All other packets stay in order. It checks empty/non-audio
recovery without C tools. SHA-256:
`4dcfe5c6f93fa8a05bef655ccf779b2fb16558166cd8800ffe692185b0a7c965`.
The [capture provenance](../../validation/2026-10-27-concealment-golden-capture.json)
retains input/oracle hashes; see [the parity ledger](../../PARITY.md).

Regenerate only through the external oracle (from the repository root):

```powershell
$env:DEADLIB_VORBIS_GOLDEN_OUTPUT = "$pwd/crates/deadlib-audio/decode/vorbis/tests/fixtures/surround.libvorbis-pcm"
cargo test -p deadlib-audio-decode-vorbis --release --test pcm regenerate_surround_pcm_golden -- --ignored
```

Set `DEADLIB_LIBVORBIS_ORACLE` as described in the oracle README. The regeneration
test writes only when explicitly invoked with that output path; ordinary tests
never update the golden.

`spanning-prime-v1.ogg` preserves native encoder version `0e5ecb23d` output:
8 channels, 48 kHz, N8192, four residue stages, 16,401 frames, serial 0x7788.
Its deterministic tones/noise/impulses and duplicate/binary comments use the
`signal` / `comments` helpers in `tests/encoding.rs` at that revision. Windows
x86-64 rustc 1.99.1 generated all six audio packets at 65,810 bytes each, forcing
the priming packet across physical pages. SHA-256:
`e1dd28e18c307f3cb3e3b553da5f6d376c3d0e3cb507738d31ce18b63f83c2c0`.
The fixture is kept independent of current encoder output so entropy improvements
cannot accidentally remove the libvorbisfile 1.3.7 PCM-seek-zero regression's
trigger. Native and Symphonia decode all declared frames; the named C regression
checks both raw-start full PCM and the PCM-seek path's lost first block.

`synthetic-setup-v1.libvorbis-pcm` retains eight procedural, non-silent setup
regressions through the pinned libvorbis oracle. It covers an unused mode,
surplus classwords and stage-free submaps for all residue types, and an
eight-stage cancellation case. The format is `DVSSET01`, a little-endian u32
set count, then u64-length-delimited `pcm-cases` responses. The deterministic
inputs are constructed by `tests/support/synthetic.rs` and `golden_cases` in
`tests/configuration_parity.rs`; no native encoder generates them. Size:
13,836 bytes; SHA-256:
`516245b6fb4f166abb48bf30df2bd3da5ea5fcee96115c0893f5891e7e9fd553`.
Regenerate explicitly with the oracle configured:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --test configuration_parity regenerate_synthetic_setup_golden -- --ignored
```

Ordinary tests only read this golden. [Configuration validation](../../CONFIGURATION_PARITY.md)
records capture and source provenance.

`residue-layout-v1.libvorbis-pcm` contains four C responses for unaligned type-two
residue begin/partition ranges, including mixed stage books, coupling and multiple
submaps. The format is `DVRGEO01`, little-endian u32 set count, then u64-length-
delimited `pcm-cases` responses. `regressions` / `sequence` in
`tests/residue_geometry.rs` and the independent procedural builder
`tests/support/residue_vectors.rs` define the inputs. Size: 45,904 bytes; SHA-256:
`92b5e01f6a1df2496d8e67fb77ac6b9b9f0192750a3b85897db17ae728ce3fda`.
The normal test explicitly selects `Residue2Layout::Libvorbis`. Regenerate with
the pinned oracle configured:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --test residue_geometry regenerate_residue_geometry_golden -- --ignored
```

See [residue geometry](../../RESIDUE_GEOMETRY.md) for the normative/reference
distinction and validation scope.

`degenerate-books-v1.libvorbis-pcm` contains eleven independent C setup responses
(twelve cases) for nonempty dimension-zero scalar books, empty floor/residue
consumers, later-floor EOF and maximum 16-bit dimensions. Format: `DVCBOOK1`,
little-endian u32 set count, then u64-length-delimited `pcm-cases` responses.
`tests/support/degenerate_books.rs` and `regressions` / `regression_cases` in
`tests/codebook_compatibility.rs` define the inputs. Size: 44,932 bytes; SHA-256:
`6b4615db3c606000482307cc37e03e945a116dfb9d68d99ba1c7f142fb6850cf`.
Normal tests explicitly enable empty-book/IEEE policies where required.
Regenerate with the pinned oracle configured:

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --test codebook_compatibility regenerate_degenerate_book_golden -- --ignored
```

See [degenerate books](../../DEGENERATE_BOOKS.md) for policy distinctions and evidence.

`codebook-extents-v1.libvorbis-headers` retains the pinned C header-unpack results
for 475 procedural scalar/mapped declarations around the extent guard: 289
accepted and 186 rejected, with exact consumed-bit positions. Format: `DVBEXT01`,
u64 little-endian count, then one acceptance byte and one u64 bit position per
book. The independent builder is `tests/support/book_extents.rs`. Size: 4,291
bytes; SHA-256:
`996c82e467390409834115f78a924112e70100c758aedacc5b37bdfc8da36053`.
The normal test enables empty books and raises other caps to isolate the extent
policy; complete setup/initialization checks are separate external tests.

```powershell
cargo test -p deadlib-audio-decode-vorbis --release --test book_limits regenerate_codebook_extent_golden -- --ignored
```

See [codebook limits](../../CODEBOOK_LIMITS.md) for reference protocol, policies
and exact reader-position evidence.

The floor-zero parameter fixtures contain only procedural synthetic C responses:

| Fixture | Wire format and coverage | Bytes | SHA-256 |
| --- | --- | --- | --- |
| `floor0-components-v1.libvorbis` | `DVF0CMP1`, u32 set count (69), then u64-length request/response pairs; reader decisions, actual maps and curves | 1,133,914 | `3d6fbe7bf8bfc0bc3475cccdda7714374296bf15f8a69a551df97bf02a3d6c5a` |
| `floor0-bark-v1.libvorbis` | `DVF0MAP1`, u32 map count (1,088), then u16 rate/grid/bins and u16 indices; 1,436,160 indices | 2,878,860 | `86d98084a0a375654641b61913ec57563375424a9b14cdeebc8a5cf3218ef52b` |
| `floor0-parameters-v1.libvorbis-pcm` | `DVF0PCM1`, u32 set count (7), then u64-length `pcm-cases` responses | 118,355 | `337578d91287d6bc811399ce14ef9b0fdb0f9f6d22bec6d405d5f2ef1103a708` |

`tests/support/floor0_vectors.rs`, `src/floor_reference.rs` and
`regressions` in `tests/floor_zero_parameters.rs` define the inputs independently
of the native encoder. Normal tests compare finite values within the original
bounds and nonfinite classes/signs per position. Manual generators are documented
in [floor-zero parameters](../../FLOOR0_PARAMETERS.md).

The codebook value/bounds fixtures contain procedural C responses:

| Fixture | Format | Bytes | SHA-256 |
| --- | --- | --- | --- |
| `codebook-floats-v1.libvorbis` | `DVQFLT01`, u64-length float and bound responses, u32 VQ set count (8), then u64-length request/response pairs | 927,296 | `183b4a69df3d03b5207de9f5e942fcf01c48420b39a0044f3ad0425d65b39289` |
| `codebook-float-pcm-v1.libvorbis-pcm` | `DVQPCM01`, u32 set count (6), then u64-length `pcm-cases` responses | 38,674 | `82e262b4e7acfaed05d33c91840d11af4f7343bce2618775f81a5667627137b2` |

`tests/support/float_vectors.rs`, `tests/codebook_floats.rs` and
`tests/codebook_float_packets.rs` define the inputs independently of the native
encoder. The first fixture includes all 20,480 custom-float probes and 83,968
byte-bound/initialization probes across every starting bit alignment. Manual
regeneration is documented in [codebook values and bounds](../../CODEBOOK_VALUES.md).

`unknown-nil-eos-v1.ogg` contains the existing generated stereo fixture's
unchanged header/audio packets, repacketized under serial 999 with one audio
packet per page. Each completed audio page carries its untrimmed raw synthesis
endpoint; an appended empty EOS page carries unknown granule `-1`. It contains
4,672 frames per channel and is 5,112 bytes; SHA-256:
`dd260ef1c699e2cd42a701001d23f75fd34a9531b861c5885acb26abc7f131b4`.
See [untimed EOS](../../UNTIMED_EOS.md) for independent PCM/duration/seek checks.
