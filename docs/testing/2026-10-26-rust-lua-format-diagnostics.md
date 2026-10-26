# Lua 5.1 binary format diagnostics

Malformed string.format conversions now produce the same error bytes as Lua
5.1 when the conversion is NUL. This includes a terminal percent sign, flags or
precision with no conversion, and an explicit NUL after the format introducer.
For example, pcall(string.format, '%', 17) returns the 30-byte error

~~~text
invalid option '%' to 'format'
~~~

The committed parent instead included a NUL after the percent sign, yielding
31 bytes. This changes error-object equality and length even when a terminal
display hides the byte. The fix omits that character only in the invalid-option
diagnostic. Lua 5.1 builds its diagnostic %c through a two-byte C string; the
local [string formatter](../../vendor/lua-src/lua-5.1.5/lstrlib.c) and
[diagnostic formatter](../../vendor/lua-src/lua-5.1.5/lobject.c) provide the reference.
Literal NULs in successful format text remain in the output.

| Check | Result |
| --- | --- |
| Tests | 494: 64 unit, 428 integration, 2 doc |
| Strict Clippy / format | Pass |
| Normal/build dependencies | Only deadlib-lua |
| Generated binary/malformed format comparisons | 544/544 |
| Native error contexts | 7/7 |
| CLI | 165/203 raw; 203/203 after exact known leading banner removal |
| New CLI format probes | 4/4 raw matches |
| Process probes | 140/140 across Rust API, C API and CLI |
| Official files: reference, Rust, cache store, cache hit | 17/18 in each mode |
| Official cache outcomes | 18 Stored; 18 fresh-process Hit |
| Build-identity fixture | 16/16 |

The generated test visits all 256 conversion bytes both at the end of a format
and before a suffix (512 cases), then 16 malformed/mixed formats with and without
arguments (32 cases). Full result/error byte strings are compared with C Lua
5.1.5. Cases include NULs in format text, missing arguments, repeated flags,
width/precision limits, unsupported conversions and valid conversion controls.
The second test checks direct and tail calls, method syntax, aliases, protected
error rethrow after collection, native call hooks and a failed coroutine.

An isolated comparison builds committed 6509a843b and the final sources with
identical unoptimized rustc flags as separate rlibs and one Rust harness.
It reproduces four parent failures and two unchanged controls against the
checksum-pinned official executable from the completed CLI audit:

| Probe | Parent/reference | Current/reference | Reference result/error bytes |
| --- | --- | --- | ---: |
| trailing_percent | mismatch | match | 30 |
| trailing_flag | mismatch | match | 30 |
| nul_conversion | mismatch | match | 30 |
| precision_nul | mismatch | match | 30 |
| invalid_nonzero | match | match | 31 |
| literal_nul | match | match | 12 |

Lua emits result/error bytes as hexadecimal plus their byte count before
printing. The observer parses those fields after removing the exact transport
newline: CRLF from Windows C stdout and logical LF from VM output capture.
No result or error bytes are normalized. Raw streams remain in JSON/TSV.
All six instruction/register layouts match between parent and current; Vm is
4,432 B and Value 16 B in both. The harness changes no compiler source and makes
no timing/allocation claim. [Earlier compiler measurements](../performance/2026-10-26-rust-lua-constant-pool.md)
retain their historical source provenance.

The expanded CLI audit builds the ordinary release executable and retains all
return codes, stdout, stderr and file effects. It exits 1 because 38 existing
cases differ only in the intentional version banner. The additional comparison
removes at most one exact known leading banner; all four new cases match raw.
Reference archive SHA-256 is
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333.
Its binary and unmodified source hashes are verified. Assumptions remain x64
Windows/MSVC /O2 /MT, binary64, 64-bit Lua integers, 32-bit native ints and C locale.

All 18 official files retain the preceding milestone's source hashes. Each mode
fails only files.lua:59, the existing Windows open-input rename failure. The
suite exits 1 as expected. Bootstrap _U=true, 100,000,000 instructions and
60-second per-file deadlines remain. Cached programs are file-entry programs;
dynamic loads compile normally. Official suite archive SHA-256 is
49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad.
No suite assertions are changed or removed.

~~~sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-format-diagnostics-audit.json
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-format-diagnostics-process.json
python scripts/check_rust_lua_suite.py --cached --output docs/testing/2026-10-26-rust-lua-format-diagnostics-suite.json
python scripts/check_rust_lua_cache_identity.py --output docs/testing/2026-10-26-rust-lua-format-diagnostics-identity.json
python scripts/check_rust_lua_format_diagnostics.py --parent 6509a843b --audit docs/testing/2026-10-26-rust-lua-format-diagnostics-audit.json --output docs/testing/2026-10-26-rust-lua-format-diagnostics-regression.json
~~~

All five current-source maps and the archived-parent map match their inputs.
Raw [CLI](2026-10-26-rust-lua-format-diagnostics-audit.json), [process](2026-10-26-rust-lua-format-diagnostics-process.json),
[suite](2026-10-26-rust-lua-format-diagnostics-suite.json), [identity](2026-10-26-rust-lua-format-diagnostics-identity.json) and
[parent comparison](2026-10-26-rust-lua-format-diagnostics-regression.json) results are retained. The comparison
also retains its Rust harness, raw TSV, build commands and build log.
Complete standalone Lua 5.1 compatibility, remaining library/debug/resource
boundaries, later dialects, native host callbacks, native code and song/noteskin
integration remain unfinished. The full objective stays active.
