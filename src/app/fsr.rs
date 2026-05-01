use std::path::Path;

fn write_dump_file(path: &Path, content: String) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create '{}': {e}", parent.display()))?;
    }
    std::fs::write(path, content).map_err(|e| format!("failed to write '{}': {e}", path.display()))
}

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
mod imp {
    use crate::screens::components::shared::test_input::{FsrBarView, FsrView};
    use hidapi::{DeviceInfo, HidApi, HidDevice};
    use std::cmp::min;
    use std::fmt::Write as _;
    use std::path::Path;
    use std::time::{Duration, Instant, SystemTime};

    const ADP_VENDOR_ID: u16 = 0x1209;
    const ADP_PRODUCT_ID: u16 = 0xB196;

    const REPORT_ID_SENSOR_VALUES: u8 = 0x01;
    const REPORT_ID_PAD_CONFIGURATION: u8 = 0x02;
    const REPORT_ID_NAME: u8 = 0x05;

    const SENSOR_COUNT: usize = 12;
    const VIEW_SENSOR_COUNT: usize = 4;
    const MAX_NAME_SIZE: usize = 50;
    const MAX_SENSOR_VALUE: u16 = 850;
    const LINEARIZATION_POWER: u32 = 4;
    const NTH_DEGREE_COEFFICIENT: f32 = 0.9;
    const FIRST_DEGREE_COEFFICIENT: f32 = 0.1;
    const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);
    const FEATURE_PROBE_IDS: [u8; 16] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
        0x0F,
    ];
    const FEATURE_REPORT_BUF_SIZE: usize = 256;
    const INPUT_REPORT_BUF_SIZE: usize = 256;
    const INPUT_REPORT_LIMIT: usize = 8;

    #[derive(Clone, Copy, Debug, Default)]
    struct ConfigReport {
        sensor_thresholds: [u16; SENSOR_COUNT],
        release_threshold: f32,
        sensor_to_button_mapping: [i8; SENSOR_COUNT],
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct InputReport {
        sensor_values: [u16; SENSOR_COUNT],
    }

    #[derive(Default)]
    pub struct Monitor {
        api: Option<HidApi>,
        device: Option<HidDevice>,
        device_name: Option<String>,
        config: ConfigReport,
        input: InputReport,
        last_open_attempt: Option<Instant>,
    }

    impl Monitor {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn poll_view(&mut self) -> Option<FsrView> {
            self.ensure_device();
            self.read_pending_reports();
            self.device.as_ref()?;
            Some(FsrView {
                device_name: self.device_name.clone(),
                bars: std::array::from_fn(|i| FsrBarView {
                    label: sensor_label(i),
                    raw_value: self.input.sensor_values[i],
                    value_norm: normalize_sensor_value(self.input.sensor_values[i]),
                    raw_threshold: self.config.sensor_thresholds[i],
                    threshold_norm: normalize_sensor_value(self.config.sensor_thresholds[i]),
                    active: self.input.sensor_values[i] >= self.config.sensor_thresholds[i],
                }),
            })
        }

        pub fn update_threshold(&mut self, sensor_index: usize, threshold: u16) -> bool {
            if sensor_index >= VIEW_SENSOR_COUNT || threshold > MAX_SENSOR_VALUE {
                return false;
            }
            self.ensure_device();
            let Some(device) = self.device.as_ref() else {
                return false;
            };
            self.config.sensor_thresholds[sensor_index] = threshold;
            if write_config(device, &self.config).is_ok() {
                return true;
            }
            self.drop_device();
            false
        }

        pub fn write_debug_dump(&mut self, path: &Path) -> Result<(), String> {
            self.ensure_device();
            self.read_pending_reports();
            super::write_dump_file(path, build_debug_dump(self))
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
            let Some(info) = api.device_list().find(|info| {
                info.vendor_id() == ADP_VENDOR_ID && info.product_id() == ADP_PRODUCT_ID
            }) else {
                return;
            };
            let Ok(device) = info.open_device(api) else {
                return;
            };
            let device_name = read_name_from_device(&device).ok();
            let Ok(config) = read_config(&device) else {
                return;
            };
            self.device_name = device_name;
            self.config = config;
            self.input = InputReport::default();
            self.device = Some(device);
        }

        fn drop_device(&mut self) {
            self.device = None;
            self.device_name = None;
            self.config = ConfigReport::default();
            self.input = InputReport::default();
        }

        fn read_pending_reports(&mut self) {
            let Some(device) = self.device.as_ref() else {
                return;
            };
            let mut buf = [0u8; 64];
            let mut lost_device = false;
            loop {
                match device.read_timeout(&mut buf, 0) {
                    Ok(0) => break,
                    Ok(len) => {
                        if let Some(report) = parse_input_report(&buf[..len]) {
                            self.input = report;
                        }
                    }
                    Err(_) => {
                        lost_device = true;
                        break;
                    }
                }
            }
            if lost_device {
                self.drop_device();
            }
        }
    }

    fn build_debug_dump(monitor: &Monitor) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "DeadSync FSR debug dump");
        let _ = writeln!(out, "generated: {:?}", SystemTime::now());
        let _ = writeln!(
            out,
            "supported_adp_vid_pid: {ADP_VENDOR_ID:04X}:{ADP_PRODUCT_ID:04X}"
        );
        let _ = writeln!(out);
        dump_current_monitor(&mut out, monitor);
        let _ = writeln!(out);
        dump_hid_devices(&mut out);
        out
    }

    fn dump_current_monitor(out: &mut String, monitor: &Monitor) {
        let _ = writeln!(out, "[current supported FSR monitor]");
        let _ = writeln!(out, "open: {}", monitor.device.is_some());
        let _ = writeln!(
            out,
            "device_name: {}",
            monitor.device_name.as_deref().unwrap_or("<none>")
        );
        if monitor.device.is_none() {
            return;
        }
        let _ = writeln!(out, "thresholds: {:?}", monitor.config.sensor_thresholds);
        let _ = writeln!(
            out,
            "release_threshold: {:.6}",
            monitor.config.release_threshold
        );
        let _ = writeln!(
            out,
            "sensor_to_button_mapping: {:?}",
            monitor.config.sensor_to_button_mapping
        );
        let _ = writeln!(
            out,
            "latest_sensor_values: {:?}",
            monitor.input.sensor_values
        );
    }

    fn dump_hid_devices(out: &mut String) {
        let _ = writeln!(out, "[hidapi devices]");
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
        let devices: Vec<DeviceInfo> = api.device_list().cloned().collect();
        let _ = writeln!(out, "count: {}", devices.len());
        for (index, info) in devices.iter().enumerate() {
            dump_device(out, &api, index, info);
        }
    }

    fn dump_device(out: &mut String, api: &HidApi, index: usize, info: &DeviceInfo) {
        let candidate = is_fsr_candidate(info);
        let _ = writeln!(out);
        let _ = writeln!(out, "[device {index}]");
        let _ = writeln!(out, "path: {}", info.path().to_string_lossy());
        let _ = writeln!(out, "vendor_id: 0x{:04X}", info.vendor_id());
        let _ = writeln!(out, "product_id: 0x{:04X}", info.product_id());
        let _ = writeln!(out, "release_number: 0x{:04X}", info.release_number());
        let _ = writeln!(out, "manufacturer: {}", opt_str(info.manufacturer_string()));
        let _ = writeln!(out, "product: {}", opt_str(info.product_string()));
        let _ = writeln!(out, "serial: {}", opt_str(info.serial_number()));
        let _ = writeln!(out, "usage_page: 0x{:04X}", info.usage_page());
        let _ = writeln!(out, "usage: 0x{:04X}", info.usage());
        let _ = writeln!(out, "interface_number: {}", info.interface_number());
        let _ = writeln!(out, "bus_type: {:?}", info.bus_type());
        let _ = writeln!(out, "fsr_candidate: {candidate}");

        match info.open_device(api) {
            Ok(device) => dump_open_device(out, info, &device, candidate),
            Err(e) => {
                let _ = writeln!(out, "open: error: {e}");
            }
        }
    }

    fn dump_open_device(out: &mut String, info: &DeviceInfo, device: &HidDevice, candidate: bool) {
        let _ = writeln!(out, "open: ok");
        dump_open_strings(out, device);
        dump_report_descriptor(out, device);
        if candidate {
            dump_feature_reports(out, device);
        } else {
            let _ = writeln!(out, "feature_reports: skipped (not FSR-like)");
        }
        dump_input_reports(out, device);
        if is_known_adp(info) {
            dump_adp_decode(out, device);
        }
    }

    fn dump_open_strings(out: &mut String, device: &HidDevice) {
        match device.get_manufacturer_string() {
            Ok(value) => {
                let _ = writeln!(out, "open_manufacturer: {}", opt_owned_str(value));
            }
            Err(e) => {
                let _ = writeln!(out, "open_manufacturer: error: {e}");
            }
        }
        match device.get_product_string() {
            Ok(value) => {
                let _ = writeln!(out, "open_product: {}", opt_owned_str(value));
            }
            Err(e) => {
                let _ = writeln!(out, "open_product: error: {e}");
            }
        }
        match device.get_serial_number_string() {
            Ok(value) => {
                let _ = writeln!(out, "open_serial: {}", opt_owned_str(value));
            }
            Err(e) => {
                let _ = writeln!(out, "open_serial: error: {e}");
            }
        }
    }

    fn dump_report_descriptor(out: &mut String, device: &HidDevice) {
        let mut buf = [0u8; hidapi::MAX_REPORT_DESCRIPTOR_SIZE];
        match device.get_report_descriptor(&mut buf) {
            Ok(len) => dump_bytes(out, "report_descriptor", &buf[..len]),
            Err(e) => {
                let _ = writeln!(out, "report_descriptor: error: {e}");
            }
        }
    }

    fn dump_feature_reports(out: &mut String, device: &HidDevice) {
        let _ = writeln!(out, "feature_reports:");
        for id in FEATURE_PROBE_IDS {
            let mut buf = [0u8; FEATURE_REPORT_BUF_SIZE];
            buf[0] = id;
            match device.get_feature_report(&mut buf) {
                Ok(len) => dump_bytes(out, &format!("  id 0x{id:02X}"), &buf[..len]),
                Err(e) => {
                    let _ = writeln!(out, "  id 0x{id:02X}: error: {e}");
                }
            }
        }
    }

    fn dump_input_reports(out: &mut String, device: &HidDevice) {
        if let Err(e) = device.set_blocking_mode(false) {
            let _ = writeln!(out, "input_reports: set_nonblocking error: {e}");
            return;
        }
        let _ = writeln!(out, "input_reports:");
        let mut seen = 0usize;
        for _ in 0..INPUT_REPORT_LIMIT {
            let mut buf = [0u8; INPUT_REPORT_BUF_SIZE];
            match device.read_timeout(&mut buf, 0) {
                Ok(0) => break,
                Ok(len) => {
                    seen += 1;
                    dump_bytes(out, &format!("  sample {}", seen - 1), &buf[..len]);
                }
                Err(e) => {
                    let _ = writeln!(out, "  read_error: {e}");
                    break;
                }
            }
        }
        if seen == 0 {
            let _ = writeln!(out, "  <none queued>");
        }
    }

    fn dump_adp_decode(out: &mut String, device: &HidDevice) {
        let _ = writeln!(out, "adp_decode:");
        match read_name_from_device(device) {
            Ok(name) => {
                let _ = writeln!(out, "  name: {name}");
            }
            Err(()) => {
                let _ = writeln!(out, "  name: error");
            }
        }
        match read_config(device) {
            Ok(config) => {
                let _ = writeln!(out, "  thresholds: {:?}", config.sensor_thresholds);
                let _ = writeln!(out, "  release_threshold: {:.6}", config.release_threshold);
                let _ = writeln!(
                    out,
                    "  sensor_to_button_mapping: {:?}",
                    config.sensor_to_button_mapping
                );
            }
            Err(()) => {
                let _ = writeln!(out, "  config: error");
            }
        }
    }

    fn dump_bytes(out: &mut String, label: &str, bytes: &[u8]) {
        let _ = writeln!(out, "{label}: len={}", bytes.len());
        for (line_idx, chunk) in bytes.chunks(16).enumerate() {
            let _ = write!(out, "    {:04X}: ", line_idx * 16);
            for byte in chunk {
                let _ = write!(out, "{byte:02X} ");
            }
            for _ in chunk.len()..16 {
                let _ = write!(out, "   ");
            }
            let _ = write!(out, " ");
            for byte in chunk {
                let ch = if byte.is_ascii_graphic() || *byte == b' ' {
                    *byte as char
                } else {
                    '.'
                };
                let _ = write!(out, "{ch}");
            }
            let _ = writeln!(out);
        }
    }

    fn is_known_adp(info: &DeviceInfo) -> bool {
        info.vendor_id() == ADP_VENDOR_ID && info.product_id() == ADP_PRODUCT_ID
    }

    fn is_fsr_candidate(info: &DeviceInfo) -> bool {
        if is_known_adp(info) {
            return true;
        }
        let haystack = format!(
            "{} {} {}",
            info.manufacturer_string().unwrap_or(""),
            info.product_string().unwrap_or(""),
            info.path().to_string_lossy()
        )
        .to_ascii_lowercase();
        [
            "fsr", "force", "dance", "step", "itg", "adp", "arrow", "sensor", "cabinet",
            "i/o", "io board", "arduino", "teensy", "pico", "rp2040", "stm32", "adafruit",
            "sparkfun", "piu", "l-tek", "ltek", "makey",
        ]
        .iter()
        .any(|needle| haystack.contains(needle))
    }

    fn opt_str(value: Option<&str>) -> &str {
        value.unwrap_or("<none>")
    }

    fn opt_owned_str(value: Option<String>) -> String {
        value.unwrap_or_else(|| "<none>".to_owned())
    }

    fn sensor_label(index: usize) -> &'static str {
        match index {
            0 => "S0",
            1 => "S1",
            2 => "S2",
            3 => "S3",
            _ => "FSR",
        }
    }

    fn read_name_from_device(device: &HidDevice) -> Result<String, ()> {
        let mut buf = [0u8; 1 + 1 + MAX_NAME_SIZE];
        buf[0] = REPORT_ID_NAME;
        let len = device.get_feature_report(&mut buf).map_err(|_| ())?;
        parse_name_report(&buf[..len]).ok_or(())
    }

    fn read_config(device: &HidDevice) -> Result<ConfigReport, ()> {
        let mut buf = [0u8; 1 + SENSOR_COUNT * 2 + 4 + SENSOR_COUNT];
        buf[0] = REPORT_ID_PAD_CONFIGURATION;
        let len = device.get_feature_report(&mut buf).map_err(|_| ())?;
        parse_config_report(&buf[..len]).ok_or(())
    }

    fn write_config(device: &HidDevice, config: &ConfigReport) -> Result<(), ()> {
        let mut buf = Vec::with_capacity(1 + SENSOR_COUNT * 2 + 4 + SENSOR_COUNT);
        buf.push(REPORT_ID_PAD_CONFIGURATION);
        for threshold in config.sensor_thresholds {
            buf.extend_from_slice(&threshold.to_le_bytes());
        }
        buf.extend_from_slice(&config.release_threshold.to_le_bytes());
        for mapping in config.sensor_to_button_mapping {
            buf.push(mapping as u8);
        }
        device.send_feature_report(&buf).map_err(|_| ())?;
        Ok(())
    }

    fn parse_input_report(bytes: &[u8]) -> Option<InputReport> {
        let payload = match bytes {
            [REPORT_ID_SENSOR_VALUES, rest @ ..] if rest.len() >= 2 + SENSOR_COUNT * 2 => rest,
            rest if rest.len() >= 2 + SENSOR_COUNT * 2 => rest,
            _ => return None,
        };

        let _button_bits = u16::from_le_bytes(payload[0..2].try_into().ok()?);
        let mut sensor_values = [0u16; SENSOR_COUNT];
        let mut offset = 2usize;
        for value in &mut sensor_values {
            let end = offset + 2;
            *value = u16::from_le_bytes(payload[offset..end].try_into().ok()?);
            offset = end;
        }
        Some(InputReport { sensor_values })
    }

    fn parse_config_report(bytes: &[u8]) -> Option<ConfigReport> {
        if bytes.len() < 1 + SENSOR_COUNT * 2 + 4 + SENSOR_COUNT
            || bytes[0] != REPORT_ID_PAD_CONFIGURATION
        {
            return None;
        }

        let mut sensor_thresholds = [0u16; SENSOR_COUNT];
        let mut offset = 1usize;
        for value in &mut sensor_thresholds {
            let end = offset + 2;
            *value = u16::from_le_bytes(bytes[offset..end].try_into().ok()?);
            offset = end;
        }

        let release_threshold = f32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
        offset += 4;

        let mut sensor_to_button_mapping = [0i8; SENSOR_COUNT];
        for value in &mut sensor_to_button_mapping {
            *value = bytes[offset] as i8;
            offset += 1;
        }

        Some(ConfigReport {
            sensor_thresholds,
            release_threshold,
            sensor_to_button_mapping,
        })
    }

    fn parse_name_report(bytes: &[u8]) -> Option<String> {
        if bytes.len() < 2 || bytes[0] != REPORT_ID_NAME {
            return None;
        }
        let size = min(bytes[1] as usize, bytes.len().saturating_sub(2));
        Some(String::from_utf8_lossy(&bytes[2..2 + size]).into_owned())
    }

    fn linearize_value(raw: u16) -> f32 {
        let raw = min(raw, MAX_SENSOR_VALUE) as f32;
        let max = MAX_SENSOR_VALUE as f32;
        let linearized_max = max.powi(LINEARIZATION_POWER as i32) / max;
        let nth = raw.powi(LINEARIZATION_POWER as i32) / linearized_max;
        nth * NTH_DEGREE_COEFFICIENT + raw * FIRST_DEGREE_COEFFICIENT
    }

    fn normalize_sensor_value(raw: u16) -> f32 {
        linearize_value(raw) / MAX_SENSOR_VALUE as f32
    }
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
)))]
mod imp {
    use crate::screens::components::shared::test_input::FsrView;
    use std::fmt::Write as _;
    use std::path::Path;
    use std::time::SystemTime;

    #[derive(Default)]
    pub struct Monitor;

    impl Monitor {
        pub const fn new() -> Self {
            Self
        }

        pub fn poll_view(&mut self) -> Option<FsrView> {
            None
        }

        pub fn update_threshold(&mut self, _sensor_index: usize, _threshold: u16) -> bool {
            false
        }

        pub fn write_debug_dump(&mut self, path: &Path) -> Result<(), String> {
            let mut out = String::new();
            let _ = writeln!(out, "DeadSync FSR debug dump");
            let _ = writeln!(out, "generated: {:?}", SystemTime::now());
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "FSR HID diagnostics are not available on this platform."
            );
            super::write_dump_file(path, out)
        }
    }
}

pub use imp::Monitor;
