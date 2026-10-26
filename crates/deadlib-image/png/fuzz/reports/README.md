# 2026-10-26 smoke campaign evidence

Original targets: native scalar bulk-copy implementation at `815245b56`.
Streaming targets: the row-decoder milestone containing this report, building on
inflater `ff0d31c27`. Harnesses are in `fuzz/fuzz_targets/`.
All codec and reference dependencies are pinned by `fuzz/Cargo.lock`.

Commands and seed regeneration are in `../README.md`; all runs used the initial
generated corpus, seed 782347, `-max_len=65536 -timeout=5 -rss_limit_mb=1024
-print_final_stats=1`, and default AddressSanitizer under Debian WSL.

| Target | Executions | Result | Peak RSS |
| --- | ---: | --- | ---: |
| decode | 10,000 | Exit 0, no crash / differential mismatch | 170 MiB |
| zlib | 20,000 | Exit 0, no crash / differential mismatch | 360 MiB |
| png_stream (CRC repaired) | 20,000 | Exit 0, no acceptance / native-row mismatch | 396 MiB |
| inflate_stream | 20,000 | Exit 0, no output / consumption mismatch | 389 MiB |

Full stdout/stderr are in the adjacent log files. libFuzzer coverage/feature
counts include reference implementations and are not native-only coverage.
No claim of exhaustive validation or fuzz saturation is made.

Toolchain: Rust nightly 1.101.0 (76456a688, 2026-10-25), cargo-fuzz 0.13.2,
Debian C++ 14.2.0, Linux 6.6.87.2-microsoft-standard-WSL2 x86-64. The instrumented
binary's `__asan_init` symbol was verified with `nm` after both runs.
