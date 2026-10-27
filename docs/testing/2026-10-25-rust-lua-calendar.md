# Lua calendar validation

`os.time` and `os.date` now implement the Lua 5.1 calendar wrapper on the pinned
Windows runtime. All **340 tests** pass: 326 integration groups, thirteen unit
groups and one documentation example. Production consumers still use the legacy
crate. The independent crate has no normal/build dependencies or C Lua linkage.
`os.setlocale` remains the missing OS function.

## Observable calendar behavior

`os.time` reads `sec`, `min`, `hour`, `day`, `month`, `year`, then `isdst` using
the existing flat callback scheduler. Optional numeric fields default to 0, 0
and 12. Required nonnumeric fields raise the reference's complete missing-field
diagnostic. Numeric strings coerce, month/year offsets wrap at the native integer
width, and `isdst` uses Lua truthiness; zero is true. Native normalization changes
an owned record and leaves the original Lua table unchanged. The native -1 time
result becomes nil, including a calendar that maps to that sentinel.

Field metamethods observe earlier mutations, nested calls, collection, arbitrary
error identity and pre-unwind `xpcall` frames. They retain the Lua 5.1 C-call limit
and yield restrictions. Raw fields and table-only index chains finish in the same
native continuation without seven extra scheduler turns. Continuation state lives
in the existing register/frame vectors. Extra call arguments are discarded before
lookups; each successful lookup value is popped before the following callback or
return hook, preserving tested finalizer timing. Caller locals and open upvalues
remain live.

`os.date` validates format before timestamp, accepts numeric formats, truncates
format bytes at the first NUL and recognizes the initial `!` and exact `*t`
suffix. Tables contain the reference's local/UTC fields and boolean DST value.
Formatting follows the vendored Lua 5.1 `loslib.c`: each percent conversion is sent
as a two-character conversion to native `strftime` with a 200-byte buffer;
ordinary bytes and a final lone percent remain literal. Native formatter policy,
including invalid-format process termination on this CRT, is preserved rather
than applying a newer Lua format whitelist.

Native acceptance bounds are measured against the actual linked provider. The
corpus includes negative timestamps, fractional inputs, epoch boundaries, leap
dates, the signed-32-bit boundary, year 3001, huge inputs, NaN and infinities.
The pinned provider accepts explicit timestamp -1 for `date`; this differs from
`time`'s -1 failure convention. No independent 1970/3000 clamp is imposed.

## Native boundary and scope

Windows uses `_time64`, `_mktime64`, `_gmtime64`, `_localtime64` and `strftime`.
The thread-local nine-int calendar result is copied immediately before another
native conversion or Lua callback can run. POSIX declarations for Linux, Android
and Apple use owned `tm` records and reentrant conversion functions, with explicit
layout assertions and musl 32-bit time symbol selection. Native timezone pointers
stay inside that module. Unsupported targets return an explicit calendar error.
Scoped unsafe code contains only platform ABI calls; compiler, interpreter and
heap storage remain safe Rust. No Lua pointers cross the boundary.

The runtime comparison contract remains Windows x86-64 MSVC, static CRT,
binary64 numbers and C locale. Timezone environments are inherited by children;
the controlled DST probes use `UTC0`, `EST5EDT` and `CET-1CEST` in separate
processes. Native timezone names/offset formatting, including UTC-prefix behavior,
is compared without rewriting output. Locale switching, host-driven concurrent
timezone/locale changes, native allocation failures and unavailable host clocks
are outside this validated contract.

## Evidence

Thirteen new integration groups cover:

| Area | Evidence |
| --- | --- |
| Fixed dates | Local/UTC table fields, format bytes, numeric coercion, native limits and exceptional floating-point inputs. |
| Calendar input | Defaults, truthiness, normalization and unchanged input fields. |
| Diagnostics | Complete argument/method/self errors, validation order and required fields. |
| Field callbacks | Ordered reads, mutation, nested conversion, collection and arbitrary error identity. |
| Protected calls | Active native frames in `xpcall`, coroutine barriers and recursive C-call-limit recovery. |
| Collection | Discarded extras, popped field values, retained locals/open upvalues and return-hook finalization. |
| Format bytes | Embedded NULs, literals, numeric formats, empty formats and trailing percent. |
| Current time | 128 official before/after brackets and local calendar round trips. |
| Generated calendars | 512 deterministic normalization, local round-trip and UTC-format inputs. |

The [93 process probes](2026-10-25-rust-lua-calendar-process.json) pass across the official reference,
Rust runner and CLI. Seventeen calendar additions cover native zone output,
twelve format edge cases, invalid-time-before-format ordering and three timezone
environments with DST gaps/folds, overflow and `isdst` hints. Existing shell,
exit, stdout and file probes remain. Status codes, stdout/stderr bytes and recorded
file contents compare exactly. Every child must terminate; equal timeouts cannot
count as passes.

Compile-only checks pass for x86-64 GNU/Linux, x86-64 musl/Linux, x86 GNU/Linux,
ARM64 macOS, x86 Windows MSVC and ARM64 Windows MSVC; see the
[log](2026-10-25-rust-lua-calendar-cross-check.log). These establish Rust type/layout checks, not
cross-target linking, ABI runtime behavior or oracle parity. Android and 32-bit
musl declarations have no target-build evidence in this milestone.

The crate's formatting, strict Clippy with dependencies excluded, and whitespace
checks pass. The normal/build dependency tree contains only `deadlib-lua`.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-25-rust-lua-calendar-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-calendar-suite.json
```

The [official report](2026-10-25-rust-lua-calendar-suite.json) retains sixteen checksum-pinned,
unchanged files, `_U=true`, a 100-million-instruction budget and sixty-second
per-file deadlines. Rust still passes twelve: `constructs`, `locals`, `nextvar`,
`vararg`, `pm`, `events`, `gc`, `closure`, `db`, `math`, `errors` and `sort`.
Fifteen reference files pass. Rust stops at `calls.lua:243` (binary dumps),
`literals.lua:161`, `strings.lua:150` and `files.lua:22` (locale support).
The reference retains its `files.lua:59` Windows open-input rename failure.
The suite command still exits unsuccessfully; these are isolated file probes,
not complete `all.lua` or internal C API coverage. All retained source hashes
match the final tree.

The [measurements](../performance/2026-10-25-rust-lua-calendar.md) retain 48 runtime and twenty-two
frontend workloads. Locale support, binary chunks, native modules, incremental
GC, broader numeric/resource/debug behavior, caches, host APIs, later dialects,
JIT and integrations remain unfinished. This milestone does not complete
arbitrary standalone Lua 5.1.
