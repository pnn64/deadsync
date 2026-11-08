# i686 suite passage and native-width compatibility

The unchanged stock Lua 5.1 `main.lua` and `all.lua` now pass in both standalone
engines on Linux i686 SSE2. Both binaries are real ELF32 x86 executables and
emit the native Lua chunk header with four-byte size fields and binary64
numbers. The official archive, source files, assertions and completion markers
remain unchanged. No Lua bootstrap is inserted. Child `lua` commands use each
engine's private executable through PATH and the official README's Lua path.

This is stock-suite passage, with T/testC and native suite libraries absent.
It does not establish full standalone Lua 5.1 parity or complete C API coverage.

The runner's new `--target i686-unknown-linux-gnu` builds both matching targets.
`--reference-fp-model sse2` explicitly selects `-m32 -msse2 -mfpmath=sse` for
the C reference. The Rust target already declares SSE2. Development C-oracle
builds use the same model through `CFLAGS_i686_unknown_linux_gnu`. Profile caches
are separate; native Windows/Linux reference builds retain their existing
compiler flags. This selection defines the arithmetic model and runs no
performance benchmark.

The default i686 C build uses x87 extended evaluation and Lua 5.1's fast integer
conversion macro. An independent C fixture converts 3.9 to 4 in that model,
and to 3 in the selected SSE2 model. The default model's broader exploratory
run retains eight failed targets. The old SSE2 regression run retains four
failed targets and a CPU-bound traceback process that was stopped after more
than nine minutes. These observations remain in the audit; they are not
rewritten into successful runs. The declared Rust i686 profile is SSE2.

Passing the stock suite alone did not expose all width errors. The broader
regressions drove these engine fixes:

- Lua integer coercion now follows the pointer-width `ptrdiff_t` model in
  library arguments, debug indexes, file operations, exit codes and native
  scalar access. Out-of-range/nonfinite values use the native minimum in the
  declared conversion profiles.
- Radix parsing saturates at native unsigned-long width, while integer formats
  follow native C long/unsigned-long width independently of Lua integer width.
- Time differences convert through the native time type before subtraction.
- Unpack checks its wide element count before converting to `usize`. A range
  of 2^32 elements no longer wraps to zero on i686; it raises the catchable
  result-limit error.

No new unsafe code or dependencies are added. The legacy production engine,
its aliases and its compiler settings remain unchanged.

| ABI fixture | Pointer / Lua integer | C long / time | Native tm size | Float evaluation / conversion of 3.9 |
| --- | --- | --- | --- | --- |
| Linux x64 | 64 / 64 bits | 64 / 64 bits | 56 bytes | Binary64 / 3 |
| Linux i686 default x87 | 32 / 32 bits | 32 / 32 bits | 44 bytes | Extended / 4 |
| Linux i686 SSE2 | 32 / 32 bits | 32 / 32 bits | 44 bytes | Binary64 / 3 |

The setup installed the i686 Rust standard library and C multilib headers and
libraries. The package plan upgraded and removed no existing packages. C
fixture binaries, system/input headers, compiler versions and target features
are pinned in the ABI metadata. The 32-bit calendar layout agrees with the
Rust boundary's existing size/offset assertions.

Large traceback requests also needed a bounded audit. On i686 SSE2, infinity,
NaN and other out-of-range levels become the minimum native integer. Lua's
negative-level behavior can then request billions of fake tail-call frames.
The in-process regression keeps bounded levels; invalid `getlocal` levels now
compare protected-call errors. The omitted huge requests execute in private
processes under a two-second deadline and 256-MiB address-space cap. These
limits apply only to the audit, with no truncation added to ordinary traces.

Two control traces match. Five large requests time out in both engines and
remain incomplete, so the strict resource audit exits 1. Equal timeouts do not
count as passes. A new Rust regression separately proves that the optional
1,000-instruction quota cancels a minimum-level request and that collection
and a later ordinary traceback recover.

| Check | Result |
| --- | --- |
| Windows x64 Rust tests | 630: 73 unit, 553 integration, 4 documentation |
| Linux x64 Rust tests | 630: 73 unit, 553 integration, 4 documentation |
| Linux i686 SSE2 Rust tests | 630: 73 unit, 553 integration, 4 documentation |
| Strict Clippy | Pass on all three targets |
| Formatting and dependency tree | Pass; only deadlib-lua in normal/build dependencies |
| i686 C/Rust stock `main.lua` and `all.lua` | All four processes pass with completion markers |
| Linux x64 stock suite | Main passes in both; all fails at `big.lua:11` in both; strict exit 1 |
| Windows x64 stock suite | Common `main.lua:53` failure in both; strict exit 1 |
| Bounded i686 traceback audit | 2/7 complete matches, five incomplete pairs; strict exit 1 |
| Shared-source Windows C fixture | Output/status match; 36,766 dispatch/drop guards under panic=abort |
| C-state lifetime | 73 created, 72 collected before shutdown, 73 destroyed |

The genuine i686 size limit lets the original large-string assertion work.
The 64-bit runtimes keep their native limits; no fabricated 32-bit bound is
imposed on them. The earlier exploratory default-i686 suite also passed stock
entry points, illustrating why the broader regressions and explicit arithmetic
profile are necessary evidence.

Raw [initial suite](2026-10-27-rust-lua-i686-suite-before.json),
[i686 final suite](2026-10-27-rust-lua-i686-suite-i686.json),
[Linux x64 suite](2026-10-27-rust-lua-i686-suite-linux.json),
[Windows suite](2026-10-27-rust-lua-i686-suite-windows.json),
[ABI fixture](2026-10-27-rust-lua-i686-suite-abi.json),
[regression audits](2026-10-27-rust-lua-i686-suite-audit.json),
[traceback resources](2026-10-27-rust-lua-i686-suite-tracebacks.json),
[native boundary](2026-10-27-rust-lua-i686-suite-boundary.json) and
[validation metadata](2026-10-27-rust-lua-i686-suite-validation.json) retain
source hashes, fresh targets, executable hashes, commands and check logs.

Complete C/auxiliary APIs, production native loading, broader numeric/device,
debug, GC and allocation/resource behavior remain open. Later dialects,
performance tuning and gameplay integrations remain deferred.
