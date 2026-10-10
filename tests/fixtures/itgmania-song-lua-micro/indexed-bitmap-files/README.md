# Native indexed BMP/GIF file controls

Generated input files exercise Windows and OS/2 indexed BMP headers,
1/4/8-bit packing, odd rows, duplicate palette colors, unused entries,
GIF global/local palettes, transparency, offsets, interlacing and
multiple frames. The original 35 requests are retained in control.json.

positive-control.json captures the 33 accepted requests with the frozen
committed harness 0.1.48 executable. native.json records actual pinned
ITGmania file decoding and CPU upload preparation. Each .rgba file is
the captured upload bytes; cases.json drives production decoder checks
with zero byte tolerance. Models fill their allocations. Sprite inputs
are power-of-two images, so no undefined padding is serialized.

Native rejects the BMP stored as misnamed.png in both load modes. These
two failures are retained in native-rejections.json and checked against
the production decoder and source-dimension probe. Native preserves a previous
wrong-format error when BMP follows another registered bitmap reader; DeadSync
matches this read-order rejection. All 35 original requests are covered: 33
exact pixel matches and two matching load rejections. Lua error-message and
other unsupported/invalid input behavior remain separate parity requirements.
GPU conversion, mip storage, framebuffer output, true-color BMP masks
and other unsupported/invalid input policy remain separate gaps.
