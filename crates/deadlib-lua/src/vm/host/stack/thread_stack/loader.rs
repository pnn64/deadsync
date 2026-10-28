//! Protected loading while another thread's stack is selected.
use super::*;

impl NativeStack<'_, '_> {
    #[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
    pub(crate) fn attach_standard_stream_thread(
        &mut self,
        thread: ThreadId,
        index: isize,
        kind: usize,
    ) -> Result<(), Error> {
        self.with_thread_stack(thread, |stack| stack.attach_standard_stream(index, kind))
    }
    /// Compile through a reader on the selected thread's native stack.
    /// Reader callbacks see that thread's stack, environment and retained status.
    /// Loading does not resume its pending computation. Parser output follows
    /// [`Self::load_reader`], including changes to the visible window and pending
    /// frames when the reader yields.
    ///
    /// # Errors
    /// Rejects invalid threads, pending errors, stack overflow and fatal quotas.
    pub fn load_reader_thread(
        &mut self,
        thread: ThreadId,
        name: Option<&[u8]>,
        reader: impl FnMut(&mut NativeStack<'_, '_>) -> Result<Option<Vec<u8>>, Error>,
    ) -> Result<NativeLoadStatus, Error> {
        self.with_thread_stack(thread, |stack| stack.load_reader(name, reader))
    }
    /// Compile a named file, or shared stdin, on the selected thread's stack.
    /// File ownership, source names and error precedence follow [`Self::load_file`].
    /// The target's globals initialize the function environment; loading preserves
    /// its pending computation and retained execution status.
    ///
    /// # Errors
    /// Rejects invalid threads, pending errors, stack overflow and fatal quotas.
    pub fn load_file_thread(
        &mut self,
        thread: ThreadId,
        path: Option<&[u8]>,
    ) -> Result<NativeLoadStatus, Error> {
        self.with_thread_stack(thread, |stack| stack.load_file(path))
    }
}
