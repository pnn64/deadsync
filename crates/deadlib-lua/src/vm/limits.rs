//! Lua 5.1 logical call-info growth, overflow reserve and protected recovery.
use super::*;

const MAX_CALLS: u16 = 20_000;
pub(super) const MAX_CSTACK: usize = 8_000;

fn depth(frames: &[Frame]) -> usize {
    frames.iter().filter(|frame| frame.is_call()).count()
}

impl Vm {
    #[inline]
    pub(super) fn check_calls(&mut self) -> Result<(), Error> {
        // Physical frames include internal continuations, so this upper bound
        // avoids walking ordinary stacks without counting helpers as Lua calls.
        if self.frames.len() + 1 < usize::from(self.call_slots) {
            return Ok(());
        }
        self.grow_calls()
    }

    #[cold]
    fn grow_calls(&mut self) -> Result<(), Error> {
        if depth(&self.frames) + 1 < usize::from(self.call_slots) {
            return Ok(());
        }
        if self.call_slots > MAX_CALLS {
            let value = self.intern(b"error in error handling");
            return Err(Error::handling(value));
        }
        self.call_slots *= 2;
        // Keep oversized logical capacity as reserve for the error handler.
        if self.call_slots > MAX_CALLS {
            return Err(self.call_error("stack overflow"));
        }
        Ok(())
    }

    pub(super) fn restore_calls(&mut self) {
        if self.call_slots > MAX_CALLS && depth(&self.frames) + 1 < usize::from(MAX_CALLS) {
            self.call_slots = MAX_CALLS;
        }
    }

    #[cold]
    pub(super) fn check_c_call(&mut self) -> Result<(), Error> {
        let depth = self.native_depth(self.frames.len());
        self.check_c_depth(depth)
    }

    pub(super) fn check_iterator_call(&mut self) -> Result<(), Error> {
        // Unlike a scheduled native continuation, an iterator boundary has
        // not been installed when its call is checked.
        let depth = self.native_depth(self.frames.len()) + 1;
        self.check_c_depth(depth)
    }

    fn check_c_depth(&mut self, depth: usize) -> Result<(), Error> {
        if depth >= 224 {
            let value = self.intern(b"error in error handling");
            return Err(Error::handling(value));
        }
        if depth == 199 {
            return Err(self.call_error("C stack overflow"));
        }
        Ok(())
    }

    #[cold]
    fn call_error(&mut self, message: &'static str) -> Error {
        let error = Error::new(ErrorKind::Runtime, 0, message);
        if self
            .frames
            .iter()
            .rev()
            .find(|f| f.is_call())
            .is_some_and(|f| matches!(f.function, FrameKind::Native(_)))
        {
            let value = self.intern(message.as_bytes());
            error.with_value(value)
        } else {
            error
        }
    }

    pub(super) fn shrink_thread_calls(&mut self, thread: ThreadId) {
        let (slots, frames) = if thread == self.thread {
            (&mut self.call_slots, &self.frames)
        } else {
            let thread = &mut self.threads[slot_index(thread.0)];
            (&mut thread.call_slots, &thread.frames)
        };
        if *slots <= MAX_CALLS && *slots > 16 && depth(frames) * 4 < usize::from(*slots) {
            *slots /= 2;
        }
    }
}
