use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use btleplug::api::{
    Central, CharPropFlags, Manager as _, Peripheral as _, PeripheralProperties, ScanFilter,
    bleuuid::uuid_from_u16,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{Device, DiscoverySnapshot, PlayerReading};

const HEART_RATE_SERVICE_UUID: u16 = 0x180d;
const HEART_RATE_MEASUREMENT_UUID: u16 = 0x2a37;
const SCAN_POLL_INTERVAL: Duration = Duration::from_millis(250);
const RETRY_INTERVAL: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Desired {
    enabled: bool,
    discover: bool,
    device_ids: [Option<Arc<str>>; 2],
}

#[derive(Debug)]
struct Shared {
    discovery: DiscoverySnapshot,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            discovery: DiscoverySnapshot {
                supported: true,
                scanning: false,
                devices: Vec::new(),
                error: None,
            },
        }
    }
}

// Gameplay reads these two packed latest values without touching the discovery
// lock. Bits 0/1 are configured/connected; the remaining bits encode BPM + 1
// so zero remains `None`.
static PLAYER_READINGS: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
static PLAYER_READINGS_GENERATION: AtomicU64 = AtomicU64::new(0);
static DISCOVERY_GENERATION: AtomicU64 = AtomicU64::new(0);

#[inline(always)]
fn encode_reading(reading: PlayerReading) -> u32 {
    u32::from(reading.configured)
        | (u32::from(reading.connected) << 1)
        | (reading.bpm.map_or(0, |bpm| u32::from(bpm) + 1) << 2)
}

#[inline(always)]
fn decode_reading(bits: u32) -> PlayerReading {
    PlayerReading {
        configured: bits & 1 != 0,
        connected: bits & 2 != 0,
        bpm: (bits >> 2).checked_sub(1).map(|bpm| bpm as u16),
    }
}

#[inline(always)]
fn publish_reading(player: usize, reading: PlayerReading) {
    publish_reading_bits(
        &PLAYER_READINGS[player],
        &PLAYER_READINGS_GENERATION,
        encode_reading(reading),
    );
}

fn publish_reading_bits(reading: &AtomicU32, generation: &AtomicU64, bits: u32) {
    if reading.swap(bits, Ordering::Release) != bits {
        generation.fetch_add(1, Ordering::Release);
    }
}

struct Runtime {
    desired: Arc<Mutex<Desired>>,
    shared: Arc<RwLock<Shared>>,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn configure(enabled: bool, discover: bool, device_ids: [Option<&str>; 2]) {
    if !enabled && RUNTIME.get().is_none() {
        return;
    }
    let runtime = RUNTIME.get_or_init(start_worker);
    let mut desired = runtime.desired.lock().unwrap_or_else(|e| e.into_inner());
    if desired.enabled == enabled
        && desired.discover == discover
        && desired
            .device_ids
            .iter()
            .zip(device_ids)
            .all(|(current, next)| current.as_deref() == next)
    {
        return;
    }
    let changed_players: [bool; 2] =
        std::array::from_fn(|player| desired.device_ids[player].as_deref() != device_ids[player]);
    let next = Desired {
        enabled,
        discover,
        device_ids: device_ids.map(|id| id.map(Arc::from)),
    };
    *desired = next.clone();
    drop(desired);

    let readings = player_readings();
    for (player, (reading, id)) in readings.into_iter().zip(next.device_ids.iter()).enumerate() {
        let reading = if !next.enabled || id.is_none() {
            PlayerReading::default()
        } else if changed_players[player] {
            PlayerReading {
                configured: true,
                ..PlayerReading::default()
            }
        } else {
            PlayerReading {
                configured: true,
                ..reading
            }
        };
        publish_reading(player, reading);
    }
}

pub fn player_readings() -> [PlayerReading; 2] {
    std::array::from_fn(|player| decode_reading(PLAYER_READINGS[player].load(Ordering::Acquire)))
}

pub fn player_readings_generation() -> u64 {
    PLAYER_READINGS_GENERATION.load(Ordering::Acquire)
}

pub fn discovery_generation() -> u64 {
    DISCOVERY_GENERATION.load(Ordering::Acquire)
}

#[inline(always)]
fn mark_discovery_changed() {
    DISCOVERY_GENERATION.fetch_add(1, Ordering::Release);
}

pub fn discovery_snapshot() -> DiscoverySnapshot {
    RUNTIME.get().map_or_else(
        || Shared::default().discovery,
        |runtime| {
            runtime
                .shared
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .discovery
                .clone()
        },
    )
}

fn start_worker() -> Runtime {
    let desired = Arc::new(Mutex::new(Desired::default()));
    let shared = Arc::new(RwLock::new(Shared::default()));
    let worker_desired = Arc::clone(&desired);
    let worker_shared = Arc::clone(&shared);
    thread::Builder::new()
        .name("heart-rate".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match runtime {
                Ok(runtime) => runtime.block_on(worker_loop(worker_desired, worker_shared)),
                Err(error) => set_error(&worker_shared, format!("Heart-rate runtime: {error}")),
            }
        })
        .expect("heart-rate worker thread must start");
    Runtime { desired, shared }
}

#[derive(Debug)]
enum MonitorEvent {
    Connected(Arc<str>, Option<String>),
    Bpm(Arc<str>, u16),
    Disconnected(Arc<str>),
}

async fn worker_loop(desired: Arc<Mutex<Desired>>, shared: Arc<RwLock<Shared>>) {
    loop {
        let enabled = desired.lock().unwrap_or_else(|e| e.into_inner()).enabled;
        if !enabled {
            set_disabled(&shared);
            tokio::time::sleep(SCAN_POLL_INTERVAL).await;
            continue;
        }

        if let Err(error) = run_enabled(&desired, &shared).await {
            set_error(&shared, error);
            tokio::time::sleep(RETRY_INTERVAL).await;
        }
    }
}

async fn run_enabled(
    desired: &Arc<Mutex<Desired>>,
    shared: &Arc<RwLock<Shared>>,
) -> Result<(), String> {
    let manager = Manager::new().await.map_err(|e| e.to_string())?;
    let adapters = manager.adapters().await.map_err(|e| e.to_string())?;
    if adapters.is_empty() {
        return Err("No Bluetooth adapter found".to_owned());
    }
    let scan_filter = ScanFilter {
        services: vec![uuid_from_u16(HEART_RATE_SERVICE_UUID)],
    };

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitors: HashMap<String, JoinHandle<()>> = HashMap::new();
    let mut devices: HashMap<String, (String, Peripheral)> = HashMap::new();
    let mut connecting = HashSet::new();
    let mut last_attempt: HashMap<String, Instant> = HashMap::new();
    let mut scanning = false;

    loop {
        let current = desired.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if !current.enabled {
            stop_scans(&adapters).await;
            let stopped: Vec<String> = monitors.keys().cloned().collect();
            monitors.into_values().for_each(|task| task.abort());
            disconnect_devices(&stopped, &devices).await;
            set_disabled(shared);
            return Ok(());
        }

        let stopped = prune_monitors(&current, &mut monitors, &mut connecting, &mut last_attempt);
        disconnect_devices(&stopped, &devices).await;
        while let Ok(event) = event_rx.try_recv() {
            let (id, label) = apply_monitor_event(&current, event);
            connecting.remove(id.as_ref());
            let label_changed = if let Some(label) = label
                && let Some((current_label, _)) = devices.get_mut(id.as_ref())
                && *current_label != label
            {
                *current_label = label;
                true
            } else {
                false
            };
            if label_changed {
                publish_devices(shared, &devices);
            }
        }

        let missing_device = current
            .device_ids
            .iter()
            .flatten()
            .any(|id| !devices.contains_key(id.as_ref()));
        let should_scan = scan_needed(
            !connecting.is_empty(),
            current.discover,
            !monitors.is_empty(),
            missing_device,
        );
        if should_scan && !scanning {
            start_scans(&adapters, &scan_filter).await?;
            scanning = true;
            set_scanning(shared, true);
        } else if !should_scan && scanning {
            stop_scans(&adapters).await;
            scanning = false;
            set_scanning(shared, false);
        }
        if scanning {
            discover_devices(&adapters, &mut devices).await?;
            publish_devices(shared, &devices);
        }

        let ready = ready_monitors(&current, &devices, &monitors, &last_attempt);
        if !ready.is_empty() {
            // Match the standalone reader: Windows BLE connection setup is
            // unreliable while discovery is still active.
            if scanning {
                stop_scans(&adapters).await;
                scanning = false;
                set_scanning(shared, false);
            }
            spawn_monitors(
                ready,
                &current,
                &event_tx,
                &mut monitors,
                &mut connecting,
                &mut last_attempt,
            );
        }
        tokio::time::sleep(SCAN_POLL_INTERVAL).await;
    }
}

async fn start_scans(adapters: &[Adapter], filter: &ScanFilter) -> Result<(), String> {
    for adapter in adapters {
        adapter
            .start_scan(filter.clone())
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn scan_needed(connecting: bool, discover: bool, has_monitor: bool, missing_device: bool) -> bool {
    !connecting && (missing_device || (discover && !has_monitor))
}

async fn stop_scans(adapters: &[Adapter]) {
    for adapter in adapters {
        let _ = adapter.stop_scan().await;
    }
}

async fn disconnect_devices(ids: &[String], devices: &HashMap<String, (String, Peripheral)>) {
    for id in ids {
        let Some((_, peripheral)) = devices.get(id) else {
            continue;
        };
        let _ = tokio::time::timeout(CONNECT_TIMEOUT, peripheral.disconnect()).await;
    }
}

async fn discover_devices(
    adapters: &[Adapter],
    devices: &mut HashMap<String, (String, Peripheral)>,
) -> Result<(), String> {
    let service = uuid_from_u16(HEART_RATE_SERVICE_UUID);
    for adapter in adapters {
        for peripheral in adapter.peripherals().await.map_err(|e| e.to_string())? {
            let Some(properties) = peripheral.properties().await.map_err(|e| e.to_string())? else {
                continue;
            };
            if !properties.services.contains(&service) {
                continue;
            }
            let id = peripheral.id().to_string();
            let label = device_name(&properties).unwrap_or_else(|| "Heart Rate Monitor".to_owned());
            devices.insert(id, (label, peripheral));
        }
    }
    Ok(())
}

fn device_name(properties: &PeripheralProperties) -> Option<String> {
    [
        properties.local_name.as_deref(),
        properties.advertisement_name.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|name| !name.is_empty())
    .map(str::to_owned)
}

fn publish_devices(shared: &Arc<RwLock<Shared>>, devices: &HashMap<String, (String, Peripheral)>) {
    let mut shared = shared.write().unwrap_or_else(|e| e.into_inner());
    if device_snapshot_matches(&shared.discovery.devices, devices) {
        if shared.discovery.error.take().is_some() {
            mark_discovery_changed();
        }
        return;
    }
    shared.discovery.devices = build_device_snapshot(devices);
    shared.discovery.error = None;
    mark_discovery_changed();
}

fn device_snapshot_matches<T>(snapshot: &[Device], devices: &HashMap<String, (String, T)>) -> bool {
    snapshot.len() == devices.len()
        && snapshot.iter().all(|device| {
            devices
                .get(device.id.as_str())
                .is_some_and(|(label, _)| *label == device.label)
        })
}

fn build_device_snapshot<T>(devices: &HashMap<String, (String, T)>) -> Vec<Device> {
    let mut snapshot: Vec<Device> = devices
        .iter()
        .map(|(id, (label, _))| Device {
            id: id.clone(),
            label: label.clone(),
        })
        .collect();
    snapshot.sort_unstable_by(|a, b| a.label.cmp(&b.label).then_with(|| a.id.cmp(&b.id)));
    snapshot
}

#[inline(always)]
fn device_selected(desired: &Desired, id: &str) -> bool {
    desired
        .device_ids
        .iter()
        .flatten()
        .any(|selected| selected.as_ref() == id)
}

#[inline(always)]
fn selected_earlier(desired: &Desired, player: usize, id: &str) -> bool {
    desired.device_ids[..player]
        .iter()
        .flatten()
        .any(|selected| selected.as_ref() == id)
}

fn prune_monitors(
    desired: &Desired,
    monitors: &mut HashMap<String, JoinHandle<()>>,
    connecting: &mut HashSet<String>,
    last_attempt: &mut HashMap<String, Instant>,
) -> Vec<String> {
    let mut stopped = Vec::new();
    monitors.retain(|id, task| {
        let keep = device_selected(desired, id) && !task.is_finished();
        if !keep {
            task.abort();
            connecting.remove(id);
            stopped.push(id.clone());
        }
        keep
    });
    connecting.retain(|id| device_selected(desired, id) && monitors.contains_key(id));
    last_attempt.retain(|id, _| device_selected(desired, id));
    stopped
}

fn ready_monitors(
    desired: &Desired,
    devices: &HashMap<String, (String, Peripheral)>,
    monitors: &HashMap<String, JoinHandle<()>>,
    last_attempt: &HashMap<String, Instant>,
) -> Vec<(Arc<str>, Peripheral)> {
    let now = Instant::now();
    let mut ready = Vec::new();
    for (player, id) in desired.device_ids.iter().enumerate() {
        let Some(id) = id else { continue };
        if selected_earlier(desired, player, id.as_ref())
            || monitors.contains_key(id.as_ref())
            || last_attempt
                .get(id.as_ref())
                .is_some_and(|last| now.duration_since(*last) < RETRY_INTERVAL)
        {
            continue;
        }
        if let Some((_, peripheral)) = devices.get(id.as_ref()) {
            ready.push((Arc::clone(id), peripheral.clone()));
        }
    }
    ready
}

#[allow(clippy::too_many_arguments)]
fn spawn_monitors(
    ready: Vec<(Arc<str>, Peripheral)>,
    desired: &Desired,
    event_tx: &mpsc::UnboundedSender<MonitorEvent>,
    monitors: &mut HashMap<String, JoinHandle<()>>,
    connecting: &mut HashSet<String>,
    last_attempt: &mut HashMap<String, Instant>,
) {
    // Establish one GATT connection at a time. Windows adapters are notably
    // less reliable when discovery or another connection races service setup.
    for (id, peripheral) in ready.into_iter().take(1) {
        let events = event_tx.clone();
        set_connecting(desired, id.as_ref());
        connecting.insert(id.to_string());
        last_attempt.insert(id.to_string(), Instant::now());
        monitors.insert(
            id.to_string(),
            tokio::spawn(async move {
                if monitor_device(&peripheral, &id, &events).await.is_err() {
                    let _ = tokio::time::timeout(CONNECT_TIMEOUT, peripheral.disconnect()).await;
                    let _ = events.send(MonitorEvent::Disconnected(id));
                }
            }),
        );
    }
}

async fn monitor_device(
    peripheral: &Peripheral,
    id: &Arc<str>,
    events: &mpsc::UnboundedSender<MonitorEvent>,
) -> Result<(), String> {
    let connected = tokio::time::timeout(CONNECT_TIMEOUT, peripheral.is_connected())
        .await
        .map_err(|_| "Timed out checking heart-rate monitor connection".to_owned())?
        .map_err(|e| e.to_string())?;
    if !connected {
        tokio::time::timeout(CONNECT_TIMEOUT, peripheral.connect())
            .await
            .map_err(|_| "Timed out connecting to heart-rate monitor".to_owned())?
            .map_err(|e| e.to_string())?;
    }
    tokio::time::timeout(CONNECT_TIMEOUT, peripheral.discover_services())
        .await
        .map_err(|_| "Timed out discovering heart-rate services".to_owned())?
        .map_err(|e| e.to_string())?;
    let label = match tokio::time::timeout(CONNECT_TIMEOUT, peripheral.properties()).await {
        Ok(Ok(Some(properties))) => device_name(&properties),
        _ => None,
    };
    let measurement_uuid = uuid_from_u16(HEART_RATE_MEASUREMENT_UUID);
    let characteristic = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == measurement_uuid && c.properties.contains(CharPropFlags::NOTIFY))
        .ok_or_else(|| "Heart Rate Measurement notifications are unavailable".to_owned())?;
    let mut notifications = tokio::time::timeout(CONNECT_TIMEOUT, peripheral.notifications())
        .await
        .map_err(|_| "Timed out opening heart-rate notifications".to_owned())?
        .map_err(|e| e.to_string())?;
    tokio::time::timeout(CONNECT_TIMEOUT, peripheral.subscribe(&characteristic))
        .await
        .map_err(|_| "Timed out subscribing to heart-rate notifications".to_owned())?
        .map_err(|e| e.to_string())?;
    let _ = events.send(MonitorEvent::Connected(Arc::clone(id), label));
    while let Some(notification) = notifications.next().await {
        if notification.uuid == measurement_uuid
            && let Ok(bpm) = parse_heart_rate_measurement(&notification.value)
        {
            let _ = events.send(MonitorEvent::Bpm(Arc::clone(id), bpm));
        }
    }
    Err("Heart-rate notification stream ended".to_owned())
}

fn apply_monitor_event(desired: &Desired, event: MonitorEvent) -> (Arc<str>, Option<String>) {
    let (id, connected, bpm, label) = match event {
        MonitorEvent::Connected(id, label) => (id, true, None, label),
        MonitorEvent::Bpm(id, bpm) => (id, true, Some(bpm), None),
        MonitorEvent::Disconnected(id) => (id, false, None, None),
    };
    for (player, selected) in desired.device_ids.iter().enumerate() {
        if selected.as_deref() == Some(id.as_ref()) {
            let current = decode_reading(PLAYER_READINGS[player].load(Ordering::Acquire));
            publish_reading(
                player,
                PlayerReading {
                    configured: true,
                    connected,
                    bpm: bpm.or(current.bpm).filter(|_| connected),
                },
            );
        }
    }
    (id, label)
}

fn set_connecting(desired: &Desired, id: &str) {
    for (player, selected) in desired.device_ids.iter().enumerate() {
        if selected.as_deref() == Some(id) {
            publish_reading(
                player,
                PlayerReading {
                    configured: true,
                    connected: false,
                    bpm: None,
                },
            );
        }
    }
}

fn set_disabled(shared: &Arc<RwLock<Shared>>) {
    let mut shared = shared.write().unwrap_or_else(|e| e.into_inner());
    if shared.discovery.scanning || shared.discovery.error.is_some() {
        shared.discovery.scanning = false;
        shared.discovery.error = None;
        mark_discovery_changed();
    }
    drop(shared);
    for player in 0..2 {
        publish_reading(player, PlayerReading::default());
    }
}

fn set_scanning(shared: &Arc<RwLock<Shared>>, scanning: bool) {
    let mut shared = shared.write().unwrap_or_else(|e| e.into_inner());
    if shared.discovery.scanning != scanning || shared.discovery.error.is_some() {
        shared.discovery.scanning = scanning;
        shared.discovery.error = None;
        mark_discovery_changed();
    }
}

fn set_error(shared: &Arc<RwLock<Shared>>, error: String) {
    let mut shared = shared.write().unwrap_or_else(|e| e.into_inner());
    if shared.discovery.scanning || shared.discovery.error.as_deref() != Some(error.as_str()) {
        shared.discovery.scanning = false;
        shared.discovery.error = Some(error);
        mark_discovery_changed();
    }
    drop(shared);
    for (player, reading) in PLAYER_READINGS.iter().enumerate() {
        let mut reading = decode_reading(reading.load(Ordering::Acquire));
        reading.connected = false;
        reading.bpm = None;
        publish_reading(player, reading);
    }
}

fn parse_heart_rate_measurement(data: &[u8]) -> Result<u16, &'static str> {
    let (&flags, rest) = data.split_first().ok_or("empty heart-rate packet")?;
    if flags & 0x01 == 0 {
        rest.first()
            .copied()
            .map(u16::from)
            .ok_or("missing 8-bit heart-rate value")
    } else {
        let bytes = rest.get(..2).ok_or("missing 16-bit heart-rate value")?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
}

#[cfg(feature = "bench-support")]
pub mod bench_support {
    use super::*;
    use std::hint::black_box;

    #[derive(Clone)]
    struct LegacyDesired {
        device_ids: [Option<String>; 2],
    }

    struct DiscoveryFixture {
        devices: HashMap<String, (String, ())>,
        snapshot: Vec<Device>,
    }

    fn legacy_desired() -> &'static LegacyDesired {
        static DESIRED: OnceLock<LegacyDesired> = OnceLock::new();
        DESIRED.get_or_init(|| LegacyDesired {
            device_ids: [Some("polar-h10".to_owned()), Some("garmin-hrm".to_owned())],
        })
    }

    fn shared_desired() -> &'static Desired {
        static DESIRED: OnceLock<Desired> = OnceLock::new();
        DESIRED.get_or_init(|| Desired {
            enabled: true,
            discover: true,
            device_ids: [Some(Arc::from("polar-h10")), Some(Arc::from("garmin-hrm"))],
        })
    }

    fn discovery_fixture() -> &'static DiscoveryFixture {
        static FIXTURE: OnceLock<DiscoveryFixture> = OnceLock::new();
        FIXTURE.get_or_init(|| {
            let devices = (0..16)
                .map(|index| {
                    (
                        format!("bluetooth-device-{index:02}"),
                        (format!("Heart Rate Monitor {index:02}"), ()),
                    )
                })
                .collect();
            let snapshot = build_device_snapshot(&devices);
            DiscoveryFixture { devices, snapshot }
        })
    }

    pub fn stable_ids_old(events: usize) -> u64 {
        let mut checksum = 0u64;
        for event_index in 0..events {
            let current = black_box(legacy_desired()).clone();
            let bpm = 55 + event_index as u16 % 100;
            let id = current.device_ids[event_index & 1]
                .as_deref()
                .expect("benchmark device ID is configured")
                .to_owned();
            checksum = checksum
                .rotate_left(5)
                .wrapping_add((u64::from(bpm) << 32) | id.len() as u64);
            black_box((id, bpm));
        }
        checksum
    }

    pub fn stable_ids_new(events: usize) -> u64 {
        let mut checksum = 0u64;
        for event_index in 0..events {
            let current = black_box(shared_desired()).clone();
            let bpm = 55 + event_index as u16 % 100;
            let event = MonitorEvent::Bpm(
                Arc::clone(
                    current.device_ids[event_index & 1]
                        .as_ref()
                        .expect("benchmark device ID is configured"),
                ),
                bpm,
            );
            let MonitorEvent::Bpm(id, bpm) = &event else {
                unreachable!("benchmark constructs a BPM event")
            };
            checksum = checksum
                .rotate_left(5)
                .wrapping_add((u64::from(*bpm) << 32) | id.len() as u64);
            black_box(event);
        }
        checksum
    }

    pub fn fixed_selection_old(iterations: usize) -> u64 {
        let desired = legacy_desired();
        let mut checksum = 0u64;
        for _ in 0..iterations {
            let selected: HashSet<&str> = black_box(&desired.device_ids)
                .iter()
                .filter_map(Option::as_deref)
                .collect();
            let missing = selected.iter().any(|id| *id != "polar-h10");

            let selected: HashSet<&str> = desired
                .device_ids
                .iter()
                .filter_map(Option::as_deref)
                .collect();
            let kept = ["polar-h10", "unused"]
                .into_iter()
                .filter(|id| selected.contains(id))
                .count();

            let mut seen = HashSet::new();
            let ready_bytes: usize = desired
                .device_ids
                .iter()
                .filter_map(Option::as_deref)
                .filter(|id| seen.insert(*id))
                .map(str::len)
                .sum();
            checksum = checksum.wrapping_add(
                ((missing as u64) << 32) | ((kept as u64) << 16) | ready_bytes as u64,
            );
        }
        checksum
    }

    pub fn fixed_selection_new(iterations: usize) -> u64 {
        let desired = shared_desired();
        let mut checksum = 0u64;
        for _ in 0..iterations {
            let desired = black_box(desired);
            let missing = desired
                .device_ids
                .iter()
                .flatten()
                .any(|id| id.as_ref() != "polar-h10");
            let kept = ["polar-h10", "unused"]
                .into_iter()
                .filter(|id| device_selected(desired, id))
                .count();
            let ready_bytes = desired
                .device_ids
                .iter()
                .enumerate()
                .filter_map(|(player, id)| id.as_ref().map(|id| (player, id)))
                .filter(|(player, id)| !selected_earlier(desired, *player, id.as_ref()))
                .map(|(_, id)| id.len())
                .sum::<usize>();
            checksum = checksum.wrapping_add(
                ((missing as u64) << 32) | ((kept as u64) << 16) | ready_bytes as u64,
            );
        }
        checksum
    }

    pub fn unchanged_discovery_old(iterations: usize) -> u64 {
        let fixture = discovery_fixture();
        let mut checksum = 0u64;
        for _ in 0..iterations {
            let snapshot = build_device_snapshot(black_box(&fixture.devices));
            let same = snapshot == fixture.snapshot;
            checksum = checksum.wrapping_add(((same as u64) << 32) | snapshot.len() as u64);
            black_box(snapshot);
        }
        checksum
    }

    pub fn unchanged_discovery_new(iterations: usize) -> u64 {
        let fixture = discovery_fixture();
        let mut checksum = 0u64;
        for _ in 0..iterations {
            let same = device_snapshot_matches(
                black_box(fixture.snapshot.as_slice()),
                black_box(&fixture.devices),
            );
            checksum = checksum.wrapping_add(((same as u64) << 32) | fixture.snapshot.len() as u64);
        }
        checksum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_8_bit_heart_rate() {
        assert_eq!(
            parse_heart_rate_measurement(&[0x16, 72, 0x00, 0x04]),
            Ok(72)
        );
    }

    #[test]
    fn parses_16_bit_heart_rate() {
        assert_eq!(
            parse_heart_rate_measurement(&[0x09, 0x2c, 0x01, 0x34, 0x12]),
            Ok(300)
        );
    }

    #[test]
    fn rejects_missing_values() {
        assert!(parse_heart_rate_measurement(&[]).is_err());
        assert!(parse_heart_rate_measurement(&[0x00]).is_err());
        assert!(parse_heart_rate_measurement(&[0x01, 1]).is_err());
    }

    #[test]
    fn packed_player_reading_round_trips_all_states() {
        for configured in [false, true] {
            for connected in [false, true] {
                for bpm in [None, Some(0), Some(72), Some(u16::MAX)] {
                    let reading = PlayerReading {
                        configured,
                        connected,
                        bpm,
                    };
                    assert_eq!(decode_reading(encode_reading(reading)), reading);
                }
            }
        }
    }

    #[test]
    fn reading_generation_advances_only_when_packed_value_changes() {
        let reading = AtomicU32::new(7);
        let generation = AtomicU64::new(11);

        publish_reading_bits(&reading, &generation, 7);
        assert_eq!(generation.load(Ordering::Relaxed), 11);

        publish_reading_bits(&reading, &generation, 9);
        assert_eq!(reading.load(Ordering::Relaxed), 9);
        assert_eq!(generation.load(Ordering::Relaxed), 12);
    }

    #[test]
    fn discovery_generation_advances_only_when_visible_status_changes() {
        let shared = Arc::new(RwLock::new(Shared::default()));
        let generation = discovery_generation();

        set_scanning(&shared, false);
        assert_eq!(discovery_generation(), generation);

        set_scanning(&shared, true);
        let scanning_generation = discovery_generation();
        assert!(scanning_generation > generation);

        set_scanning(&shared, true);
        assert_eq!(discovery_generation(), scanning_generation);
    }

    #[test]
    fn scan_stops_during_connection_and_live_preview() {
        assert!(scan_needed(false, true, false, false));
        assert!(!scan_needed(true, true, false, false));
        assert!(!scan_needed(false, true, true, false));
        assert!(scan_needed(false, false, true, true));
    }

    #[test]
    fn device_name_uses_advertisement_and_prefers_gap_name() {
        let advertisement_only = PeripheralProperties {
            advertisement_name: Some("  COROS PACE  ".to_owned()),
            ..PeripheralProperties::default()
        };
        assert_eq!(
            device_name(&advertisement_only).as_deref(),
            Some("COROS PACE")
        );

        let both = PeripheralProperties {
            local_name: Some("Polar H10".to_owned()),
            advertisement_name: Some("Polar Advertisement".to_owned()),
            ..PeripheralProperties::default()
        };
        assert_eq!(device_name(&both).as_deref(), Some("Polar H10"));
    }

    #[test]
    fn desired_clones_share_configured_device_ids() {
        let desired = Desired {
            enabled: true,
            discover: false,
            device_ids: [Some(Arc::from("polar-h10")), Some(Arc::from("garmin-hrm"))],
        };
        let cloned = desired.clone();

        for player in 0..2 {
            assert!(Arc::ptr_eq(
                desired.device_ids[player].as_ref().unwrap(),
                cloned.device_ids[player].as_ref().unwrap()
            ));
        }

        let event_id = Arc::clone(cloned.device_ids[0].as_ref().unwrap());
        let MonitorEvent::Bpm(event_id, 72) = MonitorEvent::Bpm(event_id, 72) else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(
            desired.device_ids[0].as_ref().unwrap(),
            &event_id
        ));
    }

    #[test]
    fn fixed_selection_matches_hash_set_reference() {
        let cases = [
            [None, None],
            [Some("polar-h10"), None],
            [Some("polar-h10"), Some("garmin-hrm")],
            [Some("polar-h10"), Some("polar-h10")],
        ];

        for ids in cases {
            let desired = Desired {
                enabled: true,
                discover: false,
                device_ids: ids.map(|id| id.map(Arc::from)),
            };
            let selected: HashSet<&str> = desired
                .device_ids
                .iter()
                .filter_map(|id| id.as_deref())
                .collect();
            for probe in ["polar-h10", "garmin-hrm", "missing"] {
                assert_eq!(device_selected(&desired, probe), selected.contains(probe));
            }

            let mut seen = HashSet::new();
            for (player, id) in desired.device_ids.iter().enumerate() {
                let Some(id) = id else { continue };
                assert_eq!(
                    selected_earlier(&desired, player, id.as_ref()),
                    !seen.insert(id.as_ref())
                );
            }
        }
    }

    #[test]
    fn device_snapshot_match_detects_visible_changes() {
        let mut devices = HashMap::from([
            ("b".to_owned(), ("Beta".to_owned(), ())),
            ("a".to_owned(), ("Alpha".to_owned(), ())),
        ]);
        let snapshot = build_device_snapshot(&devices);

        assert_eq!(snapshot[0].id, "a");
        assert!(device_snapshot_matches(&snapshot, &devices));

        devices.get_mut("a").unwrap().0 = "Changed".to_owned();
        assert!(!device_snapshot_matches(&snapshot, &devices));
        devices.remove("b");
        assert!(!device_snapshot_matches(&snapshot, &devices));
    }
}
