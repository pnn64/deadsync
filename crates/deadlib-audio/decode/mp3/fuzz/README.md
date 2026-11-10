# MP3 sanitizer and invariant fuzzing

This separate test-only workspace pins `libfuzzer-sys`. The production MP3
package retains zero production dependencies and denies unsafe Rust. Nightly
`cargo-fuzz` defaults enable AddressSanitizer, debug assertions, overflow checks
and coverage instrumentation.

| Target | Checked invariants |
| --- | --- |
| `frames` | ID3/VBR parsers; MSB fields against direct wire-bit indexing, including atomic failed reads; free-format inference; entropy/PCM decoding, finite output and exact paired decode/reset behavior. |
| `streams` | Exact whole-read versus fragmented-read raw and gapless PCM bits, headers, physical/sample positions, recovery status, errors and termination; stable EOF/terminal errors; raw indexed counts and repeated first/middle/last/EOF seeks; gapless indexed count/ordered PCM signature against complete streaming output. |

Inputs begin with one policy byte, followed by physical frame bytes or file
bytes. CRC, recovery, truncation and read size vary. Input is capped at 32 KiB
plus the policy byte. Frame/metadata/synchronization storage has separate limits.
Each input decodes at most 64 frames; indexed comparisons run only after a
successful complete sequential decode, with an independent 256-entry index cap.

From the repository root on Linux:

```sh
cargo +nightly fuzz build --fuzz-dir crates/deadlib-audio/decode/mp3/fuzz --target-dir /tmp/mp3-fuzz-build
python3 crates/deadlib-audio/decode/mp3/fuzz/seed.py --reference-root target/deadlib-mp3-reference --framing-report target/mp3-framing/results.json --output /tmp/mp3-seeds
python3 crates/deadlib-audio/decode/mp3/fuzz/run.py --binaries /tmp/mp3-fuzz-build/x86_64-unknown-linux-gnu/release --seeds /tmp/mp3-seeds/corpus --output /tmp/mp3-campaign --seconds 180
```

Generate the framing report with `tests/reference_framing.py` and the current
native inspector and libmad oracle. Seeds verify its input hashes and include
the optional encoder corpus, extracting isolated frames and stateful sequences. Reports retain initial/final
corpus hashes, compiler/tool/binary/source hashes, RNG seed, exact commands,
statistics and exit codes. Corpora, artifacts and binaries remain in scratch
storage. File copying does not require POSIX mode-bit changes on Windows mounts.
