//! Shared logical stdout buffering, independent of Rust's line-buffered stdout.
use std::io::{IsTerminal, Write};
use std::sync::{LazyLock, Mutex};

struct Output {
    bytes: Vec<u8>,
    size: usize,
    mode: u8,
    allocated: bool,
}
static OUTPUT: LazyLock<Mutex<Output>> = LazyLock::new(|| {
    Mutex::new(Output {
        bytes: Vec::new(),
        size: if cfg!(windows) { 4096 } else { 8192 },
        mode: if std::io::stdout().is_terminal() {
            if cfg!(windows) { 0 } else { 2 }
        } else {
            1
        },
        allocated: false,
    })
});

impl Output {
    fn emit(bytes: &[u8]) -> std::io::Result<()> {
        let mut stdout = std::io::stdout().lock();
        if cfg!(windows) {
            let mut start = 0;
            for (at, &byte) in bytes.iter().enumerate() {
                if byte == b'\n' {
                    stdout.write_all(&bytes[start..at])?;
                    stdout.write_all(b"\r\n")?;
                    start = at + 1;
                }
            }
            stdout.write_all(&bytes[start..])?;
        } else {
            stdout.write_all(bytes)?;
        }
        stdout.flush()
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Self::emit(&self.bytes)?;
        self.bytes.clear();
        Ok(())
    }
    fn write(&mut self, mut bytes: &[u8]) -> std::io::Result<()> {
        if self.mode == 0 {
            return Self::emit(bytes);
        }
        while !bytes.is_empty() {
            if self.bytes.len() == self.size {
                self.flush()?;
            }
            // Fresh fully buffered streams write whole blocks directly. Once
            // a buffer exists, a final exactly full buffer stays pending until
            // the next write or an explicit flush (notably print's final LF).
            if self.mode == 1 && !self.allocated && bytes.len() >= self.size {
                let count = bytes.len() / self.size * self.size;
                Self::emit(&bytes[..count])?;
                bytes = &bytes[count..];
                continue;
            }
            self.allocated = true;
            let available = self.size - self.bytes.len();
            let mut count = available.min(bytes.len());
            if !cfg!(windows)
                && self.mode == 2
                && let Some(at) = bytes[..count].iter().position(|&b| b == b'\n')
            {
                count = at + 1;
            }
            self.bytes
                .try_reserve(count)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::OutOfMemory))?;
            self.bytes.extend_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
            if !cfg!(windows) && self.mode == 2 && self.bytes.last() == Some(&b'\n') {
                self.flush()?;
            }
        }
        Ok(())
    }
}

pub(super) fn write(bytes: &[u8]) -> std::io::Result<()> {
    OUTPUT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .write(bytes)
}
pub(in crate::vm) fn flush() -> std::io::Result<()> {
    OUTPUT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .flush()
}
pub(super) fn configure(mode: u8, size: usize) -> std::io::Result<()> {
    let mut output = OUTPUT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    output.flush()?;
    output.mode = mode;
    output.size = size.max(1);
    if mode != 0 {
        let additional = output.size;
        output
            .bytes
            .try_reserve(additional)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::OutOfMemory))?;
    }
    output.allocated = mode != 0;
    Ok(())
}
