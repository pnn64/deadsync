# Process regression fixtures

`process-native.luac` and `process-win32.json` were captured independently with
Lua 5.1.5 through optimized `mlua` 0.12.2 on Windows/MSVC. They preserve the
original 141 process probes' exit statuses, stdout, stderr and output files.
Only the owned case-directory and repository paths are normalized. Source,
environment and native-image identities are checked before comparing results.

`scripts/check_rust_lua_process.py` runs these probes through the Rust embedding
API and the `deadlua` CLI. It does not load a native Lua library or regenerate
expectations. Process expectations for other platforms must be independently
reviewed and captured before those platform-specific probes can run there.

`gc-finalizer-phase.lua` retains the finalizer-phase regression formerly kept
under `docs/testing`.

The pinned official Lua suite remains unmodified. Its runner records the precise
Windows failure at `files.lua:59` separately: native Lua 5.1.5 also refuses that
test's rename of its open input stream. Other failures still fail the runner.
