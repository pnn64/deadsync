use super::*;
use config::{DeviceConfig, LightMode};
use deadlib_input_native::PadBackend;
use deadlib_platform::{
    host_time,
    input::{PadCode, PadId},
};
use hidapi::{DeviceInfo, HidApi, HidDevice};
use protocol::{PID, VID, decode, light_packet};
use std::{
    collections::{BTreeMap, BTreeSet},
    thread,
};

const STALE: Duration = Duration::from_millis(250);
const DISCOVERY: Duration = Duration::from_secs(2);
const LIGHT_FRAME: Duration = Duration::from_millis(20);

struct Connection {
    device: Arc<Device>,
    reader: JoinHandle<()>,
    writer: Option<JoinHandle<()>>,
}

fn candidate(info: &DeviceInfo) -> bool {
    info.vendor_id() == VID
        && info.product_id() == PID
        && info
            .product_string()
            .is_some_and(|s| s.starts_with("RE:Flex"))
        && (info.usage_page() == 0 || (info.usage_page() == 0xff00 && info.usage() == 1))
}

pub(super) fn run(service: Arc<Service>) {
    let mut connections: Vec<Connection> = Vec::new();
    let mut next_scan = Instant::now();
    let mut next_token = 1usize;
    let mut persisted = service.registry.lock().unwrap().config.clone();
    let mut dirty_at = None;
    loop {
        let stopping = service.stop.load(Ordering::Acquire);
        // Collect settings even from a reader that just disconnected, before
        // removing it. Disk I/O never happens under a device/input mutex.
        let config = {
            let mut r = service.registry.lock().unwrap();
            for c in &connections {
                r.config.devices.insert(
                    c.device.serial.clone(),
                    c.device.core.lock().unwrap().config.clone(),
                );
            }
            r.config.clone()
        };
        let changed = config != persisted;
        if changed && dirty_at.is_none() {
            dirty_at = Some(Instant::now());
        }
        if changed
            && (stopping
                || dirty_at.is_some_and(|t: Instant| t.elapsed() >= Duration::from_millis(500)))
        {
            match config.save(&service.path) {
                Ok(()) => {
                    persisted = config;
                    dirty_at = None;
                    service.save_failed.store(false, Ordering::Relaxed);
                }
                Err(e) => {
                    service.save_failed.store(true, Ordering::Relaxed);
                    service.diagnostic(format!("RE:Flex settings could not be saved: {e}"));
                    dirty_at = Some(Instant::now());
                }
            }
        }
        let mut i = 0;
        while i < connections.len() {
            if stopping
                || !connections[i].device.alive.load(Ordering::Acquire)
                || connections[i].reader.is_finished()
            {
                let c = connections.remove(i);
                c.device.alive.store(false, Ordering::Release);
                if c.reader.join().is_err() {
                    service.diagnostic("RE:Flex input worker panicked".into());
                }
                if let Some(writer) = c.writer
                    && writer.join().is_err()
                {
                    service.diagnostic("RE:Flex light worker panicked".into());
                }
                let mut r = service.registry.lock().unwrap();
                r.config.devices.insert(
                    c.device.serial.clone(),
                    c.device.core.lock().unwrap().config.clone(),
                );
                r.devices.retain(|d| d.token != c.device.token);
            } else {
                i += 1;
            }
        }
        if stopping {
            let config = service.registry.lock().unwrap().config.clone();
            if config != persisted
                && let Err(e) = config.save(&service.path)
            {
                service.diagnostic(format!("RE:Flex final settings save failed: {e}"));
            }
            break;
        }
        if Instant::now() >= next_scan {
            next_scan = Instant::now() + DISCOVERY;
            match HidApi::new() {
                Err(e) => service.diagnostic(format!("RE:Flex HID discovery failed: {e}")),
                Ok(api) => {
                    let mut candidates: BTreeMap<String, Vec<&DeviceInfo>> = BTreeMap::new();
                    for info in api.device_list().filter(|i| candidate(i)) {
                        let Some(serial) = info.serial_number().filter(|s| !s.trim().is_empty())
                        else {
                            service.diagnostic(
                                "RE:Flex device has no serial; cannot safely associate calibration"
                                    .into(),
                            );
                            continue;
                        };
                        candidates.entry(serial.to_owned()).or_default().push(info);
                    }
                    for (serial, infos) in candidates {
                        if infos.len() != 1 {
                            let mut r = service.registry.lock().unwrap();
                            if !r.config.ambiguous_serials.contains(&serial) {
                                r.config.ambiguous_serials.push(serial.clone());
                            }
                            drop(r);
                            service.diagnostic(format!(
                                "RE:Flex duplicate USB serial {serial}; devices ignored"
                            ));
                            for c in &connections {
                                if c.device.serial == serial {
                                    c.device.alive.store(false, Ordering::Release);
                                }
                            }
                            continue;
                        }
                        if service
                            .registry
                            .lock()
                            .unwrap()
                            .config
                            .ambiguous_serials
                            .contains(&serial)
                        {
                            continue;
                        }
                        if connections.iter().any(|c| c.device.serial == serial) {
                            continue;
                        }
                        let settings = {
                            let mut r = service.registry.lock().unwrap();
                            if let Some(c) = r.config.devices.get(&serial) {
                                Some(c.clone())
                            } else {
                                let assigned: BTreeSet<_> = r
                                    .config
                                    .devices
                                    .values()
                                    .filter(|d| d.enabled)
                                    .map(|d| d.player)
                                    .collect();
                                (0..2).find(|p| !assigned.contains(p)).map(|player| {
                                    let c = DeviceConfig {
                                        player,
                                        ..Default::default()
                                    };
                                    r.config.devices.insert(serial.clone(), c.clone());
                                    c
                                })
                            }
                        };
                        let Some(settings) = settings else {
                            service.diagnostic(format!(
                                "RE:Flex {serial}: both player assignments are occupied; review {}",
                                service.path.display()
                            ));
                            continue;
                        };
                        if !settings.enabled {
                            continue;
                        }
                        if connections
                            .iter()
                            .any(|c| c.device.player == settings.player)
                        {
                            service.diagnostic(format!(
                                "RE:Flex {serial}: P{} is already occupied",
                                settings.player + 1
                            ));
                            continue;
                        }
                        match connect(&api, infos[0], serial, settings, next_token, &service) {
                            Ok(c) => {
                                next_token += 1;
                                connections.push(c);
                            }
                            Err(e) => service.diagnostic(e),
                        }
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn connect(
    api: &HidApi,
    info: &DeviceInfo,
    serial: String,
    settings: DeviceConfig,
    token: usize,
    service: &Arc<Service>,
) -> Result<Connection, String> {
    let input = info
        .open_device(api)
        .map_err(|e| format!("RE:Flex {serial}: cannot open HID device: {e}"))?;
    let mut bytes = [0; 64];
    let start = Instant::now();
    let first = loop {
        if start.elapsed() > Duration::from_millis(500) || service.stop.load(Ordering::Acquire) {
            return Err(format!("RE:Flex {serial}: no valid v2 sensor report"));
        }
        let n = input
            .read_timeout(&mut bytes, 20)
            .map_err(|e| format!("RE:Flex {serial}: {e}"))?;
        if n == 0 {
            continue;
        }
        break decode(&bytes[..n])
            .ok_or_else(|| format!("RE:Flex {serial}: invalid v2 sensor report"))?;
    };
    let player = settings.player;
    let mut core = Detector::new(settings);
    core.values = first;
    let d = Arc::new(Device {
        token,
        uuid: uuid_from_bytes(format!("reflex:{serial}").as_bytes()),
        name: format!("RE:Flex P{} ({serial})", player + 1).into(),
        serial,
        player,
        core: Mutex::new(core),
        alive: AtomicBool::new(true),
        test_until: Mutex::new(None),
        light_error: AtomicBool::new(false),
    });
    // Separate handles/threads keep slow light writes off the input read path.
    // macOS uses HIDAPI's shared-device mode for these two endpoint owners.
    let output = info
        .open_device(api)
        .map_err(|e| {
            d.light_error.store(true, Ordering::Relaxed);
            service.diagnostic(format!("RE:Flex {} lights unavailable: {e}", d.serial));
        })
        .ok();
    service.registry.lock().unwrap().devices.push(d.clone());
    system_event(service, &d, true);
    let rd = d.clone();
    let rs = service.clone();
    let reader = thread::Builder::new()
        .name(format!("reflex-input-p{}", player + 1))
        .spawn(move || read_loop(input, rd, rs))
        .map_err(|e| {
            d.alive.store(false, Ordering::Release);
            service
                .registry
                .lock()
                .unwrap()
                .devices
                .retain(|v| v.token != token);
            system_event(service, &d, false);
            format!("RE:Flex input thread: {e}")
        })?;
    let writer = output.and_then(|output| {
        let wd = d.clone();
        let ws = service.clone();
        thread::Builder::new()
            .name(format!("reflex-lights-p{}", player + 1))
            .spawn(move || write_loop(output, wd, ws))
            .map_err(|e| {
                d.light_error.store(true, Ordering::Relaxed);
                service.diagnostic(format!("RE:Flex lights thread: {e}"));
            })
            .ok()
    });
    Ok(Connection {
        device: d,
        reader,
        writer,
    })
}
fn system_event(s: &Service, d: &Device, connected: bool) {
    let id = PadId((REFLEX_PAD_ID_BASE + d.player) as u32);
    let event = if connected {
        GpSystemEvent::Connected {
            name: d.name.to_string(),
            id,
            vendor_id: Some(VID),
            product_id: Some(PID),
            backend: PadBackend::Reflex,
            initial: false,
        }
    } else {
        GpSystemEvent::Disconnected {
            name: d.name.to_string(),
            id,
            backend: PadBackend::Reflex,
            initial: false,
        }
    };
    (s.system)(event);
}
fn emit(s: &Service, d: &Device, previous: u8, current: u8, now: Instant, host_nanos: u64) {
    for panel in 0..4 {
        if (previous ^ current) & (1 << panel) == 0 {
            continue;
        }
        let pressed = current & (1 << panel) != 0;
        (s.input)(PadEvent::RawButton {
            id: PadId((REFLEX_PAD_ID_BASE + d.player) as u32),
            timestamp: now,
            host_nanos,
            code: PadCode(BUTTON_CODE_BASE + panel),
            uuid: d.uuid,
            value: if pressed { 1.0 } else { 0.0 },
            pressed,
        });
    }
}
fn read_loop(input: HidDevice, d: Arc<Device>, s: Arc<Service>) {
    read_reports(
        |buffer| input.read_timeout(buffer, 10).map_err(|e| e.to_string()),
        d,
        s,
        || (Instant::now(), host_time::now_nanos()),
    );
}
fn read_reports(
    mut read: impl FnMut(&mut [u8]) -> Result<usize, String>,
    d: Arc<Device>,
    s: Arc<Service>,
    mut clock: impl FnMut() -> (Instant, u64),
) {
    let mut buffer = [0; 64];
    let mut previous = 0;
    let mut last_valid = clock().0;
    while d.alive.load(Ordering::Acquire) && !s.stop.load(Ordering::Acquire) {
        let report = read(&mut buffer);
        // No firmware timestamp exists. Capture host time immediately after
        // receipt, before locks, threshold work, or callbacks.
        let (now, host) = clock();
        match report {
            Ok(n) if n > 0 => {
                if let Some(values) = buffer.get(..n).and_then(decode) {
                    last_valid = now;
                    let current = d.core.lock().unwrap().sample(values, now);
                    emit(&s, &d, previous, current, now, host);
                    previous = current;
                }
            }
            Ok(_) => {}
            Err(e) => {
                s.diagnostic(format!("RE:Flex {} read failed: {e}", d.serial));
                break;
            }
        }
        if now.duration_since(last_valid) >= STALE {
            s.diagnostic(format!("RE:Flex {} sensor stream timed out", d.serial));
            break;
        }
    }
    let (now, host) = clock();
    emit(&s, &d, previous, 0, now, host);
    d.core.lock().unwrap().release_all();
    d.alive.store(false, Ordering::Release);
    system_event(&s, &d, false);
}

fn colors(d: &Device, s: &Service, now: Instant) -> [[u8; 3]; 4] {
    let c = d.core.lock().unwrap();
    let testing = d
        .test_until
        .lock()
        .unwrap()
        .is_some_and(|until| now < until);
    let (frames, updated) = *s.lights.lock().unwrap();
    let fresh = now.duration_since(updated) < Duration::from_millis(500);
    let frame = frames[d.player];
    let mut colors = std::array::from_fn(|i| {
        if testing {
            return [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]][i];
        }
        if fresh && frame.testing {
            return if frame.chart[i] { [255; 3] } else { [0; 3] };
        }
        match c.config.lights {
            LightMode::Off => [0; 3],
            LightMode::Input => {
                if c.panels & (1 << i) != 0 {
                    [64, 128, 255]
                } else {
                    [0; 3]
                }
            }
            LightMode::Chart => {
                if fresh && frame.chart[i] {
                    [64, 128, 255]
                } else {
                    [0; 3]
                }
            }
            LightMode::Judgment => {
                if fresh {
                    frame.judgment[i]
                } else {
                    [0; 3]
                }
            }
        }
    });
    for panel in &mut colors {
        for channel in panel {
            *channel = (u16::from(*channel) * u16::from(c.config.brightness) / 100) as u8;
        }
    }
    colors
}
fn send_frame(
    write: &mut impl FnMut(&[u8]) -> Result<usize, String>,
    colors: &[[u8; 3]; 4],
    frame: u8,
) -> Result<(), String> {
    for segment in 0..16 {
        let count = write(&light_packet(colors, segment, frame))?;
        // The bundled Windows-native implementation returns zero for a
        // synchronously completed WriteFile; asynchronous writes return bytes.
        if count != 65 && !(cfg!(windows) && count == 0) {
            return Err(format!("short light write: {count}/65"));
        }
    }
    Ok(())
}
fn write_loop(output: HidDevice, d: Arc<Device>, s: Arc<Service>) {
    write_frames(|bytes| output.write(bytes).map_err(|e| e.to_string()), d, s);
}
fn write_frames(
    mut write: impl FnMut(&[u8]) -> Result<usize, String>,
    d: Arc<Device>,
    s: Arc<Service>,
) {
    let mut frame = 0u8;
    let mut previous = None;
    while d.alive.load(Ordering::Acquire) && !s.stop.load(Ordering::Acquire) {
        let start = Instant::now();
        let colors = colors(&d, &s, start);
        if previous != Some(colors) {
            if let Err(e) = send_frame(&mut write, &colors, frame) {
                d.light_error.store(true, Ordering::Relaxed);
                s.diagnostic(format!("RE:Flex {} lights failed: {e}", d.serial));
                return;
            }
            previous = Some(colors);
            frame = frame.wrapping_add(1) & 15;
        }
        thread::sleep(LIGHT_FRAME.saturating_sub(start.elapsed()));
    }
    // Send a complete black frame on a clean close. Failure is expected after
    // unplugging; stop after the first failed write rather than retrying forever.
    let _ = send_frame(&mut write, &[[0; 3]; 4], frame);
}

#[cfg(test)]
mod tests;
