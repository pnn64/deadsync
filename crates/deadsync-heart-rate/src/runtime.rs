use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use deadlib_ble::{Event, Monitor, Selection, Subscription, Uuid};

use crate::{DiscoverySnapshot, PlayerReading};

const SUBSCRIPTION: Subscription = Subscription {
    service: Uuid::from_u128(0x0000_180d_0000_1000_8000_0080_5f9b_34fb),
    characteristic: Uuid::from_u128(0x0000_2a37_0000_1000_8000_0080_5f9b_34fb),
    unnamed_device_label: "Heart Rate Monitor",
};

type Desired = Selection<2>;

// Gameplay reads these two packed latest values without touching the discovery
// lock. Bits 0/1 are configured/connected; the remaining bits encode BPM + 1
// so zero remains `None`.
static PLAYER_READINGS: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
static PLAYER_READINGS_GENERATION: AtomicU64 = AtomicU64::new(0);

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
    monitor: Monitor,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn configure(enabled: bool, discover: bool, device_ids: [Option<&str>; 2]) {
    if !deadlib_ble::SUPPORTED || (!enabled && RUNTIME.get().is_none()) {
        return;
    }
    let runtime = RUNTIME.get_or_init(start_worker);
    let mut desired = runtime
        .desired
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    *desired = Desired {
        enabled,
        discover,
        device_ids: device_ids.map(|id| id.map(Arc::from)),
    };
    drop(desired);

    let readings = player_readings();
    for (player, (reading, id)) in readings.into_iter().zip(device_ids).enumerate() {
        let reading = if !enabled || id.is_none() {
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

#[must_use]
pub fn player_readings() -> [PlayerReading; 2] {
    std::array::from_fn(|player| decode_reading(PLAYER_READINGS[player].load(Ordering::Acquire)))
}

pub fn player_readings_generation() -> u64 {
    PLAYER_READINGS_GENERATION.load(Ordering::Acquire)
}

pub fn discovery_generation() -> u64 {
    RUNTIME
        .get()
        .map_or(0, |runtime| runtime.monitor.discovery_generation())
}

pub fn discovery_snapshot() -> DiscoverySnapshot {
    let mut snapshot = RUNTIME
        .get()
        .map_or_else(DiscoverySnapshot::default, |runtime| {
            runtime.monitor.discovery_snapshot()
        });
    if !snapshot.supported {
        snapshot.error =
            Some("Bluetooth heart-rate monitors are unsupported on this platform".to_owned());
    }
    snapshot
}

fn start_worker() -> Runtime {
    let desired = Arc::new(Mutex::new(Desired::default()));
    let monitor = Monitor::start(SUBSCRIPTION, Arc::clone(&desired), apply_monitor_event);
    Runtime { desired, monitor }
}

fn apply_monitor_event(desired: &Desired, event: Event<'_>) {
    let (id, connected, bpm) = match event {
        Event::Connected(id) => (id, true, None),
        Event::Connecting(id) | Event::Disconnected(id) => (id, false, None),
        Event::Notification { device_id, value } => {
            let Ok(bpm) = parse_heart_rate_measurement(value) else {
                return;
            };
            (device_id, true, Some(bpm))
        }
        Event::Disabled => {
            for player in 0..2 {
                publish_reading(player, PlayerReading::default());
            }
            return;
        }
        Event::Unavailable => {
            for (player, reading) in player_readings().into_iter().enumerate() {
                publish_reading(
                    player,
                    PlayerReading {
                        configured: reading.configured,
                        ..PlayerReading::default()
                    },
                );
            }
            return;
        }
    };
    for (player, selected) in desired.device_ids.iter().enumerate() {
        if selected.as_deref() == Some(id) {
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

    static READING_TEST_LOCK: Mutex<()> = Mutex::new(());

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
    fn a_shared_device_updates_both_selected_slots() {
        let _lock = READING_TEST_LOCK.lock().unwrap();
        let id: Arc<str> = Arc::from("shared-sensor");
        let desired = Desired {
            enabled: true,
            discover: false,
            device_ids: [Some(Arc::clone(&id)), Some(Arc::clone(&id))],
        };

        apply_monitor_event(&desired, Event::Connecting(&id));
        let connecting = PlayerReading {
            configured: true,
            connected: false,
            bpm: None,
        };
        assert_eq!(player_readings(), [connecting; 2]);

        apply_monitor_event(
            &desired,
            Event::Notification {
                device_id: &id,
                value: &[0, 72],
            },
        );
        let reading = PlayerReading {
            configured: true,
            connected: true,
            bpm: Some(72),
        };
        assert_eq!(player_readings(), [reading; 2]);

        apply_monitor_event(
            &desired,
            Event::Notification {
                device_id: &id,
                value: &[1],
            },
        );
        assert_eq!(player_readings(), [reading; 2]);

        apply_monitor_event(&desired, Event::Disconnected(&id));
        assert_eq!(player_readings(), [connecting; 2]);
        apply_monitor_event(&desired, Event::Disabled);
        assert_eq!(player_readings(), [PlayerReading::default(); 2]);
    }

    #[test]
    fn enabling_discovery_with_saved_devices_remains_unsupported() {
        if deadlib_ble::SUPPORTED {
            return;
        }
        let _lock = READING_TEST_LOCK.lock().unwrap();
        let generation = player_readings_generation();

        configure(true, true, [Some("polar-h10"), Some("garmin-hrm")]);

        let snapshot = discovery_snapshot();
        assert!(!snapshot.supported);
        assert!(!snapshot.scanning);
        assert!(snapshot.devices.is_empty());
        assert_eq!(
            snapshot.error.as_deref(),
            Some("Bluetooth heart-rate monitors are unsupported on this platform")
        );
        assert_eq!(player_readings(), [PlayerReading::default(); 2]);
        assert_eq!(player_readings_generation(), generation);
        assert_eq!(discovery_generation(), 0);
    }
}
