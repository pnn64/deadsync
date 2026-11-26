use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use btleplug::api::{
    Central, CharPropFlags, Manager as _, Peripheral as _, PeripheralProperties, ScanFilter,
};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{Device, Event, EventHandler, Selection, Shared, Subscription};

pub const SUPPORTED: bool = true;

const SCAN_POLL_INTERVAL: Duration = Duration::from_millis(250);
const RETRY_INTERVAL: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn start_worker<const N: usize>(
    subscription: Subscription,
    desired: Arc<Mutex<Selection<N>>>,
    shared: Arc<Shared>,
    on_event: EventHandler<N>,
) {
    thread::Builder::new()
        .name("ble-monitor".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match runtime {
                Ok(runtime) => {
                    runtime.block_on(worker_loop(subscription, desired, shared, on_event))
                }
                Err(error) => {
                    set_error(&shared, format!("Bluetooth runtime: {error}"));
                    on_event(&Selection::default(), Event::Unavailable);
                }
            }
        })
        .expect("BLE worker thread must start");
}

#[derive(Debug)]
enum MonitorEvent {
    Connected(Arc<str>, Option<String>),
    Notification(Arc<str>, Vec<u8>),
    Disconnected(Arc<str>),
}

async fn worker_loop<const N: usize>(
    subscription: Subscription,
    desired: Arc<Mutex<Selection<N>>>,
    shared: Arc<Shared>,
    on_event: EventHandler<N>,
) {
    while !shared.stopped.load(Ordering::Acquire) {
        let enabled = desired
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .enabled;
        if !enabled {
            set_disabled(&shared);
            on_event(&Selection::default(), Event::Disabled);
            tokio::time::sleep(SCAN_POLL_INTERVAL).await;
            continue;
        }

        if let Err(error) = run_enabled(subscription, &desired, &shared, on_event).await {
            set_error(&shared, error);
            on_event(&Selection::default(), Event::Unavailable);
            tokio::time::sleep(RETRY_INTERVAL).await;
        }
    }
}

async fn run_enabled<const N: usize>(
    subscription: Subscription,
    desired: &Mutex<Selection<N>>,
    shared: &Shared,
    on_event: EventHandler<N>,
) -> Result<(), String> {
    let manager = Manager::new().await.map_err(|e| e.to_string())?;
    let adapters = manager.adapters().await.map_err(|e| e.to_string())?;
    if adapters.is_empty() {
        return Err("No Bluetooth adapter found".to_owned());
    }
    let scan_filter = ScanFilter {
        services: vec![subscription.service],
    };

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitors: HashMap<String, JoinHandle<()>> = HashMap::new();
    let mut devices: HashMap<String, (String, Peripheral)> = HashMap::new();
    let mut connecting = HashSet::new();
    let mut last_attempt: HashMap<String, Instant> = HashMap::new();
    let mut scanning = false;

    loop {
        let current = desired
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if !current.enabled || shared.stopped.load(Ordering::Acquire) {
            stop_scans(&adapters).await;
            let stopped: Vec<String> = monitors
                .into_iter()
                .map(|(id, task)| {
                    task.abort();
                    id
                })
                .collect();
            disconnect_devices(&stopped, &devices).await;
            set_disabled(shared);
            on_event(&current, Event::Disabled);
            return Ok(());
        }

        let stopped = prune_monitors(&current, &mut monitors, &mut connecting, &mut last_attempt);
        disconnect_devices(&stopped, &devices).await;
        while let Ok(event) = event_rx.try_recv() {
            let (id, label) = match event {
                MonitorEvent::Connected(id, label) => {
                    on_event(&current, Event::Connected(&id));
                    (id, label)
                }
                MonitorEvent::Notification(id, value) => {
                    on_event(
                        &current,
                        Event::Notification {
                            device_id: &id,
                            value: &value,
                        },
                    );
                    (id, None)
                }
                MonitorEvent::Disconnected(id) => {
                    on_event(&current, Event::Disconnected(&id));
                    (id, None)
                }
            };
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
            discover_devices(&adapters, subscription, &mut devices).await?;
            publish_devices(shared, &devices);
        }

        if let Some((id, peripheral)) = ready_monitor(
            &current,
            &devices,
            &monitors,
            &last_attempt,
            !connecting.is_empty(),
        ) {
            // Windows adapters are less reliable when scanning or another
            // connection attempt overlaps GATT setup.
            if scanning {
                stop_scans(&adapters).await;
                scanning = false;
                set_scanning(shared, false);
            }
            on_event(&current, Event::Connecting(&id));
            connecting.insert(id.to_string());
            last_attempt.insert(id.to_string(), Instant::now());
            let events = event_tx.clone();
            let characteristic = subscription.characteristic;
            monitors.insert(
                id.to_string(),
                tokio::spawn(async move {
                    if monitor_device(&peripheral, characteristic, &id, &events)
                        .await
                        .is_err()
                    {
                        let _ =
                            tokio::time::timeout(CONNECT_TIMEOUT, peripheral.disconnect()).await;
                        let _ = events.send(MonitorEvent::Disconnected(id));
                    }
                }),
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

const fn scan_needed(
    connecting: bool,
    discover: bool,
    has_monitor: bool,
    missing_device: bool,
) -> bool {
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
    subscription: Subscription,
    devices: &mut HashMap<String, (String, Peripheral)>,
) -> Result<(), String> {
    for adapter in adapters {
        for peripheral in adapter.peripherals().await.map_err(|e| e.to_string())? {
            let Some(properties) = peripheral.properties().await.map_err(|e| e.to_string())? else {
                continue;
            };
            if !properties.services.contains(&subscription.service) {
                continue;
            }
            let id = peripheral.id().to_string();
            let label = device_name(&properties)
                .unwrap_or_else(|| subscription.unnamed_device_label.to_owned());
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

fn publish_devices(shared: &Shared, devices: &HashMap<String, (String, Peripheral)>) {
    let mut discovery = shared
        .discovery
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if device_snapshot_matches(&discovery.devices, devices) {
        if discovery.error.take().is_some() {
            shared.generation.fetch_add(1, Ordering::Release);
        }
        return;
    }
    discovery.devices = build_device_snapshot(devices);
    discovery.error = None;
    shared.generation.fetch_add(1, Ordering::Release);
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
fn device_selected<const N: usize>(desired: &Selection<N>, id: &str) -> bool {
    desired
        .device_ids
        .iter()
        .flatten()
        .any(|selected| selected.as_ref() == id)
}

fn prune_monitors<const N: usize>(
    desired: &Selection<N>,
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

fn ready_monitor<const N: usize, T: Clone>(
    desired: &Selection<N>,
    devices: &HashMap<String, (String, T)>,
    monitors: &HashMap<String, JoinHandle<()>>,
    last_attempt: &HashMap<String, Instant>,
    connecting: bool,
) -> Option<(Arc<str>, T)> {
    if connecting {
        return None;
    }
    let now = Instant::now();
    for id in desired.device_ids.iter().flatten() {
        if monitors.contains_key(id.as_ref())
            || last_attempt
                .get(id.as_ref())
                .is_some_and(|last| now.duration_since(*last) < RETRY_INTERVAL)
        {
            continue;
        }
        if let Some((_, peripheral)) = devices.get(id.as_ref()) {
            return Some((Arc::clone(id), peripheral.clone()));
        }
    }
    None
}

async fn monitor_device(
    peripheral: &Peripheral,
    characteristic_uuid: crate::Uuid,
    id: &Arc<str>,
    events: &mpsc::UnboundedSender<MonitorEvent>,
) -> Result<(), String> {
    let connected = tokio::time::timeout(CONNECT_TIMEOUT, peripheral.is_connected())
        .await
        .map_err(|_| "Timed out checking BLE device connection".to_owned())?
        .map_err(|e| e.to_string())?;
    if !connected {
        tokio::time::timeout(CONNECT_TIMEOUT, peripheral.connect())
            .await
            .map_err(|_| "Timed out connecting to BLE device".to_owned())?
            .map_err(|e| e.to_string())?;
    }
    tokio::time::timeout(CONNECT_TIMEOUT, peripheral.discover_services())
        .await
        .map_err(|_| "Timed out discovering BLE services".to_owned())?
        .map_err(|e| e.to_string())?;
    let label = match tokio::time::timeout(CONNECT_TIMEOUT, peripheral.properties()).await {
        Ok(Ok(Some(properties))) => device_name(&properties),
        _ => None,
    };
    let characteristic = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == characteristic_uuid && c.properties.contains(CharPropFlags::NOTIFY))
        .ok_or_else(|| "BLE characteristic notifications are unavailable".to_owned())?;
    let mut notifications = tokio::time::timeout(CONNECT_TIMEOUT, peripheral.notifications())
        .await
        .map_err(|_| "Timed out opening BLE notifications".to_owned())?
        .map_err(|e| e.to_string())?;
    tokio::time::timeout(CONNECT_TIMEOUT, peripheral.subscribe(&characteristic))
        .await
        .map_err(|_| "Timed out subscribing to BLE notifications".to_owned())?
        .map_err(|e| e.to_string())?;
    let _ = events.send(MonitorEvent::Connected(Arc::clone(id), label));
    while let Some(notification) = notifications.next().await {
        if notification.uuid == characteristic_uuid {
            let _ = events.send(MonitorEvent::Notification(
                Arc::clone(id),
                notification.value,
            ));
        }
    }
    Err("BLE notification stream ended".to_owned())
}

fn set_disabled(shared: &Shared) {
    let mut discovery = shared
        .discovery
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if discovery.scanning || discovery.error.is_some() {
        discovery.scanning = false;
        discovery.error = None;
        shared.generation.fetch_add(1, Ordering::Release);
    }
}

fn set_scanning(shared: &Shared, scanning: bool) {
    let mut discovery = shared
        .discovery
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if discovery.scanning != scanning || discovery.error.is_some() {
        discovery.scanning = scanning;
        discovery.error = None;
        shared.generation.fetch_add(1, Ordering::Release);
    }
}

fn set_error(shared: &Shared, error: String) {
    let mut discovery = shared
        .discovery
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if discovery.scanning || discovery.error.as_deref() != Some(error.as_str()) {
        discovery.scanning = false;
        discovery.error = Some(error);
        shared.generation.fetch_add(1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_generation_advances_only_when_visible_status_changes() {
        let shared = Shared::default();
        let generation = shared.generation.load(Ordering::Acquire);

        set_scanning(&shared, false);
        assert_eq!(shared.generation.load(Ordering::Acquire), generation);

        set_scanning(&shared, true);
        let scanning_generation = shared.generation.load(Ordering::Acquire);
        assert!(scanning_generation > generation);

        set_scanning(&shared, true);
        assert_eq!(
            shared.generation.load(Ordering::Acquire),
            scanning_generation
        );
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
    fn connection_setup_waits_for_the_previous_device() {
        let desired = Selection {
            enabled: true,
            discover: true,
            device_ids: [Some(Arc::from("b")), Some(Arc::from("a"))],
        };
        let devices = HashMap::from([
            ("a".to_owned(), ("Alpha".to_owned(), ())),
            ("b".to_owned(), ("Beta".to_owned(), ())),
        ]);
        let monitors = HashMap::new();
        let attempts = HashMap::new();
        assert!(ready_monitor(&desired, &devices, &monitors, &attempts, true).is_none());
        let (id, ()) = ready_monitor(&desired, &devices, &monitors, &attempts, false).unwrap();
        assert_eq!(id.as_ref(), "b");
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
