# deadlib-lua-legacy

The existing implementation is retained here while the independent
[`deadlib-lua`](../deadlib-lua/README.md) engine is developed. All production
consumers use this legacy package through the workspace's `mlua` alias.

`deadlib-lua-legacy` is DeadSync's maintained fork of mlua, starting from the local
mlua 0.12.2 checkout with DeadSync's first two optimization commits included.
It provides Rust bindings to Lua, LuaJIT and Luau. The initial import retains
the upstream API, features, tests, examples and benchmarks.

This is an independent fork. Its source revision and initial adaptations are
recorded in [`PROVENANCE.md`](PROVENANCE.md). The original documentation is
retained in [`UPSTREAM-README.md`](UPSTREAM-README.md), and the original MIT
copyright and license notices are preserved in [`LICENSE`](LICENSE).

## Packages

| Package | Rust crate | Purpose |
|---|---|---|
| `deadlib-lua-legacy` | `mlua` | High level Rust API |
| `deadlib-lua-legacy-sys` | `mlua_sys` | Lua FFI and VM build integration |
| `deadlib-lua-legacy-derive` | `mlua_derive` | Optional procedural macros |

The internal directory and Rust crate names are retained to keep existing code
and macro output compatible. The fork keeps its MIT license and Rust 1.88 MSRV.

## Use in DeadSync

All engine consumers share the local package through `[workspace.dependencies]`:

```toml
mlua = { workspace = true, features = ["lua51", "vendored"] }
```

Song Lua also enables `send`. Existing `use mlua::...` imports continue to work.
The workspace's patched `vendor/lua-src` remains the source of the vendored VM.

## Validation

Run from the DeadSync workspace root:

```sh
cargo test -p deadlib-lua-legacy --features lua51,vendored,send,async,serde,macros,anyhow,userdata-wrappers
cargo test -p deadsync-song-lua --lib
cargo check -p deadsync-theme-simply-love
cargo bench -p deadlib-lua-legacy --bench hot_paths --features lua51,vendored,send
```

Select one VM feature at a time. Engine builds use Lua 5.1; the other upstream
VM features are retained for future work. Further optimizations should preserve
the API's safety and error behavior and include measurements of the affected
paths. The imported `hot_paths` benchmark and regression tests cover the first
two optimization commits.
