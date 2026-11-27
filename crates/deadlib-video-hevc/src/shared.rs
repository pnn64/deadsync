//! Picture tables shared between the threads decoding one picture.
//!
//! This is the only module of the crate with `unsafe` code. It exists so
//! that picture tables can be read by one thread while another thread
//! writes other parts of them:
//!
//! - Wavefront parallel processing: several CTB rows of one picture decode
//!   at the same time, each writing its own part of the sample planes and
//!   per-picture tables while reading what the rows above have finished.
//! - Frame threading: a picture is referenced by later pictures (motion
//!   compensation, temporal MV prediction) while its own thread is still
//!   decoding and filtering its lower rows.
//!
//! # The tables
//!
//! A [`SharedTable`] is either the *owner* of a buffer (the picture state's
//! table, used like a `Vec` / slice) or a *view* of an owner's buffer (one
//! per worker thread, created by [`SharedTable::view`]). A view supports
//! only element and range indexing, which create references to exactly the
//! elements accessed and never to the whole buffer. Two threads accessing
//! disjoint elements through their views therefore never hold overlapping
//! references. Whole-buffer operations (`Deref` to a slice) panic on a view.
//!
//! # The protocol
//!
//! Views are sound as long as no element is written by one thread while
//! another thread accesses it. The wavefront schedule guarantees that:
//!
//! - every element belongs to one CTB (by its position in the picture);
//! - a worker writes only elements of the CTB it is decoding, each range
//!   within one table row (a range over several rows covers the start of
//!   its last row, which can be the CTB row's bottom row, read by the row
//!   below: Miri found this; blocks go through `cu_tree::with_block`);
//! - a worker reads elements of CTBs in its own CTB row, or of CTBs in rows
//!   above that are finished: row `r` decodes CTB `c` only after row `r - 1`
//!   has finished CTB `c + 1` (published through [`RowProgress`]);
//! - while decoding CTB `(r, c)` a worker may also read CTBs `c - 1` and
//!   later of row `r + 1` (the below-left merge candidate A0 reads its
//!   CTB's slice address and motion field, which decide that it is not
//!   available yet). Those CTBs cannot have started: row `r + 1` starts
//!   CTB `c'` only after it observes (Acquire) that row `r` finished CTB
//!   `c' + 1 >= c`, which row `r` publishes (Release) after this read. The
//!   read therefore happens before any write to them and sees their initial
//!   values, as a sequential decoder does.
//!
//! In test builds each view checks every access against that rule (see
//! [`AccessCheck`]), so the decode path cannot silently step outside it:
//! the threaded decoding tests run every WPP fixture with checking on.
//!
//! Frame threading follows a second, simpler rule, per picture: rows
//! become final (decoded, deblocked and SAO-filtered) in order, top to
//! bottom, and a final row is never written again (`crate::rowfilter`
//! rejects a slice that would). The picture's DPB entry owns its planes,
//! motion field and slice map; the picture's thread writes them through
//! views, and other pictures read them through the owner, only with range
//! indexing ending at the rows `DecodedPicture::progress` reports final
//! (`Release` / `Acquire`), never through a reference to the whole table.
//! In test builds motion compensation asserts it never reads a reference
//! row it did not wait for.

use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Index, IndexMut, Range};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// A buffer that is either owned (behaves like a `Vec<T>` / `[T]`) or a
/// view of another table's buffer for one worker thread. See the module
/// documentation for the rules.
pub struct SharedTable<T> {
    ptr: *mut T,
    len: usize,
    /// Capacity of the allocation when owned; `None` for a view.
    cap: Option<usize>,
    /// Access checking for a view, in test builds.
    #[cfg(test)]
    check: Option<(Arc<AccessCheck>, Geometry, &'static str)>,
    _owns: PhantomData<T>,
}

// SAFETY: an owned table is a `Vec<T>`, which is `Send` / `Sync` when `T`
// is. Views share the buffer between threads; the wavefront protocol (see
// the module documentation) ensures no element is accessed by one thread
// while another writes it, so sharing views is sound for `T: Send + Sync`.
unsafe impl<T: Send> Send for SharedTable<T> {}
unsafe impl<T: Send + Sync> Sync for SharedTable<T> {}

impl<T> SharedTable<T> {
    /// Number of elements.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether this table owns its buffer (false for a worker view).
    #[inline]
    pub fn is_owner(&self) -> bool {
        self.cap.is_some()
    }

    /// The owned buffer as a `Vec`. Panics on a view.
    pub fn into_vec(self) -> Vec<T> {
        let cap = self.cap.expect("into_vec on a worker view");
        let (ptr, len) = (self.ptr, self.len);
        std::mem::forget(self);
        // SAFETY: `ptr`, `len` and `cap` come from `Vec::into_raw_parts`
        // equivalent in `From<Vec<T>>` and were not changed since.
        unsafe { Vec::from_raw_parts(ptr, len, cap) }
    }

    /// A view of this table's buffer for a worker thread.
    ///
    /// # Safety
    ///
    /// The view must be dropped before this table is dropped (moving the
    /// table is fine: its buffer stays where it is). While views exist,
    /// this table may be used only through element and range indexing, not
    /// as a whole slice (`Deref`), and every access made through it or its
    /// views must follow the protocol (module documentation): no element
    /// accessed by one thread while another writes it.
    pub unsafe fn view(&self) -> SharedTable<T> {
        SharedTable {
            ptr: self.ptr,
            len: self.len,
            cap: None,
            #[cfg(test)]
            check: None,
            _owns: PhantomData,
        }
    }

    /// Attach access checking to a view (test builds only).
    #[cfg(test)]
    pub fn set_check(&mut self, check: Arc<AccessCheck>, geometry: Geometry, name: &'static str) {
        debug_assert!(!self.is_owner());
        self.check = Some((check, geometry, name));
    }

    #[inline(always)]
    fn check_read(&self, _range: Range<usize>) {
        #[cfg(test)]
        if let Some((c, g, name)) = &self.check {
            c.check(g, _range, false, name);
        }
    }

    #[inline(always)]
    fn check_write(&self, _range: Range<usize>) {
        #[cfg(test)]
        if let Some((c, g, name)) = &self.check {
            c.check(g, _range, true, name);
        }
    }

    /// Element `i`, or `None` when out of range.
    #[inline]
    pub fn get(&self, i: usize) -> Option<&T> {
        if i < self.len {
            self.check_read(i..i + 1);
            // SAFETY: in bounds; the protocol rules out a concurrent write.
            Some(unsafe { &*self.ptr.add(i) })
        } else {
            None
        }
    }

    /// Elements `r`, or `None` when the range is out of bounds. Works on
    /// views (covers exactly the range).
    #[inline]
    pub fn get_range(&self, r: Range<usize>) -> Option<&[T]> {
        if r.start <= r.end && r.end <= self.len {
            Some(&self[r])
        } else {
            None
        }
    }

    /// Element `i` for writing, or `None` when out of range.
    #[inline]
    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        if i < self.len {
            self.check_write(i..i + 1);
            // SAFETY: in bounds; the protocol rules out any concurrent access.
            Some(unsafe { &mut *self.ptr.add(i) })
        } else {
            None
        }
    }

    /// Disjoint mutable ranges `cuts[i]..cuts[i + 1]` of the buffer (the
    /// last up to `cuts`' final value). Works on views: each slice covers
    /// exactly its range. Panics unless `cuts` is non-decreasing and ends
    /// within the table.
    pub fn split_mut(&mut self, cuts: &[usize]) -> Vec<&mut [T]> {
        assert!(
            cuts.windows(2).all(|w| w[0] <= w[1]),
            "cuts must not decrease"
        );
        if let Some(&end) = cuts.last() {
            assert!(
                end <= self.len,
                "cut {end} past the end of a table of length {}",
                self.len
            );
        }
        cuts.windows(2)
            .map(|w| {
                self.check_write(w[0]..w[1]);
                // SAFETY: the ranges are in bounds and pairwise disjoint (the
                // cuts do not decrease), and `&mut self` is held for the
                // lifetime of the returned slices, so no other access
                // through this table overlaps them.
                unsafe { std::slice::from_raw_parts_mut(self.ptr.add(w[0]), w[1] - w[0]) }
            })
            .collect()
    }

    #[inline(always)]
    fn range_bounds(&self, r: &Range<usize>) {
        if r.start > r.end || r.end > self.len {
            range_fail(r, self.len);
        }
    }
}

#[cold]
#[inline(never)]
fn range_fail(r: &Range<usize>, len: usize) -> ! {
    panic!(
        "range {}..{} out of bounds for table of length {len}",
        r.start, r.end
    )
}

#[cold]
#[inline(never)]
fn index_fail(i: usize, len: usize) -> ! {
    panic!("index {i} out of bounds for table of length {len}")
}

impl<T> Default for SharedTable<T> {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl<T> From<Vec<T>> for SharedTable<T> {
    fn from(v: Vec<T>) -> Self {
        let mut v = std::mem::ManuallyDrop::new(v);
        SharedTable {
            ptr: v.as_mut_ptr(),
            len: v.len(),
            cap: Some(v.capacity()),
            #[cfg(test)]
            check: None,
            _owns: PhantomData,
        }
    }
}

impl<T> From<SharedTable<T>> for Vec<T> {
    /// The owned buffer. Panics on a view.
    fn from(t: SharedTable<T>) -> Self {
        t.into_vec()
    }
}

impl<T> Drop for SharedTable<T> {
    fn drop(&mut self) {
        if let Some(cap) = self.cap {
            // SAFETY: an owner's raw parts come from a `Vec` (see `From`).
            drop(unsafe { Vec::from_raw_parts(self.ptr, self.len, cap) });
        }
    }
}

impl<T: Clone> Clone for SharedTable<T> {
    /// Clones an owned table into a new owned table. Panics on a view.
    fn clone(&self) -> Self {
        self.deref().to_vec().into()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for SharedTable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_owner() {
            self.deref().fmt(f)
        } else {
            write!(f, "SharedTable view of {} elements", self.len)
        }
    }
}

impl<T> Deref for SharedTable<T> {
    type Target = [T];
    /// The whole buffer. Owned tables only: a view panics, since a
    /// reference to the whole buffer would overlap other threads' rows.
    #[inline]
    fn deref(&self) -> &[T] {
        assert!(self.is_owner(), "whole-table access through a worker view");
        // SAFETY: an owner has no live views while it is accessed (see
        // `view`), and its raw parts describe its allocation.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl<T> DerefMut for SharedTable<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [T] {
        assert!(self.is_owner(), "whole-table access through a worker view");
        // SAFETY: as for `deref`.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl<T> Index<usize> for SharedTable<T> {
    type Output = T;
    #[inline(always)]
    fn index(&self, i: usize) -> &T {
        if i >= self.len {
            index_fail(i, self.len);
        }
        self.check_read(i..i + 1);
        // SAFETY: in bounds; the protocol rules out a concurrent write.
        unsafe { &*self.ptr.add(i) }
    }
}

impl<T> IndexMut<usize> for SharedTable<T> {
    #[inline(always)]
    fn index_mut(&mut self, i: usize) -> &mut T {
        if i >= self.len {
            index_fail(i, self.len);
        }
        self.check_write(i..i + 1);
        // SAFETY: in bounds; the protocol rules out any concurrent access.
        unsafe { &mut *self.ptr.add(i) }
    }
}

impl<T> Index<Range<usize>> for SharedTable<T> {
    type Output = [T];
    #[inline(always)]
    fn index(&self, r: Range<usize>) -> &[T] {
        self.range_bounds(&r);
        self.check_read(r.clone());
        // SAFETY: in bounds; the protocol rules out a concurrent write to
        // any element of the range.
        unsafe { std::slice::from_raw_parts(self.ptr.add(r.start), r.end - r.start) }
    }
}

impl<T> IndexMut<Range<usize>> for SharedTable<T> {
    #[inline(always)]
    fn index_mut(&mut self, r: Range<usize>) -> &mut [T] {
        self.range_bounds(&r);
        self.check_write(r.clone());
        // SAFETY: in bounds; the protocol rules out any concurrent access
        // to the elements of the range.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.add(r.start), r.end - r.start) }
    }
}

/// Whole-table and open-ended ranges: owned tables only (through `Deref`).
macro_rules! owner_range_index {
    ($($r:ty),*) => {$(
        impl<T> Index<$r> for SharedTable<T> {
            type Output = [T];
            #[inline]
            fn index(&self, r: $r) -> &[T] {
                &self.deref()[r]
            }
        }
        impl<T> IndexMut<$r> for SharedTable<T> {
            #[inline]
            fn index_mut(&mut self, r: $r) -> &mut [T] {
                &mut self.deref_mut()[r]
            }
        }
    )*};
}
owner_range_index!(
    std::ops::RangeFrom<usize>,
    std::ops::RangeTo<usize>,
    std::ops::RangeFull,
    std::ops::RangeInclusive<usize>
);

// ---- Row progress ------------------------------------------------------

/// How far each CTB row of the picture is decoded, shared by the workers:
/// `done(r)` is the number of leading CTBs of row `r` that are finished
/// (the row width once the row is complete).
pub struct RowProgress {
    rows: Vec<RowSlot>,
    /// The first row (in decoding order) that stopped before its end (an
    /// error, or the slice ended there); `usize::MAX` while none has. Rows
    /// below it are never reached by a sequential decode, so they give up
    /// instead of waiting forever. Rows above it run to completion.
    failed: AtomicUsize,
}

/// One row's progress, and the means to sleep until it advances.
struct RowSlot {
    done: AtomicUsize,
    /// Threads asleep (or about to sleep) on `wake`: `publish` only takes
    /// the lock to notify when this is nonzero.
    sleepers: AtomicUsize,
    lock: Mutex<()>,
    wake: Condvar,
}

impl RowProgress {
    /// `initial[r]`: CTBs of row `r` finished before the parallel section
    /// starts (by earlier slices).
    pub fn new(initial: impl IntoIterator<Item = usize>) -> Self {
        Self {
            rows: initial
                .into_iter()
                .map(|n| RowSlot {
                    done: AtomicUsize::new(n),
                    sleepers: AtomicUsize::new(0),
                    lock: Mutex::new(()),
                    wake: Condvar::new(),
                })
                .collect(),
            failed: AtomicUsize::new(usize::MAX),
        }
    }

    /// Finished CTBs of row `r` (`Acquire`: everything the row's worker
    /// wrote for those CTBs is visible afterwards).
    #[inline]
    pub fn done(&self, r: usize) -> usize {
        self.rows[r].done.load(Ordering::Acquire)
    }

    /// Publish that row `r` has finished its first `n` CTBs (`Release`),
    /// waking threads waiting on it.
    #[inline]
    pub fn publish(&self, r: usize, n: usize) {
        let row = &self.rows[r];
        // SeqCst with the waiter's `sleepers` increment and re-check (see
        // `wait`): either the waiter sees the new count, or this load sees
        // the waiter and notifies it.
        row.done.store(n, Ordering::SeqCst);
        if row.sleepers.load(Ordering::SeqCst) > 0 {
            Self::notify(row);
        }
    }

    fn notify(row: &RowSlot) {
        // Taking the lock orders this notify after a waiter that has
        // checked the condition but not yet started waiting.
        drop(row.lock.lock().unwrap());
        row.wake.notify_all();
    }

    /// Row `r` stopped before its end: rows below it are abandoned.
    pub fn fail(&self, r: usize) {
        self.failed.fetch_min(r, Ordering::SeqCst);
        for row in &self.rows {
            if row.sleepers.load(Ordering::SeqCst) > 0 {
                Self::notify(row);
            }
        }
    }

    /// Whether a row above `r` stopped early, so row `r` is abandoned.
    pub fn failed_above(&self, r: usize) -> bool {
        self.failed.load(Ordering::Acquire) < r
    }

    /// Wait until row `r` has finished at least `n` CTBs. Returns false if
    /// row `r` or a row above it stopped early first (the waiting row is
    /// then abandoned). Spins briefly, then sleeps until `publish` or
    /// `fail` wakes it: yielding instead lets a busy machine deschedule a
    /// waiting row for a whole scheduler quantum, stalling every row below.
    pub fn wait(&self, r: usize, n: usize) -> bool {
        let ready = |s: &Self| {
            if s.done(r) >= n {
                Some(true)
            } else if s.failed.load(Ordering::SeqCst) <= r {
                Some(false)
            } else {
                None
            }
        };
        for _ in 0..2000 {
            if let Some(ok) = ready(self) {
                return ok;
            }
            std::hint::spin_loop();
        }
        let row = &self.rows[r];
        let mut guard = row.lock.lock().unwrap();
        row.sleepers.fetch_add(1, Ordering::SeqCst);
        let ok = loop {
            if let Some(ok) = ready(self) {
                break ok;
            }
            guard = row.wake.wait(guard).unwrap();
        };
        row.sleepers.fetch_sub(1, Ordering::SeqCst);
        drop(guard);
        ok
    }
}

// ---- Access checking (test builds) ---------------------------------------

/// How a table's elements map onto CTBs: element `i` is at table row
/// `i / row_len`, column `i % row_len`, and a CTB covers `ctb_rows` table
/// rows and `ctb_cols` table columns.
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub row_len: usize,
    pub ctb_rows: usize,
    pub ctb_cols: usize,
}

/// Validates a worker's table accesses against the wavefront protocol (in
/// test builds): writes only into the CTB being decoded, reads only from
/// the worker's own CTB row or from finished CTBs of rows above.
pub struct AccessCheck {
    progress: Arc<RowProgress>,
    /// The CTB being decoded, `(row, column)`, or `None` between CTBs.
    current: std::sync::Mutex<Option<(usize, usize)>>,
}

impl AccessCheck {
    pub fn new(progress: Arc<RowProgress>) -> Self {
        Self {
            progress,
            current: std::sync::Mutex::new(None),
        }
    }

    pub fn set_current(&self, ctb: Option<(usize, usize)>) {
        *self.current.lock().unwrap() = ctb;
    }

    fn check(&self, g: &Geometry, range: Range<usize>, write: bool, table: &str) {
        if range.is_empty() {
            return;
        }
        let Some((row, col)) = *self.current.lock().unwrap() else {
            return;
        };
        for i in [range.start, range.end - 1] {
            let (r, c) = ((i / g.row_len) / g.ctb_rows, (i % g.row_len) / g.ctb_cols);
            let ok = if write {
                r == row && c == col
            } else {
                r == row || (r < row && self.progress.done(r) > c) || (r == row + 1 && c + 1 >= col)
            };
            assert!(
                ok,
                "wavefront violation: CTB ({row}, {col}) {} {table}[{i}] in CTB ({r}, {c})",
                if write { "writes" } else { "reads" },
            );
        }
        // A range must lie within one table row: a range over several rows
        // covers whole rows in between and the start of its last row, which
        // can be the bottom row of the CTB row, read concurrently by the row
        // below (its intra references).
        let (t0, t1) = (range.start / g.row_len, (range.end - 1) / g.row_len);
        assert!(
            t0 == t1,
            "wavefront violation: CTB ({row}, {col}) accesses {table}[{}..{}], spanning table rows {t0}..={t1}",
            range.start,
            range.end,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_behaves_like_a_vec() {
        let mut t: SharedTable<u32> = vec![1, 2, 3, 4].into();
        assert_eq!(t.len(), 4);
        t[1] = 7;
        t[2..4].copy_from_slice(&[8, 9]);
        assert_eq!(&t[..], &[1, 7, 8, 9]);
        assert_eq!(t.get(4), None);
        let c = t.clone();
        assert_eq!(t.into_vec(), vec![1, 7, 8, 9]);
        assert_eq!(&c[..], &[1, 7, 8, 9]);
    }

    #[test]
    fn views_write_disjoint_elements_from_threads() {
        let t: SharedTable<u32> = vec![0; 64].into();
        // SAFETY: the views are dropped inside the scope, before `t` is
        // used again, and each thread writes only its own 16 elements.
        let views: Vec<SharedTable<u32>> = (0..4).map(|_| unsafe { t.view() }).collect();
        std::thread::scope(|s| {
            for (k, mut v) in views.into_iter().enumerate() {
                s.spawn(move || {
                    for i in k * 16..(k + 1) * 16 {
                        v[i] = i as u32;
                    }
                    v[k * 16..k * 16 + 2].copy_from_slice(&[100, 101]);
                });
            }
        });
        for k in 0..4 {
            assert_eq!(t[k * 16], 100);
            assert_eq!(t[k * 16 + 1], 101);
            assert_eq!(t[k * 16 + 5], (k * 16 + 5) as u32);
        }
    }

    #[test]
    #[should_panic(expected = "whole-table access through a worker view")]
    fn view_refuses_whole_table_access() {
        let t: SharedTable<u8> = vec![0; 4].into();
        // SAFETY: single-threaded; the view is dropped before `t`.
        let v = unsafe { t.view() };
        let _ = v.iter().count();
    }

    #[test]
    fn access_check_enforces_the_wavefront_rule() {
        // 3 CTB rows x 4 CTB columns, one element per CTB.
        let g = Geometry {
            row_len: 4,
            ctb_rows: 1,
            ctb_cols: 1,
        };
        let progress = Arc::new(RowProgress::new([4, 3, 0]));
        let check = AccessCheck::new(progress);
        check.set_current(Some((1, 1)));
        check.check(&g, 0..1, false, "t"); // row above, finished
        check.check(&g, 4 + 3..4 + 4, false, "t"); // own row, any column
        check.check(&g, 4 + 1..4 + 2, true, "t"); // own CTB
        let r = std::panic::catch_unwind(|| check.check(&g, 4 + 2..4 + 3, true, "t"));
        assert!(r.is_err(), "write outside the current CTB");
        // Row below: CTBs from one column left of the current one may be
        // read (they cannot have started), earlier ones may not.
        check.check(&g, 8..9, false, "t");
        check.set_current(Some((1, 2)));
        let r = std::panic::catch_unwind(|| check.check(&g, 8..9, false, "t"));
        assert!(
            r.is_err(),
            "read of a CTB of the row below that may be in progress"
        );
        check.set_current(Some((1, 1)));
        let check2 = AccessCheck::new(Arc::new(RowProgress::new([2, 0, 0])));
        check2.set_current(Some((1, 1)));
        let r = std::panic::catch_unwind(|| check2.check(&g, 2..3, false, "t"));
        assert!(r.is_err(), "read of an unfinished CTB above");
    }

    #[test]
    fn progress_wait_and_fail() {
        let p = Arc::new(RowProgress::new([0, 0, 0]));
        let p2 = Arc::clone(&p);
        let h = std::thread::spawn(move || p2.wait(0, 3));
        p.publish(0, 3);
        assert!(h.join().unwrap());
        // Row 1 stops early: a wait on row 1 (by row 2) gives up, a wait on
        // row 0 (by row 1's own neighbours above) does not.
        p.fail(1);
        let p3 = Arc::clone(&p);
        assert!(!std::thread::spawn(move || p3.wait(1, 1)).join().unwrap());
        assert!(p.wait(0, 3));
        assert!(p.failed_above(2) && !p.failed_above(1));
    }
}
