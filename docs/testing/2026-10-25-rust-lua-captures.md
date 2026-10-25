# Lua 5.1 lexical capture limits

The text frontend checks the 60-upvalue limit during parsing, including source
that never supplies its closing `end` tokens. Captures propagate from the owning
scope through nested functions; an overflow identifies the first overflowing
ancestor's definition line. This preserves the official test's error at the
function beginning on line 3, before its unfinished source reaches EOF.

The scanner exposes completed identifier spans. A flat, collision-checked name
pool keeps offsets into the retained source prefix, without copying another set
of identifier bytes or enlarging tokens/AST nodes. Active scopes share one local
stack. Capture nodes use indices and a free list, allowing captures added to a
parent by a child to survive that child's exit. Suspended parser heap accounting
includes these buffers. Code generation keeps its defensive capture limit.

References resolve after next-token fetch, preserving lexical-error and reader
callback precedence. Local declarations activate after their initializers;
recursive local functions activate before their bodies. Parameters, method
`self`, compatibility `arg` and loop variables have their corresponding scopes.
Repeat-body locals remain active through `until`. Record keys, field names,
method names and unbound globals consume no captures.

All 269 tests pass: 255 integration groups, thirteen unit groups and one runnable
documentation example. Ten new groups compare 398 loader observations against
vendored official Lua 5.1.5, using Windows x86-64 MSVC, static CRT and C locale.
Observations compare successful loading, protection status, complete diagnostic
bytes and reader-call counts. Seven valid-source cases additionally assert that
the reference accepts the fixture before comparing engines.

| Corpus | Paired observations |
| --- | ---: |
| Direct boundary counts, complete and unfinished functions | 15 |
| Nested propagation and owning-function diagnostics | 10 |
| Next-token and lexical-error precedence, multiple reader widths | 56 |
| Reader callback failures around the boundary | 25 |
| Globals, record keys, fields and method names | 2 |
| Shadowing and initializer activation | 6 |
| Loops, parameters, recursion, `self` and `arg` | 12 |
| Repeat-condition scopes | 6 |
| Sequential closures and surviving parent captures | 2 |
| 256 generated lexical layouts, eight repeated with chunked readers | 264 |

Formatting, strict Clippy and the zero normal/build dependency gate pass. Engine
code remains safe Rust. The [measurements](../performance/2026-10-25-rust-lua-captures.md)
include loader errors and closure-heavy frontend scaling.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-captures-suite.json
```

The [official report](2026-10-25-rust-lua-captures-suite.json) preserves sixteen
unchanged probes, the archive checksum, `_U=true`, a 100-million-instruction
budget and 60-second per-file deadlines. Rust passes eleven: `constructs.lua`,
`locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`,
`closure.lua`, `db.lua`, `math.lua` and now `errors.lua`. Fifteen reference probes
pass; the retained `files.lua` reference failure remains the Windows rename of a
still-open default input at line 59.

| Remaining Rust probe | Stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `files.lua` | `os.tmpname`, line 19 |

This is a capture-limit milestone, not complete compiler or Lua compatibility.
Successful code generation still recurses on the host stack: a completed valid
59-level nested-function fixture overflowed a small Windows debug test-thread
stack. Deep unfinished and overflowing inputs exercise the resumable parser;
successful nested code-generation probes here use at most twelve levels. Flat
code generation and exact register/expression/resource limits remain required.
Independent official probes do not cover complete `all.lua` or the C API.
Remaining libraries, binary chunks, native C modules, incremental GC, complete
debug/resource behavior, persistent caching, later dialects and integration also
remain open. Production continues using the preserved legacy engine.
