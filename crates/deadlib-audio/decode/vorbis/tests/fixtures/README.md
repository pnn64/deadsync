# Generated, redistributable fixtures

These files contain locally generated tones/silence, not third-party music.
Generated with FFmpeg's libvorbis encoder. They exercise setup parsing and
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

Regenerate only through the external oracle (from the repository root):

```powershell
$env:DEADLIB_VORBIS_GOLDEN_OUTPUT = "$pwd/crates/deadlib-audio/decode/vorbis/tests/fixtures/surround.libvorbis-pcm"
cargo test -p deadlib-audio-decode-vorbis --release --test pcm regenerate_surround_pcm_golden -- --ignored
```

Set `DEADLIB_LIBVORBIS_ORACLE` as described in the oracle README. The regeneration
test writes only when explicitly invoked with that output path; ordinary tests
never update the golden.
