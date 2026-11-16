//! Linux/glibc process ABI for the documented x64 and i686 profiles.
use crate::{
    Error,
    memory::fallible::{SliceExt, VecExt},
};
use std::{
    ffi::{c_char, c_int},
    fs::File,
    io,
    mem::MaybeUninit,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::ExitStatusExt,
    },
    process::ExitStatus,
    ptr,
};

// Public glibc spawn.h layouts. sigset_t occupies 128 bytes on both profiles;
// its native-word alignment is relevant to the x64 attribute layout.
type SignalSet = [usize; 128 / size_of::<usize>()];
#[repr(C)]
struct Attributes {
    flags: i16,
    group: c_int,
    defaults: SignalSet,
    mask: SignalSet,
    priority: c_int,
    policy: c_int,
    reserved: [c_int; 16],
}
#[repr(C)]
struct Actions {
    allocated: c_int,
    used: c_int,
    actions: *mut std::ffi::c_void,
    reserved: [c_int; 16],
}
const _: () = {
    assert!(size_of::<Attributes>() == 336);
    assert!(size_of::<Actions>() == 72 + size_of::<usize>());
};
unsafe extern "C" {
    fn pipe2(fds: *mut c_int, flags: c_int) -> c_int;
    fn fcntl(fd: c_int, command: c_int, argument: c_int) -> c_int;
    fn posix_spawn_file_actions_init(actions: *mut Actions) -> c_int;
    fn posix_spawn_file_actions_destroy(actions: *mut Actions) -> c_int;
    fn posix_spawn_file_actions_adddup2(actions: *mut Actions, fd: c_int, target: c_int) -> c_int;
    fn posix_spawn_file_actions_addclose(actions: *mut Actions, fd: c_int) -> c_int;
    fn posix_spawnattr_init(attributes: *mut Attributes) -> c_int;
    fn posix_spawnattr_destroy(attributes: *mut Attributes) -> c_int;
    fn posix_spawnattr_setsigdefault(
        attributes: *mut Attributes,
        signals: *const SignalSet,
    ) -> c_int;
    fn posix_spawnattr_setflags(attributes: *mut Attributes, flags: i16) -> c_int;
    fn sigemptyset(signals: *mut SignalSet) -> c_int;
    fn sigaddset(signals: *mut SignalSet, signal: c_int) -> c_int;
    fn posix_spawn(
        pid: *mut c_int,
        path: *const c_char,
        actions: *const Actions,
        attributes: *const Attributes,
        argv: *const *const c_char,
        env: *const *const c_char,
    ) -> c_int;
    static mut environ: *const *const c_char;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
}
impl Drop for Actions {
    fn drop(&mut self) {
        // SAFETY: only successfully initialized action records have this type.
        unsafe {
            posix_spawn_file_actions_destroy(self);
        }
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: only successfully initialized attribute records have this type.
        unsafe {
            posix_spawnattr_destroy(self);
        }
    }
}

#[derive(Debug)]
pub(super) struct Child {
    pid: Option<c_int>,
    status: i32,
}
impl Child {
    pub(super) fn wait(&mut self) -> io::Result<i32> {
        if let Some(pid) = self.pid {
            loop {
                let mut status = 0;
                // SAFETY: pid is this pipe's unreaped child, and status is live.
                let result = unsafe { waitpid(pid, &mut status, 0) };
                if result == pid {
                    self.pid = None;
                    self.status = ExitStatus::from_raw(status).code().unwrap_or(-1);
                    break;
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    // ECHILD also covers a host SIGCHLD handler which reaped it.
                    if error.raw_os_error() == Some(10) {
                        self.pid = None;
                    }
                    return Err(error);
                }
            }
        }
        Ok(self.status)
    }
}

// Keep temporary pipe descriptors above stderr even if the host closed a
// standard descriptor. This prevents the close actions undoing our dup2.
fn above_stdio(file: File) -> io::Result<File> {
    if file.as_raw_fd() >= 3 {
        return Ok(file);
    }
    // SAFETY: F_DUPFD_CLOEXEC duplicates a live owned fd, choosing fd >= 3.
    let duplicate = unsafe { fcntl(file.as_raw_fd(), 1030, 3) };
    if duplicate < 0 {
        let error = io::Error::last_os_error();
        // With RLIMIT_NOFILE <= 3, this valid minimum is beyond the descriptor
        // table and fcntl reports EINVAL. Surface descriptor exhaustion (EMFILE),
        // as pipe2/popen do, rather than blaming the Lua command or mode.
        return Err(if error.raw_os_error() == Some(22) {
            io::Error::from_raw_os_error(24)
        } else {
            error
        });
    }
    // SAFETY: the new descriptor is owned exactly once.
    Ok(unsafe { File::from_raw_fd(duplicate) })
}

pub(super) fn open(command: &[u8], reading: bool) -> Result<Result<(File, Child), i32>, Error> {
    let mut command = command.try_to_vec()?;
    command.try_push(0)?;
    Ok(spawn(&command, reading).map_err(super::super::errno))
}
fn spawn(command: &[u8], reading: bool) -> io::Result<(File, Child)> {
    let mut fds = [-1; 2];
    // SAFETY: output holds two ints. O_CLOEXEC prevents either parent end
    // leaking into concurrent children; dup2 below clears it only in this child.
    if unsafe { pipe2(fds.as_mut_ptr(), 0x80000) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: pipe2 succeeded and transferred two distinct owned descriptors.
    let (read, write) = unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) };
    let read = above_stdio(read)?;
    let write = above_stdio(write)?;
    let (parent, child) = if reading {
        (read, write)
    } else {
        (write, read)
    };
    let mut actions = MaybeUninit::<Actions>::zeroed();
    // SAFETY: native initializers receive correctly sized/aligned output records.
    check(unsafe { posix_spawn_file_actions_init(actions.as_mut_ptr()) })?;
    let mut actions = unsafe { actions.assume_init() };
    let mut attributes = MaybeUninit::<Attributes>::zeroed();
    check(unsafe { posix_spawnattr_init(attributes.as_mut_ptr()) })?;
    let mut attributes = unsafe { attributes.assume_init() };
    let mut signals: SignalSet = [0; 128 / size_of::<usize>()];
    // SAFETY: all records are initialized and owned until spawn returns. Child
    // file actions run in order. Reset Rust's inherited SIGPIPE ignore policy.
    unsafe {
        sigemptyset(&mut signals);
        sigaddset(&mut signals, 13);
        check(posix_spawnattr_setsigdefault(&mut attributes, &signals))?;
        check(posix_spawnattr_setflags(&mut attributes, 4))?;
        check(posix_spawn_file_actions_adddup2(
            &mut actions,
            child.as_raw_fd(),
            i32::from(reading),
        ))?;
        check(posix_spawn_file_actions_addclose(
            &mut actions,
            parent.as_raw_fd(),
        ))?;
        check(posix_spawn_file_actions_addclose(
            &mut actions,
            child.as_raw_fd(),
        ))?;
    }
    let argv = [
        c"sh".as_ptr(),
        c"-c".as_ptr(),
        command.as_ptr().cast(),
        ptr::null(),
    ];
    let mut pid = 0;
    // SAFETY: strings are NUL terminated and all storage lives until the
    // synchronous spawn finishes. As with getenv, the host must not concurrently
    // mutate POSIX environ. No Rust code, allocator or callback runs after fork.
    check(unsafe {
        posix_spawn(
            &mut pid,
            c"/bin/sh".as_ptr(),
            &actions,
            &attributes,
            argv.as_ptr(),
            environ,
        )
    })?;
    Ok((
        parent,
        Child {
            pid: Some(pid),
            status: -1,
        },
    ))
}
fn check(code: c_int) -> io::Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code))
    }
}
