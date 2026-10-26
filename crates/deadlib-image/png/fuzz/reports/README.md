# 2026-10-26 smoke campaign evidence

Source: native scalar bulk-copy implementation at `815245b56`.
Harness: `fuzz/fuzz_targets/{decode,zlib}.rs` in the following milestone commit.
All codec and reference dependencies are pinned by `fuzz/Cargo.lock`.

Commands and seed regeneration are in `../README.md`; both runs used the initial
generated corpus, seed 782347, `-max_len=65536 -timeout=5 -rss_limit_mb=1024
-print_final_stats=1`, and default AddressSanitizer under Debian WSL.

| Target | Executions | Result | Peak RSS |
| --- | ---: | --- | ---: |
| decode | 10,000 | Exit 0, no crash / differential mismatch | 170 MiB |
| zlib | 20,000 | Exit 0, no crash / differential mismatch | 360 MiB |

Full stdout/stderr are in the adjacent log files. libFuzzer coverage/feature
counts include reference implementations and are not native-only coverage.
No claim of exhaustive validation or fuzz saturation is made.
