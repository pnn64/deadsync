# GNU numeric reads and error order

Linux/glibc numeric reads now preserve the reference's consumed prefix, returned
value and unread suffix. Incomplete decimal/hex exponents consume their marker
and sign while converting the mantissa. `0x.` produces signed zero; bare `0x`
fails. Failed infinity/NaN words consume the mismatching byte. glibc 2.41 accepts
parenthesized NaN payloads; observations compare classification rather than
payload bits. Matching uses ctype before ASCII numeric conversion, preserving
the Turkish dotted/dotless-I distinctions.

The pinned GCC build validates the second operand first for `math.pow`,
`fmod`/`mod` and `atan2`. The Rust GNU profile now follows that order, including
method numbering and callable-table arguments. The MSVC profile keeps its
first-operand order. C leaves argument evaluation order unspecified, so these
are explicit reference-build assumptions. `ldexp` already checks its exponent
first. The official Lua sources and compiler flags remain unchanged.

GNU stream EOF state now persists across line-iterator calls. Appending bytes
after the iterator reaches EOF does not resume it until an ordinary read or a
successful seek clears the state, matching Lua's `g_read`/`clearerr` behavior.
The existing differential iterator test covers that transition. No dependency,
unsafe code or production C Lua linkage is added.

`check_rust_lua_numeric_streams.py` builds both real CLIs and runs 1,706
deterministic C-locale stream inputs plus 70 math/error expressions. Inputs cover
partial words/exponents, signs, hexadecimal rounding, overflow/underflow, NaN,
infinity, NUL and high bytes. Each row retains the value, offset, zero-length
read result and remaining bytes. Generated source bytes are identical on both
platforms; explicit byte writes prevent Windows newline translation. The
locale audit also adds ten special-token cases inside each numeric program.

| Check | Result |
| --- | --- |
| Linux stream observations | 1,706/1,706; parent 1,484/1,706 |
| Linux math/error observations | 70/70; parent 34/70 |
| Windows stream observations | 1,644/1,706; same 62 mismatches in parent/current; strict exit 1 |
| Windows math/error observations | 70/70 in parent/current |
| Windows Rust tests | 627 pass: 73 unit, 550 integration, 4 documentation |
| Linux I/O / native-error groups | 22/22 and 9/9 pass |
| New GNU regression groups | 2/2 pass |
| Extended locale CLI programs | 36/36 raw output/status matches |
| Formatting / strict Clippy | Pass on Windows and Linux |
| Normal/build dependency tree | Only deadlib-lua |
| Shared-source Windows C fixture | Output/status match; 36,766 dispatch/drop guards under panic=abort |
| C-state lifetime | 73 created, 72 collected before shutdown, 73 destroyed |
| Windows process probes | 140/140 on retry; initial 139/140 timeout report retained |
| Linux independent official files | 18/18 in reference, Rust, fresh cache store and cache hit |
| Linux full-suite entry points | `main.lua` passes in both; `all.lua` fails at `big.lua:11` in both |
| Broad Linux Rust audit | Four existing failed targets; strict exit 101 retained |

The parent is `9ae0ecbd5`. Its Linux numeric audit records 222 stream and 36
math mismatches. The separate fresh Windows parent build confirms the current
62 stream mismatches predate these fixes. They concern NUL/`0xff` boundary-byte
consumption and remain open. The strict numeric command continues to fail on
Windows; this milestone does not claim Windows stream parity.

The broad Linux run retains `debug_locals`, `os`, `package` and `stdlib` failures.
They include a buffer-size fixture, wildcard-rename cleanup, a reader-quota
fixture and a development C reference-bridge assertion in `lua_rawgeti`.
Removing two failed targets does not establish absence of other regressions.
The process retry follows a timeout in the Rust `%O` calendar abort probe; the
initial result remains separate from the complete successful rerun.

Independent suite files use the existing `_U`/`Message` bootstrap and instruction
bound. They are narrower evidence than the unchanged `all.lua` run, which uses
the official path setup and no bootstrap. The 64-bit large-string overflow
assertion still fails in both full-suite engines. It remains unchanged; the
runtime is not given a fabricated 32-bit limit. Stock T/testC and suite native
libraries remain absent, with the suite's automatic skips visible.

Raw [Linux before](2026-10-27-rust-lua-gnu-numbers-before.json),
[Linux after](2026-10-27-rust-lua-gnu-numbers-linux.json),
[Windows before](2026-10-27-rust-lua-gnu-numbers-windows-before.json),
[Windows after](2026-10-27-rust-lua-gnu-numbers-windows.json),
[locales](2026-10-27-rust-lua-gnu-numbers-locale.json),
[official files](2026-10-27-rust-lua-gnu-numbers-files.json),
[full suite](2026-10-27-rust-lua-gnu-numbers-suite.json),
[C boundary](2026-10-27-rust-lua-gnu-numbers-boundary.json),
[process](2026-10-27-rust-lua-gnu-numbers-process.json),
[initial timeout](2026-10-27-rust-lua-gnu-numbers-process-timeout.json) and
[Linux audit](2026-10-27-rust-lua-gnu-numbers-linux-audit.json) reports retain
strict outcomes. [Validation metadata](2026-10-27-rust-lua-gnu-numbers-validation.json)
pins source bytes, fresh build targets, executables, native locale inputs and
logs. No tuning or performance benchmarks accompany this change. Full standalone
Lua 5.1, the complete C/auxiliary APIs, production native loading and broader
GC/debug/resource behavior remain open. Later dialects and integrations stay
deferred.
