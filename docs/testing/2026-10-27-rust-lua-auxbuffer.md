# Native auxiliary string buffers

The development C boundary now implements `luaL_buffinit`, `luaL_prepbuffer`,
`luaL_addlstring`, `luaL_addstring`, `luaL_addvalue`, `luaL_pushresult` and
`luaL_gsub`. Its header exposes the Lua 5.1 `luaL_Buffer` layout and the
`luaL_addchar`, `luaL_putchar` and `luaL_addsize` macros. This supplies missing
operations used by C modules while production native loading remains unfinished.

Buffer chunks live in the Rust VM's native stack and remain traced while C
fills its inline buffer or calls back into Lua. Flushing, chunk merging and
final concatenation preserve Lua 5.1's visible chunk counts and stack effects.
Sized appends preserve embedded NUL bytes; C-string appends and literal
replacement stop at NUL. `luaL_addvalue` consumes the top slot, coerces numbers
and treats other nonstring values as empty, following the 5.1 reference.
Literal replacement uses nonoverlapping byte matches, with no Lua-pattern
interpretation. Empty search patterns are outside this finite fixture's scope.

The identical C module is built against official Lua 5.1.5 and the development
boundary on Windows/MSVC x64. Ninety-nine buffer scenarios check empty, partial,
exact-full and overflowing buffers; sized/string/value/character/prepared
appends; repeated preparation/results; ascending and descending chunk sizes;
and 400 deterministic mixed operations. Every scenario compares complete output
against a Lua-built expected string and emits the chunk count, inline length
and stack top after each operation. Nine literal-replacement cases check
overlaps, literal pattern characters, NUL truncation, numeric inputs and
multi-buffer results. All previous fixture cases remain enabled.

Forty callbacks collect the heap and reenter native and Lua code while outer
chunks remain live. Further cases check arbitrary error identity, subsequent
recovery and the forbidden yield across a C boundary. C-only jumps still occur
after the Rust dispatcher returns and drops its locals.

| Check | Result |
| --- | --- |
| Shared-source C fixture | Both processes exit zero; all 16,340 stdout bytes equal |
| Buffer ABI | Inline size 512, structure size 536, inline offset 24 in both builds |
| Buffer checks | All 99 scenarios and 40 collection callbacks complete |
| Error boundary | 57,162 dispatches and 57,162 drop guards; release `panic=abort` |
| Thread state lifetime | 74 created, 73 collected before shutdown, all 74 destroyed |
| Rust regression suite | 630 pass: 73 unit, 553 integration, 4 documentation |
| Static checks | Rustfmt and strict Clippy pass; normal/build dependency tree contains only `deadlib-lua` |

The final check uses a fresh checkout and initially absent Cargo target, with
exact source pins before and after execution. The official archive, source
files, shared fixture, runner and six executable/DLL artifacts are hashed in
the [raw boundary report](2026-10-27-rust-lua-auxbuffer-boundary.json).
[Validation metadata](2026-10-27-rust-lua-auxbuffer-validation.json) binds the
source revision, workspace inputs, commands, logs, checks and artifact hashes.

Reproduce the boundary comparison on Windows/MSVC with:

```powershell
python scripts/check_rust_lua_native_boundary.py --output target/lua-auxbuffer-boundary.json
```

These changes extend the development bridge and its tests. They add no runtime
dependencies or unsafe Rust and make no changes to the core runtime or legacy
engine. The complete C API, inactive/cross-thread C stacks, production library
loading, internal T/testC conformance tests and broader allocation/GC/resource
behavior remain unfinished. Full standalone Lua 5.1 parity remains unproven;
later dialects, performance tuning and gameplay integration remain deferred.
