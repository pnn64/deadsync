//! Shared fallible Lua stdout buffering over borrowed process handles.
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
        size: if GLIBC {
            0
        } else if cfg!(windows) {
            4096
        } else {
            8192
        },
        mode: if GLIBC {
            3
        } else if super::stdio::is_terminal(1) {
            if cfg!(windows) { 0 } else { 2 }
        } else {
            1
        },
        allocated: false,
    })
});

const GLIBC: bool = cfg!(all(
    target_os = "linux",
    target_env = "gnu",
    any(target_arch = "x86", target_arch = "x86_64")
));

impl Output {
    fn emit(bytes: &[u8]) -> std::io::Result<()> {
        if cfg!(windows) {
            let mut start = 0;
            for (at, &byte) in bytes.iter().enumerate() {
                if byte == b'\n' {
                    super::stdio::write_all(1, &bytes[start..at])?;
                    super::stdio::write_all(1, b"\r\n")?;
                    start = at + 1;
                }
            }
            super::stdio::write_all(1, &bytes[start..])?;
        } else {
            super::stdio::write_all(1, bytes)?;
        }
        Ok(())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Self::emit(&self.bytes)?;
        self.bytes.clear();
        Ok(())
    }
    fn write(&mut self, mut bytes: &[u8]) -> std::io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        #[cfg(all(
            target_os = "linux",
            target_env = "gnu",
            any(target_arch = "x86", target_arch = "x86_64")
        ))]
        {
            if self.mode == 3 {
                self.mode = if super::stdio::is_terminal(1) { 2 } else { 1 };
            }
            if self.size == 0 {
                self.size = super::stdio::output_buffer_size();
            }
            if self.mode != 0 {
                return self.write_glibc(bytes);
            }
        }
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
            self.allocated = true;
            self.bytes.extend_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
            if !cfg!(windows) && self.mode == 2 && self.bytes.last() == Some(&b'\n') {
                self.flush()?;
            }
        }
        Ok(())
    }

    #[cfg(all(
        target_os = "linux",
        target_env = "gnu",
        any(target_arch = "x86", target_arch = "x86_64")
    ))]
    fn write_glibc(&mut self, mut input: &[u8]) -> std::io::Result<()> {
        if input.is_empty() {
            return Ok(());
        }
        if self.allocated {
            let available = self.size - self.bytes.len();
            let mut count = available.min(input.len());
            let mut line_end = false;
            if self.mode == 2
                && input.len() <= available
                && let Some(at) = input.iter().rposition(|&b| b == b'\n')
            {
                count = at + 1;
                line_end = true;
            }
            self.bytes
                .try_reserve(count)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::OutOfMemory))?;
            self.bytes.extend_from_slice(&input[..count]);
            input = &input[count..];
            if input.is_empty() && !line_end {
                return Ok(());
            }
            self.flush()?;
        }
        self.allocated = true;
        // After entering write mode or exhausting the pending buffer, glibc
        // emits complete blocks directly and retains only the final remainder.
        let direct = if self.size >= 128 {
            input.len() / self.size * self.size
        } else {
            input.len()
        };
        Self::emit(&input[..direct])?;
        let mut tail = &input[direct..];
        if self.mode == 2
            && let Some(at) = tail.iter().rposition(|&b| b == b'\n')
        {
            Self::emit(&tail[..at + 1])?;
            tail = &tail[at + 1..];
        }
        self.bytes
            .try_reserve(tail.len())
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::OutOfMemory))?;
        self.bytes.extend_from_slice(tail);
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
    #[cfg(all(
        target_os = "linux",
        target_env = "gnu",
        any(target_arch = "x86", target_arch = "x86_64")
    ))]
    {
        // Lua supplies a null buffer to setvbuf. glibc ignores the requested
        // size and retains/allocates its native-sized buffer (or short buffer).
        let _ = size;
        if mode == 0 {
            output.size = 1;
            output.allocated = false;
        } else {
            if output.size == 0 {
                output.size = super::stdio::output_buffer_size();
            }
            if mode == 1 {
                let size = output.size;
                output
                    .bytes
                    .try_reserve(size)
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::OutOfMemory))?;
            }
        }
        output.mode = mode;
        Ok(())
    }
    #[cfg(not(all(
        target_os = "linux",
        target_env = "gnu",
        any(target_arch = "x86", target_arch = "x86_64")
    )))]
    {
        let size = size.max(1);
        if mode != 0 {
            output
                .bytes
                .try_reserve(size)
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::OutOfMemory))?;
        }
        output.mode = mode;
        output.size = size;
        output.allocated = mode != 0;
        Ok(())
    }
}
