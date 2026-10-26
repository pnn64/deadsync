#!/bin/sh
set -eu
crate=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$crate"
cargo run --quiet --manifest-path Cargo.toml --example seed_fuzz --target-dir /tmp/deadlib-gif-target -- fuzz/corpus
find tests/fixtures/image-gif -name '*.gif' -exec cp '{}' fuzz/corpus/decode/ ';'
export CC=clang CXX=clang++
export RUSTFLAGS='-Cdebuginfo=1 -Cpasses=sancov-module -Cllvm-args=-sanitizer-coverage-level=4 -Cllvm-args=-sanitizer-coverage-inline-8bit-counters -Cllvm-args=-sanitizer-coverage-pc-table -Cllvm-args=-sanitizer-coverage-trace-compares -Zsanitizer=address'
cargo +nightly build --manifest-path fuzz/Cargo.toml --release --bins --target x86_64-unknown-linux-gnu --target-dir /tmp/deadlib-gif-fuzz-target
mkdir -p fuzz/reports fuzz/artifacts
for name in decode lzw; do
    /tmp/deadlib-gif-fuzz-target/x86_64-unknown-linux-gnu/release/"$name" "fuzz/corpus/$name" -max_total_time=60 -max_len=262144 -timeout=10 -rss_limit_mb=1024 -seed=1327 -artifact_prefix=fuzz/artifacts/ > "fuzz/reports/$name-linux.log" 2>&1
    tail -5 "fuzz/reports/$name-linux.log"
done
