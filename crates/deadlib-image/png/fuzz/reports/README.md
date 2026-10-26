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
| encode_stream | 20,000 | Exit 0, generated-image/APNG roundtrips and Rust png sample checks pass | 409 MiB |

Full stdout/stderr are in the adjacent log files. libFuzzer coverage/feature
counts include reference implementations and are not native-only coverage.
No claim of exhaustive validation or fuzz saturation is made.

Toolchain: Rust nightly 1.101.0 (76456a688, 2026-10-25), cargo-fuzz 0.13.2,
Debian C++ 14.2.0, Linux 6.6.87.2-microsoft-standard-WSL2 x86-64. The instrumented
binary's `__asan_init` symbol was verified with `nm` after both runs.

The encoder campaign uses the same toolchain and seed with `-max_len=256` and
1,080 generated 12-byte seeds. It completed 20,000 runs in 547 seconds. The first
attempt stopped on a harness-only metadata placement comparison: no-PLTE streams
cannot distinguish BeforePalette from BeforeData. The generator now uses the
canonical placement; the original 12-byte input replays successfully. Failure
and replay logs are retained beside the passing campaign log. No codec defect
was masked by this change; native samples, metadata values and independent
reference checks remain asserted. Exact elapsed time and coverage are host-
dependent, and no fuzz saturation claim is made.

Conformance follow-up: palette recovery `365ab3a04` plus the text/field milestone
containing this report. `png_stream` now selects both palette policies. A fresh
176-file corpus (175 PNGSuite files plus `basic_f20.png`, 178,558 bytes) passed
20,000 executions in 103 seconds, peak RSS 396 MiB, with the same seed, toolchain
and limits. `png-conformance-linux.log` records the full run and corpus path.
Buffered/streaming acceptance and every emitted native sample remained equal.
This brings recorded campaigns to 110,000 executions across five targets.

IANA follow-up: the registration milestone containing this report passes a
further 20,000 executions, 90 seconds, peak RSS 395 MiB. The initial 176-file,
178,558-byte corpus was independently copied into fresh
`fuzz/corpus/png_registry_20261026`; all other options and tool versions match
the conformance campaign. `png-registry-linux.log` records the full run.
The current instrumented binary exports `__asan_init`. Buffered/streaming
acceptance and native sample assertions pass with registry lookup and unknown
ancillary-field recovery. Total recorded executions: 130,000.
