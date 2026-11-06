//! teejusb/fsr's line protocol. No HID input is synthesized from these readings.
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

pub const MAX_SENSORS: usize = 255; // firmware sensor IDs are nonzero uint8_t
pub const MAX_THRESHOLD: u16 = 1023;
const MAX_LINE: usize = 2048;
const RESPONSE_TIMEOUT: Duration = Duration::from_millis(350);

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    #[default]
    Auto,
    Modern,
    Legacy,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    Values(Vec<i16>),
    Thresholds(Vec<u16>),
    Saved(Vec<u16>),
}

pub fn parse_line(bytes: &[u8]) -> Option<Response> {
    let line = std::str::from_utf8(bytes).ok()?;
    let mut fields = line.split_ascii_whitespace();
    let kind = fields.next()?;
    if !matches!(kind, "v" | "t" | "s") {
        return None;
    }
    let values = fields
        .map(str::parse::<i16>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.is_empty() || values.len() > MAX_SENSORS {
        return None;
    }
    if kind == "v" {
        // Unaveraged AVR firmware can report negative readings after offsetting.
        return Some(Response::Values(values));
    }
    if values.iter().any(|&n| !(0..=1023).contains(&n)) {
        return None;
    }
    let values = values.into_iter().map(|n| n as u16).collect();
    Some(if kind == "t" {
        Response::Thresholds(values)
    } else {
        Response::Saved(values)
    })
}

/// Retains partial lines across short reads. Oversized lines are discarded through
/// their newline, never reinterpreted as a new response starting in the middle.
#[derive(Default)]
struct Lines {
    bytes: Vec<u8>,
    overflow: bool,
}

impl Lines {
    fn push(&mut self, byte: u8) -> Option<Response> {
        if byte == b'\n' {
            let result = (!self.overflow).then(|| parse_line(&self.bytes)).flatten();
            self.bytes.clear();
            self.overflow = false;
            return result;
        }
        if !self.overflow {
            if self.bytes.len() == MAX_LINE {
                self.bytes.clear();
                self.overflow = true;
            } else {
                self.bytes.push(byte);
            }
        }
        None
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn threshold_command(protocol: Protocol, index: usize, value: u16) -> io::Result<String> {
    if index >= MAX_SENSORS || value > MAX_THRESHOLD {
        return Err(invalid("threshold or sensor index out of range"));
    }
    let command = match protocol {
        Protocol::Modern => format!("{index} {value}"),
        Protocol::Legacy if index < 10 => format!("{index}{value}"),
        Protocol::Legacy => return Err(invalid("legacy firmware only addresses sensors 0-9")),
        Protocol::Auto => return Err(invalid("firmware protocol has not been identified")),
    };
    // Upstream accepts at most seven bytes, excluding the newline. Do not send
    // commands that it will silently ignore (e.g. a three-digit index + 1023).
    if command.len() > 7 {
        return Err(invalid(
            "threshold command exceeds the firmware's seven-byte limit",
        ));
    }
    Ok(command + "\n")
}

pub struct Session<T> {
    transport: T,
    lines: Lines,
    pub thresholds: Vec<u16>,
    pub values: Vec<i16>,
    pub protocol: Protocol,
    pub save_supported: Option<bool>,
}

impl<T: Read + Write> Session<T> {
    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn connect(transport: T, protocol: Protocol) -> io::Result<Self> {
        let mut session = Self {
            transport,
            lines: Lines::default(),
            thresholds: Vec::new(),
            values: Vec::new(),
            protocol,
            save_supported: None,
        };
        // Some serial adapters reset their MCU on DTR. Keep the same port open
        // through its bootloader interval instead of repeatedly resetting it.
        let boot_deadline = Instant::now() + Duration::from_secs(2);
        let initial = loop {
            match session.request(b"t\n", b't') {
                Err(error)
                    if error.kind() == io::ErrorKind::TimedOut
                        && Instant::now() < boot_deadline =>
                {
                    continue;
                }
                result => break result?,
            }
        };
        let Response::Thresholds(thresholds) = initial else {
            unreachable!();
        };
        session.thresholds = thresholds;
        session.poll_values()?;
        if protocol == Protocol::Auto {
            // This rewrites the CURRENT value, with four digits, and never saves.
            // Modern firmware accepts it; old firmware rejects its six-byte size.
            // A timeout is not proof of legacy firmware: leave it read-only until
            // the user explicitly chooses legacy. Never guess a write dialect.
            let probe = format!("0 {:04}\n", session.thresholds[0]);
            match session.request(probe.as_bytes(), b't') {
                Ok(Response::Thresholds(actual)) if actual == session.thresholds => {
                    session.protocol = Protocol::Modern;
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    let Response::Thresholds(actual) = session.request(b"t\n", b't')? else {
                        unreachable!();
                    };
                    if actual != session.thresholds {
                        return Err(invalid("thresholds changed during protocol identification"));
                    }
                }
                Err(e) => return Err(e),
                _ => return Err(invalid("unexpected protocol identification response")),
            }
        }
        if session.protocol == Protocol::Legacy && session.thresholds.len() > 10 {
            return Err(invalid("legacy firmware cannot edit more than ten sensors"));
        }
        Ok(session)
    }

    fn request(&mut self, command: &[u8], kind: u8) -> io::Result<Response> {
        // Each request is a complete line. The MCU's readBytesUntil blocks its
        // gameplay loop if a caller writes a command without the newline.
        self.transport.write_all(command)?;
        let deadline = Instant::now() + RESPONSE_TIMEOUT;
        let mut bytes = [0; 256];
        while Instant::now() < deadline {
            match self.transport.read(&mut bytes) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(count) => {
                    let mut matched = None;
                    for &byte in &bytes[..count] {
                        if let Some(response) = self.lines.push(byte) {
                            let matches = matches!(
                                (&response, kind),
                                (Response::Values(_), b'v')
                                    | (Response::Thresholds(_), b't')
                                    | (Response::Saved(_), b's')
                            );
                            if matches && matched.is_none() {
                                matched = Some(response);
                            }
                        }
                    }
                    if let Some(response) = matched {
                        return Ok(response);
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "no teejusb response",
        ))
    }

    pub fn poll_values(&mut self) -> io::Result<()> {
        let Response::Values(values) = self.request(b"v\n", b'v')? else {
            unreachable!();
        };
        if values.len() != self.thresholds.len() {
            return Err(invalid("sensor count changed; reconnect required"));
        }
        self.values = values;
        Ok(())
    }

    pub fn set_threshold(&mut self, index: usize, value: u16) -> io::Result<bool> {
        let Some(&current) = self.thresholds.get(index) else {
            return Err(invalid("unknown sensor"));
        };
        let command = threshold_command(self.protocol, index, value)?;
        if current == value {
            return Ok(false);
        }
        let mut expected = self.thresholds.clone();
        expected[index] = value;
        let Response::Thresholds(actual) = self.request(command.as_bytes(), b't')? else {
            unreachable!();
        };
        if actual.len() != expected.len() {
            return Err(invalid("sensor count changed during threshold write"));
        }
        self.thresholds = actual;
        if self.thresholds != expected {
            return Err(invalid("threshold write was not confirmed by firmware"));
        }
        Ok(true)
    }

    pub fn save(&mut self) -> io::Result<bool> {
        if self.save_supported == Some(false) {
            return Ok(false);
        }
        match self.request(b"s\n", b's') {
            Ok(Response::Saved(values)) if values == self.thresholds => {
                self.save_supported = Some(true);
                Ok(true)
            }
            Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                // Confirm the board is still alive. Old firmware ignores `s`.
                self.poll_values()?;
                self.save_supported = Some(false);
                Ok(false)
            }
            Err(e) => Err(e),
            _ => Err(invalid("saved thresholds do not match active thresholds")),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::VecDeque;

    pub(crate) struct Script {
        exchanges: VecDeque<(String, String)>,
        incoming: VecDeque<u8>,
    }
    impl Script {
        pub(crate) fn new(exchanges: &[(&str, &str)]) -> Self {
            Self {
                exchanges: exchanges
                    .iter()
                    .map(|(a, b)| (a.to_string(), b.to_string()))
                    .collect(),
                incoming: VecDeque::new(),
            }
        }
    }
    impl Read for Script {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let Some(byte) = self.incoming.pop_front() else {
                std::thread::sleep(Duration::from_millis(1));
                return Err(io::ErrorKind::TimedOut.into());
            };
            bytes[0] = byte; // exercise fragmented serial responses
            Ok(1)
        }
    }
    impl Write for Script {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let (command, response) = self.exchanges.pop_front().expect("unexpected write");
            assert_eq!(bytes, command.as_bytes());
            self.incoming.extend(response.bytes());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn signed_values_and_strict_thresholds() {
        assert_eq!(
            parse_line(b"v -300 0 1023\r"),
            Some(Response::Values(vec![-300, 0, 1023]))
        );
        for line in [
            b"t -1".as_slice(),
            b"t 1024",
            b"t",
            b"t 20 garbage",
            b"version 20",
            b"s 32768",
        ] {
            assert_eq!(parse_line(line), None);
        }
        let line = format!("t {}", vec!["500"; MAX_SENSORS].join(" "));
        assert!(parse_line(line.as_bytes()).is_some());
        assert!(parse_line((line + " 500").as_bytes()).is_none());
    }

    #[test]
    fn overlong_line_does_not_become_a_valid_suffix() {
        let mut lines = Lines::default();
        for byte in vec![b'x'; MAX_LINE + 1]
            .into_iter()
            .chain(b"t 100\n".iter().copied())
        {
            assert_eq!(lines.push(byte), None);
        }
        let result = b"t 90\r\n"
            .iter()
            .filter_map(|&b| lines.push(b))
            .collect::<Vec<_>>();
        assert_eq!(result, vec![Response::Thresholds(vec![90])]);
    }

    #[test]
    fn modern_identification_edit_and_save_are_verified() {
        let script = Script::new(&[
            ("t\n", "booting\nt 500 600\r\n"),
            ("v\n", "v -5 100\n"),
            ("0 0500\n", "t 500 600\n"),
            ("1 1023\n", "t 500 1023\n"),
            ("s\n", "s 500 1023\n"),
        ]);
        let mut session = Session::connect(script, Protocol::Auto).unwrap();
        assert_eq!(session.protocol, Protocol::Modern);
        assert!(!session.set_threshold(0, 500).unwrap());
        assert!(session.set_threshold(1, 1023).unwrap());
        assert!(session.save().unwrap());
        assert!(session.transport.exchanges.is_empty());
    }

    #[test]
    fn explicit_legacy_preserves_the_original_wire_format() {
        let script = Script::new(&[
            ("t\n", "t 500\n"),
            ("v\n", "v 50\n"),
            ("01023\n", "t 1023\n"),
        ]);
        let mut session = Session::connect(script, Protocol::Legacy).unwrap();
        assert!(session.set_threshold(0, 1023).unwrap());
        assert!(threshold_command(Protocol::Legacy, 10, 100).is_err());
        assert!(threshold_command(Protocol::Auto, 0, 100).is_err());
        assert_eq!(
            threshold_command(Protocol::Modern, 16, 1023).unwrap(),
            "16 1023\n"
        );
        assert!(threshold_command(Protocol::Modern, 100, 1023).is_err());
    }

    #[test]
    fn rejects_mismatched_counts_and_unacknowledged_writes() {
        assert!(
            Session::connect(
                Script::new(&[("t\n", "t 100 200\n"), ("v\n", "v 5\n")]),
                Protocol::Modern
            )
            .is_err()
        );
        let script = Script::new(&[
            ("t\n", "t 100 200\n"),
            ("v\n", "v 5 6\n"),
            ("1 300\n", "t 0 200\n"),
        ]);
        let mut session = Session::connect(script, Protocol::Modern).unwrap();
        assert!(session.set_threshold(1, 300).is_err());
        assert_eq!(session.thresholds, [0, 200]);
    }

    #[test]
    fn unanswered_modern_probe_never_enables_legacy_writes() {
        let script = Script::new(&[
            ("t\n", "t 50\n"),
            ("v\n", "v 2\n"),
            ("0 0050\n", ""),
            ("t\n", "t 50\n"),
        ]);
        let mut session = Session::connect(script, Protocol::Auto).unwrap();
        assert_eq!(session.protocol, Protocol::Auto);
        assert!(session.set_threshold(0, 60).is_err());
        assert!(session.transport.exchanges.is_empty());
    }

    #[test]
    fn old_firmware_without_save_support_is_detected_once() {
        let script = Script::new(&[
            ("t\n", "t 50\n"),
            ("v\n", "v 2\n"),
            ("s\n", ""),
            ("v\n", "v 2\n"),
        ]);
        let mut session = Session::connect(script, Protocol::Modern).unwrap();
        assert!(!session.save().unwrap());
        assert!(!session.save().unwrap());
        assert_eq!(session.save_supported, Some(false));
        assert!(session.transport.exchanges.is_empty());
    }

    #[test]
    fn disconnect_during_write_is_not_success_and_never_saves() {
        let script = Script::new(&[("t\n", "t 50\n"), ("v\n", "v 2\n"), ("0 100\n", "")]);
        let mut session = Session::connect(script, Protocol::Modern).unwrap();
        assert!(session.set_threshold(0, 100).is_err());
        assert_eq!(session.thresholds, [50]);
        assert!(session.transport.exchanges.is_empty());
    }

    #[test]
    fn incorrect_save_ack_is_not_persistence_confirmation() {
        let script = Script::new(&[("t\n", "t 50\n"), ("v\n", "v 2\n"), ("s\n", "s 100\n")]);
        let mut session = Session::connect(script, Protocol::Modern).unwrap();
        assert!(session.save().is_err());
        assert_eq!(session.save_supported, None);
    }
}
