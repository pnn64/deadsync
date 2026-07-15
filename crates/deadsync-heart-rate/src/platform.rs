use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use btleplug::api::{
    Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, bleuuid::uuid_from_u16,
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
    device_ids: [Option<String>; 2],
}

#[derive(Debug)]
struct Shared {
    discovery: DiscoverySnapshot,
    readings: [PlayerReading; 2],
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
            readings: [PlayerReading::default(); 2],
        }
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
        device_ids: device_ids.map(|id| id.map(str::to_owned)),
    };
    *desired = next.clone();
    drop(desired);

    let mut shared = runtime.shared.write().unwrap_or_else(|e| e.into_inner());
    for (player, (reading, id)) in shared
        .readings
        .iter_mut()
        .zip(next.device_ids.iter())
        .enumerate()
    {
        if !next.enabled || id.is_none() {
            *reading = PlayerReading::default();
        } else if changed_players[player] {
            *reading = PlayerReading {
                configured: true,
                ..PlayerReading::default()
            };
        } else {
            reading.configured = true;
        }
    }
}

pub fn player_readings() -> [PlayerReading; 2] {
    RUNTIME
        .get()
        .map_or([PlayerReading::default(); 2], |runtime| {
            runtime
                .shared
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .readings
        })
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
    Connected(String),
    Bpm(String, u16),
    Disconnected(String),
}

async fn worker_loop(desired: Arc<Mutex<Desired>>, shared: Arc<RwLock<Shared>>) {
    loop {
        let current = desired.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if !current.enabled {
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
            let id = apply_monitor_event(shared, &current, event);
            connecting.remove(&id);
        }

        let selected: HashSet<&str> = current
            .device_ids
            .iter()
            .flatten()
            .map(String::as_str)
            .collect();
        let missing_device = selected.iter().any(|id| !devices.contains_key(*id));
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
                shared,
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
            let label = properties
                .local_name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "Heart Rate Monitor".to_owned());
            devices.insert(id, (label, peripheral));
        }
    }
    Ok(())
}

fn publish_devices(shared: &Arc<RwLock<Shared>>, devices: &HashMap<String, (String, Peripheral)>) {
    let mut snapshot: Vec<Device> = devices
        .iter()
        .map(|(id, (label, _))| Device {
            id: id.clone(),
            label: label.clone(),
        })
        .collect();
    snapshot.sort_unstable_by(|a, b| a.label.cmp(&b.label).then_with(|| a.id.cmp(&b.id)));
    let mut shared = shared.write().unwrap_or_else(|e| e.into_inner());
    shared.discovery.devices = snapshot;
    shared.discovery.error = None;
}

fn prune_monitors(
    desired: &Desired,
    monitors: &mut HashMap<String, JoinHandle<()>>,
    connecting: &mut HashSet<String>,
    last_attempt: &mut HashMap<String, Instant>,
) -> Vec<String> {
    let selected: HashSet<&str> = desired
        .device_ids
        .iter()
        .flatten()
        .map(String::as_str)
        .collect();
    let mut stopped = Vec::new();
    monitors.retain(|id, task| {
        let keep = selected.contains(id.as_str()) && !task.is_finished();
        if !keep {
            task.abort();
            connecting.remove(id);
            stopped.push(id.clone());
        }
        keep
    });
    connecting.retain(|id| selected.contains(id.as_str()) && monitors.contains_key(id));
    last_attempt.retain(|id, _| selected.contains(id.as_str()));
    stopped
}

fn ready_monitors(
    desired: &Desired,
    devices: &HashMap<String, (String, Peripheral)>,
    monitors: &HashMap<String, JoinHandle<()>>,
    last_attempt: &HashMap<String, Instant>,
) -> Vec<(String, Peripheral)> {
    let now = Instant::now();
    let mut seen = HashSet::new();
    let mut ready = Vec::new();
    for id in desired.device_ids.iter().flatten() {
        if !seen.insert(id.as_str())
            || monitors.contains_key(id)
            || last_attempt
                .get(id)
                .is_some_and(|last| now.duration_since(*last) < RETRY_INTERVAL)
        {
            continue;
        }
        if let Some((_, peripheral)) = devices.get(id) {
            ready.push((id.clone(), peripheral.clone()));
        }
    }
    ready
}

#[allow(clippy::too_many_arguments)]
fn spawn_monitors(
    ready: Vec<(String, Peripheral)>,
    desired: &Desired,
    shared: &Arc<RwLock<Shared>>,
    event_tx: &mpsc::UnboundedSender<MonitorEvent>,
    monitors: &mut HashMap<String, JoinHandle<()>>,
    connecting: &mut HashSet<String>,
    last_attempt: &mut HashMap<String, Instant>,
) {
    // Establish one GATT connection at a time. Windows adapters are notably
    // less reliable when discovery or another connection races service setup.
    for (id, peripheral) in ready.into_iter().take(1) {
        let events = event_tx.clone();
        set_connecting(shared, desired, &id);
        connecting.insert(id.clone());
        last_attempt.insert(id.clone(), Instant::now());
        monitors.insert(
            id.clone(),
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
    id: &str,
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
    let _ = events.send(MonitorEvent::Connected(id.to_owned()));
    while let Some(notification) = notifications.next().await {
        if notification.uuid == measurement_uuid
            && let Ok(bpm) = parse_heart_rate_measurement(&notification.value)
        {
            let _ = events.send(MonitorEvent::Bpm(id.to_owned(), bpm));
        }
    }
    Err("Heart-rate notification stream ended".to_owned())
}

fn apply_monitor_event(
    shared: &Arc<RwLock<Shared>>,
    desired: &Desired,
    event: MonitorEvent,
) -> String {
    let (id, connected, bpm) = match event {
        MonitorEvent::Connected(id) => (id, true, None),
        MonitorEvent::Bpm(id, bpm) => (id, true, Some(bpm)),
        MonitorEvent::Disconnected(id) => (id, false, None),
    };
    let mut shared = shared.write().unwrap_or_else(|e| e.into_inner());
    for (player, selected) in desired.device_ids.iter().enumerate() {
        if selected.as_deref() == Some(id.as_str()) {
            shared.readings[player] = PlayerReading {
                configured: true,
                connected,
                bpm: bpm.or(shared.readings[player].bpm).filter(|_| connected),
            };
        }
    }
    id
}

fn set_connecting(shared: &Arc<RwLock<Shared>>, desired: &Desired, id: &str) {
    let mut shared = shared.write().unwrap_or_else(|e| e.into_inner());
    for (player, selected) in desired.device_ids.iter().enumerate() {
        if selected.as_deref() == Some(id) {
            shared.readings[player] = PlayerReading {
                configured: true,
                connected: false,
                bpm: None,
            };
        }
    }
}

fn set_disabled(shared: &Arc<RwLock<Shared>>) {
    let mut shared = shared.write().unwrap_or_else(|e| e.into_inner());
    shared.discovery.scanning = false;
    shared.discovery.error = None;
    shared.readings = [PlayerReading::default(); 2];
}

fn set_scanning(shared: &Arc<RwLock<Shared>>, scanning: bool) {
    let mut shared = shared.write().unwrap_or_else(|e| e.into_inner());
    shared.discovery.scanning = scanning;
    shared.discovery.error = None;
}

fn set_error(shared: &Arc<RwLock<Shared>>, error: String) {
    let mut shared = shared.write().unwrap_or_else(|e| e.into_inner());
    shared.discovery.scanning = false;
    shared.discovery.error = Some(error);
    for reading in &mut shared.readings {
        reading.connected = false;
        reading.bpm = None;
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
    fn scan_stops_during_connection_and_live_preview() {
        assert!(scan_needed(false, true, false, false));
        assert!(!scan_needed(true, true, false, false));
        assert!(!scan_needed(false, true, true, false));
        assert!(scan_needed(false, false, true, true));
    }
}
