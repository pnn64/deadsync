//! Lua 5.1 console input through the VM's shared standard streams.
use super::*;
use standalone::EntryMode;

fn console_write(output: &mut Option<Vec<u8>>, bytes: &[u8]) {
    if let Some(output) = output {
        output.extend_from_slice(bytes);
    } else {
        let _ = io::stdout_write(bytes);
        let _ = io::stdout_flush();
    }
}

impl Vm {
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
        console_write(&mut self.output, b"\n");
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
        let prompt = self.get_global(if first { b"_PROMPT" } else { b"_PROMPT2" })?;
        match prompt {
            Value::String(id) => console_write(
                &mut self.output,
                io::c_bytes(&self.strings.bytes[slot_index(id.0)]),
            ),
            Value::Number(number) => console_write(&mut self.output, &format_number(number)),
            _ => console_write(&mut self.output, if first { b"> " } else { b">> " }),
        }
        let Some(key) = self.io.stdin else {
            return Ok(false);
        };
        let stream = &mut self.io.files[slot_index(key.get())];
        // The default official console uses fgets(buffer, 512, stdin).
        // Consume its entire fragment even when strlen stops at an embedded NUL.
        let mut buffer = [0; 511];
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
