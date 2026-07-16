use deadsync_theme_simply_love::screens::{gameplay, player_options};

/// Game-thread owner for heart-rate configuration invalidation.
///
/// Lifetime: process. Warmup: first frame. Capacity: one fixed snapshot.
/// Unchanged gameplay frames perform one atomic generation load and no locks,
/// allocations, discovery work, eviction, or destruction. Configuration work
/// occurs only after a machine setting, profile device, or screen-mode change.
#[derive(Default)]
pub(crate) struct Runtime {
    initialized: bool,
    enabled: bool,
    discover: bool,
    profile_generation: u64,
}

impl Runtime {
    pub(crate) fn sync(&mut self, enabled: bool, discover: bool) -> bool {
        let profile_generation = deadsync_profile::runtime_heart_rate_device_generation();
        if !runtime_config_changed(
            self.initialized,
            (self.enabled, self.discover, self.profile_generation),
            (enabled, discover, profile_generation),
        ) {
            return false;
        }
        if enabled {
            deadsync_profile::with_runtime_heart_rate_device_ids(|ids| {
                deadsync_heart_rate::configure(true, discover, ids);
            });
        } else {
            deadsync_heart_rate::configure(false, discover, [None, None]);
        }
        self.initialized = true;
        self.enabled = enabled;
        self.discover = discover;
        self.profile_generation = profile_generation;
        true
    }
}

const fn runtime_config_changed(
    initialized: bool,
    current: (bool, bool, u64),
    next: (bool, bool, u64),
) -> bool {
    !initialized || current.0 != next.0 || current.1 != next.1 || current.2 != next.2
}

pub(crate) fn devices_view() -> player_options::HeartRateDevicesView {
    let snapshot = deadsync_heart_rate::discovery_snapshot();
    let readings = deadsync_heart_rate::player_readings().map(|reading| {
        player_options::HeartRateReadingView {
            configured: reading.configured,
            connected: reading.connected,
            bpm: reading.bpm,
        }
    });
    player_options::HeartRateDevicesView {
        supported: snapshot.supported,
        scanning: snapshot.scanning,
        devices: snapshot
            .devices
            .into_iter()
            .map(|device| player_options::HeartRateDeviceView {
                id: device.id,
                label: device.label,
            })
            .collect(),
        error: snapshot.error,
        readings,
    }
}

pub(crate) fn refresh_player_options(state: &mut player_options::State) {
    player_options::set_heart_rate_devices(state, &devices_view());
}

pub(crate) fn refresh_gameplay(state: &mut gameplay::State) {
    let players =
        deadsync_heart_rate::player_readings().map(|reading| gameplay::HeartRatePlayerView {
            configured: reading.configured,
            connected: reading.connected,
            bpm: reading.bpm,
        });
    gameplay::set_heart_rate_view(state, gameplay::HeartRateView { players });
}

#[cfg(test)]
mod tests {
    use super::runtime_config_changed;

    #[test]
    fn runtime_config_only_invalidates_on_input_changes() {
        let current = (true, false, 7);
        assert!(runtime_config_changed(false, current, current));
        assert!(!runtime_config_changed(true, current, current));
        assert!(runtime_config_changed(true, current, (false, false, 7)));
        assert!(runtime_config_changed(true, current, (true, true, 7)));
        assert!(runtime_config_changed(true, current, (true, false, 8)));
    }
}
