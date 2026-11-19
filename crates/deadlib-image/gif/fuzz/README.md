# Fuzzing

`decode` checks bounded arbitrary input in strict and compatibility modes,
collecting/incremental agreement including trailing-byte budgets, compositor
safety and indexed parity on reference-accepted files. Strictly encodable models
must roundtrip exactly; compatibility decoding can preserve invalid background
metadata or version declarations that the encoder deliberately rejects.
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
deterministic GIF and LZW seeds. Keep generated corpora and logs under `target/`.

For an audited ASan run with feedback restricted to the native image crates,
run from the repository root on Linux after installing cargo-fuzz and caching
the locked fuzz dependencies:

```sh
python3 crates/deadlib-image/gif/scripts/fuzz_native.py \
  --cargo-fuzz /path/to/cargo-fuzz \
  --seed-archive crates/deadlib-image/gif/bench/results/2026-11-17/fuzz/seeds.zip \
  --target-dir /tmp/gif-fuzz-build --output-dir /tmp/gif-fuzz-evidence \
  --runs 100000
```

Use fresh output and target directories. The archive includes deterministic
encoder seeds, upstream fixtures, and independently encoded benchmark inputs.
The runner retains commands, source/binary/seed hashes, execution counts, and
compiler invocation records. It audits compiled object sections to confirm that
the harness and reference crates provide no SanitizerCoverage feedback while
retaining ASan checks. Native-crate standard-library monomorphizations can
contribute feedback, so this is not a source-only coverage report or proof of
conformance. Saved results include the initial seeds and final grown corpora.
