# Source provenance

The initial fork was copied directly from the tracked working-tree files in
`C:/GitHub/mlua` (`../mlua` relative to the DeadSync checkout) on 2026-10-23.
No source was downloaded from GitHub or crates.io for this import. All tracked
source files were clean at import time. The untracked `rust-performance.md`,
build outputs, Git metadata, and upstream CI/coverage configuration were not
imported.

The local repository's origin was `https://github.com/pnn64/mlua.git`; its
upstream remote was `https://github.com/mlua-rs/mlua.git`.

| Revision | Commit |
|---|---|
| Upstream mlua 0.12.2 base | `a186d461c7772aed4cb0a9fa1d1899121c6450e7` |
| First local optimization | `a31f506bcf8a1817d5965aa5efbfc289e3f9b34e` |
| Second local optimization / imported HEAD | `079d5f10efeabee84340e72b420444149926c6d1` |

## Included optimizations

- `a31f506`: reduce integer coercion, borrowed string/byte conversion and Luau
  table iteration overhead; add the `hot_paths` benchmark and regression tests.
- `079d5f1`: reduce Lua 5.1 integer conversion, owned string conversion and
  table length overhead; extend the benchmark and regression tests.

The runtime implementation, procedural macros, FFI sources, tests and benchmarks
are copied unchanged from that local HEAD. The following initial adaptations
only change packaging and documentation.

## Initial fork adaptations

- Rename the Cargo packages to `deadlib-lua` (0.12.2), `deadlib-lua-sys` (0.13.0)
  and `deadlib-lua-derive` (0.12.2), retaining the Rust crate names `mlua`,
  `mlua_sys` and `mlua_derive`.
- Join the DeadSync workspace and use the renamed local packages through path
  dependencies, including in the standalone module examples and tests.
- Route all three engine consumers through one workspace dependency and carry
  the existing development optimization profiles over to the renamed packages.
- Point fork package metadata at the DeadSync repository and add fork READMEs.
- Preserve the original main and FFI READMEs as `UPSTREAM-README.md` beside each
  replacement README. The derive package had no upstream README.
- Preserve the original MIT license verbatim and copy it into the FFI and
  derive packages so their source distributions include the same notices.

The existing DeadSync `lua-src` patch is retained. No upstream APIs, VM features,
tests, examples or benchmarks are removed as part of the initial fork.
