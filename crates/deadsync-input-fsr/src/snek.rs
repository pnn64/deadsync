use super::{
    SNEK_PANEL_DOWN, SNEK_PANEL_LEFT, SNEK_PANEL_RIGHT, SNEK_PANEL_UP, SnekSensorSnapshot,
};
use hidapi::{HidApi, HidDevice};
use log::{debug, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const VENDOR_ID: u16 = 0x2e8a;
const PRODUCT_ID: u16 = 0x10a8;
const CONFIG_INTERFACE: i32 = 1;
const PACKET_SIZE: usize = 64;
const OUTPUT_REPORT: u8 = 0x77;
const INPUT_REPORT: u8 = 0x78;
const GET_INFO: u8 = 0x01;
const SET_EXTIO_SENSOR: u8 = 0x08;
const DISCONNECT: u8 = 0x0a;
const ERROR: u8 = 0xff;
const READ_TIMEOUT_MS: i32 = 100;
const RETRY_DELAY: Duration = Duration::from_millis(750);

#[derive(Default)]
pub(super) struct Monitor {
    stop: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
    snapshot: Arc<Mutex<SnekSensorSnapshot>>,
    shutdown_sent: bool,
}

impl Monitor {
    pub(super) fn set_active(&mut self, active: bool) {
        if self.shutdown_sent {
            return;
        }
        if active == self.worker.is_some() {
            return;
        }
        if active {
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let snapshot = Arc::clone(&self.snapshot);
            match thread::Builder::new()
                .name("snek-sensor-test".into())
                .spawn(move || worker_main(&worker_stop, &snapshot))
            {
                Ok(worker) => {
                    self.stop = Some(stop);
                    self.worker = Some(worker);
                }
                Err(error) => warn!("Could not start Snek sensor worker: {error}"),
            }
        } else {
            self.stop_worker();
        }
    }

    pub(super) fn snapshot(&self) -> SnekSensorSnapshot {
        *self.snapshot.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn shutdown(&mut self) {
        self.stop_worker();
        if self.shutdown_sent {
            return;
        }
        self.shutdown_sent = true;
        send_disconnect();
    }

    fn stop_worker(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Release);
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            warn!("Snek sensor worker panicked");
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_main(stop: &AtomicBool, shared: &Mutex<SnekSensorSnapshot>) {
    while !stop.load(Ordering::Acquire) {
        match HidApi::new().and_then(|mut api| {
            api.refresh_devices()?;
            let info = api
                .device_list()
                .find(|info| {
                    info.vendor_id() == VENDOR_ID
                        && info.product_id() == PRODUCT_ID
                        && info.interface_number() == CONFIG_INTERFACE
                })
                .ok_or_else(|| hidapi::HidError::HidApiError {
                    message: "Snekboard config interface not found".into(),
                })?;
            info.open_device(&api)
        }) {
            Ok(device) => poll_device(&device, stop, shared),
            Err(error) => {
                debug!("Snek sensor diagnostics unavailable: {error}");
                // Keep a prior read error visible while reconnecting. On the
                // first discovery attempt the snapshot is already unavailable.
            }
        }
        wait_for_retry(stop);
    }
    publish_connection(shared, false, false);
}

fn wait_for_retry(stop: &AtomicBool) {
    let slices = RETRY_DELAY.as_millis() / 25;
    for _ in 0..slices {
        if stop.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn poll_device(device: &HidDevice, stop: &AtomicBool, shared: &Mutex<SnekSensorSnapshot>) {
    let mut response = [0; PACKET_SIZE];
    if exchange(device, GET_INFO, 0, &mut response).is_err() {
        publish_connection(shared, false, true);
        return;
    }
    let version = firmware_version(&response);
    if version.is_empty() {
        log::info!("Opened Snekboard config interface (firmware version unavailable)");
    } else {
        log::info!("Opened Snekboard config interface, firmware version: {version}");
    }
    publish_connection(shared, true, false);

    let mut states = [0u32; 4];
    let mut sensor = 0usize;
    while !stop.load(Ordering::Acquire) {
        match exchange(device, SET_EXTIO_SENSOR, (sensor + 1) as u8, &mut response) {
            Ok(()) => {
                process_sensor_sample(
                    shared,
                    &mut states,
                    sensor,
                    u32::from_le_bytes(response[2..6].try_into().unwrap()),
                );
                sensor = (sensor + 1) % states.len();
            }
            Err(error) => {
                warn!("Snek sensor polling failed: {error}");
                publish_connection(shared, false, true);
                break;
            }
        }
    }

    // Selecting zero restores event delivery through the normal game endpoint.
    // DISCONNECT is deliberately reserved for application shutdown: leaving
    // Test Input merely stops diagnostic polling.
    let _ = exchange(device, SET_EXTIO_SENSOR, 0, &mut response);
}

fn send_disconnect() {
    let result = HidApi::new().and_then(|mut api| {
        api.refresh_devices()?;
        let info = api
            .device_list()
            .find(|info| {
                info.vendor_id() == VENDOR_ID
                    && info.product_id() == PRODUCT_ID
                    && info.interface_number() == CONFIG_INTERFACE
            })
            .ok_or_else(|| hidapi::HidError::HidApiError {
                message: "Snekboard config interface not found".into(),
            })?;
        info.open_device(&api)
    });
    let Ok(device) = result else {
        return;
    };
    let mut response = [0; PACKET_SIZE];
    if let Err(error) = exchange(&device, DISCONNECT, 0, &mut response) {
        warn!("Snek shutdown disconnect failed: {error}");
    }
}

fn exchange(
    device: &HidDevice,
    opcode: u8,
    payload: u8,
    response: &mut [u8; PACKET_SIZE],
) -> Result<(), String> {
    let request = build_request(opcode, payload);
    let written = device.write(&request).map_err(|e| e.to_string())?;
    if written != PACKET_SIZE {
        return Err(format!("short Snek write: {written}/{PACKET_SIZE}"));
    }
    let read = device
        .read_timeout(response, READ_TIMEOUT_MS)
        .map_err(|e| e.to_string())?;
    validate_response(response, read, opcode)
}

fn build_request(opcode: u8, payload: u8) -> [u8; PACKET_SIZE] {
    let mut request = [0; PACKET_SIZE];
    request[0] = OUTPUT_REPORT;
    request[1] = opcode;
    request[2] = payload;
    request[PACKET_SIZE - 1] = OUTPUT_REPORT.wrapping_add(opcode).wrapping_add(payload);
    request
}

fn validate_response(response: &[u8; PACKET_SIZE], read: usize, opcode: u8) -> Result<(), String> {
    if read != PACKET_SIZE {
        return Err(format!("short Snek read: {read}/{PACKET_SIZE}"));
    }
    if response[0] != INPUT_REPORT {
        return Err(format!("invalid Snek report id: 0x{:02x}", response[0]));
    }
    if response[1] == ERROR {
        return Err(format!("Snek rejected opcode 0x{opcode:02x}"));
    }
    if response[1] != opcode {
        return Err(format!(
            "mismatched Snek opcode: 0x{:02x} != 0x{opcode:02x}",
            response[1]
        ));
    }
    Ok(())
}

fn firmware_version(response: &[u8; PACKET_SIZE]) -> String {
    let payload = &response[2..];
    let end = payload
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(payload.len());
    String::from_utf8_lossy(&payload[..end]).trim().to_owned()
}

fn publish_connection(shared: &Mutex<SnekSensorSnapshot>, connected: bool, read_error: bool) {
    let mut snapshot = shared.lock().unwrap_or_else(|e| e.into_inner());
    if snapshot.connected == connected && snapshot.read_error == read_error {
        return;
    }
    snapshot.connected = connected;
    snapshot.read_error = read_error;
    if !connected {
        snapshot.sensors = [[[false; 4]; 4]; 2];
    }
    snapshot.revision = snapshot.revision.wrapping_add(1);
}

fn publish_sensor_state(shared: &Mutex<SnekSensorSnapshot>, sensor: usize, value: u32) {
    let mut snapshot = shared.lock().unwrap_or_else(|e| e.into_inner());
    snapshot.connected = true;
    snapshot.read_error = false;
    snapshot.sensors[1][SNEK_PANEL_UP][sensor] = value & (1 << 14) != 0;
    snapshot.sensors[0][SNEK_PANEL_UP][sensor] = value & (1 << 15) != 0;
    snapshot.sensors[0][SNEK_PANEL_LEFT][sensor] = value & (1 << 16) != 0;
    snapshot.sensors[1][SNEK_PANEL_LEFT][sensor] = value & (1 << 17) != 0;
    snapshot.sensors[0][SNEK_PANEL_DOWN][sensor] = value & (1 << 18) != 0;
    snapshot.sensors[1][SNEK_PANEL_DOWN][sensor] = value & (1 << 19) != 0;
    snapshot.sensors[1][SNEK_PANEL_RIGHT][sensor] = value & (1 << 20) != 0;
    snapshot.sensors[0][SNEK_PANEL_RIGHT][sensor] = value & (1 << 21) != 0;
    snapshot.revision = snapshot.revision.wrapping_add(1);
}

#[inline]
fn process_sensor_sample(
    shared: &Mutex<SnekSensorSnapshot>,
    states: &mut [u32; 4],
    sensor: usize,
    value: u32,
) {
    // poll_device publishes the connection before reading the first selector.
    // Only bits 14..21 are displayed, so unrelated firmware bits cannot
    // invalidate the snapshot or force the UI to rebuild its presentation.
    const SENSOR_BITS: u32 = 0xff << 14;
    let changed = (states[sensor] ^ value) & SENSOR_BITS != 0;
    states[sensor] = value;
    if changed {
        publish_sensor_state(shared, sensor, value);
    }
}

#[cfg(test)]
fn decode_sensor_states(states: [u32; 4]) -> [[[bool; 4]; 4]; 2] {
    const MAP: [(u32, usize, usize); 8] = [
        (14, 1, SNEK_PANEL_UP),
        (15, 0, SNEK_PANEL_UP),
        (16, 0, SNEK_PANEL_LEFT),
        (17, 1, SNEK_PANEL_LEFT),
        (18, 0, SNEK_PANEL_DOWN),
        (19, 1, SNEK_PANEL_DOWN),
        (20, 1, SNEK_PANEL_RIGHT),
        (21, 0, SNEK_PANEL_RIGHT),
    ];
    let mut out = [[[false; 4]; 4]; 2];
    for (sensor, state) in states.into_iter().enumerate() {
        for (bit, player, panel) in MAP {
            out[player][panel][sensor] = state & (1 << bit) != 0;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_has_protocol_layout_and_wrapping_checksum() {
        let request = build_request(SET_EXTIO_SENSOR, 4);
        assert_eq!(&request[..3], &[OUTPUT_REPORT, SET_EXTIO_SENSOR, 4]);
        assert!(request[3..63].iter().all(|byte| *byte == 0));
        assert_eq!(
            request[63],
            OUTPUT_REPORT.wrapping_add(SET_EXTIO_SENSOR).wrapping_add(4)
        );
        for opcode in 0..=u8::MAX {
            for payload in 0..=u8::MAX {
                let request = build_request(opcode, payload);
                assert_eq!(
                    request[63],
                    request[..63]
                        .iter()
                        .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
                );
            }
        }
    }

    #[test]
    fn response_requires_full_matching_input_report() {
        let mut response = [0; PACKET_SIZE];
        response[0] = INPUT_REPORT;
        response[1] = GET_INFO;
        assert!(validate_response(&response, PACKET_SIZE, GET_INFO).is_ok());
        assert!(validate_response(&response, PACKET_SIZE - 1, GET_INFO).is_err());
        assert!(validate_response(&response, PACKET_SIZE, DISCONNECT).is_err());
        response[1] = ERROR;
        assert!(validate_response(&response, PACKET_SIZE, GET_INFO).is_err());
    }

    #[test]
    fn firmware_version_is_bounded_and_null_terminated() {
        let mut response = [0; PACKET_SIZE];
        response[2..10].copy_from_slice(b"v1.2.3\0x");
        assert_eq!(firmware_version(&response), "v1.2.3");

        response[2..].fill(b'a');
        assert_eq!(firmware_version(&response).len(), PACKET_SIZE - 2);
    }

    #[test]
    fn board_bits_map_every_player_panel_and_sensor() {
        let expected = [
            (14, 1, SNEK_PANEL_UP),
            (15, 0, SNEK_PANEL_UP),
            (16, 0, SNEK_PANEL_LEFT),
            (17, 1, SNEK_PANEL_LEFT),
            (18, 0, SNEK_PANEL_DOWN),
            (19, 1, SNEK_PANEL_DOWN),
            (20, 1, SNEK_PANEL_RIGHT),
            (21, 0, SNEK_PANEL_RIGHT),
        ];
        for sensor in 0..4 {
            for (bit, player, panel) in expected {
                let mut raw = [0; 4];
                raw[sensor] = 1 << bit;
                let decoded = decode_sensor_states(raw);
                assert!(decoded[player][panel][sensor]);
                assert_eq!(
                    decoded
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter(|active| *active)
                        .count(),
                    1
                );
            }
        }
    }

    #[test]
    fn incremental_sensor_updates_match_complete_decoding() {
        let shared = Mutex::new(SnekSensorSnapshot {
            connected: true,
            ..SnekSensorSnapshot::unavailable()
        });
        let mut raw = [0u32; 4];
        for sensor in 0..4 {
            for value in [
                0,
                1 << 14,
                1 << 15,
                1 << 16,
                1 << 17,
                1 << 18,
                1 << 19,
                1 << 20,
                1 << 21,
                0xff << 14,
                u32::MAX,
                0x1234_5678,
                0,
            ] {
                let before = shared.lock().unwrap().revision;
                let changed = (raw[sensor] ^ value) & (0xff << 14) != 0;
                process_sensor_sample(&shared, &mut raw, sensor, value);
                let snapshot = *shared.lock().unwrap();
                assert_eq!(snapshot.sensors, decode_sensor_states(raw));
                assert_eq!(snapshot.revision, before + u64::from(changed));
                assert!(snapshot.connected);
                assert!(!snapshot.read_error);
            }
        }
    }
}
