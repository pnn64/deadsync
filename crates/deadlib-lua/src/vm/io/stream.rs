//! Buffered native streams. Lua values never contain operating-system handles.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    process::Child,
};

pub(in crate::vm) const LINE_BUFFER: usize = if cfg!(windows) { 512 } else { 8192 };
pub(super) const BUFFER: usize = 4096;

#[derive(Debug)]
pub(in crate::vm) enum Resource {
    File(File),
    Input,
    Output,
    Error,
    Pipe(Child),
}
impl Resource {
    fn read_into(&mut self, bytes: &mut [u8]) -> Result<usize, i32> {
        match self {
            Self::File(f) => f.read(bytes),
            Self::Input => std::io::stdin().read(bytes),
            Self::Pipe(c) => c.stdout.as_mut().ok_or(9)?.read(bytes),
            _ => return Err(9),
        }
        .map_err(errno)
    }
}

pub(in crate::vm) fn errno(error: std::io::Error) -> i32 {
    #[cfg(windows)]
    {
        match error.raw_os_error() {
            Some(2 | 3 | 15 | 53 | 67 | 161 | 206) => 2,
            Some(5 | 19 | 32 | 33) => 13,
            Some(6) => 9,
            Some(80 | 183) => 17,
            Some(17) => 18,
            Some(145) => 41,
            Some(188..=202) => 8,
            Some(109 | 232) => 32,
            Some(112) => 28,
            Some(4) => 24,
            Some(8 | 14) => 12,
            _ => 22,
        }
    }
    #[cfg(not(windows))]
    {
        error.raw_os_error().unwrap_or(5)
    }
}

#[derive(Debug)]
pub(in crate::vm) struct Stream {
    pub resource: Option<Resource>,
    pub readable: bool,
    pub writable: bool,
    pub text: bool,
    read: Vec<u8>,
    position: usize,
    peek: Option<(u8, u8)>,
    control_eof: bool,
    eof: bool,
    raw_len: usize,
    lookahead: Option<u8>,
    write: Vec<u8>,
    pub buffering: u8,
    pub size: usize,
}

impl Stream {
    pub fn new(resource: Resource, readable: bool, writable: bool, text: bool) -> Self {
        let mut file = Self::closed();
        file.resource = Some(resource);
        file.readable = readable;
        file.writable = writable;
        file.text = text;
        file
    }
    pub fn closed() -> Self {
        Self {
            resource: None,
            readable: false,
            writable: false,
            text: false,
            read: Vec::new(),
            position: 0,
            peek: None,
            control_eof: false,
            eof: false,
            raw_len: 0,
            lookahead: None,
            write: Vec::new(),
            buffering: 1,
            size: BUFFER,
        }
    }
    pub fn heap_bytes(&self) -> usize {
        self.read.capacity() + self.write.capacity()
    }
    fn raw(&mut self) -> Result<Option<u8>, i32> {
        if !self.readable {
            return Err(9);
        }
        if self.position == self.read.len() {
            if self.control_eof || self.eof {
                return Ok(None);
            }
            let size = if self.buffering == 0 { 1 } else { self.size };
            self.read
                .try_reserve(size.saturating_sub(self.read.len()))
                .map_err(|_| 12)?;
            self.read.resize(size, 0);
            let prefix = if let Some(byte) = self.lookahead.take() {
                self.read[0] = byte;
                1
            } else {
                0
            };
            let count = prefix
                + self
                    .resource
                    .as_mut()
                    .ok_or(9)?
                    .read_into(&mut self.read[prefix..])?;
            self.raw_len = count;
            self.read.truncate(count);
            self.position = 0;
            if count == 0 {
                self.eof = cfg!(all(target_os = "linux", target_env = "gnu"));
                return Ok(None);
            }
            if cfg!(windows) && self.text {
                let mut source = 0;
                let mut destination = 0;
                while source < count {
                    let byte = self.read[source];
                    source += 1;
                    if byte == 26 {
                        self.control_eof = true;
                        break;
                    }
                    if byte == b'\r' {
                        if source < count && self.read[source] == b'\n' {
                            source += 1;
                            self.read[destination] = b'\n';
                            destination += 1;
                            continue;
                        }
                        if source == count {
                            let mut extra = [0];
                            if self.resource.as_mut().ok_or(9)?.read_into(&mut extra)? != 0 {
                                if extra[0] == b'\n' {
                                    self.raw_len += 1;
                                    self.read[destination] = b'\n';
                                    destination += 1;
                                    continue;
                                }
                                if let Some(Resource::File(f)) = self.resource.as_mut() {
                                    f.seek(SeekFrom::Current(-1)).map_err(errno)?;
                                } else {
                                    self.lookahead = Some(extra[0]);
                                }
                            }
                        }
                    }
                    self.read[destination] = byte;
                    destination += 1;
                }
                self.read.truncate(destination);
                if destination == 0 {
                    return Ok(None);
                }
            }
        }
        let byte = self.read[self.position];
        self.position += 1;
        Ok(Some(byte))
    }
    fn decoded(&mut self) -> Result<Option<(u8, u8)>, i32> {
        Ok(self.raw()?.map(|byte| (byte, 1)))
    }
    pub fn peek(&mut self) -> Result<Option<u8>, i32> {
        if self.peek.is_none() {
            self.peek = self.decoded()?;
        }
        Ok(self.peek.map(|p| p.0))
    }
    pub fn get(&mut self) -> Result<Option<u8>, i32> {
        Ok(match self.peek.take() {
            Some(p) => Some(p.0),
            None => self.decoded()?.map(|p| p.0),
        })
    }
    pub fn clear_eof(&mut self) {
        self.eof = false;
    }
    #[cfg(windows)]
    pub fn discard_number_sentinel(&mut self) {
        // UCRT's floating-input adapter cannot put back NUL or signed EOF.
        // Only drop existing lookahead; some NaN paths never fetch a suffix.
        if matches!(self.peek, Some((0 | 255, _))) {
            self.peek = None;
        }
    }
    pub fn read_chars(&mut self, out: &mut Vec<u8>, mut count: usize) -> Result<bool, i32> {
        self.flush()?;
        if count == 0 {
            return Ok(self.peek()?.is_some());
        }
        while count != 0 {
            if self.peek.is_none() && self.position < self.read.len() {
                let len = count.min(self.read.len() - self.position);
                out.extend_from_slice(&self.read[self.position..self.position + len]);
                self.position += len;
                count -= len;
            } else {
                let Some(byte) = self.get()? else {
                    break;
                };
                out.push(byte);
                count -= 1;
            }
        }
        Ok(count == 0 || !out.is_empty())
    }
    pub fn read_line(&mut self, out: &mut Vec<u8>) -> Result<bool, i32> {
        self.flush()?;
        // fgets followed by strlen is observable for embedded NUL bytes in 5.1.
        let mut chunk = [0; LINE_BUFFER];
        loop {
            let mut len = 0;
            while len + 1 < chunk.len() {
                let Some(byte) = self.get()? else {
                    break;
                };
                chunk[len] = byte;
                len += 1;
                if byte == b'\n' {
                    break;
                }
            }
            if len == 0 {
                return Ok(!out.is_empty());
            }
            let visible = chunk[..len].iter().position(|&b| b == 0).unwrap_or(len);
            if visible != 0 && chunk[visible - 1] == b'\n' {
                out.extend_from_slice(&chunk[..visible - 1]);
                return Ok(true);
            }
            out.extend_from_slice(&chunk[..visible]);
        }
    }
    fn reconcile(&mut self) -> Result<(), i32> {
        let consumed = self.position - usize::from(self.peek.is_some());
        let unread = self.read.len() - consumed;
        if unread != 0 {
            match self.resource.as_mut() {
                Some(Resource::File(f)) => {
                    let back = if cfg!(windows) && self.text && self.buffering != 0 {
                        let position = f.stream_position().map_err(errno)?;
                        if position == f.metadata().map_err(errno)?.len() {
                            unread as i64
                                + self.read[consumed..]
                                    .iter()
                                    .filter(|&&b| b == b'\n')
                                    .count() as i64
                                + i64::from(self.control_eof)
                        } else {
                            self.raw_len as i64
                                - consumed as i64
                                - self.read[..consumed]
                                    .iter()
                                    .filter(|&&b| b == b'\n')
                                    .count() as i64
                        }
                    } else {
                        unread as i64
                    };
                    f.seek(SeekFrom::Current(-back)).map_err(errno)?;
                }
                _ => return Err(29),
            }
        }
        self.read.clear();
        self.position = 0;
        self.peek = None;
        self.control_eof = false;
        self.raw_len = 0;
        self.lookahead = None;
        Ok(())
    }
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), i32> {
        if bytes.is_empty() {
            return Ok(());
        }
        if !self.writable {
            return Err(9);
        }
        self.reconcile()?;
        if matches!(self.resource, Some(Resource::Output)) {
            return super::stdout_write(bytes).map_err(errno);
        }
        if cfg!(windows) && self.text {
            for &byte in bytes {
                if byte == b'\n' {
                    self.write.push(b'\r');
                }
                self.write.push(byte);
            }
        } else {
            self.write.extend_from_slice(bytes);
        }
        // Share Rust's stdout buffer with print, preserving mixed-write order.
        if matches!(self.resource, Some(Resource::Output | Resource::Error)) {
            match self.resource {
                Some(Resource::Output) => std::io::stdout().write_all(&self.write),
                _ => std::io::stderr().write_all(&self.write),
            }
            .map_err(errno)?;
            self.write.clear();
            if self.buffering == 0 {
                self.flush()?;
            }
            return Ok(());
        }
        if self.buffering == 0
            || self.write.len() >= self.size
            || (!cfg!(windows) && self.buffering == 2 && bytes.contains(&b'\n'))
        {
            self.flush()?;
        }
        Ok(())
    }
    pub fn flush(&mut self) -> Result<(), i32> {
        if matches!(self.resource, Some(Resource::Output)) {
            return super::stdout_flush().map_err(errno);
        }
        if !self.write.is_empty() {
            match self.resource.as_mut() {
                Some(Resource::File(f)) => f.write_all(&self.write),
                Some(Resource::Output) => std::io::stdout().write_all(&self.write),
                Some(Resource::Error) => std::io::stderr().write_all(&self.write),
                Some(Resource::Pipe(c)) => c.stdin.as_mut().ok_or(9)?.write_all(&self.write),
                _ => return Err(9),
            }
            .map_err(errno)?;
            self.write.clear();
        }
        match self.resource.as_mut() {
            Some(Resource::File(f)) => f.flush(),
            Some(Resource::Output) => std::io::stdout().flush(),
            Some(Resource::Error) => std::io::stderr().flush(),
            Some(Resource::Pipe(c)) if self.writable => c.stdin.as_mut().ok_or(9)?.flush(),
            _ => Ok(()),
        }
        .map_err(errno)
    }
    pub fn set_buffering(&mut self, mode: u8, size: usize) -> Result<(), i32> {
        self.flush()?;
        if matches!(self.resource, Some(Resource::Output)) {
            super::output::configure(mode, size).map_err(errno)?;
        }
        self.buffering = mode;
        self.size = size;
        Ok(())
    }
    pub fn seek(&mut self, mode: usize, offset: i64) -> Result<u64, i32> {
        self.flush()?;
        self.reconcile()?;
        let Some(Resource::File(f)) = self.resource.as_mut() else {
            return Err(29);
        };
        let position = match mode {
            0 if offset < 0 => return Err(22),
            0 => SeekFrom::Start(offset as u64),
            1 => SeekFrom::Current(offset),
            _ => SeekFrom::End(offset),
        };
        let result = f.seek(position).map_err(errno)?;
        self.eof = false;
        Ok(result)
    }
    pub fn close(&mut self) -> Result<Option<i32>, i32> {
        let flush = self.flush();
        let resource = self.resource.take();
        self.read = Vec::new();
        self.write = Vec::new();
        self.position = 0;
        self.peek = None;
        if let Some(Resource::Pipe(mut child)) = resource {
            drop(child.stdin.take());
            drop(child.stdout.take());
            return child
                .wait()
                .map(|s| Some(s.code().unwrap_or(-1)))
                .map_err(errno);
        }
        flush.map(|_| None)
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        // Standard stdout remains a process resource when its Lua userdata dies.
        if self.resource.is_some() && !matches!(self.resource, Some(Resource::Output)) {
            let _ = self.close();
        }
    }
}
