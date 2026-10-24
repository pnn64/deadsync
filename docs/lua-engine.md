# Independent Lua engine

The full objective is a standalone Rust Lua 5.1–5.5 implementation, followed by
song and noteskin integrations sharing the compiler, VM, heap and semantics.
The first completion target remains arbitrary standalone Lua 5.1 programs with
correct libraries, GC, closures, metatables, coroutines and errors. The current
implementation is an early functional stage of that target.

## Preservation and current scope

The mlua fork lives in `crates/deadlib-lua-legacy`, including tests, benchmarks,
examples, macros, FFI and existing optimizations. Its three Cargo packages use
the `deadlib-lua-legacy` prefix. Rust imports remain `mlua`, `mlua_sys` and
`mlua_derive`. All production consumers select the legacy workspace alias.
Current benchmark scripts and Markdown commands follow the move; historical
performance JSON keeps its recorded measurement provenance.

| Requirement | Evidence and remaining work |
| --- | --- |
| Preserve legacy and ongoing changes | Move and references updated. Full legacy feature tests pass; song Lua: 843 passed, 41 ignored; noteskin: 298 passed, 10 ignored; theme check and 3 integer-width regressions pass. Original runtime/test/example source contents were compared across the move. |
| Independent compiler/interpreter | Byte lexer, flat AST arenas, register compiler, verifier, literal folding, VM and CLI exist. Complete reference-suite coverage remains open. |
| Exact language semantics | Differential cases cover scalar bits, strings, assignments, loops, closures, varargs, methods, tail calls, multiple results, metatables, protected calls, arbitrary error identity, named locations, environments and coroutine transitions. Full diagnostics, debug metadata and broader numeric/resource edge cases remain open. |
| Object model and memory | Copyable tags and numeric IDs; vector registers, frames, prototypes and upvalue cells; interned strings; contiguous table arrays and indexed hash entries. Deleted keys retain traversal positions until rebuild. Host handle provenance and reclamation remain open. |
| Garbage collection | Unimplemented. Requires roots, cyclic graph tracing, weak tables, finalization, pressure tests and collector controls. |
| Coroutines | Shared-heap threads have separate register/frame vectors and environments, explicit resume/yield scheduling, wrappers/status, and cross-thread upvalue access. Lua 5.1 native/metamethod yield restrictions, inherited C-call depth, error identity and quota recovery are tested. Thread reclamation, debug stacks/hooks, full diagnostics and broader resource tests remain open. |
| Standard libraries | Expanded base library includes radix conversion, iteration, metatables, protected calls, output, environments and dynamic compilation. String byte operations, patterns, replacement callbacks, formatting, raw table operations/traversal/sorting, coroutine functions and the 5.1 math function set exist. Complete base, I/O, OS, package/require, debug, binary dumps and broader numeric/diagnostic compatibility remain open. |
| Persistent compiled cache | Unimplemented. Requires bounded decoding, bytecode verification, format/dialect/platform identity and collision-safe source checks. |
| Native host calls | Builtins use numeric IDs and register argument ranges. Public callback registration, host objects, error/yield handling and overhead measurements remain open. |
| Lua 5.2–5.5 | Unimplemented. Investigated differences below must extend shared infrastructure with per-version oracle gates. |
| Machine code | Optional; defer until compatibility baselines and profiles identify profitable paths. |
| Song integration | Deferred until engine gates pass. Implement live callbacks, independent baking, auto by default, dynamic fallback, and shared state/identity across strategies. |
| Noteskin integration | Deferred. Create `deadsync-noteskin-lua` with the shared engine, reusable programs, static actor/asset caches and necessary dynamic callbacks. |
| Correctness testing | Differential cases and deterministic generation exist. A checksum-pinned official 5.1 runner records twelve reference passes and three unchanged Rust passes (`locals.lua`, `vararg.lua`, `pm.lua`); other files reach missing functions. Math now reaches I/O, and sort reaches `os.clock`. Full suite passage, broader source/bytecode fuzzing and per-dialect CI remain open. |
| Performance | Initial reference harness exists. Expand throughput, startup, compile cost, allocations, live/peak memory, scaling, representative workloads and other Rust engines. Preserve samples and machine/build metadata. |
| Independent stages | The literal-folding pass is separate internally. Public compile/optimization and scene-baking stages, cached stage identities and equivalence gates remain open. |

Next work should finish the standalone engine: collection and userdata,
remaining standard libraries, complete diagnostics/debug behavior, binary loading and
caches. Production migration requires the complete compatibility and
performance gates; the current regression corpus does not establish them.

Validation also includes strict Clippy, a dependency tree containing only the new
crate, 47 differential/adversarial integration tests, verifier mutation tests and
a runnable documentation example. Generated cases include 1,000 numeric/logical
expressions, 325 metamethod-handler combinations, 5,120 arbitrary byte inputs,
4,000 byte-pattern cases, 2,000 float formats, 2,000 binary-scaling cases and 2,000
radix conversion cases. Sort tests compare comparator
traces across all permutations through length six, callback mutations, nested
sorting, errors and metatables. Coroutine generation exercises 300 resume sequences
and recursive generators, alongside nested resumers, native callbacks, environments,
failed/suspended thread cells, result arity, C-call limits and quota recovery.
The [official suite report](testing/2026-10-24-rust-lua-coroutines.md) identifies the
current failing files without changing or excluding their assertions. The initial
[performance baseline](performance/2026-10-24-rust-lua-baseline.md) records
execution ratios of 2.38–13.51 times the reference duration. Subsequent
[compatibility and dispatch measurements](performance/2026-10-24-rust-lua-dispatch.md)
recorded ratios of 2.57–6.63 with expanded semantics. The latest
[string/table measurements](performance/2026-10-24-rust-lua-stdlib.md) add six
workloads and confirm zero warmed allocation events for pattern search and
replacement workloads. The [coroutine measurements](performance/2026-10-24-rust-lua-coroutines.md)
add resume, wrap and deep-yield scripts. Substantial speed work and broader measurements remain
after the language/compatibility gates.

Pattern programs use compact instructions and class bitmaps. Backtracking,
replacement callbacks and sort comparisons resume through explicit VM state.
Scratch buffers and bounded sort partitions avoid per-match/partition allocation.
Their values and cached patterns must become collector roots or weak caches when
GC is added. Library error text, numeric conversions, resource behavior and
non-C locales still need broader validation; current coverage is not a complete
standard-library compatibility claim.

The math library uses bit-based `frexp`/`ldexp`, including subnormal tie rounding,
and preserves `modf` result arity and signed zero. Radix conversions follow the
platform unsigned-long width and saturation. The pinned MSVC random stream is
shared by Lua states on each OS thread, as in the Windows CRT; erroring random
calls still advance it. Other CRT random streams are not emulated. Differential
checks pin the Windows MSVC/C-locale reference and do not establish cross-libm
transcendental equivalence.

Coroutine switches transfer ownership of vectors without copying stack contents.
Open cells include their owning thread ID. Active registers remain direct VM
fields, and shared-heap references do not adjust reference counts. Yield-boundary
depth is inherited in call frames; fixed result counts use 16 bits and retain an
open-result sentinel. A compile-time assertion keeps frames at 64 bytes on 64-bit
targets; values remain 16 bytes in measurements. The global instruction budget
covers coroutine execution and aborts active resumer chains before restoring main.

The collector must trace active/normal/suspended stacks and native continuation
state. Surviving closures can reference cells in otherwise unreachable threads;
collecting such a thread must close those cells without retaining the entire
thread through its cell-owner ID. Failed thread frames remain available for later
debug inspection. These requirements are not yet implemented or validated by GC
tests. Commit each verified milestone while keeping unrelated edits separate.

## Reference contract

The oracle is `vendor/lua-src/lua-5.1.5`, linked only for development through the
legacy binding; its registry-capacity patch remains. Record its source digest
with measurements. Current validation uses x86-64 Windows MSVC, `+crt-static`,
binary64 numbers and the C locale. No cross-platform transcendental-bit guarantee
has been established.

Byte-exact claims must pin source, version/build options, numeric widths, locale,
CRT/libm, OS behavior, random seeds, paths, newlines and output encoding. Compare
bits and bytes directly and identity through predicates rather than addresses.
NaN payloads, unspecified traversal order and hole borders require explicit
comparison policies. Arbitrary Lua errors must eventually retain value identity.

Differential tests cover several observable PUC compiler details: literal folding
avoids division by zero and NaN constants; numeric constant pooling merges signed
zeros; numeric loops subtract before their initial increment; table list fields
flush in groups of 50, affecting duplicate keys mixed with record fields.

## Dialect investigation

These are semantic seams to implement and verify, not available dialect flags.
Official manuals are authoritative:

| Dialect | Required investigation and regression tests |
| --- | --- |
| [5.1](https://www.lua.org/manual/5.1/manual.html) | Function/thread environments, float-only numbers, old vararg `arg`, coercions, weak tables and coroutine boundaries. |
| [5.2](https://www.lua.org/manual/5.2/manual.html#8) | Lexical `_ENV`, goto, escapes/literals, ephemeron keys, length/iteration metamethods and library/yield changes. |
| [5.3](https://www.lua.org/manual/5.3/manual.html#8) | Integer/float subtypes, overflow, integer division, bitwise operations, formatting, UTF-8, packing and numeric keys. |
| [5.4](https://www.lua.org/manual/5.4/manual.html#8) | Const/closing locals, unwinding, integer-loop overflow, coercion/metamethod changes, GC modes, RNG and coroutine close. |
| [5.5](https://www.lua.org/manual/5.5/manual.html#8) | Global declarations, named varargs, read-only loop controls, call-chain limits, nil errors and collector parameters. |

The internal ISA can share infrastructure without reproducing each reference
bytecode format. Official chunk import/export requires separate validation and
must remain distinct from our persistent cache format.

## Architecture and testing research

Studied [Luna architecture](https://github.com/goliajp/luna/blob/cd4ecfe3ee2ec4c58f8cc741bdf0463d1bf5d771/docs/architecture.md)
and [compatibility testing](https://github.com/goliajp/luna/blob/cd4ecfe3ee2ec4c58f8cc741bdf0463d1bf5d771/docs/compatibility.md).
Useful ideas include a dependency-free interpreter core, an optional native-code
backend and separate frontend/binary-reader/runtime fuzzing against official Lua.
This engine currently uses safe indexed storage; no intrusive collector or JIT
interface was adopted.

Studied [OmniLua version analysis](https://github.com/ianm199/omnilua/blob/90dcf85a8cfb95305b77056af00a114fd0df30a1/specs/MULTIVERSION_ARCHITECTURE_DECISION.md)
and [benchmark methodology](https://github.com/ianm199/omnilua/blob/90dcf85a8cfb95305b77056af00a114fd0df30a1/harness/bench/README.md).
Its analysis identifies number models, environments and libraries as substantive
version boundaries. Its multiple-backend proposal is not a requirement here.
Useful practices include pinned per-version oracles, checksums before timing,
interleaved samples, separate compile/run costs and scaling workloads that expose
nonlinear behavior. No implementation code was imported from either project;
neither is a dependency.
