# Official Lua 5.1 benchmarks

`lua5.1-tests/` contains every file from
<https://www.lua.org/tests/lua5.1-tests.tar.gz>, unchanged. The archive SHA-256 is
`49e4ca6561f82ea605908c5041ab5fad66ed9930fa0686675bd51b02767f18ad`.
`lua51-source.json` records the checksum of each extracted file. Git attributes
preserve the original bytes, including the scripts' non-UTF-8 text.

Run from the repository root:

```sh
python crates/deadlib-lua/benches/run_lua51.py --samples 9
python crates/deadlib-lua/benches/run_lua51.py --files literals.lua strings.lua --samples 9
```

The runner builds the normal release `official_suite` example, freezes its
executable, and checks source identities. Each sample uses a fresh process/VM
and a private copy of the suite under ignored `target/lua51-benchmark/` output.
The clock covers `dofile` loading, compilation, and execution. Standard-library
initialization, the common bootstrap, and preparation of the small `dofile`
driver happen before the clock. Results include elapsed nanoseconds, Windows
process CPU cycles, original output, return values, source checksums, and binary
identity. `--cpu N` pins child processes where supported; `--output PATH` selects
the JSON result path. Workload order is shuffled with a fixed seed.

The bootstrap uses the upstream `_U` flag for portable tests, disables the
optional internal C test module `T`, seeds the RNG, and selects the C locale.
It does not set `_soft`; the large generated-program and sorting branches run
in full. Assertions and sources are not rewritten. Known numeric return values
are checked in addition to the upstream assertions.

The default 18 independently runnable files cover the compiler, VM, GC, debug
API, standard libraries, and large programs. The entire archive is retained,
including `all.lua`, the CLI/module tests, the memory-overflow coroutine test,
and optional C test helpers. Those additional entry points need their own
prerequisites and are not silently counted as benchmark passes. No C test
helper is compiled or linked into DeadLua.

The separate `primitive-equality.lua`, `distinct-table-equality.lua`,
`scalar-global-overwrites.lua`, `scalar-table-overwrites.lua`,
`metatable-overwrite.lua`, and `table-reads.lua` workloads exercise unobserved
interpreter comparisons, repeated global and table writes with and without
metatables, and eight million string-keyed table reads. Build
the release `official_suite` example as above, then run from this directory:

```sh
../../../target/release/examples/official_suite rust-bench primitive-equality.lua
../../../target/release/examples/official_suite rust-bench distinct-table-equality.lua
../../../target/release/examples/official_suite rust-bench scalar-global-overwrites.lua
../../../target/release/examples/official_suite rust-bench scalar-table-overwrites.lua
../../../target/release/examples/official_suite rust-bench metatable-overwrite.lua
../../../target/release/examples/official_suite rust-bench table-reads.lua
```

On Windows, append `.exe`. Repeat each command in fresh processes and compare
the `BENCH` CPU-cycle field; each script asserts its output before returning.

On Windows, `files.lua:59` rejects renaming its open input stream in both
DeadLua and native Lua 5.1. This precise failure is reported separately and
has no successful timing. All other failures fail the benchmark command;
failed files are not repeatedly timed. Comparator runtimes may skip different
locale/internal tests or reject strict Lua 5.1 assertions. Inspect their output
and coverage before comparing speed. Keep comparator code, executables, and
measurement reports in ignored output rather than committing them here.
