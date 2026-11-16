//! Fallible shell setup. Rust owns the pipe and child independently of VM storage.
use crate::Error;
use std::{
    fs::File,
    io::{self, Read, Write},
};

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod linux;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use linux as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;
#[cfg(not(any(windows, all(target_os = "linux", target_env = "gnu"))))]
mod portable;
#[cfg(not(any(windows, all(target_os = "linux", target_env = "gnu"))))]
use portable as platform;

#[derive(Debug)]
pub(in crate::vm) struct Pipe {
    file: Option<File>,
    child: platform::Child,
}
impl Pipe {
    pub(super) fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.file.as_mut().expect("live pipe").read(bytes)
    }
    pub(super) fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.file.as_mut().expect("live pipe").write_all(bytes)
    }
    pub(super) fn close(mut self) -> io::Result<i32> {
        // Closing the write end delivers EOF before waiting; closing the read
        // end lets a writer observe a broken pipe. Neither path allocates.
        drop(self.file.take());
        self.child.wait()
    }
}
impl Drop for Pipe {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = self.child.wait();
    }
}
pub(super) fn open(command: &[u8], reading: bool) -> Result<Result<Pipe, i32>, Error> {
    platform::open(command, reading).map(|result| {
        result.map(|(file, child)| Pipe {
            file: Some(file),
            child,
        })
    })
}
