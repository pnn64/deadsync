# deadlib-audio-decode-mp3

The current foundation implements checked MSB bit fields, MPEG-1/2/2.5 Layer III
headers, CBR frame boundaries, free-format length inference, side information,
MPEG protection CRC, bounded main-data reservoir assembly, ID3v1/v2 framing,
ID3 unsynchronization, Xing/Info/LAME delay metadata and VBRI table framing.
Unknown ID3 frame payloads remain available. Compressed/encrypted tag payload
interpretation is not provided; these payloads are retained verbatim.

Native raw-frame PCM decoding is now implemented. `FrameDecoder` returns
borrowed planar, normalized, unclipped f32 PCM with its frame header and byte
count. It retains reservoir, hybrid overlap and synthesis history across calls.
`DecodeOptions` controls CRC verification and MPEG-2.5 acceptance. A physically
incomplete frame can be retried without consuming state; ordinary decode errors
require `reset()`. Missing-reservoir errors retain payload for later recovery.

```sh
cargo test --locked -p deadlib-audio-decode-mp3 --target-dir target/deadlib-mp3
```

MPEG-2.5 is an explicitly separate compatibility extension: header version bits
`00` select quarter-rate MPEG-1 frequencies (8, 11.025 and 12 kHz) with MPEG-2
Layer III granule and side-information syntax. Reserved version bits `01` are
rejected. Ordinary LAME-generated files pass PCM comparisons at all nine rates.
The 8 kHz mixed-block profile uses four long subbands and 72 long lines, following
the 1997 Layer3 syntax document. An independent all-frequency equation oracle
confirms this profile. Reference decoders differ on constructed mixed blocks;
see [MPEG25.md](MPEG25.md) for the exact evidence and compatibility scope.
