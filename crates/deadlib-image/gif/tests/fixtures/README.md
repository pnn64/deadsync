# Fixture provenance

`image-gif/` contains all 20 GIF fixtures in `tests/{samples,crashtest,regression,stall}`
from [image-rs/image-gif](https://github.com/image-rs/image-gif), commit
`a5f89ddc2b29424334244c97456b4a26e208aa7d`, fetched 2026-10-26.
The upstream MIT license is included. `image-gif/SHA256SUMS` pins exact bytes.

Tests distinguish conforming images from malformed compatibility samples.
In particular, upstream documents a missing LZW end code in frame 62 of
`gifplayer-muybridge.gif`. This codec requires EOI and rejects that image.
Independent hand-packed fixtures and deterministic generated indexed data
live directly in the Rust integration tests; they use no codec to construct
their expected indices or composited pixels.
