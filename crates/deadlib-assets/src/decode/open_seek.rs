//! Verify source reuse and the reopen fallback with real file handles.
use super::*;
use std::fs::File;

#[cfg(windows)]
fn pipe_file() -> File {
    use std::os::windows::{io::OwnedHandle, process::CommandExt};
    use std::process::{Command, Stdio};
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "decode::open_seek::pipe_writer",
            "--ignored",
            "--nocapture",
        ])
        .stdout(Stdio::piped())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .unwrap();
    let input = File::from(OwnedHandle::from(child.stdout.take().unwrap()));
    assert!(child.wait().unwrap().success());
    input
}

#[cfg(windows)]
#[test]
#[ignore = "child-process pipe fixture"]
fn pipe_writer() {
    use std::io::Write;
    std::io::stdout().write_all(b"12345678").unwrap();
}

#[cfg(unix)]
fn pipe_file() -> File {
    use std::{
        io::Write,
        os::{fd::OwnedFd, unix::net::UnixStream},
    };
    let (mut writer, reader) = UnixStream::pair().unwrap();
    writer.write_all(b"12345678").unwrap();
    File::from(OwnedFd::from(reader))
}

#[test]
fn source_reader_rewinds_without_reopening_the_path() {
    let dir = crate::perf_fixture::TempDir::new("source-rewind");
    let path = dir.path.join("source");
    std::fs::write(&path, b"12345678 remaining bytes").unwrap();
    let mut input = File::open(&path).unwrap();
    input.read_exact(&mut [0; 8]).unwrap();
    let mut reader = image_reader(input, &dir.path.join("must-not-open")).unwrap();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"12345678 remaining bytes");
}

#[test]
fn source_reader_reopens_if_the_handle_cannot_seek() {
    let dir = crate::perf_fixture::TempDir::new("source-reopen");
    let path = dir.path.join("source");
    std::fs::write(&path, b"reopened contents").unwrap();
    for exists in [false, true] {
        let mut input = pipe_file();
        let mut signature = [0; 8];
        input.read_exact(&mut signature).unwrap();
        assert!(input.rewind().is_err());
        if exists {
            let mut reader = image_reader(input, &path).unwrap();
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"reopened contents");
        } else {
            let missing = dir.path.join("missing");
            let error = image_reader(input, &missing).unwrap_err();
            let image::ImageError::IoError(error) = error else {
                panic!("expected I/O error")
            };
            let expected = File::open(missing).unwrap_err();
            assert_eq!(error.kind(), expected.kind());
            assert_eq!(error.to_string(), expected.to_string());
        }
    }
}
