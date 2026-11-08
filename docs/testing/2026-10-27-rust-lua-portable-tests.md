# Portable regressions and native overflow diagnostics

The complete Rust test command now passes on Windows and Linux: 629 tests on
each platform. The former Linux failures in `debug_locals`, `os`, `package` and
`stdlib` are addressed without changing the engine implementation or the
default C-reference build. Passing this regression command does not establish
full standalone Lua 5.1 parity.

The gsub fixtures use a replacement larger than the native buffer: 513 bytes
on Windows and 8,193 on the Unix profile. Pending concatenation callbacks now
run on both platforms. The abort test still requires its callback's `started`
flag, instruction-limit error, collection and subsequent recovery. The
generated boundary matrix adds widths 8,191 and 8,193 around the GNU buffer.

The import-abort fixture uses a two-MiB comment so reading remains active beyond
its quota with either buffer size. It still checks that the reader record and
file handle are released, the module cache remains empty, and later execution
recovers. Successful wildcard renames now verify the moved binary contents and
restore the original path before the next attempt. Both native success/failure
tuples remain in the differential comparison; Unix wildcard names are valid.

Two existing `unpack` intervals crash the GNU C oracle inside the regression
process. They now execute in the separate standalone audit, alongside three
additional overflowing intervals. The Windows in-process comparisons remain.
A new Rust group checks all five intervals for catchable `too many results to
unpack` errors under a quota, followed by a successful ordinary unpack.

`check_rust_lua_unpack.py` builds fresh standalone Rust and checksum-pinned
official Lua 5.1.5 CLIs. Sixteen cases retain raw output, status, native signals
and timeouts in private child processes. The five overflowing intervals still
crash the default GCC reference with SIGSEGV. Rust returns the bounded Lua
error, matching the unmodified MSVC reference. The strict GNU audit therefore
continues to exit 1; these differences are not normalized into passes.

With `--diagnose-signed-overflow`, two additional C executables use the same
unmodified source and otherwise the same build command. Neither replaces the
strict reference. Signed-overflow sanitization reports six failing programs,
with diagnostics in `lbaselib.c:348` and `ltable.c:437`. One program that matches
in the strict comparison also produces a sanitizer diagnostic. The latter
location concerns numeric-key indexing, so the observations do not attribute
every failure solely to the unpack count. A separate `-fwrapv` control defines
signed wrapping and matches all sixteen Rust observations. These controls
document reference assumptions rather than change the active reference flags.

| Check | Result |
| --- | --- |
| Windows Rust tests | 629: 73 unit, 552 integration, 4 documentation |
| Linux Rust tests | 629: 73 unit, 552 integration, 4 documentation |
| Previously failing Linux targets | Debug locals 32/32, OS 17/17, package 25/25, stdlib 31/31 |
| Standalone Windows unpack audit | 16/16 raw output/status matches |
| Default GNU unpack audit | 11/16 matches; five native crashes; strict exit 1 |
| GNU sanitizer diagnostic | Six signed-overflow failures retained |
| GNU wrapping control | 16/16 matches; excluded from strict equality |
| Formatting and strict Clippy | Pass on both platforms |
| Normal/build dependencies | Only deadlib-lua |
| Production runtime source and flags | Unchanged from `18458ee31` |

Fresh final targets and source guards cover both platform runs. The
[Windows audit](2026-10-27-rust-lua-portable-tests-windows.json),
[GNU audit and controls](2026-10-27-rust-lua-portable-tests-linux.json),
[Linux regression audit](2026-10-27-rust-lua-portable-tests-linux-audit.json)
and [validation metadata](2026-10-27-rust-lua-portable-tests-validation.json)
retain executable hashes, reference source hashes, complete build commands,
generated programs, raw results and check logs. The parent audit retains its
four failed targets, including the original in-process C assertion.

The runtime source hashes match the prior validated revision. Its
[unchanged full-suite result](2026-10-27-rust-lua-ucrt-streams-suite.json) still
records both 64-bit engines failing `all.lua` at `big.lua:11`; that failure is
not rewritten into a pass. Stock T/testC and suite native libraries remain
absent. Full standalone Lua 5.1, the complete C/auxiliary APIs, production native
loading and broader GC/debug/resource behavior remain open. No performance
tuning or benchmarks accompany this milestone; later dialects and integrations
remain deferred.
