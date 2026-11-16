//! Retain the previous process backend outside the allocation-tested profiles.
use crate::Error;
use std::{fs::File, io};
#[derive(Debug)]
pub(super) struct Child(std::process::Child);
impl Child {
    pub(super) fn wait(&mut self) -> io::Result<i32> {
        self.0.wait().map(|status| status.code().unwrap_or(-1))
    }
}
#[cfg(unix)]
pub(super) fn open(command: &[u8], reading: bool) -> Result<Result<(File, Child), i32>, Error> {
    use std::{
        ffi::OsStr,
        os::{fd::OwnedFd, unix::ffi::OsStrExt},
        process::{Command, Stdio},
    };
    let mut command_builder = Command::new("/bin/sh");
    command_builder.arg("-c").arg(OsStr::from_bytes(command));
    if reading {
        command_builder.stdout(Stdio::piped());
    } else {
        command_builder.stdin(Stdio::piped());
    }
    Ok(command_builder
        .spawn()
        .map(|mut child| {
            let fd = if reading {
                OwnedFd::from(child.stdout.take().unwrap())
            } else {
                OwnedFd::from(child.stdin.take().unwrap())
            };
            (File::from(fd), Child(child))
        })
        .map_err(super::super::errno))
}
#[cfg(not(unix))]
pub(super) fn open(_: &[u8], _: bool) -> Result<Result<(File, Child), i32>, Error> {
    Ok(Err(38))
}
