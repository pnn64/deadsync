use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::Duration;

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Desired {
    enabled: bool,
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

pub fn configure(enabled: bool, device_ids: [Option<&str>; 2]) {
    if !enabled && RUNTIME.get().is_none() {
        return;
    }
    let runtime = RUNTIME.get_or_init(start_worker);
    let mut desired = runtime.desired.lock().unwrap_or_else(|e| e.into_inner());
    if desired.enabled == enabled
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
    for adapter in &adapters {
        adapter
            .start_scan(scan_filter.clone())
            .await
            .map_err(|e| e.to_string())?;
    }
    set_scanning(shared, true);

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitors: HashMap<String, JoinHandle<()>> = HashMap::new();
    let mut devices: HashMap<String, (String, Peripheral)> = HashMap::new();

    loop {
        let current = desired.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if !current.enabled {
            stop_scans(&adapters).await;
            monitors.into_values().for_each(|task| task.abort());
            set_disabled(shared);
            return Ok(());
        }

        discover_devices(&adapters, &mut devices).await?;
        publish_devices(shared, &devices);
        reconcile_monitors(&current, &devices, &event_tx, &mut monitors);
        while let Ok(event) = event_rx.try_recv() {
            apply_monitor_event(shared, &current, event);
        }
        tokio::time::sleep(SCAN_POLL_INTERVAL).await;
    }
}

async fn stop_scans(adapters: &[Adapter]) {
    for adapter in adapters {
        let _ = adapter.stop_scan().await;
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
            let label = properties.local_name.unwrap_or_else(|| id.clone());
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
    shared.discovery.scanning = true;
    shared.discovery.error = None;
}

fn reconcile_monitors(
    desired: &Desired,
    devices: &HashMap<String, (String, Peripheral)>,
    event_tx: &mpsc::UnboundedSender<MonitorEvent>,
    monitors: &mut HashMap<String, JoinHandle<()>>,
) {
    let selected: HashSet<&str> = desired
        .device_ids
        .iter()
        .flatten()
        .map(String::as_str)
        .collect();
    monitors.retain(|id, task| {
        let keep = selected.contains(id.as_str()) && !task.is_finished();
        if !keep {
            task.abort();
        }
        keep
    });
    for id in selected {
        if monitors.contains_key(id) {
            continue;
        }
        let Some((_, peripheral)) = devices.get(id) else {
            continue;
        };
        let id = id.to_owned();
        let peripheral = peripheral.clone();
        let events = event_tx.clone();
        monitors.insert(
            id.clone(),
            tokio::spawn(async move {
                if monitor_device(&peripheral, &id, &events).await.is_err() {
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
    if !peripheral.is_connected().await.map_err(|e| e.to_string())? {
        peripheral.connect().await.map_err(|e| e.to_string())?;
    }
    peripheral
        .discover_services()
        .await
        .map_err(|e| e.to_string())?;
    let measurement_uuid = uuid_from_u16(HEART_RATE_MEASUREMENT_UUID);
    let characteristic = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == measurement_uuid && c.properties.contains(CharPropFlags::NOTIFY))
        .ok_or_else(|| "Heart Rate Measurement notifications are unavailable".to_owned())?;
    let mut notifications = peripheral
        .notifications()
        .await
        .map_err(|e| e.to_string())?;
    peripheral
        .subscribe(&characteristic)
        .await
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

fn apply_monitor_event(shared: &Arc<RwLock<Shared>>, desired: &Desired, event: MonitorEvent) {
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
}
