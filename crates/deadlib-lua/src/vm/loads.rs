//! Resumable source/binary validation through protected loading frames.
use super::*;

pub(super) fn compile(source: &[u8], name: &[u8], depth: usize) -> Result<Program, Error> {
    if source.first() == Some(&27) {
        crate::chunk51::compile(source, name, depth)
    } else {
        crate::parser::Parser::with_native_depth(depth)
            .resume(source, true)
            .and_then(|ast| crate::compiler::compile(ast.expect("closed source"), name))
    }
}

pub(super) fn protected(function: FrameKind) -> bool {
    matches!(
        function,
        FrameKind::Native(LOAD_READER | LOAD_FILE | DO_FILE | CONT_FILE)
    )
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FileReader {
    pub key: u64,
    owned: bool,
    phase: u8,
}

#[derive(Debug)]
pub(super) struct Loading {
    pub owner: usize,
    parser: crate::parser::Parser,
    decoder: Option<Box<crate::chunk51::Decoder>>,
    native_depth: usize,
    binary_mode: bool,
    source: Vec<u8>,
    name: Box<[u8]>,
    awaiting: bool,
    closed: bool,
    probed: bool,
    pub file: Option<FileReader>,
}
pub(super) fn storage_bytes(jobs: &Vec<Loading>) -> usize {
    jobs.capacity() * std::mem::size_of::<Loading>()
        + jobs
            .iter()
            .map(|j| {
                j.source.capacity()
                    + j.name.len()
                    + j.parser.heap_bytes()
                    + j.decoder
                        .as_ref()
                        .map_or(0, |d| std::mem::size_of_val(d.as_ref()) + d.heap_bytes())
            })
            .sum::<usize>()
}

pub(super) fn message(name: &[u8], error: &Error, binary: bool) -> Vec<u8> {
    if binary {
        return error.message_bytes().to_vec();
    }
    let mut message = crate::bytecode::chunk_id_limit(name, 80);
    message.extend_from_slice(format!(":{}: ", error.line()).as_bytes());
    message.extend_from_slice(error.message_bytes());
    message
}
impl Loading {
    fn arena_bytes(&self) -> usize {
        self.parser.arena_bytes()
            + self
                .decoder
                .as_ref()
                .map_or(0, |d| std::mem::size_of_val(d.as_ref()) + d.heap_bytes())
    }
    fn resume(&mut self) -> Result<Option<Program>, Error> {
        if self.binary_mode {
            let decoder = self
                .decoder
                .get_or_insert_with(|| Box::new(crate::chunk51::Decoder::new(self.native_depth)));
            return match decoder.resume(&self.source, self.closed) {
                Ok(false) => Ok(None),
                Ok(true) => decoder.program(&self.name).map(Some),
                Err(why) => Err(crate::chunk51::diagnostic(&self.name, why)),
            };
        }
        self.parser
            .resume(&self.source, self.closed)?
            .map(|ast| crate::compiler::compile(ast, &self.name))
            .transpose()
    }
    fn read_file(&mut self, stream: &mut io::Stream) -> Result<(), (i32, bool)> {
        let file = self.file.as_mut().expect("active file reader");
        if file.phase == 0 {
            file.phase = if stream.peek().map_err(|e| (e, false))? == Some(b'#') {
                1
            } else {
                2
            };
        }
        if file.phase == 1 {
            for _ in 0..io::LINE_BUFFER {
                if matches!(stream.get().map_err(|e| (e, false))?, None | Some(b'\n')) {
                    file.phase = 2;
                    self.source.push(b'\n');
                    break;
                }
            }
            return Ok(());
        }
        if file.phase == 2 {
            self.binary_mode = stream.peek().map_err(|e| (e, false))? == Some(27);
            if file.owned && self.binary_mode {
                // Text-mode buffering may already have translated CRLF or ^Z.
                // Reopen from byte zero, then locate the first signature just
                // as luaL_loadfile does after an executable header.
                let _ = stream.close();
                *stream = io::open(&self.name[1..], b"rb").map_err(|e| (e, true))?;
                self.source.clear();
                file.phase = 3;
            } else {
                file.phase = 4;
            }
        }
        if file.phase == 3 {
            for _ in 0..io::LINE_BUFFER {
                match stream.peek().map_err(|e| (e, false))? {
                    Some(27) | None => {
                        file.phase = 4;
                        break;
                    }
                    _ => {
                        stream.get().map_err(|e| (e, false))?;
                    }
                }
            }
            if file.phase == 3 {
                return Ok(());
            }
        }
        let before = self.source.len();
        stream
            .read_chars(&mut self.source, io::LINE_BUFFER)
            .map_err(|e| (e, false))?;
        self.closed = self.source.len() - before < io::LINE_BUFFER;
        Ok(())
    }
}

impl Vm {
    /// Load a file, or stdin when `path` is absent, through the native Lua loader.
    ///
    /// This uses the VM's file streams, header handling and source/binary verifier.
    /// It does not consult the mutable global `loadfile` function. The returned
    /// function is retained in the result registers until the next VM mutation;
    /// pin it if retaining it across executions or collections.
    ///
    /// # Errors
    /// Returns the loader's error object for file, syntax or bytecode errors.
    pub fn load_file(&mut self, path: Option<&[u8]>) -> Result<FunctionId, Error> {
        let argument = path.map(|p| self.intern(p));
        // Host loading is a parser operation, not an observable Lua builtin
        // invocation. The ordinary global loadfile still delivers its hooks.
        let hooks = std::mem::take(&mut self.hooks);
        let result = self
            .call_unprotected(Value::Native(LOAD_FILE), argument.as_slice())
            .map(|_| ());
        self.hooks = hooks;
        result?;
        match self.registers[0] {
            Value::Function(function) => Ok(function),
            Value::Nil => {
                let value = self.registers[1];
                let Value::String(id) = value else {
                    unreachable!("file loader returns a string diagnostic")
                };
                Err(Error::from_bytes(
                    ErrorKind::Runtime,
                    0,
                    self.string_bytes(id).unwrap().to_vec(),
                )
                .with_value(value))
            }
            _ => unreachable!("file loader returns a function or nil"),
        }
    }

    fn file_load_raise(&self, value: Value) -> Error {
        let message = if let Value::String(id) = value {
            String::from_utf8_lossy(&self.strings.bytes[slot_index(id.0)]).into_owned()
        } else {
            "Lua error".into()
        };
        Error::raised(value, message)
    }
    fn file_load_result(
        &mut self,
        id: u64,
        frame: Frame,
        result: Result<FunctionId, Value>,
    ) -> Result<Option<usize>, Error> {
        let base = frame.base + frame.argc();
        match result {
            Ok(function) => {
                self.resize_registers(self.registers.len().max(base + 1));
                self.registers[base] = Value::Function(function);
                if id == LOAD_FILE {
                    return Ok(self.return_frame(frame, base, 1));
                }
                self.frames.last_mut().expect("active dofile").pc = 4;
                if let Some(actual) = self.start_call(base, 0, OPEN, false)? {
                    self.finish_call(base, actual, OPEN);
                }
                Ok(None)
            }
            Err(value) if id == DO_FILE => {
                self.frames.last_mut().expect("active dofile").pc = 4;
                Err(self.file_load_raise(value))
            }
            Err(value) => {
                self.resize_registers(self.registers.len().max(base + 2));
                self.registers[base] = Value::Nil;
                self.registers[base + 1] = value;
                Ok(self.return_frame(frame, base, 2))
            }
        }
    }
    fn file_load_error(&mut self, name: &[u8], what: &[u8], code: i32) -> Value {
        let mut bytes = self.take_buffer(name.len() + 64);
        bytes.extend_from_slice(b"cannot ");
        bytes.extend_from_slice(what);
        bytes.push(b' ');
        bytes.extend_from_slice(&name[1..]);
        bytes.extend_from_slice(b": ");
        bytes.extend_from_slice(&io::message(code));
        self.finish_buffer(bytes)
    }
    fn finish_file_load(&mut self) {
        let job = self.loads.pop().expect("completed file reader");
        if let Some(file) = job.file
            && file.owned
        {
            self.io.files.remove(file.key);
        }
        self.buffers.push(job.source);
    }
    pub(super) fn file_load_step(
        &mut self,
        id: u64,
        mut frame: Frame,
    ) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        if frame.pc == 2 {
            if id == DO_FILE {
                return Err(self.file_load_raise(self.registers[frame.base + 1]));
            }
            return Ok(self.return_frame(frame, frame.base, 2));
        }
        if frame.pc == 4 {
            let base = frame.base + frame.argc();
            return Ok(self.return_frame(frame, base, self.top - base));
        }
        if frame.pc == 0 {
            let args = frame.vararg_start - 1;
            let (name, opened) = if self.argument(args, frame.argc(), 0) == Value::Nil {
                (
                    b"=stdin".to_vec(),
                    Ok(FileReader {
                        key: self.io.stdin.expect("standard input initialized").get(),
                        owned: false,
                        phase: 0,
                    }),
                )
            } else {
                let path = self.string_argument(args, frame.argc(), 0)?;
                self.registers[frame.vararg_start] = Value::String(path);
                let path = io::c_bytes(&self.strings.bytes[slot_index(path.0)]);
                let mut name = Vec::with_capacity(path.len() + 1);
                name.push(b'@');
                name.extend_from_slice(path);
                let opened = io::open(path, b"r").map(|stream| FileReader {
                    key: self.io.files.insert(stream),
                    owned: true,
                    phase: 0,
                });
                (name, opened)
            };
            // Preserve all original arguments while the loaded chunk executes.
            self.registers.copy_within(
                frame.vararg_start..frame.vararg_start + frame.argc(),
                frame.base,
            );
            frame.vararg_start = frame.base;
            frame.pc = 1;
            self.frames[owner] = frame;
            let file = match opened {
                Ok(file) => file,
                Err(code) => {
                    let value = self.file_load_error(&name, b"open", code);
                    return self.file_load_result(id, frame, Err(value));
                }
            };
            let source = self.take_buffer(0);
            let before = self.loads.capacity();
            self.loads.push(Loading {
                owner,
                parser: crate::parser::Parser::with_native_depth(self.native_depth(owner + 1)),
                decoder: None,
                native_depth: self.native_depth(owner + 1),
                binary_mode: false,
                source,
                name: name.into_boxed_slice(),
                awaiting: false,
                closed: false,
                probed: false,
                file: Some(file),
            });
            self.gc_note_control((self.loads.capacity() - before) * std::mem::size_of::<Loading>());
        }
        let index = self.loads.len() - 1;
        debug_assert_eq!(self.loads[index].owner, owner);
        let file = self.loads[index].file.expect("active file reader");
        let stream = &mut self.io.files[slot_index(file.key)];
        let before = stream.heap_bytes();
        let source_before = self.loads[index].source.capacity();
        let read = self.loads[index].read_file(stream);
        let growth = stream.heap_bytes().saturating_sub(before);
        self.io.files.note_alloc(growth);
        self.gc_note_control(self.loads[index].source.capacity() - source_before);
        if let Err((code, reopen)) = read {
            let name = self.loads[index].name.clone();
            let value = self.file_load_error(&name, if reopen { b"reopen" } else { b"read" }, code);
            self.finish_file_load();
            return self.file_load_result(id, frame, Err(value));
        }
        if matches!(self.loads[index].file.expect("file reader").phase, 1 | 3) {
            return Ok(None);
        }
        let job = &mut self.loads[index];
        let before = job.arena_bytes();
        let parsed = job.resume();
        let growth = job.arena_bytes().saturating_sub(before);
        self.gc_note_control(growth);
        let compiled = match parsed {
            Ok(Some(program)) => Some(Ok(program)),
            Err(error) => Some(Err(error)),
            Ok(None) => None,
        };
        if let Some(compiled) = compiled {
            let result = match compiled {
                Ok(program) => Ok(self.load(&program)),
                Err(error) => {
                    let message = message(
                        &self.loads[index].name,
                        &error,
                        self.loads[index].binary_mode,
                    );
                    Err(self.intern(&message))
                }
            };
            self.finish_file_load();
            return self.file_load_result(id, frame, result);
        }
        Ok(None)
    }
    pub(super) fn trim_loads(&mut self) {
        let files = &mut self.io.files;
        self.loads.retain(|j| {
            let keep = self
                .frames
                .get(j.owner)
                .is_some_and(|f| protected(f.function) && matches!(f.pc, 1 | 3));
            if !keep
                && let Some(file) = j.file
                && file.owned
            {
                files.remove(file.key);
            }
            keep
        });
    }
    pub(super) fn parser_nesting(&self, end: usize) -> usize {
        self.loads
            .iter()
            .filter(|j| j.owner < end)
            .map(|j| {
                j.decoder
                    .as_ref()
                    .map_or_else(|| j.parser.nesting(), |d| d.nesting())
            })
            .sum()
    }
    pub(super) fn load_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        let base = frame.base;
        if frame.pc == 2 {
            return Ok(self.return_frame(frame, base, 2));
        }
        if frame.pc == 0 {
            let args = frame.vararg_start - 1;
            let name = match self.argument(args, frame.argc(), 1) {
                Value::Nil => b"=(load)".to_vec(),
                _ => {
                    let id = self.string_argument(args, frame.argc(), 1)?;
                    self.strings.bytes[slot_index(id.0)].to_vec()
                }
            };
            let reader = self.argument(args, frame.argc(), 0);
            if !matches!(reader, Value::Function(_) | Value::Native(_)) {
                return Err(self.argument_type_error(args, frame.argc(), 0, "function"));
            }
            let supplied_name = self.argument(args, frame.argc(), 1);
            let supplied_name = match supplied_name {
                Value::Number(n) => self.intern(format_number(n).as_slice()),
                v => v,
            };
            // lua_settop(3) retains an already supplied third argument until
            // the generic reader replaces its reserved result slot.
            let reserved = self.argument(args, frame.argc(), 2);
            self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].fill(Value::Nil);
            self.registers[base] = reader;
            self.registers[base + 1] = supplied_name;
            self.registers[base + 2] = reserved;
            self.frames[owner].vararg_count = 0;
            self.frames[owner].pc = 1;
            let source = self.take_buffer(0);
            let before = self.loads.capacity();
            self.loads.push(Loading {
                owner,
                parser: crate::parser::Parser::with_native_depth(self.native_depth(owner + 1)),
                decoder: None,
                native_depth: self.native_depth(owner + 1),
                binary_mode: false,
                source,
                name: name.into_boxed_slice(),
                awaiting: false,
                closed: false,
                probed: false,
                file: None,
            });
            self.gc_note_control((self.loads.capacity() - before) * std::mem::size_of::<Loading>());
        }
        let index = self.loads.len() - 1;
        debug_assert_eq!(self.loads[index].owner, owner);
        if self.loads[index].awaiting {
            self.loads[index].awaiting = false;
            // f_parser probes with luaZ_lookahead before luaY_parser fetches
            // its first character. An empty first probe is not sticky EOF.
            let first = !self.loads[index].probed;
            self.loads[index].probed = true;
            let result = self.registers[base + 3];
            let result = if let Value::Number(n) = result {
                self.intern(format_number(n).as_slice())
            } else {
                result
            };
            match result {
                Value::Nil => self.loads[index].closed = !first,
                Value::String(id) => {
                    self.registers[base + 2] = result;
                    let bytes = &self.strings.bytes[slot_index(id.0)];
                    let job = &mut self.loads[index];
                    if first {
                        job.binary_mode = bytes.first() == Some(&27);
                    }
                    let capacity = job.source.capacity();
                    job.source.extend_from_slice(bytes);
                    if bytes.is_empty() && !first {
                        job.closed = true;
                    }
                    let growth = job.source.capacity() - capacity;
                    self.gc_note_control(growth);
                }
                _ => {
                    return Err(Error::new(
                        ErrorKind::Runtime,
                        0,
                        "reader function must return a string",
                    ));
                }
            }
        }
        let job = &mut self.loads[index];
        let before = job.arena_bytes();
        let parsed = job.resume();
        let growth = job.arena_bytes().saturating_sub(before);
        self.gc_note_control(growth);
        let compiled = match parsed {
            Ok(Some(program)) => Some(Ok(program)),
            Err(error) => Some(Err(error)),
            Ok(None) => None,
        };
        if let Some(compiled) = compiled {
            match compiled {
                Ok(program) => {
                    let function = self.load(&program);
                    let job = self.loads.pop().expect("completed reader job");
                    self.buffers.push(job.source);
                    self.registers[base] = Value::Function(function);
                    return Ok(self.return_frame(frame, base, 1));
                }
                Err(error) => {
                    let message = message(
                        &self.loads[index].name,
                        &error,
                        self.loads[index].binary_mode,
                    );
                    let value = self.intern(&message);
                    self.protected_failure(owner, value);
                    return Ok(None);
                }
            }
        }
        if self.native_depth(self.frames.len()) >= 199 {
            return Err(Error::new(ErrorKind::Runtime, 0, "C stack overflow"));
        }
        self.loads[index].awaiting = true;
        self.registers[base + 3] = self.registers[base];
        if let Some(actual) = self.start_call(base + 3, 0, 1, false)? {
            self.finish_call(base + 3, actual, 1);
        }
        Ok(None)
    }
}
