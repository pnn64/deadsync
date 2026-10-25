//! Resumable traceback formatting, including Lua 5.1 concatenation callbacks.
use super::*;
use debug::StackEntry;

const FIRST_LEVELS: i32 = 12;
const LAST_LEVELS: i32 = 10;
const ROW_PIECES: usize = 4;
// Header, optional ellipsis and one row fit below the hidden target root.
const SCRATCH_PIECES: usize = 2 + 1 + ROW_PIECES;

#[derive(Debug)]
pub(super) struct Traceback {
    pub owner: usize,
    pub arg: usize,
    count: usize,
    pub top: usize,
    thread: ThreadId,
    level: i32,
    first: bool,
    reduce: bool,
    pending: bool,
    done: bool,
}

impl Vm {
    pub(super) fn trim_tracebacks(&mut self) {
        self.tracebacks.retain(|j| {
            self.frames
                .get(j.owner)
                .is_some_and(|f| matches!(f.function, FrameKind::Native(debug::TRACEBACK)))
        });
    }
    fn trace_row(&mut self, thread: ThreadId, entry: StackEntry) -> ([Value; ROW_PIECES], usize) {
        let mut pieces = [Value::Nil; ROW_PIECES];
        pieces[0] = self.intern(b"\n\t");
        if let StackEntry::Frame(index) = entry {
            let frame = self.debug_frames(thread)[index];
            let (source, defined, line) = if let FrameKind::Lua(id) = frame.function {
                let closure = &self.functions[slot_index(id.0)];
                let program = &self.programs[closure.program].data;
                let proto = &program.prototypes[closure.prototype];
                let pc = frame.pc().saturating_sub(1).min(proto.lines.len() - 1);
                (
                    crate::bytecode::chunk_id(&program.name),
                    Some(proto.debug.defined[0]),
                    Some(proto.lines[pc]),
                )
            } else {
                (b"[C]".to_vec(), None, None)
            };
            let mut prefix = self.take_buffer(64);
            prefix.extend_from_slice(&source);
            prefix.push(b':');
            pieces[1] = self.finish_buffer(prefix);
            let mut count = 2;
            if let Some(line) = line
                && line > 0
            {
                pieces[count] = self.intern(format!("{line}:").as_bytes());
                count += 1;
            }
            let mut row = self.take_buffer(128);
            if let Some((name, _)) = self.debug_name(thread, index) {
                row.extend_from_slice(b" in function '");
                row.extend_from_slice(&name);
                row.push(b'\'');
            } else {
                match defined {
                    Some(0) => row.extend_from_slice(b" in main chunk"),
                    Some(line) => {
                        row.extend_from_slice(b" in function <");
                        row.extend_from_slice(&source);
                        row.extend_from_slice(format!(":{line}>").as_bytes());
                    }
                    None => row.extend_from_slice(b" ?"),
                }
            }
            pieces[count] = self.finish_buffer(row);
            (pieces, count + 1)
        } else {
            pieces[1] = self.intern(b"(tail call):");
            pieces[2] = self.intern(b" ?");
            (pieces, 3)
        }
    }
    pub(super) fn traceback_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        let base = frame.base;
        if frame.pc == 0 {
            let args = frame.vararg_start - 1;
            let (thread, arg) = match self.argument(args, frame.argc(), 0) {
                Value::Thread(id) => (id, 1),
                _ => (self.thread, 0),
            };
            let requested = self.number(self.argument(args, frame.argc(), arg + 1));
            let level = requested.map_or(i32::from(thread == self.thread), debug::integer);
            // 5.1 pops the topmost argument when a numeric level was supplied,
            // even when additional arguments follow that level.
            let count = frame.argc() - usize::from(requested.is_some());
            if count > arg
                && !matches!(
                    self.argument(args, count, arg),
                    Value::String(_) | Value::Number(_)
                )
            {
                return Ok(self.return_frame(frame, frame.vararg_start + count - 1, 1));
            }
            // All retained Lua values stay in the native register window. A
            // hidden target root survives argument changes during callbacks.
            let root = base + frame.argc().max(2) + SCRATCH_PIECES;
            self.resize_registers(self.registers.len().max(root + 1));
            self.registers
                .copy_within(frame.vararg_start..frame.vararg_start + count, base);
            self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].fill(Value::Nil);
            self.registers[root] = Value::Thread(thread);
            let message = count > arg;
            self.registers[base + count] = self.intern(if message { b"\n" } else { b"" });
            self.registers[base + count + 1] = self.intern(b"stack traceback:");
            let capacity = self.tracebacks.capacity();
            self.tracebacks.push(Traceback {
                owner,
                arg,
                count: count - arg + 2,
                top: count - arg + 2,
                thread,
                level,
                first: true,
                reduce: false,
                pending: false,
                done: false,
            });
            self.gc_note_control(
                (self.tracebacks.capacity() - capacity) * std::mem::size_of::<Traceback>(),
            );
            self.frames[owner].vararg_count = 0;
            self.frames[owner].pc = 1;
        }
        let index = self.tracebacks.len() - 1;
        debug_assert_eq!(self.tracebacks[index].owner, owner);
        let start = base + self.tracebacks[index].arg;
        if self.tracebacks[index].pending {
            let job = &mut self.tracebacks[index];
            job.count -= 1;
            job.pending = false;
        }
        if self.tracebacks[index].reduce {
            let count = self.tracebacks[index].count;
            if count > 1 {
                let last = start + count - 1;
                let scalar = |v: Value| matches!(v, Value::String(_) | Value::Number(_));
                if scalar(self.registers[last - 1]) && scalar(self.registers[last]) {
                    let mut first = last - 1;
                    while first > start && scalar(self.registers[first - 1]) {
                        first -= 1;
                    }
                    let mut bytes = self.take_buffer(0);
                    for pos in first..=last {
                        let value = self.registers[pos];
                        let value = if let Value::Number(n) = value {
                            self.intern(format_number(n).as_bytes())
                        } else {
                            value
                        };
                        self.registers[pos] = value;
                        let Value::String(id) = value else {
                            unreachable!("traceback scalar was converted");
                        };
                        bytes.extend_from_slice(&self.strings.bytes[slot_index(id.0)]);
                    }
                    self.registers[first] = self.finish_buffer(bytes);
                    self.tracebacks[index].count = first - start + 1;
                } else {
                    // The concatenation continuation accounts for the C call;
                    // the surrounding traceback formatter adds no second one.
                    if self.native_depth(self.frames.len()) >= 198 {
                        return Err(Error::new(ErrorKind::Runtime, 0, "C stack overflow"));
                    }
                    self.tracebacks[index].pending = true;
                    self.concatenate(self.registers[last - 1], self.registers[last], last - 1)?;
                }
                return Ok(None);
            }
            let top = self.tracebacks[index].top;
            self.registers[start + 1..start + top].fill(Value::Nil);
            self.tracebacks[index].top = 1;
            self.tracebacks[index].reduce = false;
            if self.tracebacks[index].done {
                self.tracebacks.pop();
                return Ok(self.return_frame(frame, start, 1));
            }
        }
        let (thread, level) = (self.tracebacks[index].thread, self.tracebacks[index].level);
        let Some(entry) = self.debug_stack(thread, level) else {
            self.tracebacks[index].done = true;
            self.tracebacks[index].reduce = true;
            return Ok(None);
        };
        let next = level.wrapping_add(1);
        self.tracebacks[index].level = next;
        if next > FIRST_LEVELS && self.tracebacks[index].first {
            if self
                .debug_stack(thread, next.wrapping_add(LAST_LEVELS))
                .is_some()
            {
                let height: u64 = self.debug_frames(thread).iter().filter(|f| !matches!(f.function, FrameKind::Native(id) if (CONT_RAW..strings::ITERATOR_FIRST).contains(&id))).map(|f| 1 + u64::from(f.tail_calls)).sum();
                self.tracebacks[index].level = height
                    .saturating_sub(LAST_LEVELS as u64)
                    .min(i32::MAX as u64) as i32;
                let ellipsis = self.intern(b"\n\t...");
                self.registers[start + self.tracebacks[index].count] = ellipsis;
                self.tracebacks[index].count += 1;
                self.tracebacks[index].top = self.tracebacks[index].count;
            } else {
                self.tracebacks[index].level = level;
            }
            self.tracebacks[index].first = false;
            return Ok(None);
        }
        let (pieces, count) = self.trace_row(thread, entry);
        let position = start + self.tracebacks[index].count;
        self.registers[position..position + count].copy_from_slice(&pieces[..count]);
        self.tracebacks[index].count += count;
        self.tracebacks[index].top = self.tracebacks[index].count;
        self.tracebacks[index].reduce = true;
        Ok(None)
    }
}
