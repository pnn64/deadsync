# Lua 5.1 coroutine validation

The engine now implements `coroutine.create`, `resume`, `running`, `status`,
`wrap` and `yield`. Threads share the VM heap, compiled code, tables, closures,
metatables and native function identities. Each thread has its own registers,
frames, environment, open-cell list and native continuation state. An explicit
scheduler switches vector ownership without recursively entering the interpreter.

Ten new differential/adversarial groups bring the integration total to 47. The
verifier test, documentation example and strict Clippy also pass. Observations
compare scalar bits/bytes, result counts and identity predicates against the
vendored official Lua 5.1.5 interpreter on Windows MSVC in the C locale.

The tests cover:

- Multiple initial/yield/resume/return values, nil holes, empty results, normal
  completion, tail-position yields, dead-thread resumes and self resumes.
- Running/suspended/normal/dead status, nested resumers and protected callers.
- Wrapper function identities mixed with pattern iterators, string and arbitrary
  error objects, and source-location prefixes when wrappers propagate errors.
- Closures reading and writing cells in active, normal, suspended and failed
  threads, plus suspended threads retained across separate host executions.
- Thread environments, dynamically compiled chunks, debug environment access,
  native environments, shared thread metatables and thread keys. New threads
  inherit thread globals; native wrappers/iterators inherit their creating C
  function's environment, including environments changed through debug APIs.
- Lua 5.1 rejection of yields through protected calls, metamethods, replacement
  callbacks, traversal and sort comparators; ordinary `__call` invocation remains
  yieldable. Child coroutines can yield when resumed by these callbacks.
- Native jobs retained while child threads execute, inherited C-call depth at
  the resume limit, and quota aborts restoring a usable main thread while leaving
  unrelated suspended threads intact. A first-resume overflow leaves the new
  thread dead; an already yielded thread remains suspended and can be retried.

Generation additionally exercises 300 varying resume sequences and a recursive
generator yielding the same table identity 625 times. The CLI was checked with
a wrapped square generator, printing `1`, `4`, `9`.

```sh
cargo test -p deadlib-lua
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_suite.py --output docs/testing/rust-lua-coroutines-suite.json
```

The [official probe report](2026-10-24-rust-lua-coroutines-suite.json) retains the
checksum-pinned, unchanged twelve-file suite probes, `_U=true`, a 100-million-
instruction budget and 60-second deadlines. All twelve reference files pass.
Rust still passes `locals.lua`, `vararg.lua` and `pm.lua`; `closure.lua` stops at
missing `collectgarbage` before its coroutine section. Other stopping points
remain the missing functions listed in the [math report](2026-10-24-rust-lua-math-suite.md).
The full official coroutine/GC/debug coverage is not established by these probes.

Garbage collection of threads and surviving open cells, weak references,
finalization, debug stack inspection/hooks, complete diagnostics, broader resource
limits and later-dialect yield behavior remain open. The current heap retains
threads until VM destruction. No gameplay integration uses the new engine.
