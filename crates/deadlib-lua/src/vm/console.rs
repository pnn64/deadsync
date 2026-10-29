//! Lua 5.1 console input through the VM's shared standard streams.
use super::*;
use standalone::EntryMode;

fn console_write(io: &mut io::Io, output: &mut Option<Vec<u8>>, bytes: &[u8]) {
    if let Some(output) = output {
        output.extend_from_slice(bytes);
    } else {
        let _ = io.write_standard(1, bytes);
        let _ = io.flush_stdout();
    }
}

impl Vm {
    pub(super) fn debug_console_step(&mut self, frame: Frame) -> Result<Option<usize>, Error> {
        self.ensure_file_backend()?;
        let base = frame.base;
        if frame.pc() == 2 {
            match self.registers[base + 1] {
                Value::String(id) => {
                    let _ = self
                        .io
                        .write_standard(2, io::c_bytes(&self.strings.bytes[slot_index(id.0)]));
                }
                Value::Number(n) => {
                    let _ = self.io.write_standard(2, &format_number(n));
                }
                // The C reference passes NULL to fputs for other error values.
                _ => {
                    let _ = self.io.write_standard(2, b"(null)");
                }
            }
            let _ = self.io.write_standard(2, b"\n");
        }
        if frame.pc() != 0 {
            self.debug_console_clear(frame);
        }
        let _ = self.io.write_standard(2, b"lua_debug> ");
        let mut buffer = [0; 249];
        let len = self.console_read(&mut buffer);
        if len == 0 || io::c_bytes(&buffer[..len]) == b"cont\n" {
            return Ok(self.return_frame(frame, base, 0));
        }
        let source = io::c_bytes(&buffer[..len]);
        let name = b"=(debug command)";
        match loads::compile(
            source,
            name,
            usize::from(self.native_calls_for(self.thread, self.frames.len())),
        ) {
            Ok(program) => {
                let function = self.load(&program);
                self.resize_registers(self.registers.len().max(base + 1));
                self.registers[base] = Value::Function(function);
                self.frames.last_mut().unwrap().pc = 1;
                if let Some(actual) = self.start_call(base, 0, 0, false)? {
                    self.finish_call(base, actual, 0);
                }
            }
            Err(error) => {
                let _ = self.io.write_standard(
                    2,
                    io::c_bytes(&loads::message(name, &error, source.first() == Some(&27))),
                );
                let _ = self.io.write_standard(2, b"\n");
                self.debug_console_clear(frame);
            }
        }
        Ok(None)
    }

    fn debug_console_clear(&mut self, frame: Frame) {
        self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].fill(Value::Nil);
        self.truncate_registers(frame.base);
        let current = self.frames.last_mut().unwrap();
        current.vararg_count = 0;
        current.pc = 0;
        self.top = frame.base;
    }

    fn console_read(&mut self, buffer: &mut [u8]) -> usize {
        let key = self.standard_stream(0);
        let stream = &mut self.io.files[slot_index(key.get())];
        let mut len = 0;
        while len < buffer.len() {
            let Some(byte) = stream.get().ok().flatten() else {
                break;
            };
            buffer[len] = byte;
            len += 1;
            if byte == b'\n' {
                break;
            }
        }
        len
    }

    /// Run the Lua 5.1 interactive console using stdin and stdout.
    ///
    /// Requires an active [`Self::begin_standalone`] context. Completed chunks
    /// share globals, `=expression` prints results, and incomplete chunks read
    /// continuation lines. Prompt lookups honor global metatables. Input shares
    /// buffering with Lua's original stdin, independently of `io.input`.
    /// Recoverable chunk/print diagnostics go to `report` without a program-name
    /// prefix; stdout uses the normal output capture when enabled.
    ///
    /// # Errors
    /// Returns errors escaping the outer main context, instruction-limit errors,
    /// or an error if no standalone context is active.
    pub fn run_interactive(&mut self, mut report: impl FnMut(&[u8])) -> Result<(), Error> {
        if self.standalone == 0 {
            return Err(Error::new(
                ErrorKind::Runtime,
                0,
                "interactive console requires a standalone context",
            ));
        }
        let mut source = Vec::new();
        let mut line = Vec::new();
        let mut values = Vec::new();
        'input: loop {
            self.registers.clear();
            self.top = 0;
            source.clear();
            values.clear();
            if !self.console_line(true, &mut source)? {
                break;
            }
            let program = loop {
                match crate::compile_named(&source, b"=stdin") {
                    Ok(program) => break program,
                    Err(error) => {
                        let message = error.message_bytes();
                        // The reference tests the first occurrence, not just
                        // the suffix, of its quoted EOF token.
                        let eof = b"'<eof>'";
                        if error.kind() == ErrorKind::Syntax
                            && message.ends_with(eof)
                            && message.windows(eof.len()).position(|w| w == eof)
                                == message.len().checked_sub(eof.len())
                        {
                            line.clear();
                            if !self.console_line(false, &mut line)? {
                                break 'input;
                            }
                            source.push(b'\n');
                            source.extend_from_slice(&line);
                        } else {
                            report(&loads::message(
                                b"=stdin",
                                &error,
                                source.first() == Some(&27),
                            ));
                            continue 'input;
                        }
                    }
                }
            };
            let function = self.load_program(&program);
            drop(program);
            match self.call(Value::Function(function), &[]) {
                Ok(results) => values.extend_from_slice(results),
                Err(error) => {
                    if self.standalone == 0 || error.kind() != ErrorKind::Runtime {
                        return Err(error);
                    }
                    if let Some(bytes) = self.console_error(&error) {
                        report(&bytes);
                    }
                    continue;
                }
            }
            if !values.is_empty() {
                let print = self.get_global(b"print")?;
                if let Err(error) = self.standalone_call(print, &values, EntryMode::Protected) {
                    if self.standalone == 0 || error.kind() != ErrorKind::Runtime {
                        return Err(error);
                    }
                    let mut message = b"error calling 'print' (".to_vec();
                    let bytes = match error.value() {
                        Some(Value::String(id)) => self.string_bytes(id).unwrap().to_vec(),
                        Some(Value::Number(number)) => format_number(number),
                        _ => b"(null)".to_vec(),
                    };
                    message.extend_from_slice(io::c_bytes(&bytes));
                    message.push(b')');
                    report(&message);
                }
            }
        }
        self.registers.clear();
        self.top = 0;
        console_write(&mut self.io, &mut self.output, b"\n");
        Ok(())
    }

    fn console_error(&self, error: &Error) -> Option<Vec<u8>> {
        match error.value() {
            Some(Value::Nil) => None,
            Some(Value::String(id)) => Some(self.string_bytes(id).unwrap().to_vec()),
            Some(Value::Number(number)) => Some(format_number(number)),
            Some(_) => Some(b"(error object is not a string)".to_vec()),
            None => Some(error.message_bytes().to_vec()),
        }
    }

    fn console_line(&mut self, first: bool, line: &mut Vec<u8>) -> Result<bool, Error> {
        self.ensure_file_backend()?;
        let prompt = self.get_global(if first { b"_PROMPT" } else { b"_PROMPT2" })?;
        match prompt {
            Value::String(id) => console_write(
                &mut self.io,
                &mut self.output,
                io::c_bytes(&self.strings.bytes[slot_index(id.0)]),
            ),
            Value::Number(number) => {
                console_write(&mut self.io, &mut self.output, &format_number(number))
            }
            _ => console_write(
                &mut self.io,
                &mut self.output,
                if first { b"> " } else { b">> " },
            ),
        }
        // The default official console uses fgets(buffer, 512, stdin).
        // Consume its entire fragment even when strlen stops at an embedded NUL.
        let mut buffer = [0; 511];
        let mut len = self.console_read(&mut buffer);
        if len == 0 {
            return Ok(false);
        }
        len = buffer[..len].iter().position(|&b| b == 0).unwrap_or(len);
        if len != 0 && buffer[len - 1] == b'\n' {
            len -= 1;
        }
        if first && buffer.first() == Some(&b'=') {
            line.extend_from_slice(b"return ");
            line.extend_from_slice(&buffer[1..len]);
        } else {
            line.extend_from_slice(&buffer[..len]);
        }
        self.registers.clear();
        self.top = 0;
        Ok(true)
    }
}
