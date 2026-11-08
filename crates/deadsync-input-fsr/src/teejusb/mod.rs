//! Serial configuration for teejusb-compatible firmware, independent of HID input.
mod config;
mod protocol;
mod worker;

use deadsync_input::fsr::{BackendKind, PadDeviceId, PadView};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

#[derive(Default)]
struct Shared {
    enabled: bool,
    active: bool,
    stop: bool,
    flush: bool,
    pads: Vec<PadView>,
    /// Coalesce held-key repeats, but retain the latest desired value until ack.
    pending: BTreeMap<(usize, usize), u16>,
    diagnostics: Vec<String>,
}

#[derive(Default)]
struct Exchange {
    state: Mutex<Shared>,
    wake: Condvar,
}

impl Exchange {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn diagnostic(&self, message: String) {
        let mut state = self.lock();
        if !state.diagnostics.contains(&message) {
            log::warn!("Teejusb FSR: {message}");
            if state.diagnostics.len() >= 16 {
                state.diagnostics.remove(0);
            }
            state.diagnostics.push(message);
        }
    }
}

#[derive(Default)]
pub struct Monitor {
    exchange: Arc<Exchange>,
    worker: Option<JoinHandle<()>>,
    config_path: Option<PathBuf>,
}

impl Monitor {
    pub fn with_data_dir(path: &Path) -> Self {
        Self {
            config_path: Some(path.join("save/teejusb.json")),
            exchange: Arc::default(),
            worker: None,
        }
    }

    fn start(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let exchange = Arc::clone(&self.exchange);
        let path = self.config_path.clone();
        match std::thread::Builder::new()
            .name("teejusb-fsr".into())
            .spawn(move || worker::run(exchange, path))
        {
            Ok(handle) => self.worker = Some(handle),
            Err(e) => self
                .exchange
                .diagnostic(format!("could not start serial worker: {e}")),
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        {
            let mut state = self.exchange.lock();
            if state.enabled == enabled {
                return;
            }
            state.enabled = enabled;
            if !enabled {
                state.flush = true;
            }
        }
        if enabled {
            self.start();
        }
        self.exchange.wake.notify_one();
    }

    pub fn set_active(&mut self, active: bool) {
        if active {
            self.set_enabled(true);
        }
        let mut state = self.exchange.lock();
        state.active = active;
        if !active {
            state.flush = true;
        }
        self.exchange.wake.notify_one();
    }

    pub fn flush_pending_saves(&self) {
        self.exchange.lock().flush = true;
        self.exchange.wake.notify_one();
    }

    pub fn poll_pads_into(&self, pads: &mut Vec<PadView>) {
        let state = self.exchange.lock();
        if !state.enabled {
            return;
        }
        let first = pads.len();
        pads.extend(state.pads.iter().cloned());
        // UI repeats build on the most recently requested value, even when serial
        // acknowledgement is slower than a render frame. Failed writes roll back
        // when the worker removes their pending entries and publishes actual state.
        for pad in &mut pads[first..] {
            for button in &mut pad.buttons {
                for sensor in &mut button.sensors {
                    if let Some(&value) = state
                        .pending
                        .get(&(pad.device_id.index, sensor.firmware_index))
                    {
                        sensor.raw_threshold = value;
                        sensor.threshold_norm = button.value_curve.normalize(value);
                    }
                }
                button.aggregate_threshold = button
                    .sensors
                    .iter()
                    .map(|s| s.raw_threshold)
                    .max()
                    .unwrap_or(0);
            }
        }
    }

    pub fn set_threshold(
        &self,
        device: PadDeviceId,
        button: usize,
        sensor: Option<usize>,
        value: u16,
    ) -> bool {
        if device.backend != BackendKind::Teejusb || value > protocol::MAX_THRESHOLD {
            return false;
        }
        let mut state = self.exchange.lock();
        if !state.enabled {
            return false;
        }
        let Some(pad) = state
            .pads
            .iter()
            .find(|p| p.device_id == device && p.thresholds_editable)
        else {
            return false;
        };
        let Some(button) = pad.buttons.get(button) else {
            return false;
        };
        if value < button.min_raw_threshold || value > button.max_raw_threshold {
            return false;
        }
        let indices: Vec<_> = button
            .sensors
            .iter()
            .filter(|s| sensor.is_none_or(|index| s.firmware_index == index))
            .map(|s| s.firmware_index)
            .collect();
        if indices.is_empty() {
            return false;
        }
        for index in indices {
            state.pending.insert((device.index, index), value);
        }
        self.exchange.wake.notify_one();
        true
    }

    pub fn debug_dump(&self) -> String {
        use std::fmt::Write;
        let state = self.exchange.lock();
        let mut out = format!(
            "[teejusb serial monitor]\nconfig: {:?}\nenabled: {}\nactive: {}\n",
            self.config_path, state.enabled, state.active
        );
        for pad in &state.pads {
            let _ = writeln!(
                out,
                "{}: id={} editable={} status={:?}",
                pad.device_name, pad.device_id.index, pad.thresholds_editable, pad.status
            );
            for button in &pad.buttons {
                for sensor in &button.sensors {
                    let _ = writeln!(
                        out,
                        "  sensor {}: value={} threshold={}",
                        sensor.firmware_index, sensor.raw_value, sensor.raw_threshold
                    );
                }
            }
        }
        let _ = writeln!(out, "pending: {:?}", state.pending);
        for message in &state.diagnostics {
            let _ = writeln!(out, "diagnostic: {message}");
        }
        out
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.exchange.lock().stop = true;
        self.exchange.wake.notify_one();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            log::warn!("Teejusb FSR worker stopped unexpectedly");
        }
    }
}
