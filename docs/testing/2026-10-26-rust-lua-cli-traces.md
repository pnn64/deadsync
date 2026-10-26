# Lua 5.1 standalone message-handler validation

Standalone entry now reconstructs a parked C main frame between host operations
and delivers its return hook once at context completion. The main and each fresh
message-handler function are managed native closures with stable Lua identities,
environments and collector ownership. The main's opaque light-userdata argument
uses a VM identity, not a dereferenceable C stack address. Ordinary embedded
execution does not enable the context.

Protected calls reuse existing xpcall unwinding and handler continuations. Faulting
frames and open cells remain available until the current debug.traceback returns.
The handler reads the thread's globals, handles nil/non-string results and ignores
non-function traceback fields, as in the reference. Loader reader errors inherit
the active handler; Lua pcall temporarily disables it. Repeated reader failures
share that entry's handler identity, while successive entries receive distinct
functions. Handler failures use the existing C-depth/error-handler reserve.

Uncaught runtime failures force a full collection after the message handler is
removed. Error values remain roots through collection; finalizer failures escape
the outer main context and can replace the original error. load_program allows
the CLI to release the host Program before calling, so a failed chunk environment
does not retain userdata through that collection. Handler allocation is a collector
safe point before protection begins; unprotected failures do not produce a main
return hook. Begin/end APIs and the shared driver contain no recursive Rust calls.

All **414 crate tests** pass: 43 unit tests, 370 integration tests and one doc
example. Seven new groups check exact default traceback bytes, live-local mutation
and surviving upvalues, main identity/return delivery, nil errors/quota recovery,
post-error finalization/error roots, reclamation across 4,000 entries and Lua
ownership after dropping a host Program. Formatting, strict crate Clippy,
whitespace checks and the normal/build dependency tree pass. The tree contains
only deadlib-lua. No production dependency, C linkage or unsafe code is added.

All [140 existing process probes](2026-10-26-rust-lua-cli-traces-process.json) still match complete
status/stdout/stderr/file bytes across the development C oracle, Rust API runner
and CLI. The [official file report](2026-10-26-rust-lua-cli-traces-suite.json) keeps eighteen unchanged,
checksum-pinned files, _U=true, 100-million-instruction budgets and 60-second
deadlines. Both engines pass **17/18**; both retain the Windows files.lua:59
open-input rename failure. That command still fails and is not full all.lua,
internal testC, main.lua or native-module coverage.

The [release standalone audit](2026-10-26-rust-lua-cli-traces-audit.json) now matches **60/66** complete
process observations against checksum-pinned, unmodified official Lua 5.1.5.
Twenty-three additions retain nil/empty/non-string errors, callback lookups,
reader inheritance/handler identity, environment changes, live stack mutation,
error hooks, full collection/finalizer replacement, binary-message bytes,
recursive-handler depth and Lua stack overflow. All runtime-error/context cases
in the audit match. Six differences are retained, and the command still fails:

| Difference | Cases |
| --- | --- |
| Usage text/options | missing_e, missing_l, unknown_option, invalid_version_tail |
| Intentional project banner | version; DeadLua identifies itself |
| Interactive execution | interactive; -i is still unimplemented |

These probes do not establish complete debug/C-scratch-stack equivalence, arbitrary
internal C-function invocation, exact collector work/timing, console/signal parity
or complete standalone compatibility. Context cleanup/ownership and broader
adversarial cases still need expansion. Existing ASCII Windows argv/path and
native-chunk ABI assumptions remain. Opaque addresses are not byte-compared.

The oracle follows the [official standalone implementation](https://www.lua.org/source/5.1/lua.c.html)
and [source archive](https://www.lua.org/ftp/), pinned to SHA-256
2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333 and built
with MSVC /O2 /MT. The report includes source/binary hashes. Development prebuilt
CLI probes are supported with explicit binary hashes; the final report uses the
normal release build and does not claim prebuilt source provenance.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-26-rust-lua-cli-traces-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-26-rust-lua-cli-traces-suite.json
python scripts/check_rust_lua_cli.py --output docs/testing/2026-10-26-rust-lua-cli-traces-audit.json
```

All final report source hashes match the tree. Two
[measurement runs](../performance/2026-10-26-rust-lua-cli-traces.md) cover 66 runtime and 22 frontend
workloads. Register/resource limits, broader collector/debug/diagnostic coverage,
persistent caching, Rust callbacks, native modules, later dialects, optional JIT
and song/noteskin integrations remain unfinished. The full goal stays open.
