use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Default)]
pub struct IoStats {
    pub reads: AtomicU64,
    pub bytes: AtomicU64,
    pub seeks: AtomicU64,
    pub tells: AtomicU64,
}
impl IoStats {
    pub fn reset(&self) {
        for counter in [&self.reads, &self.bytes, &self.seeks, &self.tells] {
            counter.store(0, Ordering::Relaxed);
        }
    }
    pub fn values(&self) -> [u64; 4] {
        [&self.reads, &self.bytes, &self.seeks, &self.tells].map(|v| v.load(Ordering::Relaxed))
    }
}
pub struct Source {
    data: Arc<[u8]>,
    position: u64,
    cap: usize,
    seekable: bool,
    stats: Arc<IoStats>,
}
impl Source {
    pub fn new(data: &Arc<[u8]>, cap: usize, seekable: bool, stats: &Arc<IoStats>) -> Self {
        Self {
            data: data.clone(),
            position: 0,
            cap,
            seekable,
            stats: stats.clone(),
        }
    }
}
impl Read for Source {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let remaining = usize::try_from(self.position)
            .ok()
            .and_then(|p| self.data.get(p..))
            .unwrap_or(&[]);
        let size = output.len().min(self.cap).min(remaining.len());
        output[..size].copy_from_slice(&remaining[..size]);
        self.position += size as u64;
        self.stats.reads.fetch_add(1, Ordering::Relaxed);
        self.stats.bytes.fetch_add(size as u64, Ordering::Relaxed);
        Ok(size)
    }
}
impl Seek for Source {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        if !self.seekable {
            return Err(io::ErrorKind::Unsupported.into());
        }
        if from == SeekFrom::Current(0) {
            self.stats.tells.fetch_add(1, Ordering::Relaxed);
        } else {
            self.stats.seeks.fetch_add(1, Ordering::Relaxed);
        }
        let next = match from {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::Current(n) => self.position as i128 + n as i128,
            SeekFrom::End(n) => self.data.len() as i128 + n as i128,
        };
        self.position = next.try_into().map_err(|_| io::ErrorKind::InvalidInput)?;
        Ok(self.position)
    }
}
impl symphonia::core::io::MediaSource for Source {
    fn is_seekable(&self) -> bool {
        self.seekable
    }
    fn byte_len(&self) -> Option<u64> {
        self.seekable.then_some(self.data.len() as u64)
    }
}
