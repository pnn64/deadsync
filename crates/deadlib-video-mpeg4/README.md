# deadlib-video-mpeg4

Standalone, dependency-free decoding of MPEG-4 Visual and Microsoft MPEG-4 v2
video in Rust. The library uses `no_std` with `alloc` and exposes decoded 8-bit
Y, Cb and Cr planes.
[MPEG-4 Visual standard](https://www.iso.org/standard/39259.html)

The decoder supports progressive rectangular 4:2:0 I, P and B pictures using
H.263 quantization. It handles AC prediction, video packets, and one or four
half-pixel motion vectors per macroblock.

Use `decode_elementary_stream` for raw `.m4v` bytes and `decode_avi` for RIFF AVI
with MPEG-4 Visual or Microsoft MPEG-4 v2 (`MP42` or `DIV2`). For incremental
decoding, feed MPEG-4 Visual start-code units to `Decoder::push` and drain any
pending reference picture with `Decoder::finish`. Feed complete
Microsoft MPEG-4 v2 picture packets to `MsMpeg4v2Decoder::push_packet`.
OpenITG's `logo.mpg` contains Xvid video in an AVI container.

```sh
cargo run --release --example decode -- avi logo.mpg logo.yuv
cargo run --release --example decode -- es video.m4v video.yuv
cargo run --release --example decode -- avi mp42.avi video.yuv
cargo test --offline
```

The AVI file and its extracted elementary stream matched FFmpeg 7.1 with
`-idct simple` on all 900 frames of `logo.mpg`. The offline test fixture uses
FFmpeg's `testsrc2=size=160x90:rate=25:duration=0.4` with
`-c:v mpeg4 -threads 1 -flags +mv4 -ps 160 -g 5 -bf 0 -q:v 5 -f m4v`.
The display-order fixture uses FFmpeg's `testsrc2=size=96x64:rate=25:duration=0.48`
with `-c:v mpeg4 -threads 1 -bf 2 -g 9 -flags +mv4 -q:v 3 -f m4v`.

The MP42 decoder matched FFmpeg 7.1 with `-idct simple` on every visible Y, Cb
and Cr sample in both distinct clips from the installed ITGmania song packs.

| Song-pack clip | Dimensions | Frames |
| --- | --- | ---: |
| UPS 3 / Testament Of Axis / A-Short_People_Got_No_Reason_To_Live.avi | 320 × 240 | 2,206 |
| lua-songs / My Penny / banner.avi | 360 × 640 | 69 |

The offline MP42 test fixture uses FFmpeg's
`testsrc2=size=144x82:rate=25:duration=0.4` with
`-an -c:v msmpeg4v2 -threads 1 -vtag MP42 -g 5 -q:v 5 -fflags +bitexact -flags:v +bitexact -f avi`.

See the [decoder benchmark](bench/README.md) for cycles, throughput and
allocation comparisons with ITGmania's FFmpeg build.
