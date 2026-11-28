use super::fallible;
use crate::{
    Error,
    bytecode::{DebugCall, DebugLocal, DebugName},
};
use std::{fmt, marker::PhantomData, mem::MaybeUninit, ptr, slice};

type Word = MaybeUninit<usize>;

// The record arrays precede the active lines and byte pools. Each record
// leaves the next array suitably aligned without padding.
const _: () = {
    assert!(align_of::<DebugName>() <= align_of::<Word>());
    assert!(align_of::<DebugLocal>() <= align_of::<Word>());
    assert!(align_of::<DebugCall>() <= align_of::<Word>());
    assert!(align_of::<u32>() <= align_of::<Word>());
    assert!(size_of::<DebugName>() % align_of::<Word>() == 0);
    assert!(size_of::<DebugLocal>() % align_of::<Word>() == 0);
    assert!(size_of::<DebugCall>() % align_of::<Word>() == 0);
};

#[derive(Default)]
struct Ends {
    upvalues: usize,
    locals: usize,
    calls: usize,
    lines: usize,
    names: usize,
    temporaries: usize,
}

/// Debug records and byte pools published together from reusable compiler buffers.
#[derive(Default)]
pub(crate) struct PackedDebug {
    // Keep the Vec empty. Its capacity owns the storage, and the offsets
    // identify the initialized values whose destructors this type runs.
    words: Vec<Word>,
    ends: Ends,
    records: PhantomData<(DebugName, DebugLocal, DebugCall)>,
}

impl PackedDebug {
    pub(crate) fn try_replace(
        &mut self,
        upvalues: &mut Vec<DebugName>,
        locals: &mut Vec<DebugLocal>,
        calls: &mut Vec<DebugCall>,
        lines: &[u32],
        names: &[u8],
        temporaries: &[u8],
    ) -> Result<(), Error> {
        fn end<T>(start: usize, count: usize) -> Result<usize, Error> {
            count
                .checked_mul(size_of::<T>())
                .and_then(|bytes| start.checked_add(bytes))
                .ok_or_else(Error::memory)
        }

        let upvalue_end = end::<DebugName>(0, upvalues.len())?;
        let local_end = end::<DebugLocal>(upvalue_end, locals.len())?;
        let call_end = end::<DebugCall>(local_end, calls.len())?;
        let line_end = end::<u32>(call_end, lines.len())?;
        let name_end = end::<u8>(line_end, names.len())?;
        let temporary_end = end::<u8>(name_end, temporaries.len())?;
        let capacity = temporary_end.div_ceil(size_of::<Word>());
        if capacity > self.words.capacity() {
            let words = fallible::with_capacity(capacity)?;
            self.clear();
            self.words = words;
        } else {
            self.clear();
        }

        let base = self.words.as_mut_ptr().cast::<u8>();
        // SAFETY: checked lengths and the reserved capacity cover all six
        // disjoint ranges. The record and line ranges satisfy their alignments.
        // The buffers are borrowed separately from this allocation. Moving
        // their records leaves the scratch allocations empty and reusable.
        unsafe {
            Self::move_records(upvalues, base.cast());
            Self::move_records(locals, base.add(upvalue_end).cast());
            Self::move_records(calls, base.add(local_end).cast());
            ptr::copy_nonoverlapping(lines.as_ptr(), base.add(call_end).cast(), lines.len());
            ptr::copy_nonoverlapping(names.as_ptr(), base.add(line_end), names.len());
            ptr::copy_nonoverlapping(temporaries.as_ptr(), base.add(name_end), temporaries.len());
        }
        self.ends = Ends {
            upvalues: upvalue_end,
            locals: local_end,
            calls: call_end,
            lines: line_end,
            names: name_end,
            temporaries: temporary_end,
        };
        Ok(())
    }

    /// `output` must cover `source.len()` aligned, uninitialized records.
    unsafe fn move_records<T>(source: &mut Vec<T>, output: *mut T) {
        // SAFETY: the caller supplies disjoint storage. Clearing the source
        // length transfers ownership of every copied record.
        unsafe {
            ptr::copy_nonoverlapping(source.as_ptr(), output, source.len());
            source.set_len(0);
        }
    }

    pub(crate) fn upvalues(&self) -> &[DebugName] {
        self.view(0, self.ends.upvalues)
    }

    pub(crate) fn locals(&self) -> &[DebugLocal] {
        self.view(self.ends.upvalues, self.ends.locals)
    }

    pub(crate) fn calls(&self) -> &[DebugCall] {
        self.view(self.ends.locals, self.ends.calls)
    }

    pub(crate) fn active_lines(&self) -> &[u32] {
        self.view(self.ends.calls, self.ends.lines)
    }

    pub(crate) fn active_lines_mut(&mut self) -> &mut [u32] {
        self.view_mut(self.ends.calls, self.ends.lines)
    }

    pub(crate) fn names(&self) -> &[u8] {
        self.view(self.ends.lines, self.ends.names)
    }

    pub(crate) fn temporaries(&self) -> &[u8] {
        self.view(self.ends.names, self.ends.temporaries)
    }

    fn view<T>(&self, start: usize, end: usize) -> &[T] {
        // SAFETY: the accessors supply the exact boundaries of a range of T
        // initialized by try_replace, or an aligned empty range.
        unsafe {
            let base = self.words.as_ptr().cast::<u8>();
            slice::from_raw_parts(base.add(start).cast(), (end - start) / size_of::<T>())
        }
    }

    fn view_mut<T>(&mut self, start: usize, end: usize) -> &mut [T] {
        // SAFETY: as with view, callers select the initialized range of T.
        // The exclusive borrow rules out outstanding references to it.
        unsafe {
            let base = self.words.as_mut_ptr().cast::<u8>();
            slice::from_raw_parts_mut(base.add(start).cast(), (end - start) / size_of::<T>())
        }
    }

    fn clear(&mut self) {
        // SAFETY: the three initialized record ranges each have sole ownership
        // of their elements. The line and byte ranges need no drop.
        unsafe {
            ptr::drop_in_place(self.view_mut::<DebugName>(0, self.ends.upvalues));
            ptr::drop_in_place(self.view_mut::<DebugLocal>(self.ends.upvalues, self.ends.locals));
            ptr::drop_in_place(self.view_mut::<DebugCall>(self.ends.locals, self.ends.calls));
        }
        self.ends = Ends::default();
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.words.capacity() * size_of::<Word>()
    }

    #[cfg(test)]
    pub(crate) fn locals_mut(&mut self) -> &mut [DebugLocal] {
        self.view_mut(self.ends.upvalues, self.ends.locals)
    }

    #[cfg(test)]
    pub(crate) fn calls_mut(&mut self) -> &mut [DebugCall] {
        self.view_mut(self.ends.locals, self.ends.calls)
    }
}

impl Drop for PackedDebug {
    fn drop(&mut self) {
        self.clear();
    }
}

impl Clone for PackedDebug {
    fn clone(&self) -> Self {
        let mut copy = Self {
            words: Vec::with_capacity(self.ends.temporaries.div_ceil(size_of::<Word>())),
            ends: Ends::default(),
            records: PhantomData,
        };
        copy.try_replace(
            &mut self.upvalues().to_vec(),
            &mut self.locals().to_vec(),
            &mut self.calls().to_vec(),
            self.active_lines(),
            self.names(),
            self.temporaries(),
        )
        .expect("debug storage allocation");
        copy
    }
}

impl fmt::Debug for PackedDebug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PackedDebug")
            .field("upvalues", &self.upvalues())
            .field("locals", &self.locals())
            .field("calls", &self.calls())
            .field("active_lines", &self.active_lines())
            .field("names", &self.names())
            .field("temporaries", &self.temporaries())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::NameKind;

    #[test]
    fn packed_records_keep_ownership_across_clone_and_reuse() {
        let mut upvalues = vec![DebugName::Owned(b"upvalue".to_vec().into_boxed_slice())];
        let mut locals = vec![DebugLocal {
            name: DebugName::Owned(b"local".to_vec().into_boxed_slice()),
            start: 0,
            end: 1,
            register: 0,
        }];
        let mut calls = vec![DebugCall {
            pc: 1,
            name: DebugName::Owned(b"call".to_vec().into_boxed_slice()),
            kind: NameKind::Local,
            temporaries: 0..2,
        }];
        let mut packed = PackedDebug::default();
        packed
            .try_replace(
                &mut upvalues,
                &mut locals,
                &mut calls,
                &[3, 17],
                b"name",
                &[1, 2],
            )
            .unwrap();
        assert!(upvalues.is_empty() && locals.is_empty() && calls.is_empty());
        let copy = packed.clone();
        let capacity = packed.heap_bytes();
        packed
            .try_replace(&mut upvalues, &mut locals, &mut calls, &[9], b"other", &[])
            .unwrap();
        assert_eq!(packed.heap_bytes(), capacity);
        assert!(
            packed.upvalues().is_empty() && packed.locals().is_empty() && packed.calls().is_empty()
        );
        assert_eq!(packed.names(), b"other");
        assert!(packed.temporaries().is_empty());
        for (name, expected) in [
            (&copy.upvalues()[0], &b"upvalue"[..]),
            (&copy.locals()[0].name, &b"local"[..]),
            (&copy.calls()[0].name, &b"call"[..]),
        ] {
            assert!(matches!(name, DebugName::Owned(bytes) if bytes.as_ref() == expected));
        }
        assert_eq!(packed.active_lines(), [9]);
        assert_eq!(copy.active_lines(), [3, 17]);
        assert_eq!(copy.names(), b"name");
        assert_eq!(copy.temporaries(), [1, 2]);
    }
}
