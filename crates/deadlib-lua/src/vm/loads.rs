//! Reader-backed text loading through resumable parsing and protected frames.
use super::*;

#[derive(Debug)]
pub(super) struct Loading {
    pub owner: usize,
    parser: crate::parser::Parser,
    source: Vec<u8>,
    name: Box<[u8]>,
    awaiting: bool,
    closed: bool,
    probed: bool,
}
pub(super) fn storage_bytes(jobs: &Vec<Loading>) -> usize {
    jobs.capacity() * std::mem::size_of::<Loading>()
        + jobs
            .iter()
            .map(|j| j.source.capacity() + j.name.len() + j.parser.heap_bytes())
            .sum::<usize>()
}

impl Vm {
    pub(super) fn trim_loads(&mut self) {
        self.loads.retain(|j| {
            self.frames.get(j.owner).is_some_and(|f| {
                matches!(f.function, FrameKind::Native(LOAD_READER)) && matches!(f.pc, 1 | 3)
            })
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
                parser: crate::parser::Parser::default(),
                source,
                name: name.into_boxed_slice(),
                awaiting: false,
                closed: false,
                probed: false,
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
                    let mut message = crate::bytecode::chunk_id(&self.loads[index].name);
                    message.extend_from_slice(
                        format!(":{}: {}", error.line(), error.message()).as_bytes(),
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
