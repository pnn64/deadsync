# Lua 5.1 string and table compatibility

Three unchanged official files now pass in the Rust engine: `locals.lua`,
`vararg.lua` and `pm.lua`. All eleven probes pass in the vendored Lua 5.1.5
reference. The overall compatibility gate remains failing.

The [JSON report](2026-10-24-rust-lua-stdlib-suite.json) preserves stdout,
diagnostics, source hashes and timings. The runner uses the same checksum-pinned
[official suite](https://www.lua.org/tests/), `_U=true` portability mode,
100-million-instruction limit and 60-second process deadlines as the
[initial report](2026-10-24-rust-lua-suite.md). Files run independently; this does
not claim passage of `all.lua`, internal C API tests or the complete suite.

| File | Rust result or first stopping point |
| --- | --- |
| `constructs.lua` | Line 189: missing `debug.getinfo` |
| `locals.lua` | Pass |
| `calls.lua` | Line 239: missing reader-based `load` |
| `events.lua` | Line 7: missing `collectgarbage` |
| `nextvar.lua` | Line 230: missing `collectgarbage` |
| `literals.lua` | Dynamic chunk line 11: missing `debug.getinfo` |
| `closure.lua` | Line 13: missing `collectgarbage` |
| `vararg.lua` | Pass |
| `strings.lua` | Line 150: missing `os.setlocale` |
| `pm.lua` | Pass |
| `sort.lua` | Line 22: missing `math.random` |

Reproduce with:

```sh
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-stdlib-suite.json
cargo test -p deadlib-lua
```

Nine new differential groups compare byte-string operations, binary data,
captures, backtracking, balanced matches, frontiers, independent iterators,
replacement callbacks/tables, nested callbacks, error unwinding, formatting,
raw table operations and sorting. Generation adds 4,000 short byte-pattern cases,
2,000 float-format cases and sort comparison traces for all permutations through
length six. Scalars compare bytes, arity and floating-point bits; NaN payloads and
unspecified hash traversal order are excluded. Windows formatting tests account
for stock Lua 5.1's 32-bit C `long` and UCRT indefinite-NaN text.

String dumps, complete library diagnostics, numeric edge cases, locale support
and allocation/resource limits remain incomplete. The pattern work limit and
table operation limits are explicit engine limits. Passing the current corpus
does not establish compatibility for arbitrary scripts.
