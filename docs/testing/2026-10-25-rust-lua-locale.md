# Lua locale validation

Windows/MSVC now provides `os.setlocale`, completing this platform's Lua 5.1 OS
function set. It supports all six categories, native names/aliases, default
selection, queries, unavailable locales, NUL truncation and argument diagnostics.
Numeric conversion, number coercion/concatenation, formatting and stream scanning
use locale decimal punctuation and whitespace. Source literals retain period
syntax. Runtime and stream hexadecimal fractions share the existing correctly
rounded Rust conversion, including pinned UCRT subnormal behavior.

Character classification and casing use cached byte maps. Identifier scanning,
number tails and decimal escapes include native ctype digits such as byte 178.
Pattern caches track ctype revisions, including already created iterators and
replacement callbacks that change locales or invoke nested pattern operations.
Ordering uses native collation with Lua 5.1's NUL segmentation; equality remains
byte identity. Calendar formatting already uses the native time category.

All **366 tests** pass: 341 integration groups, twenty-four unit groups and one
documentation example. New integration groups cover locale queries/validation
without leaving C locale, and runtime hexadecimal fractions, signs, extremes,
rounding boundaries and malformed input. Existing scanner/reader, patterns,
numeric, I/O, calendar, coroutine, hooks and GC groups remain green.

All [126 isolated process probes](2026-10-25-rust-lua-locale-process.json) match exit status,
stdout/stderr bytes and recorded file contents across official Lua 5.1.5,
the Rust process runner and CLI. Fifteen locale additions cover:

| Area | Evidence |
| --- | --- |
| Interface | Six categories, queries, failed names, argument ordering and NUL-truncated names/options. |
| Numeric locales | `ptb`, `fr-FR`, `.UTF-8` and Turkish code page 1254: decimal punctuation, signs/infinities, hexadecimal fractions, coercion, formatting, concatenation, radix whitespace and source literals/errors. |
| Byte maps | All 256 byte values, case conversion, twenty positive/negative pattern classes and bracket/complement forms across those four locales. Outputs are hexadecimal encoded so NUL cannot conceal differences. |
| Lexer | Every high byte as a first/later identifier byte and escaped-string digit in C, `ptb` and UTF-8 locales. |
| Cache changes | Cached patterns, existing gmatch iterators and nested replacement callbacks observe ctype changes. |
| Independent categories | Numeric, ctype and time settings preserve each other; composite queries match the CRT. |
| Collation | Pairwise ordering/equality over case, accents, empty strings and embedded NULs in four locales. |
| Streams | Decimal/hexadecimal scanning, stop bytes, native whitespace and numeric writes. |
| Digit grammar | Non-ASCII ctype digits in backreferences and format width/precision; native literal suffix output, field-length checks and raw diagnostic bytes. |

The [official report](2026-10-25-rust-lua-locale-suite.json) retains sixteen unchanged,
checksum-pinned files, `_U=true`, 100-million-instruction limits and sixty-second
file deadlines. Rust now passes fourteen (previously twelve), adding unchanged
`literals.lua` and `strings.lua`; the reference passes fifteen. Rust still stops
at `calls.lua:243` for binary dumps. Both engines fail `files.lua:59`, the Windows
open-input rename assertion. The suite command remains unsuccessful; these are
isolated files, not complete `all.lua` or internal C API coverage.

Formatting, strict crate Clippy with dependencies excluded and owned-path
whitespace checks pass. The normal/build dependency tree contains only
`deadlib-lua`; no Cargo dependency is added. Production stays on the legacy
crate. The compiler, VM, numeric conversion and ordinary byte operations remain
safe Rust. New unsafe code is restricted to the Windows locale ABI module.

The [ABI review](2026-10-25-rust-lua-locale-abi.json) records installed Windows SDK headers and
CRT source hashes. The bridge uses the SDK category constants and reads only
the first `lconv` pointer field. It copies native storage under the same
recursive CRT locale lock used by `setlocale`, and passes ctype functions only
unsigned-byte arguments. Owned, NUL-terminated buffers bound native collation
access; no Lua values or VM pointers cross the ABI. The
[Microsoft setlocale documentation](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/setlocale-wsetlocale)
describes returned storage lifetime, and the installed CRT source verifies the
locking path. Miri cannot execute these native Windows calls.

Locale state is shared across Rust VMs under the default process-global CRT
mode. Cached Rust views are updated through this Lua API. External host CRT
locale changes and host per-thread locale modes are outside the current
contract. Non-Windows native locale calls explicitly return unsupported; no
cross-platform locale compatibility is claimed. Wider encodings, concurrent
locale semantics, invalid multibyte collation inputs and pathological NUL/
collation-length combinations need broader testing.

A [separate retained reproducer](lua51-locale-replacement-digit.lua) uses a
non-ASCII ctype digit as a replacement capture index. The C implementation
subtracts ASCII one from a signed high byte before indexing its capture array;
this reference behavior is undefined. The [isolated observation](2026-10-25-rust-lua-locale-undefined-reference.json)
records its child access-violation exit (3221225477) and Rust's checked
`false, "invalid capture index"` result. It is outside the 126 matching cases;
the native crash is not a required Rust behavior. No mismatch is normalized
into those totals.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-25-rust-lua-locale-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-locale-suite.json
```

All report hashes match the final tree. [Measurements](../performance/2026-10-25-rust-lua-locale.md)
retain two runs for 53 runtime and twenty-two frontend workloads. Atomic GC work,
exact work units, remaining diagnostics/resource behavior, binary chunks/native
modules, caches, host APIs, Lua 5.2-5.5, optional JIT and integrations remain open.
The complete standalone-engine objective remains unfinished.
