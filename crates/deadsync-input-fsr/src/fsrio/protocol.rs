//! ADP configuration reports and deferred nonvolatile saves. Keep the transport
//! injectable so power-loss behavior can be checked without a connected pad.

use std::time::{Duration, Instant};

pub(super) const SENSOR_COUNT: usize = 12;
const REPORT_CONFIG: u8 = 0x02;
const REPORT_SAVE: u8 = 0x04;
const REPORT_IDENTIFICATION: u8 = 0x09;
const REPORT_PROPERTY: u8 = 0x0B;
const REPORT_SENSOR: u8 = 0x0C;
const SAVE_DELAY: Duration = Duration::from_secs(2);
const MAX_SAVE_ATTEMPTS: u8 = 3;

pub(super) trait Transport {
    fn get_feature(&self, bytes: &mut [u8]) -> Result<usize, ()>;
    fn send_feature(&self, bytes: &[u8]) -> Result<(), ()>;
    fn write_output(&self, bytes: &[u8]) -> Result<usize, ()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ConfigReport {
    pub sensor_thresholds: [u16; SENSOR_COUNT],
    pub release_threshold: f32,
    pub sensor_to_button_mapping: [i8; SENSOR_COUNT],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SensorReport {
    threshold: u16,
    release_threshold: u16,
    button_mapping: i8,
    resistor_value: u8,
    flags: u16,
}

impl SensorReport {
    fn encode(self, index: usize) -> [u8; 10] {
        let press = self.threshold.to_le_bytes();
        let release = self.release_threshold.to_le_bytes();
        let flags = self.flags.to_le_bytes();
        [
            REPORT_SENSOR,
            index as u8,
            press[0],
            press[1],
            release[0],
            release[1],
            self.button_mapping as u8,
            self.resistor_value,
            flags[0],
            flags[1],
        ]
    }

    fn decode(bytes: &[u8], index: usize) -> Option<Self> {
        if bytes.len() < 10 || bytes[0] != REPORT_SENSOR || usize::from(bytes[1]) != index {
            return None;
        }
        Some(Self {
            threshold: u16::from_le_bytes([bytes[2], bytes[3]]),
            release_threshold: u16::from_le_bytes([bytes[4], bytes[5]]),
            button_mapping: bytes[6] as i8,
            resistor_value: bytes[7],
            flags: u16::from_le_bytes([bytes[8], bytes[9]]),
        })
    }

    fn with_threshold(self, threshold: u16) -> Self {
        // ADP Tool keeps the sensor's relative release threshold when dragging
        // its press threshold. A zero press has no usable ratio: keep release 0.
        let release = if self.threshold == 0 {
            0
        } else {
            ((u32::from(threshold) * u32::from(self.release_threshold)
                + u32::from(self.threshold) / 2)
                / u32::from(self.threshold))
            .min(u32::from(threshold)) as u16
        };
        Self {
            threshold,
            release_threshold: release,
            ..self
        }
    }
}

struct PendingSave {
    last_change: Instant,
    last_attempt: Option<Instant>,
    attempts: u8,
}

pub(super) struct Configuration {
    pub report: ConfigReport,
    sensors: Option<[SensorReport; SENSOR_COUNT]>,
    pending: Option<PendingSave>,
}

impl Configuration {
    pub fn read(transport: &impl Transport) -> Result<Self, ()> {
        let mut identification = [0u8; 64];
        identification[0] = REPORT_IDENTIFICATION;
        // Like ADP Tool, boards without identification use the legacy report.
        let modern_sensor_count = match transport.get_feature(&mut identification) {
            Err(()) => None,
            Ok(len) => {
                if len < 10 || identification[0] != REPORT_IDENTIFICATION {
                    return Err(());
                }
                let major = u16::from_le_bytes([identification[1], identification[2]]);
                let minor = u16::from_le_bytes([identification[3], identification[4]]);
                ((major, minor) > (1, 2)).then_some(usize::from(identification[6]))
            }
        };
        let (report, sensors) = if let Some(count) = modern_sensor_count {
            if count == 0 {
                return Err(());
            }
            let mut sensors = [SensorReport {
                button_mapping: -1,
                ..SensorReport::default()
            }; SENSOR_COUNT];
            let mut report = ConfigReport {
                sensor_to_button_mapping: [-1; SENSOR_COUNT],
                ..ConfigReport::default()
            };
            // The current UI exposes twelve sensors. Per-sensor writes leave
            // any additional sensors on newer boards untouched.
            for (index, sensor) in sensors.iter_mut().enumerate().take(count) {
                let mut select = [0u8; 9];
                select[0] = REPORT_PROPERTY;
                select[1..5].copy_from_slice(&2u32.to_le_bytes());
                select[5..9].copy_from_slice(&(index as u32).to_le_bytes());
                transport.send_feature(&select)?;
                let mut bytes = [0u8; 64];
                bytes[0] = REPORT_SENSOR;
                let len = transport.get_feature(&mut bytes)?;
                *sensor = SensorReport::decode(&bytes[..len], index).ok_or(())?;
                report.sensor_thresholds[index] = sensor.threshold;
                report.sensor_to_button_mapping[index] = sensor.button_mapping;
            }
            report.release_threshold = if sensors[0].threshold == 0 {
                0.0
            } else {
                f32::from(sensors[0].release_threshold) / f32::from(sensors[0].threshold)
            };
            (report, Some(sensors))
        } else {
            let mut bytes = [0u8; 41];
            bytes[0] = REPORT_CONFIG;
            let len = transport.get_feature(&mut bytes)?;
            (parse_config_report(&bytes[..len]).ok_or(())?, None)
        };
        Ok(Self {
            report,
            sensors,
            pending: None,
        })
    }

    pub fn set_threshold(
        &mut self,
        transport: &impl Transport,
        indices: &[usize],
        value: u16,
        now: Instant,
    ) -> Result<(), ()> {
        if indices.iter().any(|&index| index >= SENSOR_COUNT) {
            return Err(());
        }
        if let Some(sensors) = &mut self.sensors {
            for &index in indices {
                if sensors[index].threshold == value {
                    continue;
                }
                let next = sensors[index].with_threshold(value);
                transport.send_feature(&next.encode(index))?;
                sensors[index] = next;
                self.report.sensor_thresholds[index] = value;
                // Track each successful write, including a partial group edit.
                self.pending = Some(PendingSave {
                    last_change: now,
                    last_attempt: None,
                    attempts: 0,
                });
            }
        } else {
            let mut next = self.report;
            for &index in indices {
                next.sensor_thresholds[index] = value;
            }
            if next.sensor_thresholds != self.report.sensor_thresholds {
                transport.send_feature(&encode_config_report(&next))?;
                self.report = next;
                self.pending = Some(PendingSave {
                    last_change: now,
                    last_attempt: None,
                    attempts: 0,
                });
            }
        }
        Ok(())
    }

    pub fn save_if_due(&mut self, transport: &impl Transport, now: Instant) -> Result<(), ()> {
        if self.pending.as_ref().is_some_and(|pending| {
            now.duration_since(pending.last_change) >= SAVE_DELAY
                && pending.attempts < MAX_SAVE_ATTEMPTS
                && pending
                    .last_attempt
                    .is_none_or(|last| now.duration_since(last) >= SAVE_DELAY)
        }) {
            self.flush(transport, now)?;
        }
        Ok(())
    }

    pub fn flush(&mut self, transport: &impl Transport, now: Instant) -> Result<(), ()> {
        let Some(pending) = &mut self.pending else {
            return Ok(());
        };
        pending.last_attempt = Some(now);
        pending.attempts = pending.attempts.saturating_add(1);
        // This is an output report, NOT a feature report. ADP Tool pads the
        // command to two bytes because Linux requires at least two.
        if transport.write_output(&[REPORT_SAVE, 0])? == 0 {
            return Err(());
        }
        self.pending = None;
        Ok(())
    }

    pub fn has_pending_save(&self) -> bool {
        self.pending.is_some()
    }
}

fn encode_config_report(config: &ConfigReport) -> [u8; 41] {
    let mut bytes = [0u8; 41];
    bytes[0] = REPORT_CONFIG;
    for (index, threshold) in config.sensor_thresholds.iter().enumerate() {
        bytes[1 + index * 2..3 + index * 2].copy_from_slice(&threshold.to_le_bytes());
    }
    bytes[25..29].copy_from_slice(&config.release_threshold.to_le_bytes());
    for (index, &mapping) in config.sensor_to_button_mapping.iter().enumerate() {
        bytes[29 + index] = mapping as u8;
    }
    bytes
}

pub(super) fn parse_config_report(bytes: &[u8]) -> Option<ConfigReport> {
    if bytes.len() < 41 || bytes[0] != REPORT_CONFIG {
        return None;
    }
    let mut config = ConfigReport::default();
    for (index, threshold) in config.sensor_thresholds.iter_mut().enumerate() {
        *threshold = u16::from_le_bytes([bytes[1 + index * 2], bytes[2 + index * 2]]);
    }
    config.release_threshold = f32::from_le_bytes(bytes[25..29].try_into().ok()?);
    for (index, mapping) in config.sensor_to_button_mapping.iter_mut().enumerate() {
        *mapping = bytes[29 + index] as i8;
    }
    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, PartialEq, Eq)]
    enum Operation {
        Get(u8),
        Feature(Vec<u8>),
        Output(Vec<u8>),
    }

    struct Board {
        firmware: Option<(u16, u16)>,
        active: ConfigReport,
        saved: ConfigReport,
        sensors: [SensorReport; SENSOR_COUNT],
        saved_sensors: [SensorReport; SENSOR_COUNT],
        selected: usize,
        operations: Vec<Operation>,
        fail_sensor: Option<usize>,
        save_failures: usize,
        empty_save: bool,
        wrong_sensor_index: bool,
    }

    struct FakeTransport(RefCell<Board>);

    impl FakeTransport {
        fn new(firmware: Option<(u16, u16)>) -> Self {
            let active = ConfigReport {
                sensor_thresholds: [200; SENSOR_COUNT],
                release_threshold: 0.5,
                sensor_to_button_mapping: [0, 2, 4, 5, -1, -1, -1, -1, -1, -1, -1, -1],
            };
            let sensors = std::array::from_fn(|index| SensorReport {
                threshold: 200,
                release_threshold: 50 + index as u16,
                button_mapping: active.sensor_to_button_mapping[index],
                resistor_value: 77,
                flags: 0xA501,
            });
            Self(RefCell::new(Board {
                firmware,
                active,
                saved: active,
                sensors,
                saved_sensors: sensors,
                selected: 0,
                operations: Vec::new(),
                fail_sensor: None,
                save_failures: 0,
                empty_save: false,
                wrong_sensor_index: false,
            }))
        }

        fn power_cycle(&self) {
            let mut board = self.0.borrow_mut();
            board.active = board.saved;
            board.sensors = board.saved_sensors;
        }

        fn clear_operations(&self) {
            self.0.borrow_mut().operations.clear();
        }

        fn save_attempts(&self) -> usize {
            self.0
                .borrow()
                .operations
                .iter()
                .filter(|op| matches!(op, Operation::Output(_)))
                .count()
        }
    }

    impl Transport for FakeTransport {
        fn get_feature(&self, bytes: &mut [u8]) -> Result<usize, ()> {
            let mut board = self.0.borrow_mut();
            board.operations.push(Operation::Get(bytes[0]));
            match bytes[0] {
                REPORT_IDENTIFICATION => {
                    let (major, minor) = board.firmware.ok_or(())?;
                    bytes[1..3].copy_from_slice(&major.to_le_bytes());
                    bytes[3..5].copy_from_slice(&minor.to_le_bytes());
                    bytes[5] = 16;
                    bytes[6] = SENSOR_COUNT as u8;
                    Ok(42)
                }
                REPORT_CONFIG => {
                    // Emulate the physical board, including loss of unsaved
                    // active state on power_cycle().
                    bytes[..41].copy_from_slice(&encode_config_report(&board.active));
                    Ok(41)
                }
                REPORT_SENSOR => {
                    let index = board.selected;
                    bytes[..10].copy_from_slice(&board.sensors[index].encode(index));
                    if board.wrong_sensor_index {
                        bytes[1] = 99;
                    }
                    Ok(10)
                }
                _ => Err(()),
            }
        }

        fn send_feature(&self, bytes: &[u8]) -> Result<(), ()> {
            let mut board = self.0.borrow_mut();
            board.operations.push(Operation::Feature(bytes.to_vec()));
            match bytes[0] {
                REPORT_PROPERTY => {
                    assert_eq!(&bytes[1..5], &2u32.to_le_bytes());
                    board.selected = u32::from_le_bytes(bytes[5..9].try_into().unwrap()) as usize;
                }
                REPORT_CONFIG => {
                    board.active = parse_config_report(bytes).unwrap();
                }
                REPORT_SENSOR => {
                    let index = usize::from(bytes[1]);
                    if board.fail_sensor == Some(index) {
                        return Err(());
                    }
                    board.sensors[index] = SensorReport::decode(bytes, index).unwrap();
                }
                _ => return Err(()),
            }
            Ok(())
        }

        fn write_output(&self, bytes: &[u8]) -> Result<usize, ()> {
            assert_eq!(bytes, [0x04, 0x00]);
            let mut board = self.0.borrow_mut();
            board.operations.push(Operation::Output(bytes.to_vec()));
            if board.save_failures > 0 {
                board.save_failures -= 1;
                return Err(());
            }
            if board.empty_save {
                return Ok(0);
            }
            board.saved = board.active;
            board.saved_sensors = board.sensors;
            Ok(bytes.len())
        }
    }

    #[test]
    fn legacy_changes_survive_reopen_and_power_loss_after_the_idle_save() {
        let transport = FakeTransport::new(None);
        let mut config = Configuration::read(&transport).unwrap();
        transport.clear_operations();
        let now = Instant::now();
        config.set_threshold(&transport, &[2], 333, now).unwrap();
        // Pin the legacy wire layout independently of the fake board's codec.
        assert_eq!(
            transport.0.borrow().operations,
            vec![Operation::Feature(vec![
                0x02, 200, 0, 200, 0, 77, 1, 200, 0, 200, 0, 200, 0, 200, 0, 200, 0, 200, 0, 200,
                0, 200, 0, 200, 0, 0, 0, 0, 0x3F, 0, 2, 4, 5, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                0xFF, 0xFF,
            ])]
        );
        // Closing/reopening a handle is not a test of persistence.
        assert_eq!(
            Configuration::read(&transport)
                .unwrap()
                .report
                .sensor_thresholds[2],
            333
        );
        assert_eq!(transport.0.borrow().saved.sensor_thresholds[2], 200);
        config
            .save_if_due(&transport, now + SAVE_DELAY - Duration::from_millis(1))
            .unwrap();
        assert_eq!(transport.save_attempts(), 0);
        config.save_if_due(&transport, now + SAVE_DELAY).unwrap();
        assert_eq!(transport.save_attempts(), 1);
        assert!(!config.has_pending_save());
        transport.power_cycle();
        let restored = Configuration::read(&transport).unwrap();
        assert_eq!(restored.report.sensor_thresholds[2], 333);
        assert_eq!(restored.report.sensor_thresholds[1], 200);
        assert_eq!(restored.report.release_threshold, 0.5);
        assert_eq!(restored.report.sensor_to_button_mapping[2], 4);
        config
            .save_if_due(&transport, now + Duration::from_secs(20))
            .unwrap();
        assert_eq!(transport.save_attempts(), 1);
    }

    #[test]
    fn repeated_edits_reset_the_idle_timer_and_batch_one_save() {
        let transport = FakeTransport::new(Some((1, 2)));
        let mut config = Configuration::read(&transport).unwrap();
        let now = Instant::now();
        config.set_threshold(&transport, &[0, 1], 300, now).unwrap();
        config
            .set_threshold(&transport, &[2], 400, now + Duration::from_secs(1))
            .unwrap();
        config
            .save_if_due(&transport, now + Duration::from_secs(2))
            .unwrap();
        assert_eq!(transport.save_attempts(), 0);
        config
            .save_if_due(&transport, now + Duration::from_secs(3))
            .unwrap();
        assert_eq!(transport.save_attempts(), 1);
        transport.power_cycle();
        assert_eq!(
            &transport.0.borrow().active.sensor_thresholds[..3],
            &[300, 300, 400]
        );
    }

    #[test]
    fn leaving_or_shutting_down_flushes_without_waiting_for_the_timer() {
        let transport = FakeTransport::new(None);
        let mut config = Configuration::read(&transport).unwrap();
        let now = Instant::now();
        config.set_threshold(&transport, &[2], 300, now).unwrap();
        config.flush(&transport, now).unwrap();
        config.flush(&transport, now).unwrap();
        assert_eq!(transport.save_attempts(), 1);
        transport.power_cycle();
        assert_eq!(transport.0.borrow().active.sensor_thresholds[2], 300);
    }

    #[test]
    fn modern_edits_preserve_sensor_release_ratios_and_other_configuration() {
        let transport = FakeTransport::new(Some((2, 1)));
        let original = transport.0.borrow().sensors;
        let mut config = Configuration::read(&transport).unwrap();
        assert!(
            !transport
                .0
                .borrow()
                .operations
                .contains(&Operation::Get(REPORT_CONFIG))
        );
        transport.clear_operations();
        let now = Instant::now();
        config.set_threshold(&transport, &[2], 400, now).unwrap();
        // Sensor 2 had 200/52; only its press/release fields change to 400/104.
        assert_eq!(
            transport.0.borrow().operations,
            vec![Operation::Feature(vec![
                0x0C, 2, 0x90, 1, 104, 0, 4, 77, 1, 0xA5
            ])]
        );
        config.save_if_due(&transport, now + SAVE_DELAY).unwrap();
        transport.power_cycle();
        let after = transport.0.borrow().sensors;
        assert_eq!(
            after[2],
            SensorReport {
                threshold: 400,
                release_threshold: 104,
                ..original[2]
            }
        );
        for index in 0..SENSOR_COUNT {
            if index != 2 {
                assert_eq!(after[index], original[index]);
            }
        }
    }

    #[test]
    fn firmware_1_3_also_uses_sensor_reports() {
        let transport = FakeTransport::new(Some((1, 3)));
        let mut config = Configuration::read(&transport).unwrap();
        transport.clear_operations();
        config
            .set_threshold(&transport, &[0], 300, Instant::now())
            .unwrap();
        assert!(
            matches!(&transport.0.borrow().operations[..], [Operation::Feature(bytes)] if bytes[0] == 0x0C)
        );
    }

    #[test]
    fn no_op_edits_neither_write_nor_postpone_an_existing_save() {
        let transport = FakeTransport::new(None);
        let mut config = Configuration::read(&transport).unwrap();
        transport.clear_operations();
        let now = Instant::now();
        config.set_threshold(&transport, &[2], 200, now).unwrap();
        config.flush(&transport, now).unwrap();
        assert!(transport.0.borrow().operations.is_empty());
        config.set_threshold(&transport, &[2], 300, now).unwrap();
        config
            .set_threshold(&transport, &[2], 300, now + Duration::from_secs(1))
            .unwrap();
        config.save_if_due(&transport, now + SAVE_DELAY).unwrap();
        assert_eq!(transport.save_attempts(), 1);
    }

    #[test]
    fn partial_group_failure_still_saves_successful_sensor_writes() {
        let transport = FakeTransport::new(Some((1, 3)));
        let mut config = Configuration::read(&transport).unwrap();
        transport.0.borrow_mut().fail_sensor = Some(1);
        let now = Instant::now();
        assert!(
            config
                .set_threshold(&transport, &[0, 1, 2], 400, now)
                .is_err()
        );
        assert_eq!(&config.report.sensor_thresholds[..3], &[400, 200, 200]);
        assert!(config.has_pending_save());
        config.flush(&transport, now).unwrap();
        transport.power_cycle();
        let sensors = transport.0.borrow().sensors;
        assert_eq!(sensors[0].threshold, 400);
        assert_eq!(sensors[1].threshold, 200);
        assert_eq!(sensors[2].threshold, 200);
    }

    #[test]
    fn failed_sensor_write_does_not_change_the_cache_or_schedule_a_save() {
        let transport = FakeTransport::new(Some((1, 3)));
        let mut config = Configuration::read(&transport).unwrap();
        transport.0.borrow_mut().fail_sensor = Some(2);
        let now = Instant::now();
        assert!(config.set_threshold(&transport, &[2], 400, now).is_err());
        assert_eq!(config.report.sensor_thresholds[2], 200);
        assert!(!config.has_pending_save());
        config.flush(&transport, now).unwrap();
        assert_eq!(transport.save_attempts(), 0);
    }

    #[test]
    fn failed_save_is_retried_after_an_interval_with_a_bound_on_attempts() {
        let transport = FakeTransport::new(None);
        let mut config = Configuration::read(&transport).unwrap();
        let now = Instant::now();
        config.set_threshold(&transport, &[2], 400, now).unwrap();
        transport.0.borrow_mut().save_failures = 10;
        for second in 2..=20 {
            let result = config.save_if_due(&transport, now + Duration::from_secs(second));
            assert_eq!(result.is_err(), matches!(second, 2 | 4 | 6));
        }
        assert_eq!(transport.save_attempts(), 3);
        assert!(config.has_pending_save());
        // Closing still gets a final attempt, even after background retries end.
        transport.0.borrow_mut().save_failures = 0;
        config
            .flush(&transport, now + Duration::from_secs(21))
            .unwrap();
        assert!(!config.has_pending_save());
        transport.power_cycle();
        assert_eq!(transport.0.borrow().active.sensor_thresholds[2], 400);
    }

    #[test]
    fn empty_output_is_a_failed_save_and_a_later_retry_can_succeed() {
        let transport = FakeTransport::new(None);
        let mut config = Configuration::read(&transport).unwrap();
        let now = Instant::now();
        config.set_threshold(&transport, &[2], 400, now).unwrap();
        transport.0.borrow_mut().empty_save = true;
        assert!(config.save_if_due(&transport, now + SAVE_DELAY).is_err());
        assert!(config.has_pending_save());
        transport.0.borrow_mut().empty_save = false;
        config
            .save_if_due(&transport, now + SAVE_DELAY + Duration::from_secs(1))
            .unwrap();
        assert_eq!(transport.save_attempts(), 1);
        config
            .save_if_due(&transport, now + SAVE_DELAY * 2)
            .unwrap();
        assert_eq!(transport.save_attempts(), 2);
        assert!(!config.has_pending_save());
    }

    #[test]
    fn unexpected_sensor_response_never_falls_back_to_a_bulk_write() {
        let transport = FakeTransport::new(Some((1, 3)));
        transport.0.borrow_mut().wrong_sensor_index = true;
        assert!(Configuration::read(&transport).is_err());
        assert!(
            !transport
                .0
                .borrow()
                .operations
                .contains(&Operation::Get(REPORT_CONFIG))
        );
        assert_eq!(transport.save_attempts(), 0);
    }

    #[test]
    fn zero_press_and_rounding_produce_bounded_release_thresholds() {
        let zero = SensorReport {
            threshold: 0,
            release_threshold: 0,
            ..SensorReport::default()
        };
        assert_eq!(zero.with_threshold(100).release_threshold, 0);
        let sensor = SensorReport {
            threshold: 200,
            release_threshold: 99,
            ..zero
        };
        assert_eq!(sensor.with_threshold(101).release_threshold, 50);
        let inverted = SensorReport {
            release_threshold: 400,
            ..sensor
        };
        assert_eq!(inverted.with_threshold(101).release_threshold, 101);
    }
}
