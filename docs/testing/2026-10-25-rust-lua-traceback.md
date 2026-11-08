# Lua 5.1 traceback validation

`debug.traceback` now formats the current stack or another thread's retained
stack, including source names, current lines, inferred function names, native
frames, lost tail calls and the reference's first/last-frame truncation. Lua
callbacks run through explicit VM continuations rather than recursive interpreter
entry. Reusable job vectors move with each coroutine's register/frame vectors.

Compatibility includes Lua 5.1's unusual argument behavior. A numeric level
causes the last argument to be popped, even when extra arguments follow it. An
explicit nil or non-string/non-number message returns the remaining top argument.
Numeric strings, fractional levels, negative levels and non-finite/large indices
are compared against the pinned Windows x86-64 MSVC reference. The shared debug
index conversion now matches its signed 64-bit conversion followed by 32-bit
narrowing; this is not a claim about all engine numeric conversions.

Formatting concatenates each row right to left, so extra table arguments and
debug-local mutations can invoke `__concat`. Callback-visible slots retain the
original right-hand pieces until the complete concatenation finishes, matching
the C API's stack top even while its active operand count shrinks. The target
thread also has a hidden register root so changing a public argument cannot
invalidate a pending formatter. Nested tracebacks, collection and coroutine
switches preserve independent jobs. Native concatenation errors retain their
unprefixed error bytes; Lua callback errors retain arbitrary payload identity.

All 129 tests pass: 116 integration groups, twelve unit groups and one runnable
documentation example. Formatting, strict Clippy and the normal/build dependency
gate pass. The engine forbids unsafe code and has no normal/build dependencies.
Fourteen new integration groups cover:

- Message types, extra-argument stack behavior, default/explicit levels and
  numeric coercions, including infinities, NaN and wrapped large indices.
- `xpcall` handlers inspecting faulting frames before unwind, with table payloads.
- Truncation at multiple depths and levels, ordinary calls, lost tail records and
  30,000 tail calls without expanding them into individual frames.
- New, suspended, normal, failed and dead coroutines, plus current-thread queries.
- Concatenation callback order and all twenty queried native slots, comparing
  retained values and exact string fragments through collection.
- Nested formatting, protected callback failures and independent job recovery.
- Native error bytes and mutation of the actual pending message operand.
- Allowed child-coroutine yields, prohibited direct callback yields and parent
  formatter state across context switches and collection.
- Retention of a target thread after its public argument is overwritten, followed
  by reclamation after formatting completes.
- Quota abort inside a callback, job/root disposal, dead worker state and recovery
  in the main thread.
- The reference's native-call depth limit during recursively nested formatting
  callbacks (197 callback visits in this fixture).
- Shared debug index conversion in info, local and upvalue inspection/mutation.
- Twenty batches of 1,000 traces with full collection: over 40,000 objects reclaimed,
  reserved-capacity estimates below 4 MB and fewer than twenty retained functions.
- Unprotected-error cleanup releasing an unrelated weak local while preserving a
  host-pinned table error payload.

The differential oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC,
static CRT and C locale. Tests use a plain `lua_pcall` entry with no host error
handler beneath the chunk, compare number bits/string bytes and preserve result
arity. Target-argument overwrite and quota-root tests are engine-only: overwriting
the C implementation's only thread root while it holds a private pointer can make
that oracle unsafe. The host cleanup and bounded-storage cases are also engine-only.
Private parser values and broader native scratch-stack compatibility remain
outside the verified scope described in the [local inspection report](2026-10-25-rust-lua-debug-locals.md).

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua -- --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-traceback-suite.json
```

The [official probe report](2026-10-25-rust-lua-traceback-suite.json) adds unchanged
`errors.lua`, bringing the default selection to fifteen files. All fifteen
reference probes pass; Rust retains eight passes: `constructs.lua`, `locals.lua`,
`nextvar.lua`, `vararg.lua`, `pm.lua`, `events.lua`, `gc.lua` and `closure.lua`.
The archive checksum, individual source hashes, `_U=true`, 100-million-instruction
budget and 60-second per-file deadlines remain recorded.

| Remaining failing file | Current stopping point |
| --- | --- |
| `calls.lua` | `string.dump`, line 243 |
| `literals.lua` | `os.setlocale`, line 161 |
| `strings.lua` | `os.setlocale`, line 150 |
| `sort.lua` | `os.clock`, line 25 |
| `math.lua` | `io.tmpfile`, line 124 |
| `db.lua` | `debug.sethook`, line 19 |
| `errors.lua` | Syntax-message assertion in `checksyntax`, line 20; the `break label` case lacks the expected `near 'label'` detail |

Assertions are unchanged. These are independent probes, not full `all.lua` or C
API coverage. Hooks, complete diagnostic text, remaining libraries, binary chunks,
incremental collection pauses, broader resource limits and later dialects remain
unfinished. Production continues to use the preserved legacy engine. See the
[paired performance report](../performance/2026-10-25-rust-lua-traceback.md).
