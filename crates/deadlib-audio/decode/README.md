# Native audio codecs

This directory contains the native container and codec packages:

- [Ogg](ogg/README.md): pages, CRCs, packet assembly, streams and seeking.
- [Vorbis](vorbis/README.md): raw packets, Ogg mapping, encoding and indexed decoding.
- [FLAC](flac/README.md): native and Ogg FLAC decoding, encoding and seeking.
- [MP3](mp3/README.md): Layer III decoding, streaming, gapless playback and seeking.
- [WAVE](wav/README.md): PCM, IEEE float, ADPCM, metadata and segmented audio.

Each package documents its APIs, supported formats and limits in its README.
Integration tests use native fixtures and independent reference implementations.
External reference tests require the tools and corpora described in their test
and oracle directories. The [fuzz workspace](fuzz/README.md) exercises malformed
inputs and bounded resource use.

Run a package's Rust tests from the workspace root, for example:

```sh
cargo test -p deadlib-audio-decode-ogg
cargo test -p deadlib-audio-decode-vorbis
```

## Format specifications

- [RFC 3533](https://www.rfc-editor.org/rfc/rfc3533.txt).
- [Xiph Ogg framing](https://www.xiph.org/ogg/doc/framing.html) and
  [stream mapping](https://www.xiph.org/ogg/doc/oggstream.html).
- [Vorbis I](https://www.xiph.org/vorbis/doc/Vorbis_I_spec.html), including
  Appendix A for the Ogg mapping.
- MP3 reference editions and provenance are listed in [SOURCES.md](mp3/SOURCES.md).
