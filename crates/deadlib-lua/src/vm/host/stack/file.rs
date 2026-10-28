//! Native file loading uses the same streams and header prelude as Lua loadfile.
use super::*;

impl NativeStack<'_, '_> {
    #[cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
    pub(crate) fn attach_standard_stream(
        &mut self,
        index: isize,
        kind: usize,
    ) -> Result<(), Error> {
        let Some(Value::Userdata(id)) = self.value(index) else {
            return Err(self.error(b"standard stream userdata expected"));
        };
        if kind > 2
            || self.call.vm.userdata[slot_index(id.0)].bytes.len() != std::mem::size_of::<usize>()
        {
            return Err(self.error(b"invalid standard stream"));
        }
        let key = self.call.vm.standard_stream(kind);
        self.call.vm.userdata[slot_index(id.0)].file = Some(key);
        Ok(())
    }
    /// Compile a named file, or shared standard input when `path` is None.
    /// Paths use the prefix before NUL. The file is closed on every named-file
    /// outcome; stdin remains open, including its buffered lookahead. A temporary
    /// source-name slot is visible above the caller's values during loading.
    /// Success pushes a function; file/parser/reader failure pushes its error.
    /// File errors have status 6 and take precedence over parser outcomes.
    ///
    /// # Errors
    /// Rejects pending unprotected errors, stack overflow and fatal quotas,
    /// following [`Self::load_reader`].
    pub fn load_file(&mut self, path: Option<&[u8]>) -> Result<NativeLoadStatus, Error> {
        self.check_error()?;
        if !self.check_stack(2) {
            return Err(self.error(b"native stack overflow"));
        }
        let path = path.map(|p| io::c_bytes(p).to_vec());
        let mut name = Vec::new();
        if let Some(path) = &path {
            name.push(b'@');
            name.extend_from_slice(path);
        } else {
            name.extend_from_slice(b"=stdin");
        }
        let index = self.top() + 1;
        // luaL_loadfile's initial string push checks GC outside lua_load's
        // protection, before the source-name slot or file handle exists.
        self.check_gc()?;
        let value = self.intern(&name);
        self.push(value)?;
        let mut file = match self.call.vm.open_file_reader(path.as_deref()) {
            Ok(file) => file,
            Err(code) => return self.native_file_error(index, &name, b"open", code),
        };
        let mut read_error = None;
        let prefix = match self.call.vm.prepare_file_reader(&mut file, &name) {
            Ok(prefix) => prefix,
            Err((code, true)) => {
                self.call.vm.close_file_reader(file);
                return self.native_file_error(index, &name, b"reopen", code);
            }
            Err((code, false)) => {
                read_error = Some(code);
                Vec::new()
            }
        };
        let mut prefix = (!prefix.is_empty()).then_some(prefix);
        // The parser job traces the borrowed stream through nested collections;
        // this native scope retains responsibility for closing the owned file.
        let outcome = self.load_reader_impl(Some(&name), Some(file.borrowed()), |stack| {
            if read_error.is_some() {
                return Ok(None);
            }
            if let Some(prefix) = prefix.take() {
                return Ok(Some(prefix));
            }
            match stack.call.vm.read_file_reader(file) {
                Ok(bytes) => Ok(bytes),
                Err(code) => {
                    read_error = Some(code);
                    Ok(None)
                }
            }
        });
        self.call.vm.close_file_reader(file);
        let status = outcome?;
        if let Some(code) = read_error {
            self.set_top(index)?;
            return self.native_file_error(index, &name, b"read", code);
        }
        self.remove(index as isize)?;
        Ok(status)
    }
    fn native_file_error(
        &mut self,
        index: usize,
        original: &[u8],
        what: &[u8],
        code: i32,
    ) -> Result<NativeLoadStatus, Error> {
        // errfile formats its diagnostic after closing the file, outside the
        // protected parser. A failing collector callback must still escape.
        self.check_gc()?;
        let name = self.to_bytes(index as isize)?.unwrap_or(original).to_vec();
        let value = self.call.vm.file_load_error(&name, what, code);
        self.push(value)?;
        self.remove(index as isize)?;
        Ok(NativeLoadStatus::FileError)
    }
}
