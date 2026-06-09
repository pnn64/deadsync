//! # deadsync-hot
//!
//! An **application-agnostic** Rust hot-reload runtime. It knows nothing about
//! deadsync (or any game): it loads a reloadable `cdylib`, validates a small
//! versioned, `#[repr(C)]` header exported by that library, and hands the caller
//! back an **opaque vtable pointer** that the caller casts to its own dispatch
//! table. It handles debounced change detection, wait-until-writable,
//! unique-filename shadow copying, the header handshake, library lifetime, and
//! panic quarantine.
//!
//! ## What the consumer provides
//! * a path to the reloadable library (e.g. `target/debug/deadsync_screens.dll`),
//! * the exported entry symbol name (default [`ENTRY_SYMBOL`]),
//! * an [`Expected`] descriptor (magic / abi_version / size / layout_hash /
//!   build_hash / panic_strategy) it wants every loaded library to match.
//!
//! ## Soundness contract (the consumer is responsible for these)
//! * **One allocator / one `std`.** Host and reloadable library must share a
//!   single `std`/allocator (on MSVC: build both with `-C prefer-dynamic`), so a
//!   `Vec`/`String`/`Arc` allocated in one and freed in the other is sound.
//! * **Matching panic strategy.** `catch_unwind` across the boundary is only sound
//!   when both sides use the same panic strategy; encode it in the header and
//!   reject mismatches (this crate does, via [`Expected::panic_strategy`]).
//! * **Main-thread swaps.** Call [`Reloader::poll`] from one fixed safe point per
//!   frame on the thread that dispatches through the vtable. The returned pointer
//!   is valid until the next `poll` on the same `Reloader`; never cache it past a
//!   `poll`, and never share it across threads.
//! * **Nothing the library produces may escape into long-lived host state** as a
//!   borrow, closure, or `&'static`. By default every loaded library stays mapped
//!   for the `Reloader`'s lifetime; under bounded unloading
//!   ([`ReloaderBuilder::keep_generations`]) a pruned generation's escaped values
//!   would dangle.
//!
//! ## Library lifetime
//! By default a loaded library is **never unloaded** while the `Reloader` lives —
//! each successful reload appends to an internal keep-alive list. A consumer that
//! has proven nothing escapes can opt into bounded unloading via
//! [`ReloaderBuilder::keep_generations`].

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::time::{Duration, Instant, SystemTime};

use libloading::{Library, Symbol};

/// Conventional entry-point symbol the reloadable library exports.
pub const ENTRY_SYMBOL: &[u8] = b"deadsync_hot_entry";

/// `panic = "unwind"` marker for [`Expected::panic_strategy`].
pub const PANIC_UNWIND: u8 = 0;
/// `panic = "abort"` marker for [`Expected::panic_strategy`].
pub const PANIC_ABORT: u8 = 1;

/// The panic strategy the *currently compiled* crate was built with.
///
/// Consumers can use this to fill [`Expected::panic_strategy`] without hard-coding
/// a value, since the host and the reloadable library must agree.
pub const fn current_panic_strategy() -> u8 {
    if cfg!(panic = "abort") {
        PANIC_ABORT
    } else {
        PANIC_UNWIND
    }
}

/// The generic, app-agnostic header every reloadable library must export (by
/// pointer) from its entry symbol.
///
/// The reloadable library defines a `static HotHeaderCore { … }` and returns
/// `&HEADER` from `extern "C" fn deadsync_hot_entry() -> *const HotHeaderCore`.
/// Returning a *pointer to a static* (rather than the aggregate by value) avoids
/// MSVC `sret` ambiguity for a struct that contains a pointer.
///
/// `vtable` is **opaque**: this crate never dereferences it. The consumer casts
/// it to its own `#[repr(C)]` dispatch table after [`HotHeaderCore::verify`]
/// succeeds.
#[repr(C)]
pub struct HotHeaderCore {
    /// Sentinel identifying a deadsync-hot header. Compared against [`Expected::magic`].
    pub magic: u64,
    /// Bumped on any intentional vtable/ABI change. Compared against [`Expected::abi_version`].
    pub abi_version: u32,
    /// `0 = unwind`, `1 = abort`. Must equal [`Expected::panic_strategy`].
    pub panic_strategy: u8,
    /// Padding so the struct layout is explicit and stable.
    pub _pad: [u8; 3],
    /// `size_of::<HotHeaderCore>()` as written by the library. Compared against [`Expected::size`].
    pub size: u32,
    /// Consumer-supplied hash over all boundary-type layouts. Compared against [`Expected::layout_hash`].
    pub layout_hash: u64,
    /// Toolchain / git diagnostic hash. Compared against [`Expected::build_hash`].
    pub build_hash: u64,
    /// Opaque pointer to the consumer's dispatch table. Must be non-null.
    pub vtable: *const (),
}

impl HotHeaderCore {
    /// Validate this header against what the host expects. Returns the first
    /// field that does not match, or [`HeaderRejection::NullVtable`] if the
    /// vtable pointer is null.
    pub fn verify(&self, expected: &Expected) -> Result<(), HeaderRejection> {
        if self.magic != expected.magic {
            return Err(HeaderRejection::Magic {
                expected: expected.magic,
                found: self.magic,
            });
        }
        if self.abi_version != expected.abi_version {
            return Err(HeaderRejection::AbiVersion {
                expected: expected.abi_version,
                found: self.abi_version,
            });
        }
        if self.size != expected.size {
            return Err(HeaderRejection::Size {
                expected: expected.size,
                found: self.size,
            });
        }
        if self.panic_strategy != expected.panic_strategy {
            return Err(HeaderRejection::PanicStrategy {
                expected: expected.panic_strategy,
                found: self.panic_strategy,
            });
        }
        if self.layout_hash != expected.layout_hash {
            return Err(HeaderRejection::LayoutHash {
                expected: expected.layout_hash,
                found: self.layout_hash,
            });
        }
        if self.build_hash != expected.build_hash {
            return Err(HeaderRejection::BuildHash {
                expected: expected.build_hash,
                found: self.build_hash,
            });
        }
        match NonNull::new(self.vtable as *mut ()) {
            Some(_) => Ok(()),
            None => Err(HeaderRejection::NullVtable),
        }
    }

    /// The opaque vtable pointer as a [`NonNull`], or `None` if null.
    pub fn vtable_ptr(&self) -> Option<NonNull<()>> {
        NonNull::new(self.vtable as *mut ())
    }
}

/// What the host requires of every loaded [`HotHeaderCore`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Expected {
    /// Required [`HotHeaderCore::magic`].
    pub magic: u64,
    /// Required [`HotHeaderCore::abi_version`].
    pub abi_version: u32,
    /// Required [`HotHeaderCore::size`].
    pub size: u32,
    /// Required [`HotHeaderCore::layout_hash`].
    pub layout_hash: u64,
    /// Required [`HotHeaderCore::build_hash`].
    pub build_hash: u64,
    /// Required [`HotHeaderCore::panic_strategy`].
    pub panic_strategy: u8,
}

/// Why a loaded header was rejected. Every variant is **non-fatal**: the
/// [`Reloader`] keeps the last good vtable (or none) and waits for a better build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeaderRejection {
    /// `magic` did not match.
    Magic { expected: u64, found: u64 },
    /// `abi_version` did not match.
    AbiVersion { expected: u32, found: u32 },
    /// `size` did not match.
    Size { expected: u32, found: u32 },
    /// `panic_strategy` did not match.
    PanicStrategy { expected: u8, found: u8 },
    /// `layout_hash` did not match (a boundary struct changed layout).
    LayoutHash { expected: u64, found: u64 },
    /// `build_hash` did not match (different toolchain / git revision).
    BuildHash { expected: u64, found: u64 },
    /// The header's vtable pointer was null.
    NullVtable,
}

/// A non-recoverable error from constructing or operating a [`Reloader`].
#[derive(Debug)]
pub enum ReloadError {
    /// The builder was missing a required field.
    MissingConfig(&'static str),
    /// A filesystem operation failed.
    Io(std::io::Error),
}

impl std::fmt::Display for ReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReloadError::MissingConfig(s) => write!(f, "missing reloader config: {s}"),
            ReloadError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for ReloadError {}

impl From<std::io::Error> for ReloadError {
    fn from(e: std::io::Error) -> Self {
        ReloadError::Io(e)
    }
}

/// Structured progress/diagnostic events emitted by [`Reloader::poll`].
///
/// Supply a handler via [`ReloaderBuilder::on_event`] to route these into your
/// logging; if you don't, they are dropped silently.
#[derive(Debug)]
pub enum ReloadEvent<'a> {
    /// A change was detected and is being debounced for stability.
    Pending { path: &'a Path },
    /// The library file is changing but not yet readable (linker still writing).
    WaitingForWritable { path: &'a Path },
    /// A new library loaded and passed header validation.
    Loaded { generation: u64, shadow: &'a Path },
    /// A candidate library was rejected (header mismatch). Last good is kept.
    Rejected { reason: HeaderRejection },
    /// A load attempt failed for a non-header reason (e.g. dlopen / missing symbol).
    LoadFailed { detail: String },
    /// The caller quarantined the current generation after a panic; the runtime
    /// will fall back until a newer build validates.
    Quarantined { generation: u64 },
    /// An older generation was pruned — its library unmapped and shadow file
    /// removed under the keep-last-N policy ([`ReloaderBuilder::keep_generations`]).
    Unloaded { generation: u64, shadow: &'a Path },
}

type EventSink = Box<dyn FnMut(ReloadEvent<'_>)>;

/// Builder for a [`Reloader`].
pub struct ReloaderBuilder {
    library_path: Option<PathBuf>,
    symbol: Vec<u8>,
    expected: Option<Expected>,
    stability_window: Duration,
    shadow_dir: Option<PathBuf>,
    on_event: Option<EventSink>,
    keep_generations: Option<usize>,
}

impl ReloaderBuilder {
    /// Path to the reloadable library to watch and load.
    pub fn library_path(mut self, p: impl Into<PathBuf>) -> Self {
        self.library_path = Some(p.into());
        self
    }

    /// Entry symbol to resolve (defaults to [`ENTRY_SYMBOL`]).
    pub fn symbol(mut self, s: &[u8]) -> Self {
        self.symbol = s.to_vec();
        self
    }

    /// The header descriptor every loaded library must match.
    pub fn expected(mut self, e: Expected) -> Self {
        self.expected = Some(e);
        self
    }

    /// How long the file's `(mtime, size)` must hold steady before a change is
    /// treated as a finished build (default 500 ms).
    pub fn stability_window(mut self, d: Duration) -> Self {
        self.stability_window = d;
        self
    }

    /// Directory for unique shadow copies (default
    /// `target/deadsync-hot/<pid>/` next to the watched library, falling back to
    /// the system temp dir). A loaded `.dll` can't be overwritten, so each reload
    /// copies to a fresh filename here.
    pub fn shadow_dir(mut self, p: impl Into<PathBuf>) -> Self {
        self.shadow_dir = Some(p.into());
        self
    }

    /// Install a handler for [`ReloadEvent`]s.
    pub fn on_event(mut self, f: impl FnMut(ReloadEvent<'_>) + 'static) -> Self {
        self.on_event = Some(Box::new(f));
        self
    }

    /// Opt in to bounded unloading: keep at most `n` libraries mapped (the
    /// current one plus `n - 1` prior generations); after each successful load,
    /// older generations are unmapped and their shadow files deleted, emitting
    /// [`ReloadEvent::Unloaded`]. Default (unset) keeps every generation mapped.
    /// `n` is clamped to a minimum of 1 so the library backing `current` is
    /// never dropped.
    ///
    /// # Safety
    ///
    /// Only sound if, by the time `n` newer generations have loaded, nothing a
    /// pruned generation produced is still reachable — no returned values into
    /// its `rodata`, no caches holding its data, no retained function pointers,
    /// closures, trait objects, TLS, threads, or callbacks. The hot library must
    /// be a pure dispatch target, reached only through [`Reloader::current`].
    pub fn keep_generations(mut self, n: usize) -> Self {
        self.keep_generations = Some(n.max(1));
        self
    }

    /// Finalize the configuration. Creates (and cleans) the shadow directory.
    pub fn build(self) -> Result<Reloader, ReloadError> {
        let library_path = self
            .library_path
            .ok_or(ReloadError::MissingConfig("library_path"))?;
        let expected = self.expected.ok_or(ReloadError::MissingConfig("expected"))?;

        let (shadow_dir, owns_layout) = match self.shadow_dir {
            // A caller-supplied dir is used as-is and never auto-cleaned (we
            // can't know what else lives under its parent).
            Some(d) => (d, false),
            // Our default `.../deadsync-hot/<pid>` layout: we own the parent and
            // may prune sibling pid dirs left by dead runs.
            None => (default_shadow_dir(&library_path), true),
        };
        if owns_layout {
            // Best-effort: remove stale per-pid shadow dirs from prior runs.
            clean_stale_shadow_dirs(&shadow_dir);
        }
        let _ = std::fs::create_dir_all(&shadow_dir);

        Ok(Reloader {
            library_path,
            symbol: self.symbol,
            expected,
            stability_window: self.stability_window,
            shadow_dir,
            on_event: self.on_event,
            keep_generations: self.keep_generations,
            libraries: Vec::new(),
            current: None,
            loaded_sig: None,
            pending: None,
            generation: 0,
            quarantined: false,
            last_shadow: None,
            dispatch_thread: None,
        })
    }
}

/// One successfully loaded generation: its mapped library paired with the
/// generation counter and the shadow file backing it, so a pruned generation
/// can be unmapped *and* its shadow file deleted.
struct LoadedLib {
    library: Library,
    generation: u64,
    shadow: PathBuf,
}

/// A poll-driven hot-reload loop over a single reloadable library.
///
/// Construct via [`Reloader::builder`]; call [`Reloader::poll`] once per frame at
/// a safe point and dispatch through the returned (opaque) vtable pointer.
pub struct Reloader {
    library_path: PathBuf,
    symbol: Vec<u8>,
    expected: Expected,
    stability_window: Duration,
    shadow_dir: PathBuf,
    on_event: Option<EventSink>,

    /// Keep-last-N policy: `Some(n)` unmaps generations older than the most
    /// recent `n` after each successful load; `None` keeps every generation
    /// mapped for the `Reloader`'s life. See [`ReloaderBuilder::keep_generations`].
    keep_generations: Option<usize>,

    /// Every successfully loaded library still mapped, oldest first. The last
    /// entry always backs `current`. Push-only unless `keep_generations` prunes
    /// the front.
    libraries: Vec<LoadedLib>,
    /// The current opaque vtable pointer (into the last library in `libraries`).
    current: Option<NonNull<()>>,
    /// `(mtime, size)` of the source file backing the currently loaded library
    /// (or the last build we attempted), so we don't reload an unchanged file.
    loaded_sig: Option<(SystemTime, u64)>,
    /// A change awaiting stability: `(sig, first_seen)`.
    pending: Option<((SystemTime, u64), Instant)>,
    /// Monotonic counter of successful loads (for unique shadow names + events).
    generation: u64,
    /// When set, the current generation panicked; fall back until a newer build.
    quarantined: bool,
    /// Shadow path of the most recently loaded library (for `Loaded` events).
    last_shadow: Option<PathBuf>,
    /// The thread `poll` was first called on. The reloader owns raw library
    /// handles and hands out a raw vtable pointer that is only valid until the
    /// next `poll` swap, so every `poll` + dispatch must happen on this one
    /// thread. Recorded on first poll and `debug_assert`-checked after.
    dispatch_thread: Option<std::thread::ThreadId>,
}

impl Reloader {
    /// Start configuring a [`Reloader`].
    pub fn builder() -> ReloaderBuilder {
        ReloaderBuilder {
            library_path: None,
            symbol: ENTRY_SYMBOL.to_vec(),
            expected: None,
            stability_window: Duration::from_millis(500),
            shadow_dir: None,
            on_event: None,
            keep_generations: None,
        }
    }

    /// The vtable pointer to use this frame, or `None` if no validated library is
    /// active (first load hasn't happened yet, or the current generation is
    /// quarantined). The pointer is valid until the next call to `poll`.
    pub fn current(&self) -> Option<NonNull<()>> {
        if self.quarantined {
            None
        } else {
            self.current
        }
    }

    /// Number of libraries successfully loaded and kept mapped so far.
    pub fn loaded_count(&self) -> usize {
        self.libraries.len()
    }

    /// The current generation counter: 0 before any library has loaded, then
    /// incremented on every successful hot swap. Consumers compare it across
    /// frames to detect a generation change — when pointer/identity-keyed host
    /// caches must be invalidated.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Mark the active generation as broken (e.g. it panicked when dispatched).
    /// The runtime returns `None` from [`Reloader::current`]/[`Reloader::poll`]
    /// until a *newer* build is detected and validated, preventing per-frame
    /// panic/log spam from a known-bad library.
    pub fn quarantine_current(&mut self) {
        if !self.quarantined {
            self.quarantined = true;
            let generation = self.generation;
            self.emit(ReloadEvent::Quarantined { generation });
        }
    }

    /// Detect, debounce, validate, and (on success) swap in a new library.
    ///
    /// Returns the vtable pointer to use this frame (same as [`Reloader::current`]
    /// after any swap). Errors during a reload are reported through the event
    /// sink and are **non-fatal**: the last good vtable (or `None`) is retained.
    pub fn poll(&mut self) -> Option<NonNull<()>> {
        // The reloader hands out a raw vtable pointer valid only until the next
        // swap and owns the loaded library handles, so poll + dispatch must stay
        // on one thread. Record it on first poll and assert it never changes
        // (debug builds).
        let tid = std::thread::current().id();
        match self.dispatch_thread {
            Some(t) => debug_assert_eq!(
                t, tid,
                "Reloader::poll must always run on the same thread: the returned \
                 vtable pointer and the loaded library handles are not valid \
                 across threads"
            ),
            None => self.dispatch_thread = Some(tid),
        }

        let sig = match file_signature(&self.library_path) {
            Some(s) => s,
            None => return self.current(), // file not present yet → fall back
        };

        // Already loaded (or already attempted) this exact build → nothing to do.
        if self.loaded_sig == Some(sig) {
            return self.current();
        }

        // Debounce: the file must hold the same (mtime, size) for the stability
        // window before we treat the build as finished.
        let now = Instant::now();
        match self.pending {
            Some((psig, _)) if psig == sig => {}
            _ => {
                self.pending = Some((sig, now));
                self.emit_pending();
                return self.current();
            }
        }
        let first_seen = self.pending.expect("pending set above").1;
        if now.duration_since(first_seen) < self.stability_window {
            return self.current();
        }

        // Stable. Make sure the linker has released the file before copying.
        if !is_readable(&self.library_path) {
            self.emit_waiting();
            // Re-arm the stability timer so we don't spin; try again next frame.
            self.pending = Some((sig, now));
            return self.current();
        }

        self.pending = None;

        match self.attempt_load() {
            Ok(vptr) => {
                // Commit: this build is loaded; don't reload the same file again.
                self.loaded_sig = Some(sig);
                self.current = Some(vptr);
                self.quarantined = false;
                let generation = self.generation;
                // `emit` after state update; borrow the just-pushed shadow path.
                if let Some(sink) = self.on_event.as_mut() {
                    if let Some(lib_path) = self.last_shadow.as_deref() {
                        sink(ReloadEvent::Loaded {
                            generation,
                            shadow: lib_path,
                        });
                    }
                }
                // Under keep-last-N, unmap generations older than the window now
                // that the new one is current (emits `Unloaded` per pruned gen).
                self.prune_old_generations();
                self.current()
            }
            Err(LoadFailure::Rejected(reason)) => {
                // Deterministic: an unchanged file with a bad header will always
                // fail, so remember it and stop retrying until it changes.
                self.loaded_sig = Some(sig);
                self.emit(ReloadEvent::Rejected { reason });
                self.current()
            }
            Err(LoadFailure::Other(detail)) => {
                // Transient (copy / dlopen / missing symbol): leave `loaded_sig`
                // untouched so the same file is retried after the next stability
                // window rather than being given up on until the next edit.
                self.emit(ReloadEvent::LoadFailed { detail });
                self.current()
            }
        }
    }

    /// Copy → dlopen → resolve symbol → call entry → validate header.
    fn attempt_load(&mut self) -> Result<NonNull<()>, LoadFailure> {
        let generation = self.generation + 1;
        let shadow = self.shadow_path(generation);

        std::fs::copy(&self.library_path, &shadow)
            .map_err(|e| LoadFailure::Other(format!("shadow copy failed: {e}")))?;
        // Best-effort: copy the matching .pdb so debuggers/symbolization work.
        copy_sidecar_pdb(&self.library_path, &shadow);

        // SAFETY: loading an arbitrary library is inherently unsafe; the consumer
        // controls which file is watched, and we validate its exported header
        // before exposing the vtable.
        let library = unsafe { Library::new(&shadow) }
            .map_err(|e| LoadFailure::Other(format!("Library::new failed: {e}")))?;

        let vptr = {
            // SAFETY: the symbol is an `extern "C" fn() -> *const HotHeaderCore`
            // by contract; we immediately validate the returned header.
            let entry: Symbol<'_, unsafe extern "C" fn() -> *const HotHeaderCore> =
                unsafe { library.get(&self.symbol[..]) }
                    .map_err(|e| LoadFailure::Other(format!("symbol {:?} not found: {e}", String::from_utf8_lossy(&self.symbol))))?;

            let header_ptr = unsafe { entry() };
            let header = NonNull::new(header_ptr as *mut HotHeaderCore)
                .ok_or_else(|| LoadFailure::Other("entry returned a null header".to_string()))?;
            // SAFETY: header points to a `static` inside the just-loaded library,
            // which we keep mapped for the lifetime of this `Reloader`.
            let header = unsafe { header.as_ref() };
            header
                .verify(&self.expected)
                .map_err(LoadFailure::Rejected)?;
            header
                .vtable_ptr()
                .ok_or(LoadFailure::Rejected(HeaderRejection::NullVtable))?
        };

        // Commit: keep the library mapped (so `vptr` stays valid) and advance.
        self.libraries.push(LoadedLib {
            library,
            generation,
            shadow: shadow.clone(),
        });
        self.last_shadow = Some(shadow);
        self.generation = generation;
        Ok(vptr)
    }

    /// Under keep-last-N, unmap every generation older than the most recent
    /// `keep` and delete its shadow files. No-op when `keep_generations` is unset.
    ///
    /// SAFETY: `keep` is clamped to >= 1 and `current` is always the last entry,
    /// so this never drops the live generation. Runs on the dispatch thread in
    /// `poll`, so no dispatch is in flight; soundness of unmapping an older
    /// generation rests on the [`ReloaderBuilder::keep_generations`] contract.
    fn prune_old_generations(&mut self) {
        let Some(keep) = self.keep_generations else {
            return;
        };
        while self.libraries.len() > keep {
            // Front is the oldest; never the `current`-backing (last) entry,
            // since keep >= 1 guarantees at least one entry remains.
            let LoadedLib {
                library,
                generation,
                shadow,
            } = self.libraries.remove(0);
            debug_assert!(
                generation < self.generation,
                "pruned generation must be older than the current one",
            );
            // Drop the library FIRST so Windows releases the file lock, then the
            // shadow .dll (and sidecar .pdb) can be deleted in the same call.
            drop(library);
            let _ = std::fs::remove_file(&shadow);
            let _ = std::fs::remove_file(shadow.with_extension("pdb"));
            if let Some(sink) = self.on_event.as_mut() {
                sink(ReloadEvent::Unloaded {
                    generation,
                    shadow: &shadow,
                });
            }
        }
    }

    fn shadow_path(&self, generation: u64) -> PathBuf {
        let stem = self
            .library_path
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("hotlib");
        let ext = self
            .library_path
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or("dll");
        self.shadow_dir
            .join(format!("{stem}-{generation}.{ext}"))
    }

    fn emit(&mut self, event: ReloadEvent<'_>) {
        if let Some(sink) = self.on_event.as_mut() {
            sink(event);
        }
    }

    fn emit_pending(&mut self) {
        let path = self.library_path.clone();
        if let Some(sink) = self.on_event.as_mut() {
            sink(ReloadEvent::Pending { path: &path });
        }
    }

    fn emit_waiting(&mut self) {
        let path = self.library_path.clone();
        if let Some(sink) = self.on_event.as_mut() {
            sink(ReloadEvent::WaitingForWritable { path: &path });
        }
    }
}

enum LoadFailure {
    Rejected(HeaderRejection),
    Other(String),
}

/// Run `f`, catching an unwind so a panic inside the reloadable library becomes a
/// recoverable `None` (the caller should then [`Reloader::quarantine_current`]).
///
/// Only sound when host and library share one `std` and the same panic strategy.
pub fn guard<R>(f: impl FnOnce() -> R) -> Option<R> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok()
}

/// Build a `u64` layout hash from the size+align of each listed type. A consumer
/// puts this in both its [`Expected::layout_hash`] and the header its library
/// exports; it changes whenever any boundary type's layout could change.
#[macro_export]
macro_rules! hot_layout_hash {
    ($($t:ty),+ $(,)?) => {{
        let mut h: u64 = 0xcbf29ce484222325; // FNV-1a offset basis
        $(
            h ^= ::core::mem::size_of::<$t>() as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
            h ^= ::core::mem::align_of::<$t>() as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        )+
        h
    }};
}

fn file_signature(path: &Path) -> Option<(SystemTime, u64)> {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md.modified().ok()?;
    Some((mtime, md.len()))
}

/// Best-effort check that the file can be opened for reading. The **primary**
/// quiescence guard is the `(mtime, size)` stability window in [`Reloader::poll`];
/// this is a secondary check that the producer isn't holding an exclusive lock at
/// the instant we copy. It does not, by itself, prove no writer is mid-write (a
/// producer using a permissive share mode could still allow the open) — a
/// partial/failed load is therefore retried after the next stability window
/// rather than treated as permanent.
fn is_readable(path: &Path) -> bool {
    for attempt in 0..5 {
        if std::fs::File::open(path).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10 * (attempt + 1)));
    }
    false
}

fn default_shadow_dir(library_path: &Path) -> PathBuf {
    let base = library_path
        .parent()
        .map(|p| p.join("deadsync-hot"))
        .unwrap_or_else(|| std::env::temp_dir().join("deadsync-hot"));
    base.join(std::process::id().to_string())
}

/// Best-effort removal of our own per-pid shadow dirs left by prior runs.
///
/// Only ever called for the default `.../deadsync-hot/<pid>` layout. To avoid
/// touching anything we didn't create, this prunes only sibling directories
/// whose name is an all-digit (pid-style) string other than ours. It does not
/// probe pid liveness — a colliding live pid is astronomically unlikely and the
/// removal is best-effort regardless.
fn clean_stale_shadow_dirs(our_dir: &Path) {
    let Some(parent) = our_dir.parent() else {
        return;
    };
    let ours = our_dir.file_name();
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if Some(name.as_os_str()) == ours.map(OsStr::new) {
            continue;
        }
        let is_pid_dir = name
            .to_str()
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
        if !is_pid_dir {
            continue;
        }
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Best-effort copy of `foo.pdb` next to a shadow-copied `foo.dll`.
fn copy_sidecar_pdb(source: &Path, shadow: &Path) {
    let src_pdb = source.with_extension("pdb");
    if src_pdb.exists() {
        let dst_pdb = shadow.with_extension("pdb");
        let _ = std::fs::copy(&src_pdb, &dst_pdb);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn well_formed() -> (HotHeaderCore, Expected) {
        let vtable_target: u64 = 0;
        let header = HotHeaderCore {
            magic: 0xABCD,
            abi_version: 3,
            panic_strategy: PANIC_UNWIND,
            _pad: [0; 3],
            size: size_of::<HotHeaderCore>() as u32,
            layout_hash: 0x1111,
            build_hash: 0x2222,
            vtable: (&vtable_target as *const u64) as *const (),
        };
        let expected = Expected {
            magic: 0xABCD,
            abi_version: 3,
            size: size_of::<HotHeaderCore>() as u32,
            layout_hash: 0x1111,
            build_hash: 0x2222,
            panic_strategy: PANIC_UNWIND,
        };
        (header, expected)
    }

    #[test]
    fn accepts_matching_header() {
        let (header, expected) = well_formed();
        assert_eq!(header.verify(&expected), Ok(()));
        assert!(header.vtable_ptr().is_some());
    }

    #[test]
    fn rejects_null_vtable() {
        let (mut header, expected) = well_formed();
        header.vtable = std::ptr::null();
        assert_eq!(header.verify(&expected), Err(HeaderRejection::NullVtable));
    }

    #[test]
    fn rejects_each_field_mismatch() {
        let (header, base) = well_formed();
        assert!(matches!(
            header.verify(&Expected { magic: 1, ..base }),
            Err(HeaderRejection::Magic { .. })
        ));
        assert!(matches!(
            header.verify(&Expected { abi_version: 99, ..base }),
            Err(HeaderRejection::AbiVersion { .. })
        ));
        assert!(matches!(
            header.verify(&Expected { size: 1, ..base }),
            Err(HeaderRejection::Size { .. })
        ));
        assert!(matches!(
            header.verify(&Expected { panic_strategy: PANIC_ABORT, ..base }),
            Err(HeaderRejection::PanicStrategy { .. })
        ));
        assert!(matches!(
            header.verify(&Expected { layout_hash: 7, ..base }),
            Err(HeaderRejection::LayoutHash { .. })
        ));
        assert!(matches!(
            header.verify(&Expected { build_hash: 7, ..base }),
            Err(HeaderRejection::BuildHash { .. })
        ));
    }

    #[test]
    fn layout_hash_macro_is_stable_and_distinct() {
        let a = hot_layout_hash!(u8, u64, [u8; 3]);
        let b = hot_layout_hash!(u8, u64, [u8; 3]);
        let c = hot_layout_hash!(u8, u64, [u8; 4]);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn guard_catches_panic() {
        let ok = guard(|| 21 * 2);
        assert_eq!(ok, Some(42));
        let bad = guard(|| -> i32 { panic!("boom") });
        assert_eq!(bad, None);
    }

    #[test]
    fn builder_requires_path_and_expected() {
        let (_h, expected) = well_formed();
        assert!(matches!(
            Reloader::builder().expected(expected).build(),
            Err(ReloadError::MissingConfig("library_path"))
        ));
        assert!(matches!(
            Reloader::builder().library_path("x.dll").build(),
            Err(ReloadError::MissingConfig("expected"))
        ));
    }
}
