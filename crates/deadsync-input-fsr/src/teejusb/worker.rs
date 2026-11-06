use super::Exchange;
use super::config::{Config, DeviceConfig};
use super::protocol::{MAX_THRESHOLD, Protocol, Session};
use deadsync_input::fsr::{
    BackendKind, ButtonLabel, ButtonView, PadDeviceId, PadView, SensorView, ValueCurve,
};
use serialport::{SerialPort, SerialPortInfo, SerialPortType};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SCAN_INTERVAL: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(33);
const SAVE_DELAY: Duration = Duration::from_secs(2);
const HEARTBEAT: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct Candidate {
    id: String,
    port: String,
    name: String,
}

struct Device<T = Box<dyn SerialPort>> {
    id: usize,
    candidate: Candidate,
    config_index: usize,
    session: Session<T>,
    groups: Vec<Vec<usize>>,
    configured_groups: usize,
    name: Arc<str>,
    held: Vec<bool>,
    last_poll: Instant,
    dirty: Option<Instant>,
    save_attempts: u8,
}

impl<T> Device<T> {
    fn save_due(&self, now: Instant, flush: bool) -> bool {
        self.dirty.is_some_and(|edited| {
            (flush || now.saturating_duration_since(edited) >= SAVE_DELAY)
                && (flush || self.save_attempts < 3)
        })
    }
}

fn usb_identity(info: &SerialPortInfo) -> Option<String> {
    let SerialPortType::UsbPort(usb) = &info.port_type else {
        return None;
    };
    let serial = usb.serial_number.as_deref()?.trim();
    if serial.is_empty() {
        return None;
    }
    Some(format!("usb:{:04x}:{:04x}:{serial}", usb.vid, usb.pid))
}

fn identity(info: &SerialPortInfo, serial_counts: &HashMap<String, usize>) -> String {
    if let Some(id) = usb_identity(info)
        && serial_counts.get(&id) == Some(&1)
    {
        return id;
    }
    // Never share settings between indistinguishable boards. Port association is
    // intentionally explicit when the device supplies no unique USB serial.
    format!("port:{}", info.port_name)
}

fn auto_candidate(info: &SerialPortInfo) -> bool {
    let SerialPortType::UsbPort(usb) = &info.port_type else {
        return false;
    };
    if matches!(usb.vid, 0x16c0 | 0x2341 | 0x2a03 | 0x1b4f | 0x2e8a | 0x239a) {
        return true;
    }
    let description = format!(
        "{} {}",
        usb.manufacturer.as_deref().unwrap_or(""),
        usb.product.as_deref().unwrap_or("")
    )
    .to_ascii_lowercase();
    [
        "teejus",
        "teensy",
        "arduino",
        "pro micro",
        "rp2040",
        "dance",
        "fsr",
    ]
    .iter()
    .any(|word| description.contains(word))
}

fn settings_index(config: &Config, candidate: &Candidate) -> Option<usize> {
    config
        .devices
        .iter()
        .position(|entry| entry.id.as_deref() == Some(&candidate.id))
        .or_else(|| {
            config.devices.iter().position(|entry| {
                entry.id.is_none() && entry.port.as_deref() == Some(&candidate.port)
            })
        })
}

fn candidates(config: &mut Config, active: bool) -> VecDeque<Candidate> {
    let ports = serialport::available_ports().unwrap_or_else(|error| {
        log::debug!("Teejusb FSR serial enumeration: {error}");
        Vec::new()
    });
    let mut serial_counts = HashMap::new();
    for port in &ports {
        if let Some(id) = usb_identity(port) {
            *serial_counts.entry(id).or_insert(0) += 1;
        }
    }
    for (id, &count) in &serial_counts {
        if count > 1 && !config.ambiguous_serials.contains(id) {
            config.ambiguous_serials.push(id.clone());
        }
    }
    for id in &config.ambiguous_serials {
        serial_counts.insert(id.clone(), 2);
    }
    let mut result = VecDeque::new();
    for info in &ports {
        let candidate = Candidate {
            id: identity(info, &serial_counts),
            port: info.port_name.clone(),
            name: match &info.port_type {
                SerialPortType::UsbPort(usb) => usb
                    .product
                    .clone()
                    .unwrap_or_else(|| info.port_name.clone()),
                _ => info.port_name.clone(),
            },
        };
        if settings_index(config, &candidate).is_some()
            || (active && config.auto_detect && auto_candidate(info))
        {
            result.push_back(candidate);
        }
    }
    // Native enumeration is not available on every OS/adapter. Explicit paths
    // work regardless of VID/PID, product name, or enumeration support.
    for entry in &config.devices {
        let Some(port) = &entry.port else {
            continue;
        };
        if !result.iter().any(|c| &c.port == port)
            && !ports.iter().any(|p| &p.port_name == port)
            && entry.id.as_ref().is_none_or(|id| id.starts_with("port:"))
        {
            result.push_back(Candidate {
                id: format!("port:{port}"),
                port: port.clone(),
                name: port.clone(),
            });
        }
    }
    result
}

fn open(
    candidate: &Candidate,
    settings: &DeviceConfig,
) -> Result<Session<Box<dyn SerialPort>>, String> {
    let port = serialport::new(&candidate.port, 115_200)
        .timeout(Duration::from_millis(20))
        .dtr_on_open(true) // native USB CDC implementations may require DTR
        .open()
        .map_err(|e| e.to_string())?;
    let _ = port.clear(serialport::ClearBuffer::Input);
    Session::connect(port, settings.protocol).map_err(|e| e.to_string())
}

fn restore<T: std::io::Read + std::io::Write>(
    session: &mut Session<T>,
    settings: &DeviceConfig,
) -> Result<bool, String> {
    if settings.thresholds.is_empty() || session.protocol == Protocol::Auto {
        return Ok(false);
    }
    if settings.thresholds.len() != session.thresholds.len() {
        return Err(
            "saved sensor count differs from firmware; saved thresholds were not applied".into(),
        );
    }
    // Validate every command before changing anything on the board.
    for (index, &value) in settings.thresholds.iter().enumerate() {
        super::protocol::threshold_command(session.protocol, index, value)
            .map_err(|e| e.to_string())?;
    }
    let mut changed = false;
    for (index, &value) in settings.thresholds.iter().enumerate() {
        changed |= session
            .set_threshold(index, value)
            .map_err(|e| e.to_string())?;
    }
    Ok(changed)
}

fn pad_view<T>(device: &mut Device<T>) -> PadView {
    let curve = ValueCurve::linear(MAX_THRESHOLD);
    for (index, &reading) in device.session.values.iter().enumerate() {
        let threshold = i32::from(device.session.thresholds[index]);
        // Upstream's fixed one-unit hysteresis. These are display indicators;
        // the actual HID/keyboard events remain authoritative for gameplay.
        if i32::from(reading) > threshold {
            device.held[index] = true;
        } else if i32::from(reading) < threshold - 1 {
            device.held[index] = false;
        }
    }
    let buttons = device
        .groups
        .iter()
        .enumerate()
        .map(|(group_index, group)| {
            let sensors = group
                .iter()
                .map(|&index| {
                    let raw_value =
                        device.session.values[index].clamp(0, MAX_THRESHOLD as i16) as u16;
                    let raw_threshold = device.session.thresholds[index];
                    SensorView {
                        firmware_index: index,
                        label: None,
                        raw_value,
                        raw_threshold,
                        value_norm: curve.normalize(raw_value),
                        threshold_norm: curve.normalize(raw_threshold),
                        active: device.held[index],
                        enabled: true,
                    }
                })
                .collect();
            ButtonView {
                label: if group_index < device.configured_groups {
                    ButtonLabel::Group((group_index + 1) as u16)
                } else {
                    ButtonLabel::Sensor((group[0] + 1) as u16)
                },
                aggregate_value: group
                    .iter()
                    .map(|&i| device.session.values[i].clamp(0, 1023) as u16)
                    .max()
                    .unwrap_or(0),
                aggregate_threshold: group
                    .iter()
                    .map(|&i| device.session.thresholds[i])
                    .max()
                    .unwrap_or(0),
                active: group.iter().any(|&i| device.held[i]),
                sensors,
                min_raw_threshold: 0,
                max_raw_threshold: if group.iter().any(|&i| i >= 100) {
                    999
                } else {
                    MAX_THRESHOLD
                },
                value_curve: curve,
                release_threshold: None,
            }
        })
        .collect();
    let editable = device.session.protocol != Protocol::Auto;
    PadView {
        device_id: PadDeviceId {
            backend: BackendKind::Teejusb,
            index: device.id,
        },
        device_name: Arc::clone(&device.name),
        thresholds_editable: editable,
        status: (!editable)
            .then(|| Arc::from("Firmware not identified; thresholds are read-only.")),
        is_p2_side: false,
        buttons,
        supports_advanced: true,
        simple_per_sensor_bars: false,
        supports_sensor_toggle: false,
        auto_recalibration: None,
        debounce_micros: None,
    }
}

pub(super) fn run(exchange: Arc<Exchange>, path: Option<PathBuf>) {
    let mut config = match Config::load(path.as_deref()) {
        Ok(config) => config,
        Err(error) => {
            exchange.diagnostic(error);
            return;
        } // never overwrite an invalid settings file
    };
    let mut devices: Vec<Device> = Vec::new();
    let mut queue = VecDeque::new();
    let mut last_scan = Instant::now() - SCAN_INTERVAL;
    let mut next_id = 0usize;
    let mut host_dirty = None;
    let mut was_active = false;
    loop {
        let (enabled, active, stop, flush, pending) = {
            let mut state = exchange.lock();
            (
                state.enabled,
                state.active,
                state.stop,
                std::mem::take(&mut state.flush),
                state
                    .pending
                    .iter()
                    .map(|(&key, &value)| (key, value))
                    .collect::<Vec<_>>(),
            )
        };
        if active && !was_active {
            last_scan = Instant::now() - SCAN_INTERVAL;
        }
        was_active = active;
        let mut disconnected = Vec::new();
        for device in &mut devices {
            let mut failed = apply_edits(device, &mut config, &exchange, &pending, &mut host_dirty);
            if !failed && enabled && !stop && active {
                if let Err(error) = device.session.poll_values() {
                    log::debug!(
                        "Teejusb FSR {} disconnected: {error}",
                        device.candidate.port
                    );
                    failed = true;
                }
                device.last_poll = Instant::now();
            } else if !failed && device.last_poll.elapsed() >= HEARTBEAT {
                // Check the OS handle without sending telemetry requests during
                // gameplay. Serial printing runs in the firmware's input loop.
                failed = device.session.transport().bytes_to_read().is_err();
                device.last_poll = Instant::now();
            }
            if !failed && device.save_due(Instant::now(), flush || stop || !enabled) {
                device.save_attempts += 1;
                if !config.devices[device.config_index].save_to_board {
                    device.dirty = None;
                } else {
                    match device.session.save() {
                        Ok(_) => device.dirty = None, // host copy also covers boards without EEPROM support
                        Err(error) => {
                            exchange.diagnostic(format!(
                                "{}: board save failed; keeping computer settings: {error}",
                                device.candidate.port
                            ));
                            device.dirty = Some(Instant::now());
                        }
                    }
                }
            }
            if failed || !enabled {
                disconnected.push(device.id);
            }
        }
        if !disconnected.is_empty() {
            devices.retain(|d| !disconnected.contains(&d.id));
            exchange
                .lock()
                .pending
                .retain(|(id, _), _| !disconnected.contains(id));
        }
        if host_dirty.is_some_and(|edited: Instant| edited.elapsed() >= SAVE_DELAY)
            || (host_dirty.is_some() && (flush || stop || !enabled))
        {
            match config.save(path.as_deref()) {
                Ok(()) => host_dirty = None,
                Err(error) => {
                    exchange.diagnostic(format!("could not save computer settings: {error}"));
                    host_dirty = Some(Instant::now());
                }
            }
        }
        if stop {
            break;
        }
        if enabled {
            if last_scan.elapsed() >= SCAN_INTERVAL {
                let ambiguous_before = config.ambiguous_serials.len();
                queue = candidates(&mut config, active);
                if config.ambiguous_serials.len() != ambiguous_before {
                    host_dirty.get_or_insert_with(Instant::now);
                    exchange.diagnostic("Duplicate USB serial detected; using separate port settings. Reopen the editor to identify each board.".into());
                }
                // Re-identify live connections when a second board reveals a
                // formerly unique serial to be ambiguous. Never reuse that old
                // identity's saved thresholds for either of the two boards.
                devices.retain(|device| !config.ambiguous_serials.contains(&device.candidate.id));
                last_scan = Instant::now();
            }
            // At most one connection attempt per iteration; already connected
            // boards are serviced between probes. None of this runs on the UI.
            if let Some(candidate) = queue.pop_front()
                && !devices.iter().any(|d| d.candidate.port == candidate.port)
            {
                let existing = settings_index(&config, &candidate);
                let settings = existing
                    .map(|i| config.devices[i].clone())
                    .unwrap_or_else(|| {
                        DeviceConfig::discovered(candidate.id.clone(), candidate.port.clone())
                    });
                match open(&candidate, &settings) {
                    Ok(mut session) => {
                        match settings.groups_for(session.thresholds.len()) {
                            Ok(groups) => {
                                let restored = match restore(&mut session, &settings) {
                                    Ok(changed) => changed,
                                    Err(error) => {
                                        exchange.diagnostic(format!("{}: {error}", candidate.port));
                                        // Count changes are visible and editable, but never replay
                                        // the old array against a new firmware layout.
                                        if settings.thresholds.len() == session.thresholds.len() {
                                            continue;
                                        }
                                        false
                                    }
                                };
                                let config_index = existing.unwrap_or_else(|| {
                                    config.devices.push(settings.clone());
                                    config.devices.len() - 1
                                });
                                let entry = &mut config.devices[config_index];
                                entry.id = Some(candidate.id.clone());
                                entry.port = Some(candidate.port.clone());
                                host_dirty.get_or_insert_with(Instant::now);
                                let name = format!(
                                    "{} ({})",
                                    settings.name.as_deref().unwrap_or(&candidate.name),
                                    candidate.port
                                )
                                .into();
                                let count = session.thresholds.len();
                                devices.push(Device {
                                    id: next_id,
                                    candidate,
                                    config_index,
                                    session,
                                    groups,
                                    configured_groups: settings.groups.len(),
                                    name,
                                    held: vec![false; count],
                                    last_poll: Instant::now(),
                                    dirty: restored.then(Instant::now),
                                    save_attempts: 0,
                                });
                                next_id += 1; // old queued edits can never target a reconnected board
                            }
                            Err(error) => {
                                exchange.diagnostic(format!("{}: {error}", candidate.port))
                            }
                        }
                    }
                    Err(error) => {
                        if existing.is_some() {
                            exchange.diagnostic(format!("{}: {error}", candidate.port));
                        } else {
                            log::debug!("Teejusb FSR probe {}: {error}", candidate.port);
                        }
                    }
                }
            }
        } else {
            queue.clear();
            last_scan = Instant::now() - SCAN_INTERVAL;
        }
        {
            let mut shared = exchange.lock();
            shared.pads = devices.iter_mut().map(pad_view).collect();
            shared
                .pending
                .retain(|(id, _), _| devices.iter().any(|d| d.id == *id));
        }
        let state = exchange.lock();
        if !state.stop {
            let _ = exchange.wake.wait_timeout(state, POLL_INTERVAL);
        }
    }
}

fn apply_edits<T: std::io::Read + std::io::Write>(
    device: &mut Device<T>,
    config: &mut Config,
    exchange: &Exchange,
    pending: &[((usize, usize), u16)],
    host_dirty: &mut Option<Instant>,
) -> bool {
    for &((id, index), value) in pending.iter().filter(|((id, _), _)| *id == device.id) {
        let result = device.session.set_threshold(index, value);
        match &result {
            Ok(true) => {
                let now = Instant::now();
                config.devices[device.config_index]
                    .thresholds
                    .clone_from(&device.session.thresholds);
                device.dirty = Some(now);
                device.save_attempts = 0;
                *host_dirty = Some(now);
            }
            Ok(false) => {}
            Err(error) => exchange.diagnostic(format!(
                "{}: threshold edit failed: {error}",
                device.candidate.port
            )),
        }
        let mut shared = exchange.lock();
        // Publish the readback before removing the optimistic edit. Otherwise a
        // slow discovery probe later in this iteration could expose the old UI
        // threshold and make the next held-key repeat start from that old value.
        if let Some(pad) = shared.pads.iter_mut().find(|pad| pad.device_id.index == id) {
            for button in &mut pad.buttons {
                for sensor in &mut button.sensors {
                    if let Some(&threshold) = device.session.thresholds.get(sensor.firmware_index) {
                        sensor.raw_threshold = threshold;
                        sensor.threshold_norm = button.value_curve.normalize(threshold);
                    }
                }
                button.aggregate_threshold = button
                    .sensors
                    .iter()
                    .map(|sensor| sensor.raw_threshold)
                    .max()
                    .unwrap_or(0);
            }
        }
        // A new key repeat may have arrived while waiting for the acknowledgement.
        if shared.pending.get(&(id, index)) == Some(&value) {
            shared.pending.remove(&(id, index));
        }
        if result.is_err() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::protocol::tests::Script;
    use super::*;

    fn test_device(count: usize, commands: &[(&str, &str)]) -> Device<Script> {
        let thresholds = format!("t {}\n", vec!["500"; count].join(" "));
        let values = format!("v {}\n", vec!["0"; count].join(" "));
        let mut exchanges = vec![("t\n", thresholds.as_str()), ("v\n", values.as_str())];
        exchanges.extend_from_slice(commands);
        Device {
            id: 7,
            candidate: Candidate {
                id: "usb:test".into(),
                port: "test".into(),
                name: "Test".into(),
            },
            config_index: 0,
            session: Session::connect(Script::new(&exchanges), Protocol::Modern).unwrap(),
            groups: (0..count).map(|i| vec![i]).collect(),
            configured_groups: 0,
            name: "Test".into(),
            held: vec![false; count],
            last_poll: Instant::now(),
            dirty: None,
            save_attempts: 0,
        }
    }

    #[test]
    fn variable_layouts_keep_all_sensors_and_group_activity_uses_any_sensor() {
        for count in [1, 4, 5, 8, 16, 32, 255] {
            let mut device = test_device(count, &[]);
            let pad = pad_view(&mut device);
            assert_eq!(pad.buttons.len(), count);
            assert_eq!(pad.buttons[count - 1].sensors[0].firmware_index, count - 1);
            assert_eq!(pad.buttons[0].max_raw_threshold, 1023);
        }
        let mut device = test_device(32, &[]);
        device.groups = vec![(0..32).rev().collect()];
        device.configured_groups = 1;
        device.session.thresholds[5] = 100;
        device.session.values[5] = 101;
        device.session.values[0] = -20;
        let pad = pad_view(&mut device);
        assert_eq!(pad.buttons[0].sensors.len(), 32);
        assert_eq!(pad.buttons[0].sensors[0].firmware_index, 31);
        assert!(pad.buttons[0].active); // value 101 < maximum group threshold 500
        assert_eq!(pad.buttons[0].sensors[31].raw_value, 0); // negative values never wrap
        device.session.values[5] = 99;
        assert!(pad_view(&mut device).buttons[0].active); // still within release hysteresis
        device.session.values[5] = 98;
        assert!(!pad_view(&mut device).buttons[0].active);
    }

    #[test]
    fn only_confirmed_edits_are_saved_and_newer_pending_values_survive_ack() {
        let mut device = test_device(2, &[("0 100\n", "t 100 500\n"), ("1 200\n", "t 100 500\n")]);
        let mut config = Config::default();
        config
            .devices
            .push(DeviceConfig::discovered("usb:test".into(), "test".into()));
        let exchange = Exchange::default();
        exchange.lock().pads.push(pad_view(&mut device));
        exchange.lock().pending.insert((7, 0), 110); // arrived after this batch was taken
        exchange.lock().pending.insert((7, 1), 200);
        let mut dirty = None;
        assert!(apply_edits(
            &mut device,
            &mut config,
            &exchange,
            &[((7, 0), 100), ((7, 1), 200)],
            &mut dirty
        ));
        assert_eq!(config.devices[0].thresholds, [100, 500]);
        assert_eq!(
            exchange.lock().pads[0].buttons[0].sensors[0].raw_threshold,
            100
        );
        assert!(dirty.is_some());
        assert_eq!(exchange.lock().pending.get(&(7, 0)), Some(&110));
        assert!(!exchange.lock().pending.contains_key(&(7, 1)));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("teejusb.json");
        config.save(Some(&path)).unwrap();
        let saved = Config::load(Some(&path)).unwrap();
        let mut reconnected = test_device(2, &[("0 100\n", "t 100 500\n")]);
        assert!(restore(&mut reconnected.session, &saved.devices[0]).unwrap());
        assert_eq!(reconnected.session.thresholds, [100, 500]);
    }

    #[test]
    fn changed_sensor_count_never_replays_a_partial_saved_array() {
        let mut device = test_device(5, &[]);
        let settings = DeviceConfig {
            thresholds: vec![10, 20, 30, 40],
            ..DeviceConfig::default()
        };
        assert!(restore(&mut device.session, &settings).is_err());
        assert_eq!(device.session.thresholds, [500; 5]);
    }

    #[test]
    fn saves_wait_for_idle_bound_retries_and_flush_on_exit() {
        let mut device = test_device(4, &[]);
        let now = Instant::now();
        assert!(!device.save_due(now, true));
        device.dirty = Some(now);
        assert!(!device.save_due(now + Duration::from_millis(1999), false));
        assert!(device.save_due(now + SAVE_DELAY, false));
        assert!(device.save_due(now, true));
        device.save_attempts = 3;
        assert!(!device.save_due(now + SAVE_DELAY, false));
        assert!(device.save_due(now, true));
    }

    #[test]
    fn no_op_edits_do_not_postpone_saving_or_replace_computer_settings() {
        let mut device = test_device(4, &[]);
        let edited = Instant::now() - SAVE_DELAY;
        device.dirty = Some(edited);
        let mut config = Config::default();
        config.devices.push(DeviceConfig::default());
        let mut host_dirty = None;
        assert!(!apply_edits(
            &mut device,
            &mut config,
            &Exchange::default(),
            &[((7, 0), 500)],
            &mut host_dirty
        ));
        assert_eq!(device.dirty, Some(edited));
        assert!(device.save_due(Instant::now(), false));
        assert!(host_dirty.is_none());
        assert!(config.devices[0].thresholds.is_empty());
    }

    #[test]
    fn stale_connection_commands_are_not_applied_to_new_connection() {
        let mut device = test_device(4, &[]);
        let mut config = Config::default();
        config.devices.push(DeviceConfig::default());
        let mut dirty = None;
        assert!(!apply_edits(
            &mut device,
            &mut config,
            &Exchange::default(),
            &[((6, 0), 100)],
            &mut dirty
        ));
        assert!(dirty.is_none());
        assert!(config.devices[0].thresholds.is_empty());
    }
    fn usb(port: &str, serial: Option<&str>) -> SerialPortInfo {
        SerialPortInfo {
            port_name: port.into(),
            port_type: SerialPortType::UsbPort(serialport::UsbPortInfo {
                vid: 0x16c0,
                pid: 0x0486,
                serial_number: serial.map(str::to_owned),
                manufacturer: None,
                product: None,
            }),
        }
    }

    #[test]
    fn unique_usb_serial_survives_port_renumbering_but_duplicates_do_not_alias() {
        let first = usb("COM3", Some("123"));
        let second = usb("COM8", Some("123"));
        let id = usb_identity(&first).unwrap();
        let mut counts = HashMap::from([(id.clone(), 1)]);
        assert_eq!(identity(&first, &counts), identity(&second, &counts));
        counts.insert(id, 2);
        assert_ne!(identity(&first, &counts), identity(&second, &counts));
        assert_eq!(identity(&usb("COM3", None), &counts), "port:COM3");
    }

    #[test]
    fn settings_do_not_follow_reused_port_to_a_different_usb_device() {
        let mut config = Config::default();
        config
            .devices
            .push(DeviceConfig::discovered("usb:old".into(), "COM3".into()));
        let mut candidate = Candidate {
            id: "usb:new".into(),
            port: "COM3".into(),
            name: "pad".into(),
        };
        assert_eq!(settings_index(&config, &candidate), None);
        candidate.id = "usb:old".into();
        candidate.port = "COM9".into();
        assert_eq!(settings_index(&config, &candidate), Some(0));
    }
}
