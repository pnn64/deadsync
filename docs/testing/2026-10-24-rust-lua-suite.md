# Official Lua 5.1 suite probes

The new engine does not pass these official suite files yet. All eight files pass
under the vendored reference interpreter; all eight fail under the new engine.
This is a compatibility gap report, not a passing test-suite claim.

```sh
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-24-rust-lua-suite.json
```

The runner downloads the [official Lua 5.1 suite](https://www.lua.org/tests/),
checks SHA-256 `49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad`,
and extracts it under `target/lua-reference-suites`. Files are executed unchanged,
independently, in separate processes with the suite directory as their working
directory. `_U=true` selects the documented basic-test portability mode, and the
bootstrap defines `Message` to print the suite's notices. A 100-million-instruction
Rust limit and 60-second per-process deadline bound execution. Failures cause a
nonzero exit. `all.lua`, internal C API tests and the complete suite remain open.

The [JSON report](2026-10-24-rust-lua-suite.json) contains captured output,
diagnostics, time, file digests and engine source hashes. Initial stopping points:

| File | Reference | Rust stopping point |
| --- | --- | --- |
| `constructs.lua` | Pass | Line 152: missing `string.gsub` |
| `locals.lua` | Pass | Line 92: missing `string.format` |
| `calls.lua` | Pass | Line 124: missing string library |
| `events.lua` | Pass | Line 7: missing `collectgarbage` |
| `nextvar.lua` | Pass | Line 174: missing `table.foreach` |
| `literals.lua` | Pass | Line 6: missing `string.len` |
| `closure.lua` | Pass | Line 13: missing `collectgarbage` |
| `vararg.lua` | Pass | Line 14: missing `table.getn` |

The first run also exposed `getfenv(nil)` being rejected instead of using its
default stack level. That behavior was corrected and added to differential
regressions; `locals.lua` now proceeds to its missing-library call at line 92.
Later failures inside these files are still unknown. Reaching the current first
failure does not prove that every earlier Lua behavior is correct.

Targeted differential coverage separately exercises metatable dispatch order,
shared comparison handlers, deleted-key iteration, native function identity,
protected calls, error identity, handler execution before unwind, function/thread
environments, `loadstring`, named error locations, print output and tail calls
through resumable native frames. These tests pass, but they cover a smaller scope
than the official suite.
