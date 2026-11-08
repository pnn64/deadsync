# Resumable Lua 5.1 scanner validation

The compiler now uses a scanner that can pause when its append-only input is
exhausted and resume when more bytes arrive. An explicit closed-input flag
distinguishes a request for input from an EOF token. Cursor, line, delimiter,
numeric and string-decoding state survive suspension; partial tokens are not
restarted from their beginning. Fixed-source compilation uses the same scanner
without copying the source into an additional input buffer.

Five scanner groups bring the unit total to eight alongside the arena/verifier
tests. All 69 existing integration groups, the documentation example and strict
Clippy pass. Coverage includes:

- Every two-way split and one-byte delivery of representative keywords, names,
  numbers, punctuation, comments, quoted strings and long strings.
- Decimal escapes, escaped newlines, CR/LF pairs, long delimiters and malformed
  input split across boundaries, comparing token kinds, bytes, number bits,
  lines and errors with fixed-source scanning.
- 2,000 deterministic arbitrary byte inputs under one-byte delivery.
- 100 KB names, strings and comments delivered one byte at a time, retaining
  partial state across requests.
- The next-character fetch before token completion, checked against protected
  reader-based `load` calls in vendored Lua 5.1.5. Fragments including a complete
  quote, punctuation, an invalid character and an oversized escape request the
  next chunk before returning a token/error. A reader's arbitrary Lua error
  remains the load result's error object when the surrounding call is protected.

```sh
cargo test -p deadlib-lua
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-stream-lexer-suite.json
```

The [official report](2026-10-25-rust-lua-stream-lexer-suite.json) retains thirteen
unchanged files, `_U=true`, a 100-million-instruction budget and 60-second
deadlines. All thirteen reference probes pass. Rust retains six passes:
`locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua` and `gc.lua`.
The remaining stopping points are unchanged from the
[userdata report](2026-10-25-rust-lua-userdata.md).

This is lexical resumption, not a reader-loader implementation. The parser still
consumes fixed input and compilation still has unfinished diagnostics/resource
compatibility. Reader call order, early parse/compile errors, callback protection,
environments, yielding and binary chunks need end-to-end validation when loading
is connected. The [measurements](../performance/2026-10-25-rust-lua-stream-lexer.md)
cover fixed-source compilation scaling; one-byte delivery is exercised by tests.
