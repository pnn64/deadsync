//! Hot-reload boundary ABI — type definitions only.
//!
//! This module defines the handshake between the host (the `deadsync` exe, which
//! links the live engine rlib) and a reloadable `deadsync-screens` **cdylib**.
//! It deliberately contains **no loader logic** — polling, shadow copying,
//! `dlopen`, `catch_unwind`, and the keep-alive ring all live in the standalone
//! `deadsync-hot` runtime crate.
//!
//! # Boundary invariants (must hold for soundness)
//!
//! 1. **Same toolchain / same engine rlib layout.** The host hands the cdylib
//!    `&menu::State` and `&menu::HostContext` **by reference** (read by field
//!    layout) and the cdylib returns a real `Vec<Actor>` **by value** over the
//!    Rust ABI, so `menu::State`, `menu::HostContext`, `Actor`, and everything
//!    reachable by value through them must have **identical layout** in both
//!    artifacts. `extern "Rust"` is also not stable across rustc versions, so
//!    this requires the *same rustc*. [`BUILD_HASH`] + [`LAYOUT_HASH`] +
//!    [`HotHeader::panic_strategy`] encode that contract and a stale cdylib is
//!    rejected at load.
//!
//! 2. **One shared allocator (both built `-C prefer-dynamic`).** A `Vec<Actor>`
//!    (and every `Arc<str>` it carries) is allocated in the cdylib and dropped
//!    in the host, so both artifacts must link **one shared `std`/global
//!    allocator** — otherwise the host frees cdylib heap with a different
//!    allocator (UB). The shared-allocator build is folded into [`BUILD_HASH`]
//!    (via the `DEADSYNC_SHARED_ALLOC` build flag), so an *asymmetric* pairing
//!    (one side dynamic, the other static) disagrees on `build_hash` and is
//!    rejected at load. The remaining *both-static* case (matching hash, but two
//!    separate allocators) is closed by [`SHARED_ALLOC`]: the host refuses to
//!    enable hot loading unless it was itself built `-C prefer-dynamic`, so a
//!    static host never loads a cdylib and no heap crosses. This is why the hot
//!    dev build cannot use `lto` (incompatible with `prefer-dynamic`); the
//!    non-hot shipping build has no boundary and keeps it.
//!
//! Only [`HotHeader`] is read "blind" through a raw exported symbol, so it is
//! the one type that strictly requires `#[repr(C)]`. [`ScreenVTable`] is also
//! `#[repr(C)]` to freeze its array layout. The render output itself crosses as
//! a non-`repr(C)` `Vec<Actor>`, sound only under invariants 1 and 2.
//!
//! # Adding a hot surface
//!
//! Surfaces are registered in **one** place — the [`hot_surface_registry!`]
//! invocation in this module — and the cdylib maps each to its local renderer in
//! its own `hot_local_renders!`. To add a surface `Foo`:
//!
//! 1. Give its screen a host-owned `State` and a per-frame `Context`, plus a
//!    `build_context(&State) -> Context` and a pure `get_actors(&State,
//!    &Context, f32) -> Vec<Actor>` (the in-lib fallback). Keep `Context` to
//!    read-only views of host data (the render path only reads `Context`).
//! 2. Add one line to [`hot_surface_registry!`] with the next free slot number;
//!    this generates the `FooSurface` marker + [`HotSurface`] impl, slot-checks
//!    it (uniqueness + `< MAX_SURFACES`), and folds it into [`LAYOUT_HASH`].
//!    Also add `offset_of!` lines for its `State`/`Context` fields (the one part
//!    not auto-derivable in `macro_rules!`).
//! 3. Add `FooSurface => render::get_actors` to the cdylib's `hot_local_renders!`
//!    (the typed binding there rejects a mismatched render fn at compile time).
//! 4. Dispatch it host-side: `Self::hot_actors::<FooSurface>(&mut self.hot_reloader,
//!    &state, alpha)`.
//!
//! No new cdylib, reloader, or `extern "C"` symbol is needed — the existing
//! `deadsync-screens` library carries all surfaces in one vtable.

#![allow(dead_code)] // Wired up by the cdylib and the runtime crate.

use crate::engine::present::actors::{Actor, SpriteSource, TextContent};
use crate::screens::components::shared::visual_style_bg;
use crate::screens::menu;
use crate::screens::menu::state::{ArrowCloudStatusKey, GrooveStatusKey, StatusTextCache};
use core::mem::offset_of;
use core::ptr::NonNull;

// The generic header + validation live in the standalone, app-agnostic
// `deadsync-hot` runtime crate. This rlib re-exports the core type as
// `HotHeader` (so the cdylib and tests keep naming `deadsync::hot::HotHeader`)
// and supplies the deadsync-specific [`ScreenVTable`], [`EXPECTED`] descriptor,
// and layout/build hashes the runtime validates against.
pub use deadsync_hot::HotHeaderCore as HotHeader;
pub use deadsync_hot::{Expected, HeaderRejection};

/// Sentinel identifying a deadsync hot-reload header. Bump only on a hard format
/// break of [`HotHeader`] itself (not on vtable/state changes — that's
/// [`LAYOUT_HASH`]).
pub const MAGIC: u64 = 0xDEAD_5719_C0DE_0001;

/// Bumped on any intentional change to the [`ScreenVTable`] shape/semantics.
/// `2`: vtable became a counted [`Option<HotEntry>; MAX_SURFACES`] array with a
/// type-erased entry signature (was a single typed `menu_get_actors` field).
/// `3`: the entry returns a real `Option<Vec<Actor>>` by value over the Rust ABI
/// (was an `extern "C"` POD `ActorBlob` byte handle) — the shared-allocator
/// boundary. A `2` cdylib is rejected on `abi_version` before any heap crosses.
pub const ABI_VERSION: u32 = 3;

/// Panic strategy of this build: `0` = unwind, `1` = abort. Host and cdylib must
/// match or `catch_unwind` across the boundary is unsound. The pilot runs the
/// dev profile (unwind) on both sides.
pub const PANIC_STRATEGY: u8 = if cfg!(panic = "abort") { 1 } else { 0 };

/// Whether this build links a **shared** `std`/global allocator (`true` when
/// built `-C prefer-dynamic`). Emitted by `build.rs` as `DEADSYNC_SHARED_ALLOC`.
///
/// The boundary moves a real `Vec<Actor>` (and its `Arc<str>`s) allocated in the
/// cdylib and dropped in the host, so it is sound **only** when both artifacts
/// share one allocator (boundary invariant #2). This flag is also folded into
/// [`BUILD_HASH`], which rejects every *asymmetric* pairing (one side dynamic,
/// the other static) at load. The remaining *both-static* footgun — where the
/// hashes still match — is closed host-side: the host refuses to enable hot
/// loading at all unless `SHARED_ALLOC` is `true` (see
/// `App::build_hot_reloader`), so a static host never loads any cdylib and no
/// heap ever crosses a mismatched allocator.
pub const SHARED_ALLOC: bool = env!("DEADSYNC_SHARED_ALLOC").as_bytes()[0] == b'1';

// --- FNV-1a (const) ---------------------------------------------------------

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

const fn fnv1a_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    hash
}

const fn fnv1a_u64(hash: u64, value: u64) -> u64 {
    fnv1a_bytes(hash, &value.to_le_bytes())
}

/// Fold a type's `size_of` + `align_of` into the running hash.
const fn mix_layout<T>(hash: u64) -> u64 {
    fnv1a_u64(fnv1a_u64(hash, size_of::<T>() as u64), align_of::<T>() as u64)
}

/// Fold a single field offset into the running hash.
const fn mix_off(hash: u64, offset: usize) -> u64 {
    fnv1a_u64(hash, offset as u64)
}

/// Compile-time hash over the layout of every type that crosses the boundary.
///
/// Three layers of coverage:
///   1. `size`/`align` of every boundary type — the header, vtable,
///      the per-screen `State`/`HostContext`, the `Actor` payload tree, and the
///      nested-by-value types `State`/`HostContext` carry (`visual_style_bg::State`,
///      the status-cache instantiations, and the Copy status-key enums).
///   2. The `#[repr(C)]` field offsets of [`HotHeader`] the loader reads blind.
///   3. **Every field offset of `State` and `HostContext`** — the two types the
///      host still passes by `extern "Rust"` reference (read by field layout). This
///      makes the hash sensitive to a field being added, removed, or reordered even
///      when the struct's total `size`/`align` coincidentally lands the same.
///
/// Residual gap (documented, narrow): a field whose type is swapped for a
/// *different* type of identical `size` **and** `align`, at an unchanged offset,
/// can still slip past — the offsets and the whole-struct size/align are all
/// unchanged. Mixing the nested status-cache/key types (layer 1) shrinks this to
/// fields that are neither one of those types nor offset-shifting. Acceptable for
/// the single-developer, same-checkout pilot; closed structurally once the
/// `hot_surfaces!` registry co-generates this hash from each surface's field set
/// (see the extensibility plan). The fields below must be kept in sync with
/// `menu::state` until then.
pub const LAYOUT_HASH: u64 = {
    let mut h = FNV_OFFSET;
    h = mix_layout::<HotHeader>(h);
    h = mix_layout::<ScreenVTable>(h);
    h = mix_layout::<menu::State>(h);
    h = mix_layout::<menu::HostContext>(h);
    h = mix_layout::<Actor>(h);
    h = mix_layout::<SpriteSource>(h);
    h = mix_layout::<TextContent>(h);
    // Nested-by-value types reachable through `State`/`HostContext`, so a layout
    // change inside one of them is caught even if it doesn't shift an outer offset.
    h = mix_layout::<visual_style_bg::State>(h);
    h = mix_layout::<GrooveStatusKey>(h);
    h = mix_layout::<ArrowCloudStatusKey>(h);
    h = mix_layout::<StatusTextCache<GrooveStatusKey, 3>>(h);
    h = mix_layout::<StatusTextCache<ArrowCloudStatusKey, 1>>(h);
    // Pin the repr(C) field offsets the loader dereferences before it trusts
    // anything else in the header.
    h = mix_off(h, offset_of!(HotHeader, magic));
    h = mix_off(h, offset_of!(HotHeader, size));
    h = mix_off(h, offset_of!(HotHeader, layout_hash));
    h = mix_off(h, offset_of!(HotHeader, build_hash));
    h = mix_off(h, offset_of!(HotHeader, vtable));
    // Pin every field offset of the two by-reference boundary structs.
    h = mix_off(h, offset_of!(menu::State, selected_index));
    h = mix_off(h, offset_of!(menu::State, active_color_index));
    h = mix_off(h, offset_of!(menu::State, rainbow_mode));
    h = mix_off(h, offset_of!(menu::State, started_by_p2));
    h = mix_off(h, offset_of!(menu::State, bg));
    h = mix_off(h, offset_of!(menu::State, i18n_revision));
    h = mix_off(h, offset_of!(menu::State, info_text_cache));
    h = mix_off(h, offset_of!(menu::State, groovestats_text_cache));
    h = mix_off(h, offset_of!(menu::State, arrowcloud_text_cache));
    h = mix_off(h, offset_of!(menu::State, menu_lr_chord));
    h = mix_off(h, offset_of!(menu::State, menu_lr_undo));
    h = mix_off(h, offset_of!(menu::HostContext, info_text));
    h = mix_off(h, offset_of!(menu::HostContext, menu_labels));
    h = mix_off(h, offset_of!(menu::HostContext, footer_title));
    h = mix_off(h, offset_of!(menu::HostContext, footer_side));
    h = mix_off(h, offset_of!(menu::HostContext, gs));
    h = mix_off(h, offset_of!(menu::HostContext, ac));
    h = mix_off(h, offset_of!(menu::HostContext, screen_center_x));
    h = mix_off(h, offset_of!(menu::HostContext, bg_elapsed_s));
    h = mix_off(h, offset_of!(menu::HostContext, menu_font));
    // Surface identity + per-surface State/Context size/align, folded from the
    // single `hot_surface_registry!` list so every registered surface is hash-
    // covered (a surface cannot exist without contributing here).
    h = mix_registered_surfaces(h);
    h
};

/// Compile-time hash over the toolchain identity: full `rustc -vV`, the git
/// short rev, the crate version, target arch/os, and the panic strategy.
///
/// `extern "Rust"` is **not** stable across rustc versions, so a toolchain swap
/// must invalidate a previously-built cdylib even if every layout is unchanged.
/// Both `DEADSYNC_RUSTC_VERSION` and `DEADSYNC_BUILD_HASH` are emitted by
/// `build.rs`.
///
/// `DEADSYNC_SHARED_ALLOC` (`"1"`/`"0"`, also emitted by `build.rs` by sniffing
/// `-C prefer-dynamic` in the rustflags) is folded in so a cdylib built with a
/// **static** `std` (its own allocator) disagrees on `build_hash` and is rejected
/// at load — before any cdylib-allocated `Vec<Actor>` could be freed host-side
/// (boundary invariant #2).
pub const BUILD_HASH: u64 = {
    let mut h = FNV_OFFSET;
    h = fnv1a_bytes(h, env!("DEADSYNC_RUSTC_VERSION").as_bytes());
    h = fnv1a_bytes(h, env!("DEADSYNC_BUILD_HASH").as_bytes());
    h = fnv1a_bytes(h, env!("CARGO_PKG_VERSION").as_bytes());
    h = fnv1a_bytes(h, std::env::consts::ARCH.as_bytes());
    h = fnv1a_bytes(h, std::env::consts::OS.as_bytes());
    h = fnv1a_bytes(h, &[PANIC_STRATEGY]);
    h = fnv1a_bytes(h, env!("DEADSYNC_SHARED_ALLOC").as_bytes());
    h
};

// --- Boundary types ---------------------------------------------------------

/// Maximum number of hot surfaces a single [`ScreenVTable`] can carry. Raising
/// this requires bumping [`ABI_VERSION`] — the array length is part of the
/// vtable's `#[repr(C)]` layout and is covered by [`LAYOUT_HASH`].
pub const MAX_SURFACES: usize = 8;

/// A type-erased hot render entry. The concrete `&State`/`&Context` references
/// are reconstructed *inside the cdylib thunk* (which knows the surface's real
/// types); the host only ever holds them erased and relies on [`LAYOUT_HASH`] to
/// guarantee both artifacts agree on those layouts.
///
/// Returns `Option<Vec<Actor>>` **by value over the Rust ABI** (not `extern
/// "C"`): `Some(actors)` on success, `None` when the cdylib caught a panic in
/// the render path. The `Vec` and every `Arc<str>` it carries are allocated in
/// the cdylib and dropped in the host, so this is sound **only** because both
/// artifacts are built `-C prefer-dynamic` against one shared `std`/global
/// allocator (see boundary invariant #2) — a static-allocator cdylib must be
/// rejected at load. A non-FFI-safe return type is permitted here because the
/// caller and callee are the same rustc/target (enforced by [`BUILD_HASH`]); the
/// entry is `unsafe` because it dereferences the two erased `&State`/`&Context`
/// raw pointers under the layout handshake.
pub type HotEntry = unsafe fn(*const (), *const (), f32) -> Option<Vec<Actor>>;

// The vtable stores entries as `Option<HotEntry>` and relies on the
// null-function-pointer niche so an unpublished slot is a plain null pointer the
// host reads as `None` (never a wild call). Assert that niche holds: `Option`
// must not grow the pointer and the pointer must be word-sized.
const _: () = {
    assert!(size_of::<Option<HotEntry>>() == size_of::<HotEntry>());
    assert!(size_of::<HotEntry>() == size_of::<*const ()>());
};

/// The reloadable dispatch table: one optional [`HotEntry`] per surface slot. A
/// second hot surface adds one line to the [`hot_surface_registry!`] list (which
/// assigns its slot, generates its [`HotSurface`] impl, and folds it into
/// [`LAYOUT_HASH`]) and one mapping in the cdylib's `hot_local_renders!`; it does
/// **not** need its own cdylib or its own reloader. Render-only — input / audio /
/// navigation stay host-owned and never run in the cdylib.
///
/// A slot the cdylib does not implement is `None`. `Option<HotEntry>` is FFI-safe
/// via the null-function-pointer niche, so an unpublished slot is a plain null
/// pointer the host reads as `None` and falls back to its in-lib renderer — never
/// an out-of-bounds read or a call through uninitialized memory. This per-slot
/// presence is why no separate `count` field is needed.
///
/// `#[repr(C)]` freezes the array layout; [`LAYOUT_HASH`] mixes its size/align.
#[repr(C)]
pub struct ScreenVTable {
    /// Per-slot entries indexed by [`HotSurface::SLOT`]; `None` = not published.
    pub entries: [Option<HotEntry>; MAX_SURFACES],
}

/// Binds a host-owned `State`/`Context` pair to a fixed vtable [`SLOT`] and
/// supplies the host-side glue ([`build_context`]) plus the **in-lib fallback**
/// renderer ([`render`] — the empty stub under `feature = "hot"`).
///
/// # Safety
/// This trait is `unsafe` because its associated types cross the hot boundary by
/// erased reference. An implementor must guarantee that `State` and `Context`
/// have a **stable, identical layout** in the host rlib and the cdylib (same
/// toolchain + same engine source, enforced at load by [`LAYOUT_HASH`] /
/// [`BUILD_HASH`]), and that both artifacts share one allocator (boundary
/// invariant #2) so a `Vec<Actor>` allocated in the cdylib can be dropped
/// host-side.
///
/// Implementations are generated by [`hot_surface_registry!`] so that slot
/// numbering, the uniqueness/bounds assertion, and the [`LAYOUT_HASH`] surface
/// contributions all derive from one list and cannot drift apart.
///
/// [`SLOT`]: HotSurface::SLOT
/// [`build_context`]: HotSurface::build_context
/// [`render`]: HotSurface::render
pub unsafe trait HotSurface {
    /// Host-owned per-screen state, passed to the cdylib by erased reference.
    type State: 'static;
    /// Per-frame render snapshot, built host-side by [`build_context`](Self::build_context).
    type Context: 'static;
    /// This surface's fixed index into [`ScreenVTable::entries`].
    const SLOT: usize;
    /// Stable identifier mixed into [`LAYOUT_HASH`] and used in logs.
    const LABEL: &'static str;
    /// Resolve process-globals into the render snapshot (runs host-side).
    fn build_context(state: &Self::State) -> Self::Context;
    /// In-lib fallback renderer (the empty stub under `feature = "hot"`), used
    /// before the first successful load and after a quarantined generation.
    fn render(state: &Self::State, ctx: &Self::Context, alpha: f32) -> Vec<Actor>;
}

/// Compile-time check that every registered slot is in range and unique. A
/// duplicate or out-of-range slot fails the build at the `const _` call site.
const fn assert_surface_slots(surfaces: &[(usize, &str)]) {
    let mut i = 0;
    while i < surfaces.len() {
        assert!(surfaces[i].0 < MAX_SURFACES, "hot surface SLOT out of range");
        let mut j = i + 1;
        while j < surfaces.len() {
            assert!(surfaces[i].0 != surfaces[j].0, "duplicate hot surface SLOT");
            j += 1;
        }
        i += 1;
    }
}

/// The single registry of hot surfaces. One invocation generates, for every
/// surface: a zero-sized marker type, its [`HotSurface`] impl (slot / label /
/// glue / fallback), an entry in [`REGISTERED_SURFACES`] (driving the slot
/// uniqueness + bounds assertion), and a [`LAYOUT_HASH`] contribution
/// (`State` / `Context` size+align, slot, label). Because all of these derive
/// from this one list, a surface cannot exist without being slot-checked and
/// hash-covered — closing the lockstep desync risk structurally.
macro_rules! hot_surface_registry {
    ( $( $slot:literal => $name:ident {
            state: $state:ty,
            context: $ctx:ty,
            label: $label:literal,
            build: $build:path,
            render: $render:path $(,)?
    } ),+ $(,)? ) => {
        $(
            /// Zero-sized hot-surface marker; see [`HotSurface`].
            pub struct $name;
            // SAFETY: layout equality is enforced at load by LAYOUT_HASH /
            // BUILD_HASH, and both artifacts share one allocator (boundary
            // invariant #2) so a `Vec<Actor>` built by `render` can be dropped
            // host-side.
            unsafe impl HotSurface for $name {
                type State = $state;
                type Context = $ctx;
                const SLOT: usize = $slot;
                const LABEL: &'static str = $label;
                fn build_context(state: &Self::State) -> Self::Context {
                    $build(state)
                }
                fn render(state: &Self::State, ctx: &Self::Context, alpha: f32) -> Vec<Actor> {
                    $render(state, ctx, alpha)
                }
            }
        )+

        /// Every registered `(slot, label)` — the single list both the slot
        /// assertion and the [`LAYOUT_HASH`] surface mix iterate.
        pub const REGISTERED_SURFACES: &[(usize, &str)] = &[ $( ($slot, $label) ),+ ];

        // Build-time: every slot is < MAX_SURFACES and all slots are distinct.
        const _: () = assert_surface_slots(REGISTERED_SURFACES);

        /// Fold each registered surface's `State` / `Context` layout, slot, and
        /// label into the running [`LAYOUT_HASH`].
        const fn mix_registered_surfaces(mut h: u64) -> u64 {
            $(
                h = mix_layout::<$state>(h);
                h = mix_layout::<$ctx>(h);
                h = mix_off(h, $slot);
                h = fnv1a_bytes(h, $label.as_bytes());
            )+
            h
        }
    };
}

hot_surface_registry! {
    0 => MenuSurface {
        state: menu::State,
        context: menu::HostContext,
        label: "menu",
        build: menu::build_host_context,
        render: menu::get_actors,
    },
}

/// What this host build expects of any cdylib it loads. Built from this rlib's
/// own consts; a cdylib compiled against a different engine state will disagree
/// on `layout_hash` / `build_hash` and be rejected by the runtime.
pub const EXPECTED: Expected = Expected {
    magic: MAGIC,
    abi_version: ABI_VERSION,
    size: size_of::<HotHeader>() as u32,
    layout_hash: LAYOUT_HASH,
    build_hash: BUILD_HASH,
    panic_strategy: PANIC_STRATEGY,
};

/// Reinterpret a validated opaque vtable pointer as a [`ScreenVTable`].
///
/// # Safety
/// `ptr` must come from a [`HotHeader`] that passed
/// [`HotHeaderCore::verify`](deadsync_hot::HotHeaderCore::verify) against
/// [`EXPECTED`] (so it points to a `ScreenVTable` of the agreed layout) and the
/// owning library must still be loaded.
pub unsafe fn screen_vtable<'a>(ptr: NonNull<()>) -> &'a ScreenVTable {
    unsafe { &*(ptr.as_ptr() as *const ScreenVTable) }
}

/// Font keys consumed by hot render paths.
///
/// Render code must **not** name these consts directly: a cdylib that referenced
/// `font_keys::MISO` would bake a pointer into *its own* rodata, which dangles
/// the moment that cdylib is unloaded. They are the single authoritative source
/// the **host** reads to populate `HostContext`, so the `&'static str` handed to
/// the cdylib points into the exe's rodata and stays valid across reloads.
pub mod font_keys {
    /// Default body font used by menu status/info text.
    pub const MISO: &str = "miso";
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn noop_get_actors(
        _state: *const (),
        _ctx: *const (),
        _alpha: f32,
    ) -> Option<Vec<Actor>> {
        Some(Vec::new())
    }

    static TEST_VTABLE: ScreenVTable = {
        let mut entries: [Option<HotEntry>; MAX_SURFACES] = [None; MAX_SURFACES];
        entries[MenuSurface::SLOT] = Some(noop_get_actors as HotEntry);
        ScreenVTable { entries }
    };

    fn well_formed_header() -> HotHeader {
        HotHeader {
            magic: MAGIC,
            abi_version: ABI_VERSION,
            panic_strategy: PANIC_STRATEGY,
            _pad: [0; 3],
            size: size_of::<HotHeader>() as u32,
            layout_hash: LAYOUT_HASH,
            build_hash: BUILD_HASH,
            vtable: &TEST_VTABLE as *const ScreenVTable as *const (),
        }
    }

    #[test]
    fn expected_matches_header_size() {
        assert_eq!(EXPECTED.size as usize, size_of::<HotHeader>());
    }

    #[test]
    fn hashes_are_seeded() {
        // A zero hash would mean the const folding silently produced nothing.
        assert_ne!(LAYOUT_HASH, 0);
        assert_ne!(BUILD_HASH, 0);
        assert_ne!(LAYOUT_HASH, BUILD_HASH);
    }

    #[test]
    fn field_offset_mixing_is_layout_sensitive() {
        // Proves the `mix_off`/`offset_of!` technique LAYOUT_HASH relies on
        // actually distinguishes a field reorder: two structs with identical
        // field *types* but swapped *order* hash differently, because the
        // offsets differ. (Total size/align alone would not catch this.)
        #[repr(C)]
        struct A {
            x: u8,
            y: u64,
        }
        #[repr(C)]
        struct B {
            y: u64,
            x: u8,
        }
        let ha = mix_off(mix_off(FNV_OFFSET, offset_of!(A, x)), offset_of!(A, y));
        let hb = mix_off(mix_off(FNV_OFFSET, offset_of!(B, x)), offset_of!(B, y));
        assert_ne!(ha, hb, "field reorder must change the offset hash");
    }

    #[test]
    fn registered_surfaces_are_slot_unique_and_in_range() {
        // Mirrors the compile-time `assert_surface_slots`, and pins the pilot's
        // single registered surface so an accidental registry edit is caught.
        assert_eq!(MenuSurface::SLOT, 0);
        assert_eq!(MenuSurface::LABEL, "menu");
        assert!(REGISTERED_SURFACES.iter().all(|(slot, _)| *slot < MAX_SURFACES));
        for (i, (slot_a, _)) in REGISTERED_SURFACES.iter().enumerate() {
            for (slot_b, _) in &REGISTERED_SURFACES[i + 1..] {
                assert_ne!(slot_a, slot_b, "registered slots must be unique");
            }
        }
    }

    #[test]
    fn unpublished_slot_is_none() {
        // A published surface resolves to `Some`; any slot the vtable did not
        // populate is a safe `None` (graceful fallback, never a wild call).
        assert!(TEST_VTABLE.entries[MenuSurface::SLOT].is_some());
        assert!(TEST_VTABLE.entries[MAX_SURFACES - 1].is_none());
    }

    #[test]
    fn dev_profile_is_unwind() {
        // The pilot requires both sides on unwind; assert this build qualifies.
        assert_eq!(PANIC_STRATEGY, 0);
    }

    #[test]
    fn well_formed_header_verifies_and_dispatches() {
        let header = well_formed_header();
        assert_eq!(header.verify(&EXPECTED), Ok(()));
        // The opaque pointer round-trips back to the exact vtable.
        let vt = unsafe { screen_vtable(header.vtable_ptr().unwrap()) };
        assert!(std::ptr::eq(vt, &TEST_VTABLE));
    }

    #[test]
    fn each_mismatch_is_reported() {
        assert!(matches!(
            HotHeader { magic: MAGIC ^ 1, ..well_formed_header() }.verify(&EXPECTED),
            Err(HeaderRejection::Magic { .. })
        ));
        assert!(matches!(
            HotHeader { abi_version: ABI_VERSION + 1, ..well_formed_header() }.verify(&EXPECTED),
            Err(HeaderRejection::AbiVersion { .. })
        ));
        assert!(matches!(
            HotHeader { size: 0, ..well_formed_header() }.verify(&EXPECTED),
            Err(HeaderRejection::Size { .. })
        ));
        assert!(matches!(
            HotHeader { panic_strategy: PANIC_STRATEGY ^ 1, ..well_formed_header() }.verify(&EXPECTED),
            Err(HeaderRejection::PanicStrategy { .. })
        ));
        assert!(matches!(
            HotHeader { layout_hash: LAYOUT_HASH ^ 1, ..well_formed_header() }.verify(&EXPECTED),
            Err(HeaderRejection::LayoutHash { .. })
        ));
        assert!(matches!(
            HotHeader { build_hash: BUILD_HASH ^ 1, ..well_formed_header() }.verify(&EXPECTED),
            Err(HeaderRejection::BuildHash { .. })
        ));
        assert!(matches!(
            HotHeader { vtable: std::ptr::null(), ..well_formed_header() }.verify(&EXPECTED),
            Err(HeaderRejection::NullVtable)
        ));
    }
}
