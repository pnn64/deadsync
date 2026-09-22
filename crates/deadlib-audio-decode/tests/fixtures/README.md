`timeline.mp3` is a generated 2.4-second stereo chirp, containing an Info/LAME
header, encoder delay and padding. It contains no third-party recording.
Generated with FFmpeg 7.1 / libmp3lame using:

```sh
ffmpeg -f lavfi -i 'aevalsrc=0.3*sin(2*PI*(220*t+70*t*t))|0.2*sin(2*PI*(330*t+110*t*t)):s=44100:d=2.4' -c:a libmp3lame -b:a 96k -map_metadata -1 -id3v2_version 0 -write_id3v1 0 timeline.mp3
```

The MP3 tests derive Xing, ID3-prefixed and headerless variants from these same
encoded audio frames. The PCM hashes, duration bits and seek hashes were captured
from the decoder before adding timeline options. Tests do not require FFmpeg.
