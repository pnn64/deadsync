use std::sync::{Arc, Mutex};

use crate::{EventHandler, Selection, Shared, Subscription};

pub const SUPPORTED: bool = false;

pub(crate) fn start_worker<const N: usize>(
    _subscription: Subscription,
    _desired: Arc<Mutex<Selection<N>>>,
    _shared: Arc<Shared>,
    _on_event: EventHandler<N>,
) {
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Monitor, Uuid};

    #[test]
    fn unsupported_monitor_reports_no_devices_or_events() {
        let selection = Arc::new(Mutex::new(Selection {
            enabled: true,
            discover: true,
            device_ids: [Some(Arc::from("saved-device"))],
        }));
        let monitor = Monitor::start(
            Subscription {
                service: Uuid::nil(),
                characteristic: Uuid::nil(),
                unnamed_device_label: "Unknown device",
            },
            selection,
            |_, _| panic!("unsupported BLE emitted a worker event"),
        );

        let snapshot = monitor.discovery_snapshot();
        assert!(!snapshot.supported);
        assert!(!snapshot.scanning);
        assert!(snapshot.devices.is_empty());
        assert_eq!(monitor.discovery_generation(), 0);
    }
}
