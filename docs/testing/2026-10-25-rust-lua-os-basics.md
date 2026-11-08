# Lua OS basics validation

The partial `os` module provides `difftime`, `getenv`, `remove` and `tmpname`.
It shares the builtin `_LOADED` registry, native argument diagnostics, native
environments, collector and existing file-error tuples. Production code still
forbids unsafe Rust and has zero normal/build dependencies. Legacy production
consumers remain on `deadlib-lua-legacy`.

`difftime` validates the optional second argument before the required first
argument, matching evaluation in the pinned MSVC C build. Both numbers convert
to signed 64-bit `time_t` before subtraction. Fractional values truncate first;
NaN/infinite/out-of-range conversions and negative times retain observed CRT
behavior. Zero is accepted. Negative converted times return positive zero and
record `EINVAL`. This differs from a floating-point subtraction and from the
current Microsoft documentation's claim about zero. The installed Windows SDK
10.0.26100.0 `ucrt/time/difftime.cpp` and the actual oracle both accept zero;
the source file SHA-256 is
`8ad6004b5a2b40876140c9769f8c449920ffdac1dd09e08ac8754197cb1c84e6`.
See [Microsoft's difftime reference](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/difftime-difftime32-difftime64?view=msvc-170)
for the documented parameter domain; differential observations govern this
milestone's pinned behavior.

Environment names coerce numbers to strings and stop at their first NUL.
Queries return a byte string or one nil result. The engine reads the current
OS process environment; it does not share a mutable CRT environment snapshot
with external native libraries. Windows paths and environment names/values
currently use UTF-8 conversion. Byte-exact reference checks cover ASCII;
Windows ANSI code pages and external CRT environment updates remain open.
Unix paths/environment values retain raw OS bytes, but this run does not
establish POSIX reference coverage.

`remove` returns one true result on success or nil, the complete filename/error
text and numeric errno on failure. Paths use the same conversion and embedded-NUL
handling as file opening. The Windows checks retain denial for directories and
files still held open. Missing components, invalid paths and empty names retain
their observed tuples. This corpus does not establish every device, permission,
sharing or symlink behavior.

Temporary names use the system temporary directory, process ID, timestamp and an
atomic sequence, rather than reproducing the CRT's unspecified name spelling.
Windows checks unused names without creating files, matching `tmpnam`'s file
creation behavior. POSIX code creates private empty files with exclusive create
and mode 0600. Name spelling/location and the next name after external CRT calls
are outside byte-exact comparisons. The host must provide an accessible temporary
directory; encoding and unusual-directory cases need broader coverage.

All **317 tests** pass: 303 integration groups, thirteen unit groups and one
documentation example. Eight new differential groups compare against vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT and C locale:

| Group | Observations |
| --- | --- |
| Time differences | 22 numeric/string inputs, 484 ordered pairs and 44 default/nil-start calls; exact result bits. |
| Argument errors | Complete bytes for missing/nil/wrong types, simultaneous invalid operands, aliases and method bad-self errors. |
| Environment | Missing/empty/invalid names, numeric coercion, NUL truncation, positive ASCII environment values and result arity. |
| File removal | Success arity, repeated removal, NUL boundaries, missing files and full failure tuples. |
| Open handles/directories | Failed removal preserves file bytes and leaves the directory intact. |
| Invalid components | Missing parents, file parents, trailing separators, wildcards and punctuation retain complete error tuples. |
| Temporary names | Sixteen unique names, platform file-creation predicates, binary file round trips, collection and removal. |
| Module/native state | `require`/`package.loaded` identity, native environments, registry retention and removed-field error names. |

Formatting, strict Clippy, whitespace and zero normal/build dependency checks
pass. Commands:

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-os-basics-suite.json
```

The [official probe report](2026-10-25-rust-lua-os-basics-suite.json) retains
sixteen unchanged checksum-pinned files, `_U=true`, a 100-million-instruction
budget and 60-second per-file deadlines. All 65 recorded source/script hashes
match the retained tree. Rust passes eleven files: `constructs.lua`, `locals.lua`,
`nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`, `closure.lua`,
`db.lua`, `math.lua` and `errors.lua`. Fifteen reference files pass.
The suite command deliberately exits unsuccessfully while failures remain.

| Remaining Rust probe | Current stop |
| --- | --- |
| `calls.lua` | Missing binary `string.dump`, line 243. |
| `literals.lua` | Missing `os.setlocale`, line 161. |
| `strings.lua` | Missing `os.setlocale`, line 150. |
| `sort.lua` | Missing `os.clock`, line 25. |
| `files.lua` | Missing `os.setlocale`, line 22, after both temporary-name calls succeed. |

The C reference's retained `files.lua` failure remains the Windows open-input
rename assertion at line 59. No official assertions are changed or excluded.
These are independent file probes, not a complete `all.lua` or C API run.

[Measurements](../performance/2026-10-25-rust-lua-os-basics.md) retain five paired
samples in each of two runs for 41 runtime and twenty-two frontend workloads.
`os.clock`, calendar/time zones, locales, rename and process operations remain
unfinished. Binary chunks, native modules, incremental GC, broader numeric,
resource/debug compatibility, caches, later dialects and integrations also remain
open. This milestone does not complete standalone Lua 5.1.
