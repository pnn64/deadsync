//! Sample processing is independent of USB, rendering, and file persistence.
use crate::{config::DeviceConfig, protocol::SENSORS};
use std::time::{Duration, Instant};

pub struct Detector {
    pub config: DeviceConfig,
    pub values: [u16; SENSORS],
    pub active: [bool; SENSORS],
    pub panels: u8,
    pub calibration: Option<Calibration>,
    pub calibration_failed: bool,
    pub revision: u64,
}

pub struct Calibration {
    started: Instant,
    count: u32,
    sum: [u64; SENSORS],
    min: [u16; SENSORS],
    max: [u16; SENSORS],
}
impl Detector {
    pub fn new(config: DeviceConfig) -> Self {
        Self {
            config,
            values: [0; SENSORS],
            active: [false; SENSORS],
            panels: 0,
            calibration: None,
            calibration_failed: false,
            revision: 0,
        }
    }
    pub fn calibrate(&mut self, now: Instant) {
        self.calibration = Some(Calibration {
            started: now,
            count: 0,
            sum: [0; SENSORS],
            min: [u16::MAX; SENSORS],
            max: [0; SENSORS],
        });
        self.calibration_failed = false;
        self.release_all();
    }
    pub fn release_all(&mut self) {
        self.active.fill(false);
        self.panels = 0;
    }
    pub fn sample(&mut self, values: [u16; SENSORS], now: Instant) -> u8 {
        self.values = values;
        if let Some(c) = &mut self.calibration {
            c.count += 1;
            for (i, &v) in values.iter().enumerate() {
                c.sum[i] += u64::from(v);
                c.min[i] = c.min[i].min(v);
                c.max[i] = c.max[i].max(v);
            }
            if now.duration_since(c.started) >= Duration::from_millis(500) && c.count >= 32 {
                let stable = (0..SENSORS)
                    .all(|i| !self.config.sensors_enabled[i] || c.max[i] - c.min[i] <= 12);
                if stable {
                    self.config.baseline = Some(std::array::from_fn(|i| {
                        (c.sum[i] / u64::from(c.count)) as u16
                    }));
                    self.revision += 1;
                }
                self.calibration_failed = !stable;
                self.calibration = None;
            }
            return 0;
        }
        let Some(base) = self.config.baseline else {
            return 0;
        };
        self.panels = 0;
        for i in 0..SENSORS {
            let delta = i32::from(values[i]) - i32::from(base[i]);
            if !self.config.sensors_enabled[i] {
                self.active[i] = false;
            } else if !self.active[i] && delta >= i32::from(self.config.press[i]) {
                self.active[i] = true;
            } else if self.active[i] && delta <= i32::from(self.config.release[i]) {
                self.active[i] = false;
            }
            if self.active[i] {
                self.panels |= 1 << (i / 4);
            }
        }
        self.panels
    }
    pub fn set_pair(&mut self, sensor: usize, press: u16, release: u16) -> bool {
        if sensor >= SENSORS || press > 4095 || release >= press {
            return false;
        }
        if self.config.press[sensor] != press || self.config.release[sensor] != release {
            self.config.press[sensor] = press;
            self.config.release[sensor] = release;
            self.revision += 1;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn detector() -> Detector {
        Detector::new(DeviceConfig {
            baseline: Some([1000; 16]),
            ..Default::default()
        })
    }
    #[test]
    fn independent_sensors_or_together_without_premature_release() {
        let now = Instant::now();
        let mut d = detector();
        let mut v = [1000; 16];
        v[0] = 1030;
        assert_eq!(d.sample(v, now), 1);
        v[0] = 1026;
        assert_eq!(d.sample(v, now), 1);
        v[1] = 1030;
        v[0] = 1025;
        assert_eq!(d.sample(v, now), 1);
        v[1] = 1025;
        assert_eq!(d.sample(v, now), 0);
        v[15] = 1030;
        assert_eq!(d.sample(v, now), 8);
        d.config.sensors_enabled[15] = false;
        assert_eq!(d.sample(v, now), 0);
        assert_eq!(d.sample([0; 16], now), 0); // signed baseline subtraction
    }
    #[test]
    fn startup_never_implicitly_zeros_a_loaded_pad() {
        let now = Instant::now();
        let mut d = Detector::new(DeviceConfig::default());
        for _ in 0..100 {
            assert_eq!(d.sample([2000; 16], now), 0);
        }
        assert!(d.config.baseline.is_none());
        d.calibrate(now);
        for i in 0..=50 {
            assert_eq!(d.sample([1000; 16], now + Duration::from_millis(i * 10)), 0);
        }
        assert_eq!(d.config.baseline, Some([1000; 16]));
        assert_eq!(d.revision, 1);
        assert_eq!(d.sample([1030; 16], now + Duration::from_secs(1)), 15);
    }
    #[test]
    fn unstable_calibration_keeps_previous_baseline_and_changes_are_atomic() {
        let now = Instant::now();
        let mut d = detector();
        d.calibrate(now);
        for i in 0..=50 {
            d.sample(
                [1000 + (i % 2) as u16 * 100; 16],
                now + Duration::from_millis(i * 10),
            );
        }
        assert!(d.calibration_failed);
        assert_eq!(d.config.baseline, Some([1000; 16]));
        assert!(!d.set_pair(0, 20, 20));
        assert_eq!(d.config.press[0], 30);
        assert!(d.set_pair(0, 40, 20));
        assert_eq!(d.revision, 1);
        assert!(d.set_pair(0, 40, 20));
        assert_eq!(d.revision, 1);
    }
}
