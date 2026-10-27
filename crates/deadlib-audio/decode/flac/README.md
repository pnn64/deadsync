# deadlib-audio-decode-flac

Independent pure-Rust FLAC implementation under development, based on
[RFC 9639](https://www.rfc-editor.org/rfc/rfc9639.html). Registered in the Cargo
workspace only; it is not used by DeadSync playback. No production dependencies.

Current foundation:

- Checked MSB-first bit reader/writer and bounded unary decoding.
- FLAC CRC-8/CRC-16 and native streaming MD5 (RFC 1321).
- STREAMINFO, SEEKTABLE, VORBIS_COMMENT, PICTURE and CUESHEET parsing.
- Borrowed metadata blocks retain ordering, unknown/application payloads and tag
  bytes. Tags and picture text are interpreted separately from structural parsing;
  non-UTF-8 contents are retained. Picture URIs are never fetched.
- Configurable metadata and frame byte budgets; checked metadata lengths.

Validation: `cargo test -p deadlib-audio-decode-flac` (5 integration tests, passing).
Tests cover bit offsets/widths, unary boundaries, independent checksum known
answers, MD5 chunk boundaries, metadata roundtrips/truncation and resource limits.
This is foundation evidence, not yet a full codec conformance claim.

See [PLAN.md](PLAN.md) for the full scope and unfinished milestones. Decoding,
encoding, seeking, Ogg mapping, differential corpus validation, fuzzing and
performance measurements remain to be implemented.
