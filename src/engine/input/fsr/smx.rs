use crate::engine::input::fsr::{BarView as FsrBarView, VIEW_SENSOR_COUNT, View as FsrView};
use hidapi::{DeviceInfo as HidDeviceInfo, HidApi, HidDevice};
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::time::{Duration, Instant, SystemTime};

const SMX_VENDOR_ID: u16 = 0x2341;
const SMX_PRODUCT_ID: u16 = 0x8037;
const SMX_PRODUCT_NAME: &str = "stepmaniax";

const HID_REPORT_INPUT_STATE: u8 = 0x03;
const HID_REPORT_COMMAND: u8 = 0x05;
const HID_REPORT_DATA: u8 = 0x06;
const PACKET_FLAG_END_OF_COMMAND: u8 = 0x01;
const PACKET_FLAG_HOST_CMD_FINISHED: u8 = 0x02;
const PACKET_FLAG_START_OF_COMMAND: u8 = 0x04;
const PACKET_FLAG_DEVICE_INFO: u8 = 0x80;
const HID_PACKET_SIZE: usize = 64;
const HID_PAYLOAD_SIZE: usize = 61;

const CONFIG_SIZE: usize = 250;
const CONFIG_FLAGS_OFFSET: usize = 2;
const CONFIG_PANEL_SETTINGS_OFFSET: usize = 56;
const PANEL_SETTINGS_SIZE: usize = 16;
const PANEL_FSR_LOW_OFFSET: usize = 2;
const PANEL_FSR_HIGH_OFFSET: usize = 6;
const PLATFORM_FLAG_FSR: u8 = 1 << 1;

const PANEL_COUNT: usize = 9;
const PANEL_SENSOR_COUNT: usize = 4;
const SENSOR_DETAIL_SIZE: usize = 10;
const SENSOR_DETAIL_BITS: usize = SENSOR_DETAIL_SIZE * 8;

const MIN_FSR_THRESHOLD: u16 = 5;
const MAX_FSR_THRESHOLD: u16 = 250;
const SENSOR_TEST_CALIBRATED: u8 = b'1';
const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const SENSOR_REQUEST_INTERVAL: Duration = Duration::from_millis(16);
const INPUT_REPORT_LIMIT: usize = 32;

const VIEW_PANELS: [(usize, &str); VIEW_SENSOR_COUNT] = [(3, "L"), (7, "D"), (1, "U"), (5, "R")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandKind {
    DeviceInfo,
    ReadConfig,
    WriteConfig,
    ReadSensorData,
}

#[derive(Clone, Debug)]
struct Command {
    kind: CommandKind,
    packets: Vec<[u8; HID_PACKET_SIZE]>,
}

#[derive(Clone, Copy, Debug)]
struct DeviceInfo {
    is_player_2: bool,
    firmware_version: u16,
    serial_hex: [u8; 33],
}

impl DeviceInfo {
    fn serial(&self) -> &str {
        std::str::from_utf8(&self.serial_hex[..32]).unwrap_or("<invalid>")
    }
}

#[derive(Clone, Copy, Debug)]
struct Config {
    bytes: [u8; CONFIG_SIZE],
}

impl Config {
    fn parse(bytes: &[u8]) -> Option<Self> {
        let mut out = [0u8; CONFIG_SIZE];
        out.copy_from_slice(bytes.get(..CONFIG_SIZE)?);
        Some(Self { bytes: out })
    }

    const fn master_version(self) -> u8 {
        self.bytes[0]
    }

    const fn config_version(self) -> u8 {
        self.bytes[1]
    }

    const fn is_fsr(self) -> bool {
        self.master_version() >= 4 && self.bytes[CONFIG_FLAGS_OFFSET] & PLATFORM_FLAG_FSR != 0
    }

    fn panel_fsr_low(self, panel: usize, sensor: usize) -> u8 {
        self.bytes[panel_sensor_offset(panel, PANEL_FSR_LOW_OFFSET, sensor)]
    }

    fn set_panel_fsr_thresholds(&mut self, panel: usize, threshold: u8) {
        for sensor in 0..PANEL_SENSOR_COUNT {
            let low = panel_sensor_offset(panel, PANEL_FSR_LOW_OFFSET, sensor);
            let high = panel_sensor_offset(panel, PANEL_FSR_HIGH_OFFSET, sensor);
            self.bytes[low] = threshold;
            self.bytes[high] = threshold.saturating_add(1);
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PanelData {
    have_data: bool,
    sensor_level: [i16; PANEL_SENSOR_COUNT],
    bad_sensor: [bool; PANEL_SENSOR_COUNT],
    dip: u8,
    bad_jumper: [bool; PANEL_SENSOR_COUNT],
}

#[derive(Clone, Copy, Debug, Default)]
struct SensorTestData {
    panel: [PanelData; PANEL_COUNT],
}

#[derive(Default)]
pub struct Monitor {
    api: Option<HidApi>,
    device: Option<HidDevice>,
    device_name: Option<String>,
    info: Option<DeviceInfo>,
    config: Option<Config>,
    test_data: Option<SensorTestData>,
    input_state: u16,
    pending: VecDeque<Command>,
    in_flight: Option<CommandKind>,
    in_flight_since: Option<Instant>,
    read_buffer: Vec<u8>,
    last_open_attempt: Option<Instant>,
    last_sensor_request: Option<Instant>,
}

impl Monitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn poll_view(&mut self) -> Option<FsrView> {
        self.service();
        let config = self.config?;
        if !config.is_fsr() {
            return None;
        }
        let data = self.test_data?;
        Some(FsrView {
            device_name: self.device_name.clone(),
            bars: std::array::from_fn(|i| {
                let (panel, label) = VIEW_PANELS[i];
                let raw_value = panel_value(data.panel[panel]);
                let raw_threshold = u16::from(config.panel_fsr_low(panel, 0));
                FsrBarView {
                    label,
                    raw_value,
                    value_norm: normalize(raw_value, MAX_FSR_THRESHOLD),
                    raw_threshold,
                    threshold_norm: normalize(raw_threshold, MAX_FSR_THRESHOLD),
                    min_raw_threshold: MIN_FSR_THRESHOLD,
                    max_raw_threshold: MAX_FSR_THRESHOLD,
                    active: self.input_state & (1u16 << panel) != 0,
                }
            }),
        })
    }

    pub fn update_threshold(&mut self, sensor_index: usize, threshold: u16) -> bool {
        if sensor_index >= VIEW_SENSOR_COUNT
            || !(MIN_FSR_THRESHOLD..=MAX_FSR_THRESHOLD).contains(&threshold)
        {
            return false;
        }
        self.service();
        let Some(info) = self.info else {
            return false;
        };
        if info.firmware_version < 5 {
            return false;
        }
        let Some(mut config) = self.config else {
            return false;
        };
        if !config.is_fsr() {
            return false;
        }
        let (panel, _) = VIEW_PANELS[sensor_index];
        config.set_panel_fsr_thresholds(panel, threshold as u8);
        self.config = Some(config);
        self.pending.retain(|cmd| {
            cmd.kind != CommandKind::WriteConfig && cmd.kind != CommandKind::ReadConfig
        });
        self.pending.push_back(write_config_command(config));
        self.pending.push_back(read_config_command(info));
        true
    }

    pub fn debug_dump(&mut self) -> String {
        self.service();
        let mut out = String::new();
        let _ = writeln!(out, "DeadSync StepManiaX FSR debug dump");
        let _ = writeln!(out, "generated: {:?}", SystemTime::now());
        let _ = writeln!(
            out,
            "supported_smx_vid_pid: {SMX_VENDOR_ID:04X}:{SMX_PRODUCT_ID:04X}"
        );
        let _ = writeln!(out);
        dump_current_monitor(&mut out, self);
        let _ = writeln!(out);
        dump_hid_devices(&mut out);
        out
    }

    fn service(&mut self) {
        self.ensure_device();
        self.read_pending_reports();
        self.queue_bootstrap_commands();
        self.send_next_command();
        self.drop_timed_out_command();
    }

    fn ensure_device(&mut self) {
        if self.device.is_some() {
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
        if self.api.is_none() {
            self.api = HidApi::new().ok();
        }
        let Some(api) = self.api.as_mut() else {
            return;
        };
        if api.refresh_devices().is_err() {
            self.api = None;
            return;
        }
        let Some(info) = api.device_list().find(|info| is_smx_device(info)) else {
            return;
        };
        let Ok(device) = info.open_device(api) else {
            return;
        };
        if device.set_blocking_mode(false).is_err() {
            return;
        }
        self.device_name = Some(device_label(info, None));
        self.device = Some(device);
        self.pending.push_back(device_info_command());
    }

    fn drop_device(&mut self) {
        self.device = None;
        self.device_name = None;
        self.info = None;
        self.config = None;
        self.test_data = None;
        self.input_state = 0;
        self.pending.clear();
        self.in_flight = None;
        self.in_flight_since = None;
        self.read_buffer.clear();
        self.last_sensor_request = None;
    }

    fn queue_bootstrap_commands(&mut self) {
        if self.device.is_none() {
            return;
        }
        if self.info.is_none() {
            self.queue_command(device_info_command());
            return;
        }
        let info = self.info.expect("checked above");
        if self.config.is_none() {
            self.queue_command(read_config_command(info));
            return;
        }
        if self.config.is_some_and(Config::is_fsr) && self.should_request_sensor_data() {
            self.queue_command(sensor_data_command());
        }
    }

    fn should_request_sensor_data(&self) -> bool {
        if self.in_flight == Some(CommandKind::ReadSensorData)
            || self
                .pending
                .iter()
                .any(|cmd| cmd.kind == CommandKind::ReadSensorData)
        {
            return false;
        }
        self.last_sensor_request
            .is_none_or(|last| last.elapsed() >= SENSOR_REQUEST_INTERVAL)
    }

    fn queue_command(&mut self, command: Command) {
        if self.in_flight == Some(command.kind)
            || self
                .pending
                .iter()
                .any(|pending| pending.kind == command.kind)
        {
            return;
        }
        self.pending.push_back(command);
    }

    fn send_next_command(&mut self) {
        if self.in_flight.is_some() {
            return;
        }
        let Some(command) = self.pending.pop_front() else {
            return;
        };
        let Some(device) = self.device.as_ref() else {
            return;
        };
        for packet in &command.packets {
            if device.write(packet).is_err() {
                self.drop_device();
                return;
            }
        }
        if command.kind == CommandKind::ReadSensorData {
            self.last_sensor_request = Some(Instant::now());
        }
        self.in_flight = Some(command.kind);
        self.in_flight_since = Some(Instant::now());
    }

    fn drop_timed_out_command(&mut self) {
        if self
            .in_flight_since
            .is_some_and(|sent| sent.elapsed() >= COMMAND_TIMEOUT)
        {
            self.drop_device();
        }
    }

    fn read_pending_reports(&mut self) {
        if self.device.is_none() {
            return;
        }
        let mut lost_device = false;
        for _ in 0..INPUT_REPORT_LIMIT {
            let mut buf = [0u8; HID_PACKET_SIZE];
            let result = self
                .device
                .as_ref()
                .map(|device| device.read_timeout(&mut buf, 0));
            match result {
                None => break,
                Some(Ok(0)) => break,
                Some(Ok(len)) => self.handle_report(&buf[..len]),
                Some(Err(_)) => {
                    lost_device = true;
                    break;
                }
            }
        }
        if lost_device {
            self.drop_device();
        }
    }

    fn handle_report(&mut self, report: &[u8]) {
        let Some((&report_id, rest)) = report.split_first() else {
            return;
        };
        match report_id {
            HID_REPORT_INPUT_STATE => {
                if rest.len() >= 2 {
                    self.input_state = u16::from_le_bytes([rest[0], rest[1]]);
                }
            }
            HID_REPORT_DATA => self.handle_data_report(rest),
            _ => {}
        }
    }

    fn handle_data_report(&mut self, report: &[u8]) {
        if report.len() < 2 {
            return;
        }
        let flags = report[0];
        let size = report[1] as usize;
        let Some(payload) = report.get(2..2 + size) else {
            return;
        };

        if flags & PACKET_FLAG_DEVICE_INFO != 0 {
            if let Some(info) = parse_device_info(payload) {
                self.device_name = Some(device_label_from_info(info));
                self.info = Some(info);
            }
            self.in_flight = None;
            self.in_flight_since = None;
            return;
        }

        if flags & PACKET_FLAG_START_OF_COMMAND != 0 {
            self.read_buffer.clear();
        }
        self.read_buffer.extend_from_slice(payload);

        if flags & PACKET_FLAG_END_OF_COMMAND != 0 {
            let packet = std::mem::take(&mut self.read_buffer);
            self.handle_command_packet(&packet);
            self.in_flight = None;
            self.in_flight_since = None;
        }
        if flags & PACKET_FLAG_HOST_CMD_FINISHED != 0 {
            self.in_flight = None;
            self.in_flight_since = None;
        }
    }

    fn handle_command_packet(&mut self, packet: &[u8]) {
        match packet.first().copied() {
            Some(b'G') => {
                if let Some(config) = parse_config_packet(packet) {
                    self.config = Some(config);
                }
            }
            Some(b'y') => {
                if let Some(data) = parse_sensor_test_packet(packet) {
                    self.test_data = Some(data);
                }
            }
            _ => {}
        }
    }
}

fn panel_sensor_offset(panel: usize, field_offset: usize, sensor: usize) -> usize {
    CONFIG_PANEL_SETTINGS_OFFSET + panel * PANEL_SETTINGS_SIZE + field_offset + sensor
}

fn device_info_command() -> Command {
    let mut packet = [0u8; HID_PACKET_SIZE];
    packet[0] = HID_REPORT_COMMAND;
    packet[1] = PACKET_FLAG_DEVICE_INFO;
    Command {
        kind: CommandKind::DeviceInfo,
        packets: vec![packet],
    }
}

fn read_config_command(info: DeviceInfo) -> Command {
    regular_command(
        CommandKind::ReadConfig,
        if info.firmware_version >= 5 {
            b"G"
        } else {
            b"g\n"
        },
    )
}

fn write_config_command(config: Config) -> Command {
    let mut payload = Vec::with_capacity(CONFIG_SIZE + 2);
    payload.push(b'W');
    payload.push(CONFIG_SIZE as u8);
    payload.extend_from_slice(&config.bytes);
    regular_command(CommandKind::WriteConfig, &payload)
}

fn sensor_data_command() -> Command {
    regular_command(CommandKind::ReadSensorData, b"y1\n")
}

fn regular_command(kind: CommandKind, payload: &[u8]) -> Command {
    let mut packets = Vec::new();
    let mut offset = 0usize;
    while offset < payload.len() {
        let size = (payload.len() - offset).min(HID_PAYLOAD_SIZE);
        let mut packet = [0u8; HID_PACKET_SIZE];
        packet[0] = HID_REPORT_COMMAND;
        packet[1] = if offset == 0 {
            PACKET_FLAG_START_OF_COMMAND
        } else {
            0
        };
        if offset + size == payload.len() {
            packet[1] |= PACKET_FLAG_END_OF_COMMAND;
        }
        packet[2] = size as u8;
        packet[3..3 + size].copy_from_slice(&payload[offset..offset + size]);
        packets.push(packet);
        offset += size;
    }
    Command { kind, packets }
}

fn parse_device_info(payload: &[u8]) -> Option<DeviceInfo> {
    if payload.len() < 23 {
        return None;
    }
    let mut serial_hex = [0u8; 33];
    for (i, byte) in payload[4..20].iter().enumerate() {
        let hi = byte >> 4;
        let lo = byte & 0x0F;
        serial_hex[i * 2] = hex_digit(hi);
        serial_hex[i * 2 + 1] = hex_digit(lo);
    }
    Some(DeviceInfo {
        is_player_2: payload[2] == b'1',
        firmware_version: u16::from_le_bytes([payload[20], payload[21]]),
        serial_hex,
    })
}

fn parse_config_packet(packet: &[u8]) -> Option<Config> {
    if packet.len() < 2 || packet[0] != b'G' {
        return None;
    }
    let size = packet[1] as usize;
    if size < CONFIG_SIZE || packet.len() < 2 + CONFIG_SIZE {
        return None;
    }
    Config::parse(&packet[2..2 + CONFIG_SIZE])
}

fn parse_sensor_test_packet(packet: &[u8]) -> Option<SensorTestData> {
    if packet.len() < 3 || packet[0] != b'y' || packet[1] != SENSOR_TEST_CALIBRATED {
        return None;
    }
    let count = (packet[2] as usize).min(SENSOR_DETAIL_BITS);
    if packet.len() < 3 + count * 2 {
        return None;
    }

    let mut bits = [0u16; SENSOR_DETAIL_BITS];
    for i in 0..count {
        let offset = 3 + i * 2;
        bits[i] = u16::from_le_bytes([packet[offset], packet[offset + 1]]);
    }

    let mut out = SensorTestData::default();
    for panel in 0..PANEL_COUNT {
        let mut bytes = [0u8; SENSOR_DETAIL_SIZE];
        for (byte_index, byte) in bytes.iter_mut().enumerate() {
            for bit in 0..8 {
                let bit_index = byte_index * 8 + bit;
                if bit_index < count && bits[bit_index] & (1u16 << panel) != 0 {
                    *byte |= 1u8 << bit;
                }
            }
        }
        if bytes[0] & 0b0000_0111 != 0b0000_0010 {
            continue;
        }
        let mut panel_data = PanelData {
            have_data: true,
            bad_sensor: [
                bytes[0] & (1 << 3) != 0,
                bytes[0] & (1 << 4) != 0,
                bytes[0] & (1 << 5) != 0,
                bytes[0] & (1 << 6) != 0,
            ],
            dip: bytes[9] & 0x0F,
            bad_jumper: [
                bytes[9] & (1 << 4) != 0,
                bytes[9] & (1 << 5) != 0,
                bytes[9] & (1 << 6) != 0,
                bytes[9] & (1 << 7) != 0,
            ],
            ..PanelData::default()
        };
        for sensor in 0..PANEL_SENSOR_COUNT {
            let offset = 1 + sensor * 2;
            panel_data.sensor_level[sensor] =
                i16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        }
        out.panel[panel] = panel_data;
    }
    Some(out)
}

fn panel_value(panel: PanelData) -> u16 {
    if !panel.have_data {
        return 0;
    }
    panel
        .sensor_level
        .into_iter()
        .map(scale_fsr_sensor_value)
        .max()
        .unwrap_or(0)
}

fn scale_fsr_sensor_value(raw: i16) -> u16 {
    let mut value = i32::from(raw);
    if (-10..0).contains(&value) {
        value = 0;
    }
    value = (value >> 2).max(0);
    value.min(i32::from(MAX_FSR_THRESHOLD)) as u16
}

fn normalize(value: u16, max_value: u16) -> f32 {
    value as f32 / max_value as f32
}

fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        _ => b'a' + nibble - 10,
    }
}

fn is_smx_device(info: &HidDeviceInfo) -> bool {
    if info.vendor_id() != SMX_VENDOR_ID || info.product_id() != SMX_PRODUCT_ID {
        return false;
    }
    info.product_string()
        .is_some_and(|product| product.to_ascii_lowercase().contains(SMX_PRODUCT_NAME))
}

fn device_label(info: &HidDeviceInfo, smx_info: Option<DeviceInfo>) -> String {
    if let Some(smx_info) = smx_info {
        return device_label_from_info(smx_info);
    }
    format!(
        "{} FSR",
        info.product_string().unwrap_or("StepManiaX").trim()
    )
}

fn device_label_from_info(info: DeviceInfo) -> String {
    format!(
        "StepManiaX P{} fw{}",
        if info.is_player_2 { 2 } else { 1 },
        info.firmware_version
    )
}

fn dump_current_monitor(out: &mut String, monitor: &Monitor) {
    let _ = writeln!(out, "[current StepManiaX monitor]");
    let _ = writeln!(out, "open: {}", monitor.device.is_some());
    let _ = writeln!(
        out,
        "device_name: {}",
        monitor.device_name.as_deref().unwrap_or("<none>")
    );
    if let Some(info) = monitor.info {
        let _ = writeln!(out, "player: P{}", if info.is_player_2 { 2 } else { 1 });
        let _ = writeln!(out, "firmware_version: {}", info.firmware_version);
        let _ = writeln!(out, "serial: {}", info.serial());
    } else {
        let _ = writeln!(out, "info: <none>");
    }
    if let Some(config) = monitor.config {
        let _ = writeln!(out, "master_version: {}", config.master_version());
        let _ = writeln!(out, "config_version: {}", config.config_version());
        let _ = writeln!(out, "fsr: {}", config.is_fsr());
        for (panel, label) in VIEW_PANELS {
            let thresholds = [
                config.panel_fsr_low(panel, 0),
                config.panel_fsr_low(panel, 1),
                config.panel_fsr_low(panel, 2),
                config.panel_fsr_low(panel, 3),
            ];
            let _ = writeln!(out, "panel {label} ({panel}) fsr_low: {thresholds:?}");
        }
    } else {
        let _ = writeln!(out, "config: <none>");
    }
    let _ = writeln!(out, "input_state: 0x{:04X}", monitor.input_state);
    if let Some(data) = monitor.test_data {
        for (panel, label) in VIEW_PANELS {
            let panel_data = data.panel[panel];
            let _ = writeln!(
                out,
                "panel {label} ({panel}) have={} sensors={:?} bad={:?} dip={} bad_jumper={:?}",
                panel_data.have_data,
                panel_data.sensor_level,
                panel_data.bad_sensor,
                panel_data.dip,
                panel_data.bad_jumper
            );
        }
    } else {
        let _ = writeln!(out, "test_data: <none>");
    }
}

fn dump_hid_devices(out: &mut String) {
    let _ = writeln!(out, "[hidapi StepManiaX devices]");
    let mut api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            let _ = writeln!(out, "hidapi_open: error: {e}");
            return;
        }
    };
    if let Err(e) = api.refresh_devices() {
        let _ = writeln!(out, "refresh_devices: error: {e}");
        return;
    }
    let mut count = 0usize;
    for (index, info) in api
        .device_list()
        .filter(|info| info.vendor_id() == SMX_VENDOR_ID && info.product_id() == SMX_PRODUCT_ID)
        .enumerate()
    {
        count += 1;
        let _ = writeln!(out);
        let _ = writeln!(out, "[smx device {index}]");
        let _ = writeln!(out, "path: {}", info.path().to_string_lossy());
        let _ = writeln!(
            out,
            "product: {}",
            info.product_string().unwrap_or("<none>")
        );
        let _ = writeln!(
            out,
            "manufacturer: {}",
            info.manufacturer_string().unwrap_or("<none>")
        );
        let _ = writeln!(out, "serial: {}", info.serial_number().unwrap_or("<none>"));
        let _ = writeln!(out, "interface_number: {}", info.interface_number());
    }
    let _ = writeln!(out, "count: {count}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_panel_offsets_match_public_packet() {
        assert_eq!(panel_sensor_offset(0, PANEL_FSR_LOW_OFFSET, 0), 58);
        assert_eq!(panel_sensor_offset(0, PANEL_FSR_HIGH_OFFSET, 3), 65);
        assert_eq!(panel_sensor_offset(8, PANEL_FSR_LOW_OFFSET, 0), 186);
        assert_eq!(panel_sensor_offset(8, PANEL_FSR_HIGH_OFFSET, 3), 193);
    }

    #[test]
    fn config_threshold_write_sets_all_panel_sensors() {
        let mut config = Config {
            bytes: [0u8; CONFIG_SIZE],
        };
        config.set_panel_fsr_thresholds(7, 174);
        for sensor in 0..PANEL_SENSOR_COUNT {
            assert_eq!(config.panel_fsr_low(7, sensor), 174);
            assert_eq!(
                config.bytes[panel_sensor_offset(7, PANEL_FSR_HIGH_OFFSET, sensor)],
                175
            );
        }
    }
}
