# Hot reload (live UI iteration)

A developer-only workflow for iterating on the title menu's render code **without
restarting the game**. Edit `src/screens/menu/render.rs`, and the change shows up
in the running window in well under a second — no lost menu state, no asset reload.

This is a dev tool. The shipping (`release`) build has no hot-reload boundary and
is completely unaffected by anything here.

## How it works (one paragraph)

The menu's pure render path is compiled into a reloadable `cdylib`
(`deadsync-screens`). At runtime the host watches that library and, when a fresh
build appears, validates a small exported header and dispatches each frame through
the newest version. The render output crosses the boundary as a real `Vec<Actor>`
by value, so the host and the cdylib **must share one allocator** — both are built
with `-C prefer-dynamic` against one dynamic `std`. A mismatch is rejected at load
(see [Troubleshooting](#troubleshooting)).

## Prerequisites

- A normal working build of the game (see the README's *Getting Started*).
- `cargo-watch` is **not** required — the `cargo xtask hot-watch` task has its own
  watcher.
- **`-C prefer-dynamic` on both sides.** The host and the watcher must use the
  *identical* `RUSTFLAGS`, or the host will refuse to load the cdylib.

## Quickstart (Windows / PowerShell)

Two terminals, both with the same `RUSTFLAGS`.

**Terminal A — the host:**

```powershell
$env:RUSTFLAGS = "-C prefer-dynamic"
# A prefer-dynamic exe needs the dynamic `std-*.dll` at runtime — put the
# toolchain's bin dir on PATH (or copy the dll next to the exe):
$env:PATH = "$(rustc --print sysroot)\bin;$env:PATH"
cargo run --profile hot --bin deadsync --features hot
```

**Terminal B — the watcher:**

```powershell
$env:RUSTFLAGS = "-C prefer-dynamic"
cargo xtask hot-watch
```

Now edit `src/screens/menu/render.rs` and save — the watcher rebuilds just the
cdylib and republishes it, and the host swaps it in on the next frame.

`cargo xtask hot-watch --help` lists the options (profile, custom RUSTFLAGS, the
watched crate, `--no-lld`, poll interval, `--once`, …). By default it also links
with the bundled `rust-lld`, which is several times faster than the MSVC linker on
the warm relink — the dominant cost per edit.

> Other platforms: the same `-C prefer-dynamic` requirement applies; substitute
> your platform's mechanism for putting the dynamic `std` library on the loader
> path (e.g. `LD_LIBRARY_PATH` on Linux, `DYLD_LIBRARY_PATH` on macOS).

## What reloads, and what needs a restart

Only the two cdylib sources are watched:

- `src/screens/menu/render.rs` (the `#[path]`-included render path)
- `crates/deadsync-screens/src/lib.rs`

Editing **engine code** changes the statically-linked rlib (and the host), which
this fast loop does **not** rebuild. For engine edits, stop and restart the host
with a normal `cargo run`.

Likewise, changing the **shared boundary** — `src/hot`, or the layout of the menu's
`State` / `HostContext` — changes the host/cdylib contract. Rebuild the host too,
not just the cdylib (see the caveat under Troubleshooting).

## Troubleshooting

- **`hot(screens): disabled — host built without -C prefer-dynamic`** — the host
  terminal didn't have `RUSTFLAGS="-C prefer-dynamic"` set. Set it (in both
  terminals) and rebuild/restart the host.
- **The cdylib loads but is rejected / falls back to the in-lib render** — the
  header handshake failed. The most common cause is a host and cdylib built with
  different `RUSTFLAGS`, a different toolchain, or out-of-sync boundary layouts.
  Rebuild both with identical flags.
- **Edited the boundary and nothing rejected, but behavior is wrong** — the
  `BUILD_HASH` handshake is derived from *committed* source (`git HEAD`), so it
  does **not** detect *uncommitted* edits to the shared boundary (`src/hot`, the
  `State` / `HostContext` layouts). Pure `render.rs` edits are always safe; if you
  touch shared-contract code, rebuild the host.

A note on `RUSTFLAGS`: keep them per-terminal (don't put `-C prefer-dynamic` in
`.cargo/config.toml`) so they never leak into release or CI builds.

## Adding a new hot screen

Hot reload is generic — the menu is just the first surface. A screen becomes
hot-reloadable by implementing the `HotSurface` contract (`src/hot/mod.rs`):

1. **`State`** — host-owned, persistent across reloads (caches, selection, input
   trackers). It is never touched by the cdylib's render.
2. **`Context`** — a per-frame snapshot of every process-global the render needs,
   as plain owned values (`Arc<str>`, `Copy` keys, numbers — never borrows or
   `&'static` into a global). The menu calls its concrete type `HostContext`.
3. **`build_context(&State) -> Context`** — resolves the globals into that snapshot.
   Runs host-side, so it may read anything.
4. **`render(&State, &Context, alpha) -> Vec<Actor>`** — the pure render. Reads
   only its two inputs; this is the part that compiles into the cdylib.
5. **Register one line** in `hot_surface_registry!` (slot, label, the two types,
   and the two functions). That single entry also wires the surface into the
   layout-hash handshake, so a new surface can't drift out of lockstep.

`MenuSurface` (slot 0) is the reference implementation; copy its shape.

Keys (font/texture names, text) emitted by a hot render may point into the
reloadable image; the host re-homes them into host-owned memory at the boundary
(`normalize_hot_actors`), so render code never has to thread host-owned keys by
hand.
