//! Standalone C main-frame context and protected message-handler entry.
use super::*;
use threads::NativeKind;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum EntryMode {
    Message,
    Protected,
    Unprotected,
}

impl Vm {
    /// Whether a standalone main context remains active.
    ///
    /// Errors outside protected entry abort the context. Protected chunk errors
    /// leave it available for subsequent calls or [`Self::end_standalone`].
    pub fn standalone_active(&self) -> bool {
        self.standalone != 0
    }

    /// Begin a Lua 5.1 standalone main context.
    ///
    /// Subsequent calls retain an observable outer C frame and run the current
    /// `debug.traceback` message handler before unwinding uncaught errors. Finish
    /// the context with [`Self::end_standalone`] to deliver its return hook.
    /// Ordinary embedded execution is unchanged when no context is active.
    ///
    /// # Errors
    /// Returns an error if the VM is executing or a context is already active.
    pub fn begin_standalone(&mut self) -> Result<(), Error> {
        if !self.frames.is_empty() || self.thread != self.main || self.standalone != 0 {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "standalone context requires an idle main thread",
            ));
        }
        let Value::Native(id) = self.new_native(NativeKind::StandaloneMain, &[])? else {
            unreachable!()
        };
        self.standalone = id;
        Ok(())
    }

    /// Finish a standalone context and deliver the outer C main return hook.
    ///
    /// # Errors
    /// Returns hook errors or instruction-limit errors. A context that aborted
    /// outside protected entry has already unwound and delivers no return hook.
    pub fn end_standalone(&mut self) -> Result<(), Error> {
        self.reset_budget();
        let id = std::mem::take(&mut self.standalone);
        if id == 0 {
            return Ok(());
        }
        self.registers.clear();
        self.frames.clear();
        self.standalone_main(id, 1);
        let result = self.drive();
        self.finish_execution(result).map(|_| ())
    }

    pub(super) fn standalone_main(&mut self, id: u64, pc: u32) {
        // Smain's address is opaque in the reference; use the context's stable
        // identity for the C frame's light-userdata argument.
        let base = self.registers.len();
        self.registers.push(Value::LightUserdata(id));
        // The call began before initialization installed any hook. Recreating
        // its parked frame between host operations must not emit another call.
        self.frames.push(Frame {
            function: FrameKind::Native(id),
            pc,
            // Saved host results precede the opaque argument while rebuilding
            // this frame. They remain live C-main scratch slots during callbacks.
            base: 0,
            stack_start: 0,
            result: 0,
            wanted: FRAME_OPEN,
            barriers: 0,
            vararg_start: base,
            vararg_count: 1,
            tail_calls: 0,
            c_level_bias: self.threads[slot_index(self.thread.0)].native_bias,
        });
        self.top = self.registers.len();
    }

    pub(super) fn standalone_setter(&mut self) {
        self.registers.clear();
        self.frames.clear();
        self.standalone_main(self.standalone, 0);
    }

    pub(super) fn standalone_call(
        &mut self,
        function: Value,
        arguments: &[Value],
        mode: EntryMode,
    ) -> Result<&[Value], Error> {
        self.reset_budget();
        if arguments.len() > u32::MAX as usize {
            return Err(Error::new(ErrorKind::Limit, 0, "too many arguments"));
        }
        self.registers.clear();
        self.frames.clear();
        self.standalone_main(self.standalone, 0);
        let handler = if mode == EntryMode::Message {
            self.new_native(NativeKind::StandaloneTraceback, &[])?
        } else {
            Value::Nil
        };
        self.registers.push(handler);
        self.registers.push(function);
        self.registers.extend_from_slice(arguments);
        self.push_record(Frame {
            function: FrameKind::Native(CONT_STANDALONE),
            pc: match mode {
                EntryMode::Message => 0,
                EntryMode::Unprotected => 4,
                EntryMode::Protected => 6,
            },
            base: 1,
            stack_start: 1,
            result: 0,
            wanted: FRAME_OPEN,
            barriers: 1,
            vararg_start: 2,
            vararg_count: arguments.len() as u32,
            tail_calls: 0,
            c_level_bias: 0,
        });
        // A fresh C message-handler closure is an allocation safe point before
        // lua_pcall installs that handler. Keep its failure outside protection.
        if mode == EntryMode::Message {
            self.maybe_gc();
        }
        let result = self.drive();
        let handled = self
            .frames
            .get(1)
            .is_some_and(|f| matches!(f.function, FrameKind::Native(CONT_STANDALONE)) && f.pc == 2);
        if result
            .as_ref()
            .is_err_and(|e| e.kind() == ErrorKind::Runtime)
            && !handled
        {
            self.standalone = 0;
        }
        if handled
            && result
                .as_ref()
                .is_err_and(|e| e.kind() == ErrorKind::Runtime)
        {
            let error = self.finish_execution(result).unwrap_err();
            // docall removes its message handler before forcing collection.
            // Finalizer failures therefore escape the outer C main context.
            if mode == EntryMode::Message
                && let Err(error) = self.collect_garbage()
            {
                self.standalone = 0;
                return Err(error);
            }
            return Err(error);
        }
        self.finish_execution(result)
    }

    pub(super) fn standalone_driver(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        let index = self.frames.len() - 1;
        match frame.pc {
            0 | 4 | 6 => {
                self.frames[index].pc += 1;
                if let Some(actual) =
                    self.start_call(frame.vararg_start, frame.argc(), OPEN, false)?
                {
                    self.finish_call(frame.vararg_start, actual, OPEN);
                }
                Ok(None)
            }
            1 | 5 | 7 => {
                let count = self.top - frame.vararg_start;
                self.registers.copy_within(frame.vararg_start..self.top, 0);
                self.truncate_registers(count);
                self.top = count;
                Ok(Some(count))
            }
            2 => {
                let value = self.registers[frame.base + 1];
                let message = match value {
                    Value::String(id) => self.string_bytes(id).unwrap().to_vec(),
                    Value::Number(number) => format_number(number),
                    _ => b"Lua error".to_vec(),
                };
                Err(Error::from_bytes(ErrorKind::Runtime, 0, message).with_value(value))
            }
            _ => unreachable!("standalone protected entry phase"),
        }
    }

    pub(super) fn standalone_native(
        &mut self,
        id: u64,
        frame: Frame,
        kind: u8,
    ) -> Result<Option<usize>, Error> {
        if kind == 2 {
            if self.frames.len() != 1 || (frame.pc == 0 && self.standalone != id) {
                return Err(Error::new(
                    ErrorKind::Runtime,
                    0,
                    "standalone main is not a callable entry",
                ));
            }
            return if frame.pc == 1 {
                Ok(self.return_frame(frame, 0, 0))
            } else {
                Ok(Some(0))
            };
        }
        let base = frame.base;
        let owner = self.frames.len() - 1;
        match frame.pc {
            0 => {
                let value = self.registers[frame.vararg_start];
                self.registers[base] = value;
                if !matches!(value, Value::String(_) | Value::Number(_)) {
                    return Ok(self.return_frame(frame, base, 1));
                }
                self.frames[owner].pc = 1;
                let key = self.intern(b"debug");
                self.get_table(Value::Table(self.globals), key, base + 1)?;
            }
            1 => {
                if !matches!(self.registers[base + 1], Value::Table(_)) {
                    return Ok(self.return_frame(frame, base, 1));
                }
                self.frames[owner].pc = 2;
                let key = self.intern(b"traceback");
                self.get_table(self.registers[base + 1], key, base + 2)?;
            }
            2 => {
                if !matches!(
                    self.registers[base + 2],
                    Value::Function(_) | Value::Native(_)
                ) {
                    return Ok(self.return_frame(frame, base, 1));
                }
                self.frames[owner].pc = 3;
                self.registers[base + 3] = self.registers[base];
                self.registers[base + 4] = Value::Number(2.);
                if let Some(actual) = self.start_call(base + 2, 2, 1, false)? {
                    self.finish_call(base + 2, actual, 1);
                }
            }
            3 => return Ok(self.return_frame(frame, base + 2, 1)),
            _ => unreachable!("standalone traceback phase"),
        }
        Ok(None)
    }
}
