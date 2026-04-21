#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
mod imp {
    use crate::screens::components::shared::test_input::{FsrBarView, FsrView};
    use hidapi::{HidApi, HidDevice};
    use std::cmp::min;
    use std::time::{Duration, Instant};

    const ADP_VENDOR_ID: u16 = 0x1209;
    const ADP_PRODUCT_ID: u16 = 0xB196;

    const REPORT_ID_SENSOR_VALUES: u8 = 0x01;
    const REPORT_ID_PAD_CONFIGURATION: u8 = 0x02;
    const REPORT_ID_NAME: u8 = 0x05;

    const SENSOR_COUNT: usize = 12;
    const MAX_NAME_SIZE: usize = 50;
    const MAX_SENSOR_VALUE: u16 = 850;
    const LINEARIZATION_POWER: u32 = 4;
    const NTH_DEGREE_COEFFICIENT: f32 = 0.9;
    const FIRST_DEGREE_COEFFICIENT: f32 = 0.1;
    const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);

    #[derive(Clone, Copy, Debug, Default)]
    struct ConfigReport {
        sensor_thresholds: [u16; SENSOR_COUNT],
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
                self.device = None;
                self.device_name = None;
                self.input = InputReport::default();
            }
        }
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

    fn parse_input_report(bytes: &[u8]) -> Option<InputReport> {
        let payload = match bytes {
            [REPORT_ID_SENSOR_VALUES, rest @ ..] if rest.len() >= 2 + SENSOR_COUNT * 2 => rest,
            rest if rest.len() >= 2 + SENSOR_COUNT * 2 => rest,
            _ => return None,
        };

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

        Some(ConfigReport { sensor_thresholds })
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

    #[derive(Default)]
    pub struct Monitor;

    impl Monitor {
        pub const fn new() -> Self {
            Self
        }

        pub fn poll_view(&mut self) -> Option<FsrView> {
            None
        }
    }
}

pub use imp::Monitor;
