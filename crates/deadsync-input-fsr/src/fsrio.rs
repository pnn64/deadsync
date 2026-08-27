#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
mod imp {
    use arrayvec::ArrayVec;
    use deadsync_input::fsr::{
        BackendKind, ButtonLabel, ButtonView, ButtonViews, PadDeviceId, PadView, SensorView,
        SensorViews, ValueCurve,
    };
    use hidapi::{DeviceInfo, HidApi, HidDevice};
    use std::cmp::min;
    use std::ffi::CString;
    use std::fmt::Write as _;
    use std::time::{Duration, Instant, SystemTime};

    const ADP_VENDOR_ID: u16 = 0x1209;
    const ADP_PRODUCT_ID: u16 = 0xB196;

    const REPORT_ID_SENSOR_VALUES: u8 = 0x01;
    const REPORT_ID_PAD_CONFIGURATION: u8 = 0x02;
    const REPORT_ID_NAME: u8 = 0x05;

    const SENSOR_COUNT: usize = 12;
    const BUTTON_COUNT: usize = 16;
    const MAX_NAME_SIZE: usize = 50;
    const MAX_SENSOR_VALUE: u16 = 850;
    #[cfg(any(test, feature = "bench-support"))]
    const LINEARIZATION_POWER: u32 = 4;
    const NTH_DEGREE_COEFFICIENT: f32 = 0.9;
    const FIRST_DEGREE_COEFFICIENT: f32 = 0.1;
    const VALUE_CURVE: ValueCurve = ValueCurve::quartic_blend(
        MAX_SENSOR_VALUE,
        NTH_DEGREE_COEFFICIENT,
        FIRST_DEGREE_COEFFICIENT,
    );
    const SENSOR_NORMALIZED: [f32; MAX_SENSOR_VALUE as usize + 1] = {
        let mut values = [0.0; MAX_SENSOR_VALUE as usize + 1];
        let mut raw = 0usize;
        while raw <= MAX_SENSOR_VALUE as usize {
            values[raw] = VALUE_CURVE.normalize(raw as u16);
            raw += 1;
        }
        values
    };
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

    struct Device {
        id: usize,
        path: CString,
        handle: HidDevice,
        name: String,
        config: ConfigReport,
        input: InputReport,
    }

    #[derive(Default)]
    pub struct Monitor {
        api: Option<HidApi>,
        devices: Vec<Device>,
        next_device_id: usize,
        last_scan_attempt: Option<Instant>,
    }

    impl Monitor {
        pub fn new() -> Self {
            Self::default()
        }

        /// FSRIO streams sensor values continuously, so there's no test mode to
        /// toggle; live reads happen in `poll_pads`.
        pub const fn set_active(&mut self, _active: bool) {}

        /// Expose every connected FSRIO board, grouping sensors by the board's
        /// sensor-to-button mapping.
        pub fn poll_pads(&mut self) -> Vec<PadView> {
            self.ensure_devices();
            self.read_pending_reports();
            self.devices.iter().map(pad_view).collect()
        }

        /// Set the threshold for one or all hardware sensors mapped to a button.
        pub fn set_threshold(
            &mut self,
            device: PadDeviceId,
            button: usize,
            sensor: Option<usize>,
            value: u16,
        ) -> bool {
            if device.backend != BackendKind::Fsrio || value > MAX_SENSOR_VALUE {
                return false;
            }
            self.ensure_devices();
            let Some(device_index) = self.devices.iter().position(|d| d.id == device.index) else {
                return false;
            };
            let device = &mut self.devices[device_index];
            let indices = group_sensor_indices(&device.config.sensor_to_button_mapping, button);
            if indices.is_empty() {
                return false;
            }
            match sensor {
                Some(index) => {
                    if !indices.contains(&index) {
                        return false;
                    }
                    device.config.sensor_thresholds[index] = value;
                }
                None => {
                    for index in indices {
                        device.config.sensor_thresholds[index] = value;
                    }
                }
            }
            if write_config(&device.handle, &device.config).is_ok() {
                return true;
            }
            self.devices.remove(device_index);
            false
        }

        /// FSRIO has no per-sensor enable bit; Advanced exposes thresholds only.
        pub const fn set_sensor_enabled(
            &mut self,
            _device: PadDeviceId,
            _button: usize,
            _sensor: usize,
            _enabled: bool,
        ) -> bool {
            false
        }

        /// FSRIO does not expose auto-recalibration.
        pub const fn set_auto_recalibration(
            &mut self,
            _device: PadDeviceId,
            _enabled: bool,
        ) -> bool {
            false
        }

        /// FSRIO does not expose a panel debounce setting.
        pub const fn set_debounce_micros(&mut self, _device: PadDeviceId, _micros: u16) -> bool {
            false
        }

        pub fn debug_dump(&mut self) -> String {
            self.ensure_devices();
            self.read_pending_reports();
            build_debug_dump(self)
        }

        fn ensure_devices(&mut self) {
            let now = Instant::now();
            if self
                .last_scan_attempt
                .is_some_and(|last| now.duration_since(last) < REOPEN_INTERVAL)
            {
                return;
            }
            self.last_scan_attempt = Some(now);
            if self.api.is_none() {
                self.api = HidApi::new().ok();
            }
            let Some(api) = self.api.as_mut() else {
                return;
            };
            if api.refresh_devices().is_err() {
                return;
            }
            let mut infos: Vec<_> = api
                .device_list()
                .filter(|info| is_known_adp(info))
                .cloned()
                .collect();
            infos.sort_unstable_by(|a, b| a.path().to_bytes().cmp(b.path().to_bytes()));
            self.devices.retain(|device| {
                infos
                    .iter()
                    .any(|info| info.path() == device.path.as_c_str())
            });

            for info in infos {
                if self
                    .devices
                    .iter()
                    .any(|device| device.path.as_c_str() == info.path())
                {
                    continue;
                }
                let Ok(handle) = info.open_device(api) else {
                    continue;
                };
                let name = read_name_from_device(&handle).unwrap_or_else(|()| "FSR Pad".to_owned());
                let Ok(config) = read_config(&handle) else {
                    continue;
                };
                let id = self.next_device_id;
                self.next_device_id = self.next_device_id.wrapping_add(1);
                self.devices.push(Device {
                    id,
                    path: info.path().to_owned(),
                    handle,
                    name,
                    config,
                    input: InputReport::default(),
                });
            }
        }

        fn read_pending_reports(&mut self) {
            let mut index = 0;
            while index < self.devices.len() {
                if read_pending(&mut self.devices[index]) {
                    index += 1;
                } else {
                    self.devices.remove(index);
                }
            }
        }
    }

    fn pad_view(device: &Device) -> PadView {
        PadView {
            device_id: PadDeviceId {
                backend: BackendKind::Fsrio,
                index: device.id,
            },
            device_name: device.name.clone(),
            is_p2_side: false,
            buttons: button_views(&device.config, &device.input),
            supports_advanced: true,
            simple_per_sensor_bars: false,
            supports_sensor_toggle: false,
            auto_recalibration: None,
            debounce_micros: None,
        }
    }

    fn button_views(config: &ConfigReport, input: &InputReport) -> ButtonViews {
        mapped_buttons(&config.sensor_to_button_mapping)
            .into_iter()
            .map(|button| button_view(config, input, button))
            .collect()
    }

    fn button_view(config: &ConfigReport, input: &InputReport, button: usize) -> ButtonView {
        let sensors: SensorViews = sensor_indices(&config.sensor_to_button_mapping, button)
            .into_iter()
            .map(|index| {
                let raw_value = input.sensor_values[index];
                let raw_threshold = config.sensor_thresholds[index];
                SensorView {
                    firmware_index: index,
                    label: None,
                    raw_value,
                    value_norm: normalize_sensor_value(raw_value),
                    raw_threshold,
                    threshold_norm: normalize_sensor_value(raw_threshold),
                    active: raw_value >= raw_threshold && raw_threshold > 0,
                    enabled: true,
                }
            })
            .collect();
        let aggregate_value = sensors.iter().map(|s| s.raw_value).max().unwrap_or(0);
        let aggregate_threshold = sensors.iter().map(|s| s.raw_threshold).max().unwrap_or(0);
        ButtonView {
            label: ButtonLabel::Hid((button + 1) as u8),
            sensors,
            min_raw_threshold: 0,
            max_raw_threshold: MAX_SENSOR_VALUE,
            aggregate_value,
            aggregate_threshold,
            active: aggregate_value >= aggregate_threshold && aggregate_threshold > 0,
            value_curve: VALUE_CURVE,
            release_threshold: None,
        }
    }

    fn read_pending(device: &mut Device) -> bool {
        let mut buf = [0u8; 64];
        loop {
            match device.handle.read_timeout(&mut buf, 0) {
                Ok(0) => return true,
                Ok(len) => {
                    if let Some(report) = parse_input_report(&buf[..len]) {
                        device.input = report;
                    }
                }
                Err(_) => return false,
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
        let _ = writeln!(out, "open_count: {}", monitor.devices.len());
        for device in &monitor.devices {
            let _ = writeln!(out, "device {}:", device.id);
            let _ = writeln!(out, "  path: {}", device.path.to_string_lossy());
            let _ = writeln!(out, "  name: {}", device.name);
            let _ = writeln!(out, "  thresholds: {:?}", device.config.sensor_thresholds);
            let _ = writeln!(
                out,
                "  release_threshold: {:.6}",
                device.config.release_threshold
            );
            let _ = writeln!(
                out,
                "  sensor_to_button_mapping: {:?}",
                device.config.sensor_to_button_mapping
            );
            let _ = writeln!(
                out,
                "  latest_sensor_values: {:?}",
                device.input.sensor_values
            );
        }
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
            "fsr", "force", "dance", "step", "itg", "adp", "arrow", "sensor", "cabinet", "i/o",
            "io board", "arduino", "teensy", "pico", "rp2040", "stm32", "adafruit", "sparkfun",
            "piu", "l-tek", "ltek", "makey",
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

    #[cfg(any(test, feature = "bench-support"))]
    #[allow(clippy::suboptimal_flops)] // Must mirror the table's original, non-fused rounding.
    fn normalize_sensor_value_reference(raw: u16) -> f32 {
        let raw = f32::from(min(raw, MAX_SENSOR_VALUE));
        let max = f32::from(MAX_SENSOR_VALUE);
        let linearized_max = max.powi(LINEARIZATION_POWER as i32) / max;
        let nth = raw.powi(LINEARIZATION_POWER as i32) / linearized_max;
        (nth * NTH_DEGREE_COEFFICIENT + raw * FIRST_DEGREE_COEFFICIENT) / max
    }

    fn normalize_sensor_value(raw: u16) -> f32 {
        SENSOR_NORMALIZED[min(raw, MAX_SENSOR_VALUE) as usize]
    }

    fn mapped_buttons(mapping: &[i8; SENSOR_COUNT]) -> ArrayVec<usize, SENSOR_COUNT> {
        let mut buttons = ArrayVec::new();
        for &mapped in mapping {
            if mapped >= 0 {
                let button = mapped as usize;
                if button < BUTTON_COUNT && !buttons.contains(&button) {
                    buttons.push(button);
                }
            }
        }
        buttons.sort_unstable();
        buttons
    }

    fn group_sensor_indices(
        mapping: &[i8; SENSOR_COUNT],
        group: usize,
    ) -> ArrayVec<usize, SENSOR_COUNT> {
        mapped_buttons(mapping)
            .get(group)
            .map_or_else(ArrayVec::new, |&button| sensor_indices(mapping, button))
    }

    fn sensor_indices(
        mapping: &[i8; SENSOR_COUNT],
        button: usize,
    ) -> ArrayVec<usize, SENSOR_COUNT> {
        mapping
            .iter()
            .enumerate()
            .filter_map(|(index, &mapped)| {
                (mapped >= 0 && mapped as usize == button).then_some(index)
            })
            .collect()
    }

    #[cfg(feature = "bench-support")]
    pub(crate) mod bench_support {
        use super::*;
        use std::hint::black_box;

        const BENCH_BUTTON_COUNT: usize = 4;
        const MAPPING: [i8; SENSOR_COUNT] = [0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3];

        pub fn sensor_groups_old(events: usize) -> u64 {
            let mut checksum = 0u64;
            for event in 0..events {
                for button in 0..BENCH_BUTTON_COUNT {
                    let indices: Vec<_> = (0..SENSOR_COUNT)
                        .filter(|&index| {
                            let mapped = MAPPING[index];
                            mapped >= 0 && mapped as usize == button
                        })
                        .collect();
                    checksum = checksum
                        .wrapping_add(indices.len() as u64)
                        .wrapping_add(indices[(event + button) % indices.len()] as u64);
                    black_box(&indices);
                }
            }
            checksum
        }

        pub fn sensor_groups_new(events: usize) -> u64 {
            let mut checksum = 0u64;
            for event in 0..events {
                for button in 0..BENCH_BUTTON_COUNT {
                    let indices = sensor_indices(&MAPPING, button);
                    checksum = checksum
                        .wrapping_add(indices.len() as u64)
                        .wrapping_add(indices[(event + button) % indices.len()] as u64);
                    black_box(&indices);
                }
            }
            checksum
        }

        pub fn normalization_old(events: usize) -> u64 {
            let mut checksum = 0u64;
            for event in 0..events {
                let raw = (event % (u16::MAX as usize + 1)) as u16;
                checksum = checksum.wrapping_add(u64::from(
                    black_box(normalize_sensor_value_reference(raw)).to_bits(),
                ));
            }
            checksum
        }

        pub fn normalization_new(events: usize) -> u64 {
            let mut checksum = 0u64;
            for event in 0..events {
                let raw = (event % (u16::MAX as usize + 1)) as u16;
                checksum = checksum
                    .wrapping_add(u64::from(black_box(normalize_sensor_value(raw)).to_bits()));
            }
            checksum
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn stack_sensor_groups_match_allocating_reference() {
            let mappings = [
                [-1; SENSOR_COUNT],
                [0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3],
                [3; SENSOR_COUNT],
                [0, -1, 0, -1, 2, -1, 2, -1, 1, 1, 3, 3],
            ];
            for mapping in mappings {
                for button in mapped_buttons(&mapping) {
                    let expected: Vec<_> = mapping
                        .iter()
                        .enumerate()
                        .filter_map(|(index, &mapped)| {
                            (mapped >= 0 && mapped as usize == button).then_some(index)
                        })
                        .collect();
                    assert_eq!(sensor_indices(&mapping, button).as_slice(), expected);
                }
            }
        }

        #[test]
        fn mapped_button_views_follow_firmware_mapping() {
            let mut config = ConfigReport {
                sensor_to_button_mapping: [0, 2, 4, 5, -1, -1, -1, -1, -1, -1, -1, -1],
                ..ConfigReport::default()
            };
            config.sensor_thresholds[..4].copy_from_slice(&[100, 200, 300, 400]);
            let mut input = InputReport::default();
            input.sensor_values[..4].copy_from_slice(&[90, 250, 350, 390]);

            let buttons = button_views(&config, &input);
            assert_eq!(buttons.len(), 4);
            assert_eq!(
                buttons
                    .iter()
                    .map(|button| button.label)
                    .collect::<Vec<_>>(),
                [
                    ButtonLabel::Hid(1),
                    ButtonLabel::Hid(3),
                    ButtonLabel::Hid(5),
                    ButtonLabel::Hid(6),
                ]
            );
            assert_eq!(buttons[2].sensors[0].firmware_index, 2);
            assert_eq!(buttons[2].aggregate_threshold, 300);
            assert!(buttons[2].active);
            assert!(!buttons[3].active);
        }

        #[test]
        fn stock_fsrio_mapping_exposes_all_eight_buttons() {
            let mut mapping = [-1; SENSOR_COUNT];
            for (index, mapped) in mapping[..8].iter_mut().enumerate() {
                *mapped = index as i8;
            }
            assert_eq!(
                mapped_buttons(&mapping).as_slice(),
                [0, 1, 2, 3, 4, 5, 6, 7]
            );
            assert_eq!(group_sensor_indices(&mapping, 7).as_slice(), [7]);
            assert!(group_sensor_indices(&mapping, 8).is_empty());
        }

        #[test]
        fn normalization_table_matches_original_formula() {
            for raw in 0..=u16::MAX {
                assert_eq!(
                    normalize_sensor_value(raw).to_bits(),
                    normalize_sensor_value_reference(raw).to_bits(),
                    "raw value {raw}"
                );
            }
        }
    }
}

pub use imp::Monitor;

#[cfg(feature = "bench-support")]
pub(crate) use imp::bench_support;
