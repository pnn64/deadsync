//! Reader-backed text loading through resumable parsing and protected frames.
use super::*;

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
            .map(|j| j.source.capacity() + j.name.len() + j.parser.heap_bytes())
            .sum::<usize>()
}

impl Vm {
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
                if self.native_depth(self.frames.len()) >= 200 {
                    return Err(Error::new(ErrorKind::Runtime, 0, "C stack overflow"));
                }
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
        bytes.extend_from_slice(io::message(code));
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
        let read = (|| {
            let job = &mut self.loads[index];
            let file = job.file.as_mut().expect("file reader");
            if file.phase == 0 {
                file.phase = if stream.peek()? == Some(b'#') { 1 } else { 2 };
            }
            if file.phase == 1 {
                // Bound work even for an arbitrarily long executable header.
                for _ in 0..io::LINE_BUFFER {
                    if matches!(stream.get()?, None | Some(b'\n')) {
                        file.phase = 2;
                        job.source.push(b'\n');
                        break;
                    }
                }
            } else {
                let start = job.source.len();
                stream.read_chars(&mut job.source, io::LINE_BUFFER)?;
                job.closed = job.source.len() - start < io::LINE_BUFFER;
            }
            Ok::<_, i32>(())
        })();
        let growth = stream.heap_bytes().saturating_sub(before);
        self.io.files.note_alloc(growth);
        self.gc_note_control(self.loads[index].source.capacity() - source_before);
        if let Err(code) = read {
            let name = self.loads[index].name.clone();
            let value = self.file_load_error(&name, b"read", code);
            self.finish_file_load();
            return self.file_load_result(id, frame, Err(value));
        }
        if self.loads[index].file.expect("file reader").phase == 1 {
            return Ok(None);
        }
        let job = &mut self.loads[index];
        let before = job.parser.arena_bytes();
        let parsed = job.parser.resume(&job.source, job.closed);
        let growth = job.parser.arena_bytes().saturating_sub(before);
        self.gc_note_control(growth);
        let compiled = match parsed {
            Ok(Some(ast)) => Some(crate::compiler::compile(ast, &self.loads[index].name)),
            Err(error) => Some(Err(error)),
            Ok(None) => None,
        };
        if let Some(compiled) = compiled {
            let result = match compiled {
                Ok(program) => Ok(self.load(&program)),
                Err(error) => {
                    let mut message = crate::bytecode::chunk_id_limit(&self.loads[index].name, 80);
                    message.extend_from_slice(format!(":{}: ", error.line()).as_bytes());
                    message.extend_from_slice(error.message_bytes());
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
            .map(|j| j.parser.nesting())
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
                return Err(Error::new(
                    ErrorKind::Runtime,
                    0,
                    "bad argument #1 to 'load' (function expected)",
                ));
            }
            let supplied_name = self.argument(args, frame.argc(), 1);
            let supplied_name = match supplied_name {
                Value::Number(n) => self.intern(format_number(n).as_bytes()),
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
                self.intern(format_number(n).as_bytes())
            } else {
                result
            };
            match result {
                Value::Nil => self.loads[index].closed = !first,
                Value::String(id) => {
                    self.registers[base + 2] = result;
                    let bytes = &self.strings.bytes[slot_index(id.0)];
                    let job = &mut self.loads[index];
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
        let before = job.parser.arena_bytes();
        let parsed = job.parser.resume(&job.source, job.closed);
        let growth = job.parser.arena_bytes().saturating_sub(before);
        self.gc_note_control(growth);
        let compiled = match parsed {
            Ok(Some(ast)) => Some(crate::compiler::compile(ast, &self.loads[index].name)),
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
                    let mut message = crate::bytecode::chunk_id_limit(&self.loads[index].name, 80);
                    message.extend_from_slice(format!(":{}: ", error.line()).as_bytes());
                    message.extend_from_slice(error.message_bytes());
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
