# Generated, redistributable fixtures

These files contain locally generated tones/silence, not third-party music.
Generated with FFmpeg's libvorbis encoder. They exercise setup parsing and
provide independently decodable fixtures for the later PCM differential suite.
Ogg stream serials are arbitrary and may change when regenerated.

```powershell
ffmpeg -f lavfi -i 'aevalsrc=0.25*sin(2*PI*440*t)|0.125*sin(2*PI*997*t):s=48000:d=0.08' -c:a libvorbis -q:a 3 -metadata TITLE=NativeVorbisHeaderFixture -y stereo.ogg
ffmpeg -f lavfi -i 'anullsrc=r=44100:cl=7.1' -t 0.02 -c:a libvorbis -q:a 1 -y eight-channel.ogg
```
