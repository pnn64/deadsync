# Lua 5.1 source-diagnostic validation

The text frontend preserves expected-token labels, `near` context and the source
line at which the scanner reports an error. Missing closing symbols identify the
opening symbol and line when the failure is on another line. Assignment targets,
call statements, parameters and ambiguous newline calls follow the reference's
diagnostic order. Local/parameter limits identify the owning main function or
function definition; assignment limits account for active syntax/native depth.
Other compiler/resource limits still need compatibility work.

`Error::message_bytes()` retains non-UTF-8 source/token bytes. Rust `message()`
and display replace invalid UTF-8 for readability; Lua loading functions receive
the original bytes. Source-name truncation, NUL-terminated diagnostic context,
control-character labels, decoded string escapes, long-string delimiters and
scanner lookahead are covered. Raised runtime values retain their existing
`Error::value()` behavior.

The scanner stores cold span descriptors for diagnostic context. Completed string
tokens are decoded again only when formatting a failure; successful scans keep
no duplicate diagnostic text. Long-token scanning checks eight bytes at once
using safe array reads and zero-byte detection, then finds the first delimiter
within a marked chunk. The tail uses byte checks. Neither path reads outside the
available input or changes reader requests.

Compatibility includes the pinned reference's deprecated nested `[[` handling
and embedded-NUL membership behavior in dot/numeral scanning. Lexical failures
that require fetching the next current character remain pending until the reader
provides it. Reader errors can therefore take precedence over those failures.
Ordinary unrecognized bytes are lexical byte tokens; the parser reports their
context. The scanner unit assertion for `@` now reflects that token behavior.

All 226 tests pass: 212 integration groups, thirteen unit groups and one runnable
documentation example. Sixteen new integration groups provide 13,439 reference
observations and one host-only raw-byte API check:

| Corpus | Reference observations |
| --- | ---: |
| Expected tokens and statement errors | 27 |
| Closing symbols and opening lines | 18 |
| Quoted strings and decoded context | 15 |
| Long strings/comments | 12 |
| Numerals and malformed numbers | 16 |
| Lookahead and current scanner buffer | 13 |
| Every byte in three token contexts | 768 |
| Source-name lengths, prefixes and binary bytes | 29 |
| Generated truncations, deletions and insertions | 4,188 |
| Local, parameter and assignment bounds | 35 |
| Reader widths and request counts | 112 |
| Long-token byte lanes, delimiters and diagnostic lines | 8,192 |
| Named-file headers, CRLF and binary diagnostics | 6 |
| Embedded-NUL executable values/arity | 4 |
| Pending lexical faults versus reader errors | 4 |

The generated source corpus mutates three valid programs using thirteen insertion
bytes. The lane corpus places every byte in each of eight positions within long
strings/comments, with unlevelled and levelled delimiters, and checks the full
resulting diagnostic. Reader cases compare both returned bytes and callback
counts. Four fixed NUL cases additionally execute the loaded function and compare
scalar values and arity. Generated sources have text prefixes, so the C oracle
does not interpret them as untrusted binary chunks.

The reference is vendored official Lua 5.1.5, Windows x86-64 MSVC, static CRT,
C locale and the configured compatibility options. The reader-error identity
cases use an explicit Lua `pcall` around the whole observation, preventing the
binding's inherited host error handler from transforming the arbitrary error
object. Named-file fixtures are uniquely owned under `target` and removed after
the comparison. Engine code remains safe Rust with zero normal/build dependencies.
Formatting, strict Clippy and the dependency gate pass.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-syntax-suite.json
```

The [official report](2026-10-25-rust-lua-syntax-suite.json) retains sixteen
unchanged probes, the pinned archive checksum, `_U=true`, a 100-million-instruction
budget and 60-second per-file deadlines. Rust still passes ten: `constructs.lua`,
`locals.lua`, `nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua`,
`closure.lua`, `db.lua` and `math.lua`. Fifteen reference probes pass. `errors.lua`
now passes its initial syntax assertions and stops in the runtime-message checker
at line 12; runtime operand names remain incomplete.

| Remaining probe | Rust stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `errors.lua` | Runtime-message assertion, line 12 |
| `files.lua` | `os.tmpname`, line 19 |

The retained `files.lua` reference failure is the Windows rename of a still-open
default input file at line 59. These are independent probes, not complete
`all.lua` or C API coverage. The corpus does not establish all syntax/compiler
diagnostics: register/upvalue/constant bounds and wider nesting/resource cases
remain open. Binary chunks, runtime operand provenance, debug/scratch behavior,
remaining OS/base functions, native C modules, incremental GC, persistent caches,
later dialects and gameplay integrations also remain unfinished. Production uses
the preserved legacy engine. See the
[paired measurements](../performance/2026-10-25-rust-lua-syntax.md).
