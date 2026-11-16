# PNG and zlib fuzzing

This isolated Cargo workspace keeps libFuzzer and reference codecs out of the
production dependency graph. Targets check bounded input handling, round trips,
streaming boundaries, native sample values, metadata and animation composition.

Run with nightly Rust, rust-src, Clang/C++ and cargo-fuzz from this directory:

```sh
cargo +nightly fuzz list
cargo +nightly fuzz run decode -- -max_len=65536 -timeout=10 -rss_limit_mb=1024
```

Available targets:

- `decode`
- `zlib`
- `png_stream`
- `inflate_stream`
- `encode_stream`
- `profiles`
- `deflate_stream`
- `huffman`
- `text_body`
- `reference_samples`
- `compositor`
- `decoder_limits`

The targets set decode/output budgets before exercising arbitrary input.
Structured targets additionally generate small valid inputs and compare exact
native values or an independent mathematical model. `reference_samples` checks
sample-depth conversions, `compositor` checks APNG disposal/blending, and the
streaming targets exercise chunk and writer boundaries.

Reproduce a saved failure with `cargo +nightly fuzz run <target> <artifact>`.
Keep local campaigns and reports under ignored output directories. Promote a
confirmed defect into a focused regression test with the current implementation.

`examples/reference_check.rs` provides the standalone reference checker;
`scripts/native_rustc.py` supplies native-only coverage instrumentation, and
`scripts/archive_native_seeds.py` handles local corpus archives.
