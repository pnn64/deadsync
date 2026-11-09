//! Native RE:Flex v2 input, host calibration, and lights. The editor reads this
//! service's snapshots; closing the editor never stops gameplay sensor reads.
mod config;
mod detection;
mod mapping;
mod protocol;
mod view;
pub use mapping::install_default_bindings;
/// Vendor-specific codes avoid matching generic joystick button bindings.
pub const BUTTON_CODE_BASE: u32 = 0x5246_0000;
#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
mod io;

use config::Config;
pub use config::LightMode;
use deadlib_input_native::{GpSystemEvent, uuid_from_bytes};
use deadlib_platform::input::{PadEvent, REFLEX_PAD_ID_BASE};
use deadsync_input::fsr::{BackendKind, HostPadAction, PadDeviceId, PadView};
use detection::Detector;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(crate) struct Device {
    token: usize,
    serial: String,
    uuid: [u8; 16],
    player: usize,
    name: Arc<str>,
    core: Mutex<Detector>,
    alive: AtomicBool,
    test_until: Mutex<Option<Instant>>,
    light_error: AtomicBool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct LightFrame {
    pub chart: [bool; 4],
    pub judgment: [[u8; 3]; 4],
    /// The Test Lights screen overrides the selected gameplay mode.
    pub testing: bool,
}
struct Registry {
    config: Config,
    devices: Vec<Arc<Device>>,
    diagnostics: Vec<String>,
}
struct Service {
    path: PathBuf,
    registry: Mutex<Registry>,
    lights: Mutex<([LightFrame; 2], Instant)>,
    stop: AtomicBool,
    save_failed: AtomicBool,
    thread: Mutex<Option<JoinHandle<()>>>,
    input: Arc<dyn Fn(PadEvent) + Send + Sync>,
    system: Arc<dyn Fn(GpSystemEvent) + Send + Sync>,
}
static SERVICE: OnceLock<Arc<Service>> = OnceLock::new();

pub fn start(
    data_dir: &Path,
    input: impl Fn(PadEvent) + Send + Sync + 'static,
    system: impl Fn(GpSystemEvent) + Send + Sync + 'static,
) {
    if std::env::var_os("DEADSYNC_MOCK_PADS").is_some() || SERVICE.get().is_some() {
        return;
    }
    let path = data_dir.join("save/reflex.json");
    let (config, diagnostics) = match Config::load(&path) {
        Ok(c) => (c, Vec::new()),
        Err(e) => {
            let error = format!(
                "RE:Flex settings {}: {e}; service disabled, file preserved",
                path.display()
            );
            log::error!("{error}");
            (
                Config {
                    enabled: false,
                    ..Default::default()
                },
                vec![error],
            )
        }
    };
    let service = Arc::new(Service {
        path,
        registry: Mutex::new(Registry {
            config,
            devices: Vec::new(),
            diagnostics,
        }),
        lights: Mutex::new(([LightFrame::default(); 2], Instant::now())),
        stop: AtomicBool::new(false),
        save_failed: AtomicBool::new(false),
        thread: Mutex::new(None),
        input: Arc::new(input),
        system: Arc::new(system),
    });
    if SERVICE.set(service.clone()).is_err() {
        return;
    }
    #[cfg(any(
        windows,
        target_os = "linux",
        target_os = "freebsd",
        target_os = "macos"
    ))]
    if service.registry.lock().unwrap().config.enabled {
        let worker = service.clone();
        match std::thread::Builder::new()
            .name("reflex-discovery".into())
            .spawn(move || io::run(worker))
        {
            Ok(thread) => *service.thread.lock().unwrap() = Some(thread),
            Err(e) => service.diagnostic(format!("RE:Flex worker failed: {e}")),
        }
    }
}
impl Service {
    fn diagnostic(&self, message: String) {
        let mut r = self.registry.lock().unwrap();
        if r.diagnostics.last() == Some(&message) {
            return;
        }
        log::warn!("{message}");
        if r.diagnostics.len() == 16 {
            r.diagnostics.remove(0);
        }
        r.diagnostics.push(message);
    }
}

pub fn shutdown() {
    if let Some(s) = SERVICE.get() {
        s.stop.store(true, Ordering::Release);
        if let Some(thread) = s.thread.lock().unwrap().take()
            && thread.join().is_err()
        {
            log::error!("RE:Flex service panicked during shutdown");
        }
    }
}
pub fn connected() -> bool {
    SERVICE.get().is_some_and(|s| {
        s.registry
            .lock()
            .unwrap()
            .devices
            .iter()
            .any(|d| d.alive.load(Ordering::Acquire))
    })
}
pub fn poll_pads_into(pads: &mut Vec<PadView>) {
    if let Some(s) = SERVICE.get() {
        for d in &s.registry.lock().unwrap().devices {
            if d.alive.load(Ordering::Acquire) {
                pads.push(view::pad(d, s.save_failed.load(Ordering::Relaxed)));
            }
        }
    }
}
fn device(id: PadDeviceId) -> Option<Arc<Device>> {
    SERVICE.get()?.registry.lock().unwrap().device(id)
}
impl Registry {
    fn device(&self, id: PadDeviceId) -> Option<Arc<Device>> {
        if id.backend != BackendKind::Reflex {
            return None;
        }
        self.devices
            .iter()
            .find(|d| d.token == id.index && d.alive.load(Ordering::Acquire))
            .cloned()
    }
}
pub fn set_threshold(id: PadDeviceId, button: usize, sensor: Option<usize>, value: u16) -> bool {
    let Some(d) = device(id) else { return false };
    if button >= 4 || value == 0 || value > 4095 || sensor.is_some_and(|s| s / 4 != button) {
        return false;
    }
    let mut core = d.core.lock().unwrap();
    for i in button * 4..button * 4 + 4 {
        if sensor.is_none_or(|s| s == i) {
            let gap = core.config.press[i] - core.config.release[i];
            core.set_pair(i, value, value.saturating_sub(gap));
        }
    }
    true
}
pub fn set_sensor_pair(
    id: PadDeviceId,
    button: usize,
    sensor: usize,
    press: u16,
    release: u16,
) -> bool {
    if button >= 4 || sensor / 4 != button {
        return false;
    }
    device(id).is_some_and(|d| d.core.lock().unwrap().set_pair(sensor, press, release))
}
pub fn set_sensor_enabled(id: PadDeviceId, button: usize, sensor: usize, enabled: bool) -> bool {
    let Some(d) = device(id) else { return false };
    if button >= 4 || sensor / 4 != button {
        return false;
    }
    let mut c = d.core.lock().unwrap();
    if c.config.sensors_enabled[sensor] != enabled {
        c.config.sensors_enabled[sensor] = enabled;
        c.revision += 1;
    }
    true
}
pub fn host_action(id: PadDeviceId, action: HostPadAction) -> bool {
    let Some(d) = device(id) else { return false };
    if action == HostPadAction::SwitchPlayer {
        let Some(s) = SERVICE.get() else { return false };
        let mut r = s.registry.lock().unwrap();
        // Reconnect after releasing the old slots; never reuse an input slot
        // while another reader can still deliver events for it.
        let old = d.player;
        for c in r.config.devices.values_mut() {
            c.player = 1 - c.player;
        }
        for other in &r.devices {
            let mut core = other.core.lock().unwrap();
            core.config.player = 1 - core.config.player;
            core.revision += 1;
            other.alive.store(false, Ordering::Release);
        }
        log::info!(
            "RE:Flex {} reassigned from P{} to P{}",
            d.serial,
            old + 1,
            2 - old
        );
        return true;
    }
    let mut c = d.core.lock().unwrap();
    match action {
        HostPadAction::Calibrate => c.calibrate(Instant::now()),
        HostPadAction::Brightness(delta) => {
            c.config.brightness = (i16::from(c.config.brightness) + delta).clamp(0, 100) as u8;
            c.revision += 1;
        }
        HostPadAction::CycleLights => {
            c.config.lights = c.config.lights.next();
            c.revision += 1;
        }
        HostPadAction::TestLights => {
            *d.test_until.lock().unwrap() = Some(Instant::now() + Duration::from_secs(3))
        }
        HostPadAction::SwitchPlayer => unreachable!(),
    }
    true
}
pub fn set_lights(frames: [LightFrame; 2]) {
    if let Some(s) = SERVICE.get() {
        *s.lights.lock().unwrap() = (frames, Instant::now());
    }
}
pub fn trigger_label(id: usize, code: u32) -> Option<String> {
    let panel = protocol::PANEL_NAMES.get(code.checked_sub(BUTTON_CODE_BASE)? as usize)?;
    if !(REFLEX_PAD_ID_BASE..REFLEX_PAD_ID_BASE + 2).contains(&id) {
        return None;
    }
    Some(format!("RE:Flex P{} {panel}", id - REFLEX_PAD_ID_BASE + 1))
}
pub fn needs_default_mapping(player: usize) -> bool {
    player < 2
        && SERVICE.get().is_some_and(|s| {
            let r = s.registry.lock().unwrap();
            !r.config.mapping_initialized[player]
                && r.devices
                    .iter()
                    .any(|d| d.player == player && d.alive.load(Ordering::Acquire))
        })
}
pub fn mark_mapping_initialized(player: usize) {
    if player < 2
        && let Some(s) = SERVICE.get()
    {
        s.registry.lock().unwrap().config.mapping_initialized[player] = true;
    }
}
pub fn debug_dump() -> String {
    let Some(s) = SERVICE.get() else {
        return "RE:Flex: service not started\n".into();
    };
    let r = s.registry.lock().unwrap();
    let mut out = format!(
        "RE:Flex settings: {}\nenabled: {}\n",
        s.path.display(),
        r.config.enabled
    );
    use std::fmt::Write;
    for d in &r.devices {
        let c = d.core.lock().unwrap();
        let _ = writeln!(
            out,
            "{} P{} token={} raw={:?} baseline={:?} press={:?} release={:?} active={:?}",
            d.serial,
            d.player + 1,
            d.token,
            c.values,
            c.config.baseline,
            c.config.press,
            c.config.release,
            c.active
        );
    }
    for diagnostic in &r.diagnostics {
        let _ = writeln!(out, "{diagnostic}");
    }
    out
}
