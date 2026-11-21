//! Buffered native streams. Lua values never contain operating-system handles.
use crate::{
    locale::{ascii_space, is_space},
    memory::bytes::ByteOutput,
    pattern::{find_byte, scan_boundary},
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};

pub(in crate::vm) const LINE_BUFFER: usize = if cfg!(windows) { 512 } else { 8192 };
pub(super) const BUFFER: usize = 4096;

#[derive(Debug)]
pub(in crate::vm) enum Resource {
    File(File),
    Input,
    Output,
    Error,
    Pipe(super::process::Pipe),
}
impl Resource {
    fn read_into(&mut self, bytes: &mut [u8]) -> Result<usize, i32> {
        match self {
            Self::File(f) => f.read(bytes),
            Self::Input => super::stdio::read(bytes),
            Self::Pipe(c) => c.read(bytes),
            _ => return Err(9),
        }
        .map_err(errno)
    }
}

pub(in crate::vm) fn errno(error: std::io::Error) -> i32 {
    if error.kind() == std::io::ErrorKind::OutOfMemory {
        return 12;
    }
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
    peek: Option<u8>,
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
                    if byte == b'\r' {
                        if source < count && self.read[source] == b'\n' {
                            let pairs = 1 + self.read[source + 1..]
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .take_while(|&&bytes| bytes == *b"\r\n")
                                .count();
                            source += pairs * 2 - 1;
                            self.read[destination..destination + pairs].fill(b'\n');
                            destination += pairs;
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
                    } else if byte == 26 {
                        self.control_eof = true;
                        break;
                    } else if source < count && !matches!(self.read[source], b'\r' | 26) {
                        let start = source - 1;
                        source += scan_boundary(&self.read[source..], |b| (b == b'\r') | (b == 26));
                        if destination != start {
                            self.read.copy_within(start..source, destination);
                        }
                        destination += source - start;
                        continue;
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
    pub fn peek(&mut self) -> Result<Option<u8>, i32> {
        if self.peek.is_none() {
            self.peek = self.raw()?;
        }
        Ok(self.peek)
    }
    pub fn get(&mut self) -> Result<Option<u8>, i32> {
        match self.peek.take() {
            byte @ Some(_) => Ok(byte),
            None => self.raw(),
        }
    }
    pub(super) fn skip_space(&mut self) -> Result<(), i32> {
        while self.peek()?.is_some_and(is_space) {
            self.peek = None;
            let tail = &self.read[self.position..];
            if tail.len() >= 64 && ascii_space(tail[0]) && ascii_space(tail[7]) {
                // Only skip bytes whose classification is independent of LC_CTYPE.
                self.position += scan_boundary(tail, |byte| !ascii_space(byte));
            }
        }
        Ok(())
    }
    /// Skip at most `limit` bytes, returning true at a delimiter or EOF.
    /// When `consume` is false, a found delimiter remains in lookahead.
    pub(in crate::vm) fn skip_until(
        &mut self,
        delimiter: u8,
        mut limit: usize,
        consume: bool,
    ) -> Result<bool, i32> {
        while limit != 0 {
            if self.peek.is_none() && self.readable && self.position < self.read.len() {
                let available = &self.read[self.position..];
                let bytes = &available[..available.len().min(limit)];
                if let Some(offset) = find_byte(bytes, delimiter) {
                    self.position += offset + 1;
                    self.peek = (!consume).then_some(delimiter);
                    return Ok(true);
                }
                self.position += bytes.len();
                limit -= bytes.len();
            } else {
                let Some(byte) = self.peek()? else {
                    return Ok(true);
                };
                let matched = byte == delimiter;
                if !matched || consume {
                    self.peek = None;
                }
                if matched {
                    return Ok(true);
                }
                limit -= 1;
            }
        }
        Ok(false)
    }
    pub fn clear_eof(&mut self) {
        self.eof = false;
    }
    #[cfg(windows)]
    pub fn discard_number_sentinel(&mut self) {
        // UCRT's floating-input adapter cannot put back NUL or signed EOF.
        // Only drop existing lookahead; some NaN paths never fetch a suffix.
        if matches!(self.peek, Some(0 | 255)) {
            self.peek = None;
        }
    }
    pub fn read_chars(&mut self, out: &mut impl ByteOutput, mut count: usize) -> Result<bool, i32> {
        self.flush()?;
        if count == 0 {
            return Ok(self.peek()?.is_some());
        }
        while count != 0 {
            if self.peek.is_none() && self.position < self.read.len() {
                let len = count.min(self.read.len() - self.position);
                out.extend_bytes(&self.read[self.position..self.position + len])
                    .map_err(|_| 12)?;
                self.position += len;
                count -= len;
            } else {
                out.reserve_bytes(1).map_err(|_| 12)?;
                let Some(byte) = self.get()? else {
                    break;
                };
                out.push_byte(byte);
                count -= 1;
            }
        }
        Ok(count == 0 || !out.is_empty())
    }
    pub fn read_line(&mut self, out: &mut impl ByteOutput) -> Result<bool, i32> {
        self.flush()?;
        // fgets followed by strlen is observable for embedded NUL bytes in 5.1.
        let mut chunk = [0; LINE_BUFFER];
        loop {
            let mut len = 0;
            while len + 1 < chunk.len() {
                if self.peek.is_none() && self.readable && self.position < self.read.len() {
                    let room = chunk.len() - 1 - len;
                    let buffered = &self.read[self.position..];
                    let bytes = &buffered[..room.min(buffered.len())];
                    let newline = find_byte(bytes, b'\n');
                    let count = newline.map_or(bytes.len(), |index| index + 1);
                    chunk[len..len + count].copy_from_slice(&bytes[..count]);
                    self.position += count;
                    len += count;
                    if newline.is_some() {
                        break;
                    }
                    continue;
                }
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
            let visible = find_byte(&chunk[..len], 0).unwrap_or(len);
            out.reserve_bytes(visible).map_err(|_| 12)?;
            if visible != 0 && chunk[visible - 1] == b'\n' {
                out.extend_bytes(&chunk[..visible - 1]).map_err(|_| 12)?;
                return Ok(true);
            }
            out.extend_bytes(&chunk[..visible]).map_err(|_| 12)?;
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
        let newlines = if cfg!(windows) && self.text {
            bytes.iter().filter(|&&byte| byte == b'\n').count()
        } else {
            0
        };
        let additional = bytes.len().checked_add(newlines).ok_or(12)?;
        self.write.try_reserve(additional).map_err(|_| 12)?;
        if newlines > bytes.len() / 8 {
            for &byte in bytes {
                if byte == b'\n' {
                    self.write.push(b'\r');
                }
                self.write.push(byte);
            }
        } else {
            let mut remaining = bytes;
            for _ in 0..newlines {
                let at = find_byte(remaining, b'\n').expect("counted newline in remaining text");
                self.write.extend_from_slice(&remaining[..at]);
                self.write.extend_from_slice(b"\r\n");
                remaining = &remaining[at + 1..];
            }
            self.write.extend_from_slice(remaining);
        }
        if matches!(self.resource, Some(Resource::Error)) {
            super::stdio::write_all(2, &self.write).map_err(errno)?;
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
                Some(Resource::Output) => super::stdio::write_all(1, &self.write),
                Some(Resource::Error) => super::stdio::write_all(2, &self.write),
                Some(Resource::Pipe(c)) => c.write_all(&self.write),
                _ => return Err(9),
            }
            .map_err(errno)?;
            self.write.clear();
        }
        match self.resource.as_mut() {
            Some(Resource::File(f)) => f.flush(),
            Some(Resource::Output | Resource::Error) => Ok(()),
            Some(Resource::Pipe(_)) if self.writable => Ok(()),
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
    pub fn seek(&mut self, mode: usize, offset: i64) -> Result<i64, i32> {
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
        Ok(i64::try_from(result).unwrap_or(-1))
    }
    pub fn close(&mut self) -> Result<Option<i32>, i32> {
        let flush = self.flush();
        let resource = self.resource.take();
        self.read = Vec::new();
        self.write = Vec::new();
        self.position = 0;
        self.peek = None;
        if let Some(Resource::Pipe(child)) = resource {
            return child.close().map(Some).map_err(errno);
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
