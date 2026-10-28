# deadlib-audio-decode-wav

Independent pure-Rust WAV implementation. No production dependencies, playback
integration, or changes to the existing DeadSync decoder. Registered only as a
Cargo workspace member. License follows the repository (GPL-3.0).

`Wave::parse(&bytes)` validates a complete form while borrowing every payload.
`wave.chunks()` exposes unknown chunks and original padding without copying;
`wave.as_bytes()` retains the exact form. Bytes after the RIFF boundary are excluded.
`wave.pcm()` provides interleaved U8/I16/I24/I32/F32/F64 access. Native typed slices
are available when byte order and alignment permit. Caller-buffer reads work for
unaligned or swapped input and return whole frames, counted in interleaved samples.
Integer values keep their container scale (valid bits are left justified); U8 to
i32 explicitly removes the 128 bias. Float reads preserve bit patterns including
NaN payloads, signed zero, and subnormals. No implicit float normalization occurs.

Build and test by package name:

```sh
cargo test -p deadlib-audio-decode-wav
```

See [COVERAGE.md](COVERAGE.md) for evidence and limitations and [PLAN.md](PLAN.md)
for the full outstanding objective. The current implementation is under development.
