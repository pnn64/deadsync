# Reference clips

Both clips use FFmpeg's synthetic `testsrc2=size=46x30:rate=8` input. The `*.yuv` files contain FFmpeg-decoded, display-ordered YUV420p frames. Tests read the checked-in fixtures directly.

- `baseline.h264` is a 10-frame x264 Baseline Annex B stream with CAVLC, P pictures and two keyframes.
- `high.h264` is the H.264 track from a 13-frame x264 High profile MP4, converted to Annex B with FFmpeg's `h264_mp4toannexb` filter. It uses CABAC, B pictures, two slices per picture and two keyframes.
- `high.avcc-config` is the MP4's `avcC` payload. `high.avcc` contains the track samples in decode order. Packet sizes and presentation times come from FFprobe and are recorded in the test. The track time base is 1/16384 s.
