//! The immutable bytecode and its per-instruction metadata share a block.
use super::fallible;
use crate::{
    Error,
    bytecode::{Instruction, InstructionMetadata},
};
use std::{fmt, mem::MaybeUninit, slice};

type Word = MaybeUninit<u32>;
const _: () = {
    assert!(align_of::<Instruction>() <= align_of::<Word>());
    assert!(align_of::<InstructionMetadata>() <= align_of::<Word>());
    assert!(size_of::<Instruction>() % align_of::<InstructionMetadata>() == 0);
    assert!((size_of::<Instruction>() + size_of::<InstructionMetadata>()) % size_of::<Word>() == 0);
};

#[derive(Clone, Default)]
pub(crate) struct PackedCode {
    words: Vec<Word>,
    count: usize,
    #[cfg(test)]
    missing_metadata: bool,
}

impl PackedCode {
    const WORDS_PER_INSTRUCTION: usize =
        (size_of::<Instruction>() + size_of::<InstructionMetadata>()) / size_of::<Word>();

    pub(crate) fn byte_count(count: usize) -> Option<usize> {
        count.checked_mul(Self::WORDS_PER_INSTRUCTION * size_of::<Word>())
    }

    fn reserve(&mut self, count: usize) -> Result<usize, Error> {
        let words = count
            .checked_mul(Self::WORDS_PER_INSTRUCTION)
            .ok_or_else(Error::memory)?;
        if words > self.words.capacity() {
            self.words = fallible::with_capacity(words)?;
        }
        self.count = 0;
        #[cfg(test)]
        {
            self.missing_metadata = false;
        }
        Ok(words)
    }

    pub(crate) fn try_write(
        &mut self,
        count: usize,
        mut emit: impl FnMut(usize) -> Result<(Instruction, InstructionMetadata), Error>,
    ) -> Result<(), Error> {
        let words = self.reserve(count)?;
        // SAFETY: the block covers both adjacent arrays with the required
        // alignment. Instruction and metadata values are Copy, so a failed
        // emit cannot leak values. Expose the views only after the whole write.
        unsafe {
            let instructions = self.words.as_mut_ptr().cast::<Instruction>();
            let metadata = instructions.add(count).cast::<InstructionMetadata>();
            for pc in 0..count {
                let (instruction, line) = emit(pc)?;
                instructions.add(pc).write(instruction);
                metadata.add(pc).write(line);
            }
            self.words.set_len(words);
        }
        self.count = count;
        Ok(())
    }

    pub(crate) fn try_copy_from(
        &mut self,
        instructions: &[Instruction],
        metadata: &[InstructionMetadata],
    ) -> Result<(), Error> {
        let count = instructions.len();
        assert_eq!(count, metadata.len());
        let words = self.reserve(count)?;
        // SAFETY: reservation covers two adjacent, aligned ranges, both
        // disjoint from the two slices borrowed by the caller.
        unsafe {
            let code = self.words.as_mut_ptr().cast::<Instruction>();
            code.copy_from_nonoverlapping(instructions.as_ptr(), count);
            code.add(count)
                .cast::<InstructionMetadata>()
                .copy_from_nonoverlapping(metadata.as_ptr(), count);
            self.words.set_len(words);
        }
        self.count = count;
        Ok(())
    }

    #[inline]
    pub(crate) fn code(&self) -> &[Instruction] {
        // SAFETY: count changes only after every instruction is initialized.
        unsafe { slice::from_raw_parts(self.words.as_ptr().cast(), self.count) }
    }

    #[inline]
    pub(crate) fn metadata(&self) -> &[InstructionMetadata] {
        #[cfg(test)]
        if self.missing_metadata {
            return &[];
        }
        // SAFETY: count describes both initialized arrays. The metadata starts
        // at the end of the instruction array in the same allocation.
        unsafe {
            let code = self.words.as_ptr().cast::<Instruction>();
            slice::from_raw_parts(code.add(self.count).cast(), self.count)
        }
    }

    pub(crate) fn metadata_mut(&mut self) -> &mut [InstructionMetadata] {
        // SAFETY: the exclusive borrow covers both arrays and the metadata is
        // initialized. Derive the pointer from the full allocation.
        unsafe {
            let code = self.words.as_mut_ptr().cast::<Instruction>();
            slice::from_raw_parts_mut(code.add(self.count).cast(), self.count)
        }
    }

    #[cfg(test)]
    pub(crate) fn code_mut(&mut self) -> &mut [Instruction] {
        // SAFETY: the exclusive borrow covers the initialized instructions.
        unsafe { slice::from_raw_parts_mut(self.words.as_mut_ptr().cast(), self.count) }
    }

    #[cfg(test)]
    pub(crate) fn omit_metadata(&mut self) {
        self.missing_metadata = true;
    }

    pub(crate) fn heap_bytes(&self) -> usize {
        self.words.capacity() * size_of::<Word>()
    }
}

impl fmt::Debug for PackedCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PackedCode")
            .field("code", &self.code())
            .field("metadata", &self.metadata())
            .finish()
    }
}
