use super::{ButtonLight, CabinetLight, Player, State};
use log::{debug, warn};
use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::time::{Duration, Instant};

const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);
const PLAYER_SEXTETS: usize = 6;
const REPORT_SIZE: usize = 1 + PLAYER_SEXTETS * 2 + 1;
const LINE_FEED: u8 = b'\n';

pub struct Driver {
    port: String,
    file: Option<File>,
    last_open_attempt: Option<Instant>,
    last_report: [u8; REPORT_SIZE],
    warned_missing: bool,
}

impl Driver {
    pub fn new(port: String) -> Self {
        Self {
            port,
            file: None,
            last_open_attempt: None,
            last_report: [0; REPORT_SIZE],
            warned_missing: false,
        }
    }

    pub fn set(&mut self, state: &State) {
        let report = build_report(state);
        if report == self.last_report {
            return;
        }
        self.ensure_file();
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if let Err(e) = file.write_all(&report) {
            warn!("Litboard lights write failed: {e}");
            self.drop_file();
            return;
        }
        self.last_report = report;
    }

    fn ensure_file(&mut self) {
        if self.file.is_some() {
            return;
        }
        let now = Instant::now();
        if self
            .last_open_attempt
            .is_some_and(|last| now.duration_since(last) < REOPEN_INTERVAL)
        {
            return;
        }
        self.last_open_attempt = Some(now);
        let port_path = platform_port_path(&self.port);
        match OpenOptions::new().write(true).open(port_path.as_ref()) {
            Ok(file) => {
                if let Err(e) = configure_serial(&file) {
                    warn!("Litboard serial setup failed for {}: {e}", self.port);
                    return;
                }
                debug!("Opened Litboard serial lights output at {}", self.port);
                self.warned_missing = false;
                self.file = Some(file);
            }
            Err(e) => {
                if !self.warned_missing {
                    debug!("No Litboard serial lights output at {}: {e}", self.port);
                    self.warned_missing = true;
                }
            }
        }
    }

    fn drop_file(&mut self) {
        self.file = None;
    }
}

#[cfg(windows)]
fn platform_port_path(port: &str) -> Cow<'_, str> {
    if port.starts_with("\\\\.\\") {
        Cow::Borrowed(port)
    } else {
        Cow::Owned(format!("\\\\.\\{port}"))
    }
}

#[cfg(not(windows))]
fn platform_port_path(port: &str) -> Cow<'_, str> {
    Cow::Borrowed(port)
}

fn build_report(state: &State) -> [u8; REPORT_SIZE] {
    let mut report = [0u8; REPORT_SIZE];
    report[0] = pack_cabinet(state);
    pack_controller(state, Player::P1, &mut report[1..7]);
    pack_controller(state, Player::P2, &mut report[7..13]);
    report[13] = LINE_FEED;
    report
}

fn pack_cabinet(state: &State) -> u8 {
    pack_printable(
        state.cabinet(CabinetLight::MarqueeUpperLeft),
        state.cabinet(CabinetLight::MarqueeUpperRight),
        state.cabinet(CabinetLight::MarqueeLowerLeft),
        state.cabinet(CabinetLight::MarqueeLowerRight),
        state.cabinet(CabinetLight::BassLeft),
        state.cabinet(CabinetLight::BassRight),
    )
}

fn pack_controller(state: &State, player: Player, out: &mut [u8]) {
    let left = state.button(player, ButtonLight::Left);
    let right = state.button(player, ButtonLight::Right);
    let up = state.button(player, ButtonLight::Up);
    let down = state.button(player, ButtonLight::Down);
    let start = state.button(player, ButtonLight::Start);

    out[0] = pack_printable(left, right, up, down, start, false);
    out[1] = pack_printable(false, false, false, false, false, false);
    out[2] = pack_printable(left, right, up, down, false, false);
    out[3] = pack_printable(false, false, false, false, false, false);
    out[4] = pack_printable(false, false, false, false, false, false);
    out[5] = pack_printable(false, false, false, false, false, false);
}

fn pack_printable(b0: bool, b1: bool, b2: bool, b3: bool, b4: bool, b5: bool) -> u8 {
    let plain = u8::from(b0)
        | (u8::from(b1) << 1)
        | (u8::from(b2) << 2)
        | (u8::from(b3) << 3)
        | (u8::from(b4) << 4)
        | (u8::from(b5) << 5);
    ((plain + 0x10) & 0x3f) + 0x30
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
fn configure_serial(file: &File) -> io::Result<()> {
    use std::mem::zeroed;
    use std::os::fd::AsRawFd;

    let fd = file.as_raw_fd();
    // SAFETY: `termios` is an all-plain-data C struct; zeroed is the standard
    // initialization pattern before `tcgetattr` fills it for a valid fd.
    let mut termios: libc::termios = unsafe { zeroed() };
    // SAFETY: `fd` comes from a live `File` and `termios` points to valid memory.
    if unsafe { libc::tcgetattr(fd, &mut termios) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `termios` was initialized by `tcgetattr`.
    unsafe { libc::cfmakeraw(&mut termios) };
    // SAFETY: `termios` is valid and B115200 is a supported libc speed constant.
    if unsafe { libc::cfsetispeed(&mut termios, libc::B115200) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `termios` is valid and B115200 is a supported libc speed constant.
    if unsafe { libc::cfsetospeed(&mut termios, libc::B115200) } != 0 {
        return Err(io::Error::last_os_error());
    }
    termios.c_cflag |= libc::CLOCAL | libc::CREAD;
    termios.c_cflag &= !libc::PARENB;
    termios.c_cflag &= !libc::CSTOPB;
    termios.c_cflag &= !libc::CSIZE;
    termios.c_cflag |= libc::CS8;
    // SAFETY: `fd` comes from a live `File` and `termios` contains a valid
    // terminal configuration.
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn configure_serial(file: &File) -> io::Result<()> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Devices::Communication::{
        COMMTIMEOUTS, DCB, GetCommState, NOPARITY, ONESTOPBIT, SetCommState, SetCommTimeouts,
    };
    use windows_sys::Win32::Foundation::HANDLE;

    let handle = file.as_raw_handle() as HANDLE;
    let mut dcb = DCB::default();
    dcb.DCBlength = size_of::<DCB>() as u32;
    // SAFETY: `handle` comes from a live COM port `File`; `dcb` is valid output memory.
    if unsafe { GetCommState(handle, &mut dcb) } == 0 {
        return Err(io::Error::last_os_error());
    }
    dcb.BaudRate = 115200;
    dcb.ByteSize = 8;
    dcb.StopBits = ONESTOPBIT;
    dcb.Parity = NOPARITY;
    // SAFETY: `handle` comes from a live COM port `File`; `dcb` has a valid length and fields.
    if unsafe { SetCommState(handle, &dcb) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let timeouts = COMMTIMEOUTS {
        ReadIntervalTimeout: 50,
        ReadTotalTimeoutConstant: 50,
        ReadTotalTimeoutMultiplier: 10,
        WriteTotalTimeoutConstant: 50,
        WriteTotalTimeoutMultiplier: 10,
    };
    // SAFETY: `handle` comes from a live COM port `File`; `timeouts` is valid input memory.
    if unsafe { SetCommTimeouts(handle, &timeouts) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(all(
    not(windows),
    not(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))
))]
fn configure_serial(_file: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_uses_sextet_stream_order() {
        let mut state = State::default();
        state.set_cabinet(CabinetLight::MarqueeUpperLeft, true);
        state.set_cabinet(CabinetLight::BassRight, true);
        state.set_button(Player::P1, ButtonLight::Left, true);
        state.set_button(Player::P1, ButtonLight::Start, true);
        state.set_button(Player::P2, ButtonLight::Right, true);

        let report = build_report(&state);
        assert_eq!(report.len(), REPORT_SIZE);
        assert_eq!(report[0] & 0x3f, 0x21);
        assert_eq!(report[1] & 0x3f, 0x11);
        assert_eq!(report[3] & 0x3f, 0x01);
        assert_eq!(report[7] & 0x3f, 0x02);
        assert_eq!(report[9] & 0x3f, 0x02);
        assert_eq!(report[13], LINE_FEED);
    }
}
