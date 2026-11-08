# Lua 5.1 native integer and result boundaries

select, unpack, string.char and string.rep now convert numeric arguments through
the existing 64-bit Lua integer conversion and then narrow to a 32-bit native int.
This preserves numeric-string conversion, fractional truncation and the measured
MSVC x64 non-finite/out-of-range behavior. Large repetition counts narrow before
output sizing/reservation; e.g. string.rep('xy', 4294967299) produces 'xyxyxy'.
This is the pinned platform behavior, not a claim about every C float-to-int ABI.

Unpack and string.byte now honor Lua 5.1's 8,000-slot native stack check, including
unused original arguments. One-argument unpack can return 7,999 values; explicit
start/end arguments leave room for 7,997. Empty intervals return no values before
the capacity check. Integer range overflow and requests exceeding capacity fail before allocating
result storage. Errors are catchable Runtime errors with reference text, rather
than the old uncatchable million-result Limit error. String-byte arithmetic and
stack overflow retain their distinct diagnostics. Unpack type-checks its table at
entry and borrows it during the result loop, avoiding repeated handle checks.
No dispatch/frame/value field, dependency, unsafe code or production C link is added.

All **422 tests** pass: 43 unit, 378 integration and one documentation example.
Seven additions cover native narrowing, numeric strings, non-finite/large values,
fractional truncation, 126 generated inputs across four functions (504 calls),
64 explicit result-boundary probes with unused arguments, default/empty/overflow
precedence and collection while 7,999 returned objects remain live. Large result
successes are summarized in Lua before pcall returns: the development binding's
API checks otherwise reject PUC pcall's own status push after huge open results.
The standalone oracle independently checks the native behavior without those
binding API checks. Formatting, strict all-target Clippy, whitespace checks and a
normal/build dependency tree containing only deadlib-lua pass.

The [release CLI audit](2026-10-26-rust-lua-native-bounds-audit.json) adds sixteen new
cases against official Lua 5.1.5: integer/character/repetition narrowing, uncaught
result errors, empty/overflow ranges, 7,996-7,999 explicit result counts and unused
argument capacity. All sixteen match complete status/stdout/stderr/file bytes.
The full audit retains **88/126 raw matches**; all thirty-eight differences are
the intentional project version banner. Removing at most one exact known leading
banner from each stderr yields **126/126**; raw observations and both banners are
retained. The strict command still exits 1 and is not a full-byte CLI pass.

All [140 process probes](2026-10-26-rust-lua-native-bounds-process.json) pass. Eighteen
[official isolated files](2026-10-26-rust-lua-native-bounds-suite.json) are checksum-identical to the prior
milestone; both engines pass **17/18**. Both retain files.lua:59's Windows open-input
rename failure. The official command still exits 1 with _U=true, 100-million-
instruction budgets and 60-second file deadlines. This does not establish all.lua,
internal testC or complete suite passage.

Reference definitions are in the official [base library](https://www.lua.org/source/5.1/lbaselib.c.html),
[string library](https://www.lua.org/source/5.1/lstrlib.c.html),
[API stack check](https://www.lua.org/source/5.1/lapi.c.html) and
[auxiliary API](https://www.lua.org/source/5.1/lauxlib.h.html). The standalone
oracle uses the unmodified checksum-pinned 5.1.5 archive (SHA-256
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333), MSVC
/O2 /MT, binary64 numbers, 64-bit lua_Integer and 32-bit int. Development binding
patches remain isolated from production, and native execution keeps the 8,000
limit despite the idle auxiliary-thread registry-capacity patch.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-native-bounds-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-native-bounds-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-native-bounds-suite.json
```

All final report source hashes match the tree. Two [measurement runs](../performance/2026-10-26-rust-lua-native-bounds.md)
cover 72 runtime and 22 frontend workloads. Other native stack APIs and larger
resource/memory bounds, artificial table/pattern work limits, full diagnostics,
debug/C scratch slots, collector timing, terminal signals/modes, native modules,
later dialects, persistent caches, Rust callbacks, JIT and song/noteskin integration
remain unfinished. The full standalone/engine objective stays open.
