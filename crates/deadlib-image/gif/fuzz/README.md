# Fuzzing

`decode` checks bounded arbitrary input, all incremental states, exact native
reencoding, compositor safety and indexed parity on reference-accepted files.
`lzw` checks arbitrary code streams, fragmented input, expansion limits and
encoder roundtrips. Deterministic malformed tests separately cover frozen full
dictionaries and the longest legal KwKwK chain.

With cargo-fuzz and nightly Rust, run from this directory:

```sh
cargo fuzz run decode -- -max_total_time=60 -max_len=262144
cargo fuzz run lzw -- -max_total_time=60 -max_len=16384
```

Or build libFuzzer binaries directly (Linux):

```sh
RUSTFLAGS='-Cdebuginfo=1 -Cpasses=sancov-module -Cllvm-args=-sanitizer-coverage-level=4 -Cllvm-args=-sanitizer-coverage-inline-8bit-counters -Cllvm-args=-sanitizer-coverage-pc-table -Cllvm-args=-sanitizer-coverage-trace-compares -Zsanitizer=address' \
  cargo +nightly build --release --bins --target x86_64-unknown-linux-gnu
```

Seed decode with `../tests/fixtures/image-gif` and encoder-generated files.
`cargo run -p deadlib-image-gif --example seed_fuzz -- <directory>` generates
deterministic GIF and LZW seeds. Recorded smoke logs are in `reports/`; a bounded
run is evidence of exercised paths, not an exhaustive security guarantee.
