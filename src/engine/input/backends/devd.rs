use log::warn;
use std::ffi::c_void;
use std::mem::size_of;
use std::os::fd::RawFd;

const DEVD_SOCKET: &[u8] = b"/var/run/devd.seqpacket.pipe\0";
const DEVD_BUF_LEN: usize = 4096;

pub(super) enum DevdEvent {
    Create(String),
    Destroy(String),
}

pub(super) struct DevdWatch {
    fd: RawFd,
}

impl DevdWatch {
    pub(super) fn new() -> Option<Self> {
        // SAFETY: this is a straightforward libc socket creation call with no
        // aliasing requirements; we check the returned fd before using it.
        let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET, 0) };
        if fd < 0 {
            warn!(
                "freebsd input could not open devd seqpacket socket: {}",
                std::io::Error::last_os_error()
            );
            return None;
        }
        if !configure_socket(fd) {
            // SAFETY: `fd` is still owned by this constructor path and has not been
            // moved into any wrapper yet, so closing it here is correct cleanup.
            unsafe {
                libc::close(fd);
            }
            return None;
        }
        // SAFETY: zero-initializing `sockaddr_un` is valid and gives us a clean
        // buffer before setting the required fields below.
        let mut addr = unsafe { std::mem::zeroed::<libc::sockaddr_un>() };
        addr.sun_len = size_of::<libc::sockaddr_un>() as u8;
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        let path = &DEVD_SOCKET[..DEVD_SOCKET.len() - 1];
        if path.len() >= addr.sun_path.len() {
            warn!("freebsd input devd socket path is too long");
            // SAFETY: `fd` is still uniquely owned here and must be closed on the
            // early-return path.
            unsafe {
                libc::close(fd);
            }
            return None;
        }
        let mut i = 0;
        while i < path.len() {
            addr.sun_path[i] = path[i] as libc::c_char;
            i += 1;
        }
        // SAFETY: `addr` is fully initialized as a Unix-domain socket address and
        // we pass its address with the correct byte size for `sockaddr_un`.
        let rc = unsafe {
            libc::connect(
                fd,
                (&raw const addr).cast(),
                size_of::<libc::sockaddr_un>() as libc::socklen_t,
            )
        };
        if rc == 0 {
            return Some(Self { fd });
        }
        warn!(
            "freebsd input could not connect to devd seqpacket socket: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: `fd` is still owned locally because the connection failed, so we
        // must close it before returning.
        unsafe {
            libc::close(fd);
        }
        None
    }

    #[inline(always)]
    pub(super) const fn fd(&self) -> RawFd {
        self.fd
    }

    pub(super) fn collect_events(&self, out: &mut Vec<DevdEvent>) {
        let mut buf = [0u8; DEVD_BUF_LEN];
        loop {
            // SAFETY: `buf` is a valid writable byte array of length `buf.len()`,
            // and `self.fd` remains owned by this watcher for the duration of the
            // call.
            let n = unsafe { libc::recv(self.fd, buf.as_mut_ptr().cast::<c_void>(), buf.len(), 0) };
            if n > 0 {
                if let Some(event) = parse_packet(&buf[..n as usize]) {
                    out.push(event);
                }
                continue;
            }
            if n == 0 {
                warn!("freebsd input devd seqpacket socket closed");
                return;
            }
            let err = std::io::Error::last_os_error();
            let raw = err.raw_os_error();
            if raw == Some(libc::EAGAIN) || raw == Some(libc::EWOULDBLOCK) {
                return;
            }
            warn!("freebsd input devd seqpacket read failed: {err}");
            return;
        }
    }
}

impl Drop for DevdWatch {
    fn drop(&mut self) {
        // SAFETY: `fd` is owned by `DevdWatch` and closed exactly once here on
        // drop.
        unsafe {
            libc::close(self.fd);
        }
    }
}

fn configure_socket(fd: RawFd) -> bool {
    // SAFETY: `fd` is an open descriptor created by `socket`, and `F_GETFL` does
    // not require any extra pointer arguments.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
    if flags < 0 {
        warn!(
            "freebsd input could not query devd socket flags: {}",
            std::io::Error::last_os_error()
        );
        return false;
    }
    // SAFETY: `F_SETFL` updates the descriptor flags for this valid fd. We only
    // add `O_NONBLOCK`.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } != 0 {
        warn!(
            "freebsd input could not make devd socket nonblocking: {}",
            std::io::Error::last_os_error()
        );
        return false;
    }
    // SAFETY: `F_SETFD` updates close-on-exec on this valid fd without borrowing
    // any Rust-managed memory.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } == 0 {
        return true;
    }
    warn!(
        "freebsd input could not set devd socket close-on-exec: {}",
        std::io::Error::last_os_error()
    );
    false
}

fn parse_packet(packet: &[u8]) -> Option<DevdEvent> {
    let text = std::str::from_utf8(packet).ok()?;
    let text = text.trim_matches(|ch: char| ch.is_ascii_whitespace() || ch == '\0');
    let text = match text.as_bytes().first().copied() {
        Some(b'!') | Some(b'+') | Some(b'-') | Some(b'?') => &text[1..],
        _ => text,
    };
    let mut system = None;
    let mut subsystem = None;
    let mut event_type = None;
    let mut cdev = None;
    for field in text.split_ascii_whitespace() {
        let Some((key, value)) = field.split_once('=') else {
            continue;
        };
        let value = value.trim_matches('"');
        match key {
            "system" => system = Some(value),
            "subsystem" => subsystem = Some(value),
            "type" => event_type = Some(value),
            "cdev" => cdev = Some(value),
            _ => {}
        }
    }
    if !system.is_some_and(|value| value.eq_ignore_ascii_case("DEVFS"))
        || !subsystem.is_some_and(|value| value.eq_ignore_ascii_case("CDEV"))
    {
        return None;
    }
    let path = cdev_path(cdev?);
    if event_type.is_some_and(|value| value.eq_ignore_ascii_case("CREATE")) {
        return Some(DevdEvent::Create(path));
    }
    if event_type.is_some_and(|value| value.eq_ignore_ascii_case("DESTROY")) {
        return Some(DevdEvent::Destroy(path));
    }
    None
}

#[inline(always)]
fn cdev_path(cdev: &str) -> String {
    if cdev.starts_with('/') {
        return cdev.to_owned();
    }
    format!("/dev/{cdev}")
}
