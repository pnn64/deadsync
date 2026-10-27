# Standalone official suite entry points

`check_rust_lua_full_suite.py` runs the unchanged official Lua 5.1 `main.lua`
and `all.lua` files through the real Rust and C CLIs. The older independent-file
runner embeds Lua and supplies a bootstrap, which does not exercise the
standalone process behavior required by these entry points.

The suite archive and reference source remain checksum-pinned. Every run has
its own extracted test directory and a private executable named `lua` placed
first on PATH. Child commands therefore invoke the same engine as their
parent. The environment uses `LUA_PATH="?;./?.lua"`, as required by the suite
README, clears inherited `LUA_INIT`/`LUA_CPATH`, and supplies no Lua bootstrap.
All extracted source files are hashed before and after execution. Raw stdout,
stderr, return codes, completion markers and timeouts are retained. A zero exit
without the entry point's completion marker remains a failure. A timeout kills
the run's process tree and remains a failure. Runtime output is not asserted
byte-equal: temporary names, memory counts, timings and version banners differ.

The first Linux run found that Rust's `os.tmpname()` added a `.tmp` suffix.
`require` translates dots in module names into directory separators, so the
suite's first temporary module could not load. The POSIX implementation now
reserves an extensionless file under `/tmp`, matching the pinned reference's
template. It keeps atomic creation, owner-only permissions and collision
retrying. The Windows temporary-name behavior remains in place. A new Unix
Rust regression writes a module to the generated name, requires it through
`package.path='?'`, checks its value and removes the file.

| Platform and entry | Official C | Rust before | Rust after |
| --- | --- | --- | --- |
| Linux `main.lua` | Pass | Fail loading temporary module | Pass |
| Linux `all.lua` | Fail at `big.lua:11` | Fail at unsupported native locales | Same locale failure |
| Windows `main.lua` | Fail at `main.lua:53` | Same first failing assertion | Same first failing assertion |
| Windows `all.lua` | Fail inside `main.lua:53` | Same first failing assertion | Same first failing assertion |

Both platform audits retain nonzero exits until all requested entries pass.

On Windows, the reference's temporary name itself contains a dot; its module
search fails before the first child command succeeds. That common failure is
retained. On Linux x86-64, the C reference progresses through the preceding
tests, then fails `big.lua:11`: its 129-way concatenation does not report the
overflow expected by this test. The reference's limit uses native `size_t`;
this four-GiB expression fits the 64-bit limit. The harness does not rewrite
the assertion or impose a 32-bit limit on the implementation. Rust's current
first failure is earlier, at `os.setlocale('C')`, because native POSIX locales
remain unsupported.

Validation includes 627 Windows Rust tests, strict Clippy on Windows and Linux,
the dependency tree, 140 Windows process probes and two focused Linux temporary
name tests. Linux runs use Debian/glibc x86-64 and a native filesystem for the
suite's working directories. Fresh Cargo targets build the final source bytes;
existing performance reports are unchanged and no benchmarks run.

Raw [Windows before](2026-10-27-rust-lua-full-suite-windows-before.json),
[Linux before](2026-10-27-rust-lua-full-suite-linux-before.json),
[Windows after](2026-10-27-rust-lua-full-suite-windows.json),
[Linux after](2026-10-27-rust-lua-full-suite-linux.json) and
[process checks](2026-10-27-rust-lua-full-suite-process.json) are retained.
[Validation metadata](2026-10-27-rust-lua-full-suite-validation.json) pins
sources, copied executables, build inputs and check logs.

These are stock standalone runs. The internal T/testC module and the suite's
native test libraries are absent; their automatically skipped tests are not
covered. Neither complete `all.lua` passage nor full standalone Lua 5.1 parity
is established. Native POSIX locales, the remaining C/auxiliary APIs,
production native-module loading and broader compatibility/resource behavior
remain open. Later dialects, performance tuning and integrations stay deferred.
