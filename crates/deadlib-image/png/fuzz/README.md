# PNG and zlib fuzz targets

This is an isolated Cargo workspace. It never adds libFuzzer, png, flate2, or any
other codec to the production dependency graph. Both targets bound decompression,
metadata, frame counts and input sizes before exercising untrusted inputs.

On Linux with nightly Rust, rust-src, a C++ compiler, and cargo-fuzz:

```sh
cargo run -p deadlib-image-png --example seed_fuzz
cd crates/deadlib-image/png
cargo +nightly fuzz run decode -- -runs=10000 -seed=782347 -max_len=65536 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
cargo +nightly fuzz run zlib -- -runs=20000 -seed=782347 -max_len=65536 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
```

Seed generation copies the checked-in independent PNGSuite and APNG fixtures and
extracts their IDAT zlib streams. Working corpora, crash artifacts and builds are
ignored. Committed fixtures and the seed generator reproduce the initial corpus;
do not reuse a previously mutated corpus when reproducing an exact smoke run.

`decode` checks for native parser failures and compares every native sample when
the independent Rust png decoder also accepts the image. Reference ancillary
policies differ, so a reference rejection does not automatically mean a native
sample bug. `zlib` compares every native-accepted stream with bounded flate2
decompression. Integration tests separately check generated valid streams and
native output rejected for truncation/checksum/limit errors. Coverage counters
include the harness and reference codecs, not just the native implementation.

The harness type-checks on Windows, but the instrumented MSVC build encountered
unresolved `__start___sancov_*` / `__stop___sancov_*` linker symbols on this host.
The smoke campaigns therefore use Debian WSL and Linux nightly. If the checkout
is on a Windows filesystem, select a Linux build directory with
`--target-dir /tmp/deadlib-image-fuzz-target` and install cargo-fuzz on Linux's
filesystem. Campaign logs and exact tool versions are recorded in `reports/`.

Smoke campaigns are a regression check, not exhaustive validation. Longer runs,
additional structured generators, CRC-repaired chunk mutations, APNG frame
differential fuzzing, and coverage review remain required.
