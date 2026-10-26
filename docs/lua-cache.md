# Persistent Lua programs

ProgramCache is an opt-in disk cache for the shared Lua 5.1 compiler representation.
It accepts the same source bytes and chunk names as compile_named, including
native Lua 5.1 input. It returns a Program and CacheStatus (Hit, Stored or Bypassed).
Programs retain the normal host-clone, VM-binding and garbage-collection rules.
The cache contains code and immutable metadata; every VM builds its own runtime
strings, closures, environments and other objects.

~~~rust
use deadlib_lua::{ProgramCache, Vm};

let source = std::fs::read("script.lua")?;
let cache = ProgramCache::new("compiled-lua")
    .with_limits(64 * 1024 * 1024, 256 * 1024 * 1024);
let compiled = cache.compile_named(&source, b"@script.lua")?;
let mut vm = Vm::new();
vm.execute(&compiled.program)?;
~~~

The example leaves error conversion to the embedding application's error type.
Cache I/O errors never replace source compilation errors. Missing, damaged,
incompatible, oversized or unreadable entries trigger compilation; a failed
store returns the compiled program with Bypassed status. Invalid Lua returns the
normal compile_named error and does not replace an existing entry.

Use a directory trusted to hold executable code. Integrity and structural checks
detect damaged or malformed entries; they do not authenticate authors or prove
that a deliberately forged payload implements its claimed source. Cache files
include the original source bytes. This format is distinct from official Lua
chunks and is not accepted by Lua's loadstring/load or the standalone CLI.

## Identity and persistence

The filename is a compact index derived from source and chunk-name bytes. Every
load compares both byte sequences in full; a filename collision causes a miss.
A cache entry also binds its schema/dialect, target OS/architecture, pointer
width, endianness, compiler build identity and all locale character classes
observed by the lexer. A locale revision change during compilation bypasses
publication. The caller must still coordinate process-global locale changes
with compilation, as for ordinary uncached compilation.

The dependency-free Cargo build script derives the build identity from sorted
Rust source paths/content, the crate manifest and build script, target settings,
encoded Rust flags, optimization/debug settings and rustc version information.
It normalizes CRLF checkout endings. Changes to compiler/runtime sources
invalidate old entries. Cargo watches the source directory, including newly
added files, through its documented
[build-script change detection](https://doc.rust-lang.org/cargo/reference/build-scripts.html#change-detection).
Direct rustc builds without the build-script identity compile normally and
bypass the disk cache. New configuration options that affect compilation must
be included in the identity when introduced.

Saving creates a unique temporary file in the cache directory, writes and
closes it, then calls [fs::rename](https://doc.rust-lang.org/std/fs/fn.rename.html)
to replace the entry. Failed replacement preserves the existing entry and
removes the writer's temporary file when possible. Concurrent writers do not
share a temporary filename. Cache data is regenerable; writes are not fsynced.
Directory-wide eviction, a memory cache and package/CLI integration are separate
future work. Standard compile/compile_named continue to perform compilation
without cache I/O.

## Format and bounds

The private format starts with DeadLuaCache, a NUL byte, schema version 1 and
dialect byte 0x51. It uses explicit little-endian integer fields, not Rust struct
layouts or addresses. Strings and arrays carry u32 lengths. Instructions retain
their opcode/u8 destination/u16 operand/u32 operand fields; numeric constants
retain binary64 bits, including signed zero and NaN payloads.

The payload preserves the entry, source-name pool, flat prototypes, constants,
captures and per-site closure bindings, register/parameter flags, line/hook maps,
collector liveness masks and indices, debug locals/upvalue names/calls/temporary
ranges, and retained native words/debug offsets used by string.dump. It ends
with a CRC32C over all preceding bytes. Trailing data is rejected.

The checksum advances through eight-byte blocks using one compile-time 8 KiB
lookup table and a scalar tail. Safe byte-array loads support unaligned input;
no CPU-specific instructions or runtime table allocation are required. The
polynomial, initial/final complements and stored CRC32C bytes remain those of
schema version 1. [Paired measurements](performance/2026-10-26-rust-lua-cache-crc.md)
record the speed/memory tradeoff against the earlier one-byte implementation.

The reader checks wire lengths against remaining input and charges object and
payload allocation sizes before reserving arrays or copying strings. It then
runs Program::verify; native re-export ranges and tree edges receive additional
checks. Defaults are 64 MiB per file and 256 MiB of decoded object/payload storage.
Allocator metadata and the input-file buffer are separate from the decoded
budget. These limits govern cache use, not the compiler's normal resource limits.
A cache produced by a trusted compiler is still executable code, not a sandbox.

Cache reloads trade filesystem work, validation and decoding against parsing and
compilation. They are useful for some large source programs; tiny programs and
already compiled native chunks can reload more slowly. The
[measurements](performance/2026-10-26-rust-lua-constant-pool.md) include both
profitable and unprofitable cases, allocation counts and process CPU counters.
