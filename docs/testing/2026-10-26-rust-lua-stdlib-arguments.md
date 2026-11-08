# Lua 5.1 library argument validation

`math.ldexp` now validates its exponent before its mantissa, matching the pinned
Windows/MSVC Lua 5.1.5 build. For example, `pcall(math.ldexp,nil,nil)` now reports
bad argument #2; the previous engine reported #1. Missing arguments follow the
same order. Successful scaling and string-number coercions retain their results.
The C expression's argument evaluation order can vary by compiler; this is the
existing Windows/MSVC reference contract, not a portable Lua language guarantee.

The new `scripts/check_rust_lua_stdlib.py` runs 5,507 bounded cases across 65
library entry points. It observes protected-call status, primitive result bytes,
object types and complete process output. Cases include missing arguments,
booleans, numbers, numeric/invalid/NUL-containing strings, tables, functions,
threads and userdata. The generator omits environment/local/type-metatable
mutation and exit/shell/file APIs. Its input selection is fixed in the script;
this is regression coverage, not exhaustive standard-library validation.

| Check | Result |
| --- | --- |
| Previous executable, library matrix | 5,442/5,507 match; 51 ldexp and 14 loadlib differences |
| Current executable, library matrix | 5,493/5,507 match; 14 loadlib differences |
| Rust tests | 555 pass: 72 unit, 479 integration, 4 documentation |
| Formatting and strict Clippy | Pass |
| Normal/build dependency tree | Only deadlib-lua |
| Expanded standalone CLI | 183/221 raw; 221/221 after one exact leading version-banner removal |
| Official math.lua, C/Rust/cache store/cache hit | Pass in all four modes |

The [parent matrix](2026-10-26-rust-lua-stdlib-arguments-parent-library.json)
and [current matrix](2026-10-26-rust-lua-stdlib-arguments-library.json) execute
identical generated source and retain both complete outputs. Both observe every
case, finish successfully at the process level, and exit the audit with status 1
because semantic differences remain. No mismatch allowlist changes that status.

All fourteen remaining differences are `package.loadlib` calls with valid
string arguments. Official Lua attempts Windows DLL loading and reports an
`open` failure for these paths. The Rust engine returns its unsupported-loader
message and `absent`. Actual native-module loading and these failure results
remain compatibility work; the audit keeps their exact bytes.

The [parent CLI audit](2026-10-26-rust-lua-stdlib-arguments-parent-cli.json)
fails exactly the two added ldexp cases. The
[current CLI audit](2026-10-26-rust-lua-stdlib-arguments-cli.json) resolves both.
The 38 intentional project-version banner differences remain visible in raw
observations, and the strict CLI audit still exits 1.

New Rust tests compare competing invalid arguments, missing arguments, ignored
extras, successful coercions and imported native chunks against the development
C oracle. Persistent-cache tests now store and hit eight valid entries in fresh
children, including the ldexp regression, then execute again after collection.
The [official math report](2026-10-26-rust-lua-stdlib-arguments-math.json) keeps
the prior unchanged source hash, bootstrap, assertions and execution limits.
Other official files were not rerun for this isolated math validation change.

[Validation metadata](2026-10-26-rust-lua-stdlib-arguments-validation.json)
pins five complete source maps, the previous committed executable, current
executable, retained C executables, build inputs and check logs. Windows linking
produces distinct executable hashes on separate builds; each C audit executable
is retained separately. The final Rust source revision used a new Cargo target,
and the final ordinary CLI build followed the other builders.

No performance benchmarks ran. Full standalone Lua 5.1, complete library/debug
behavior, native C modules, the full official suite, later dialects and gameplay
integration remain unfinished. Compatibility work continues before tuning.
