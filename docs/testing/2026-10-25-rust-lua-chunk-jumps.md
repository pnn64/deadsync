# Lua 5.1 long-branch export validation

Source-compiled `string.dump` output now uses guarded jump relays when native
instruction expansion exceeds Lua 5.1's signed 18-bit branch range. Batched
relocation patches existing branches and source-PC maps, then repeats until
every encoded edge fits. Ordinary fallthrough skips each relay; redirected
branches enter it. Neither instruction reserves or changes a Lua register.
Short branches bypass relay construction. Imported native re-export retains
the original words and does not use source lowering.

Cuts use complete source-IR boundaries and preserve native compare/skip groups,
iterator pairs, closure capture bindings and extended SETLIST payloads. Open
CALL/VARARG result producers remain adjacent to their consumers, as required
by the native verifier. Relay line numbers and named local ranges inherit the
originating branch's scope rather than the physically adjacent skipped body.
Export omits CLOSE operations when no live captured local can be affected;
captured locals ending at the close PC still require closing. Instruction-hook
counts depend on the emitted representation; equality with C's instruction
counts or compiler output bytes is not claimed.

All **393 tests** pass: 42 unit tests, 350 integration tests and one doc example.
Five new test groups establish:

| Area | Evidence |
| --- | --- |
| Expanded control flow | Bodies with 80,000 and 120,000 increments cover skipped/taken IF arms, numeric/generic for, while and repeat, including forward and backward edges. C accepts every original source. Exported chunks load and execute in both engines; scalar results also match original-source execution in C. |
| Protected word groups | A deliberately aligned closure captures 60 locals near the first relay cut; a 27,000-element constructor has extended payload words whose opcode bits can resemble JMP. Execution and imported re-export are checked. |
| Open varargs | An aligned VARARG/SETLIST pair stays adjacent across relocation and passes the C loader and execution comparison. |
| Register pressure | A C-accepted function with 195 outer locals and a 52-argument call exports with the same 249-register stack bound; relays add no scratch register. |
| Debug scope | Count hooks on a skipped 80,000-line body see neither its named local nor its body lines. Both process and integration tests check the result. |

All [138 process probes](2026-10-25-rust-lua-chunk-jumps-process.json) match complete exit status,
stdout/stderr bytes and recorded file bytes across the C oracle, Rust runner
and CLI. The two additions cover large dump/load control flow and relay hook
scope. Source-generated output is compared through observations, not against
the C compiler's different byte stream.

The [official report](2026-10-25-rust-lua-chunk-jumps-suite.json) retains sixteen unchanged,
checksum-pinned files with `_U=true`, 100-million-instruction budgets and
60-second file deadlines. Both engines pass **15/16**. Both retain the Windows
`files.lua:59` open-input rename failure; the command exits unsuccessfully.
These are isolated file probes, not `all.lua`, complete C API coverage or a
claim of full Lua 5.1 compatibility.

Formatting, strict crate Clippy without dependencies, whitespace checks and
the normal/build dependency tree pass. The tree contains only `deadlib-lua`.
No production dependency or unsafe code is added. Production consumers retain
the legacy engine.

```sh
cargo test -p deadlib-lua
cargo fmt -p deadlib-lua --check
cargo clippy -p deadlib-lua --all-targets --no-deps -- -D warnings
cargo tree -p deadlib-lua --edges normal,build
python scripts/check_rust_lua_process.py --output docs/testing/2026-10-25-rust-lua-chunk-jumps-process.json
python scripts/check_rust_lua_suite.py --output docs/testing/2026-10-25-rust-lua-chunk-jumps-suite.json
```

The native contract remains Windows x86-64 MSVC Lua 5.1.5, little endian,
size_t 8, int/instruction 4 and binary64 8. Broader register/operand limits,
uncommon source IR forms, adversarial C-accepted metadata, complete diagnostics,
debug mutation and collector equivalence remain unfinished. Persistent caching,
native modules, later dialects, host APIs, optional JIT and integrations remain
open. All recorded source hashes match the final tree. Two
[measurement runs](../performance/2026-10-25-rust-lua-chunk-jumps.md) cover 62 runtime and 22 frontend
workloads.
