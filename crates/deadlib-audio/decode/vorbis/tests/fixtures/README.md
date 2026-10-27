# Generated, redistributable fixtures

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
