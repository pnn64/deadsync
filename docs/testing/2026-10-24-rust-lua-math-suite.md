# Lua 5.1 math and numeric conversions

The Rust engine now provides the Lua 5.1 math function set, including inverse and
hyperbolic trigonometry, `atan2`, angle conversion, `modf`, `frexp`, `ldexp`,
`random` and `randomseed`. `tonumber` supports bases 2–36, C-string termination,
all C-locale whitespace, and platform unsigned-long overflow behavior.

All 37 differential/adversarial integration groups, the verifier mutation test
and documentation example pass. Strict Clippy passes. New generation compares
2,000 binary-scaling cases and 2,000 radix conversion cases. Targeted checks
include signed zero, subnormal ties, infinities/NaNs, multiple-result arity,
32-bit MSVC unsigned long, seeded random sequences, error-path advancement and
random state shared by separate VMs on the same OS thread.

`ldexp` normalizes and rounds directly in binary, avoiding intermediate floating
overflow and double rounding. Windows `frexp` returns exponent -1 for nonfinite
inputs, matching this reference. The independent random implementation uses
the MSVC linear congruential stream; its constants and per-thread state were
checked against the installed Microsoft Windows SDK 10.0.26100.0 UCRT source
(`ucrt/stdlib/rand.cpp`) and the vendored official interpreter. Other CRT streams
are not emulated. Cross-platform transcendental bit equivalence and complete
library diagnostics remain unproven.

The [JSON report](2026-10-24-rust-lua-math-suite.json) contains twelve unchanged
official suite probes. All pass under the reference; Rust passes `locals.lua`,
`vararg.lua` and `pm.lua`. `math.lua` reaches line 124, missing `io.tmpfile`;
`sort.lua` advances from `math.random` to missing `os.clock` at line 25. Other
stopping points remain those in the [string/table report](2026-10-24-rust-lua-stdlib-suite.md).
The overall suite gate still fails; no assertions or files were rewritten to
obtain these results.

```sh
cargo test -p deadlib-lua
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-math-suite.json
```

These are independent file probes using `_U=true`, the pinned official archive,
a 100-million-instruction budget and 60-second process deadlines. Full `all.lua`,
I/O, OS, GC, coroutines, debug, binary chunks and additional numeric edge cases
remain open. Earlier performance reports retain their source hashes; this math
extension makes no new throughput claim.
