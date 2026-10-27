# deadlib-audio-decode-mp3

Independent pure-Rust MPEG Audio Layer III implementation in development.
Registered in the Cargo workspace only; no playback integration and no
production dependencies. References are confined to tests and benchmarks.

The current foundation implements checked MSB bit fields, MPEG-1/2/2.5 Layer III
headers, CBR frame boundaries, free-format length inference, side information,
MPEG protection CRC, bounded main-data reservoir assembly, ID3v1/v2 framing,
ID3 unsynchronization, Xing/Info/LAME delay metadata and VBRI table framing.
Unknown ID3 frame payloads remain available. Compressed/encrypted tag payload
interpretation is not provided; these payloads are retained verbatim.

PCM decoding and encoding are not yet implemented. No standards-conformance,
PCM-parity or performance-completion claim is made by the framing milestone.
See [PLAN.md](PLAN.md) for the full requested scope and verification gates.
The foundation's eight tests and 84-file/732-frame libmad comparison are recorded
in [COVERAGE.md](COVERAGE.md).

```sh
cargo test --locked -p deadlib-audio-decode-mp3 --target-dir target/deadlib-mp3
```

MPEG-2.5 is an explicitly separate compatibility extension: header version bits
`00` select quarter-rate MPEG-1 frequencies (8, 11.025 and 12 kHz) with MPEG-2
Layer III granule and side-information syntax. Reserved version bits `01` are
rejected. Complete DSP compatibility, especially the 8 kHz scalefactor bands,
remains a required differential gate.
