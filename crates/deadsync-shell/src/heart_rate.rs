use deadsync_theme_simply_love::screens::{gameplay, player_options, select_music};

/// Game-thread owner for heart-rate configuration invalidation.
///
/// Lifetime: process. Warmup: first owning-screen frame. Capacity: one fixed
/// configuration key and two fixed view keys. Unchanged Player Options and
/// Select Music frames perform at most two atomic generation loads and no
/// locks, allocations, discovery clones, eviction, or destruction. A miss
/// rebuilds at most two device choices/readings on a menu frame. Configuration
/// work occurs only after a machine setting, profile device, or screen-mode
/// change. Entries are replaced in place and destroyed at app shutdown;
/// existing frame timing accounts for their bounded miss cost.
#[derive(Default)]
pub(crate) struct Runtime {
    initialized: bool,
    enabled: bool,
    discover: bool,
    profile_generation: u64,
    player_options_view_key: Option<(u64, u64)>,
    select_music_view_key: Option<(bool, u64, u64)>,
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

    pub(crate) fn refresh_player_options(&mut self, state: &mut player_options::State) -> bool {
        let key = (
            deadsync_heart_rate::discovery_generation(),
            deadsync_heart_rate::player_readings_generation(),
        );
        if self.player_options_view_key == Some(key) {
            return false;
        }
        // Capture both generations before the discovery lock. A concurrent
        // publication may cause one redundant refresh, but cannot be hidden.
        let view = devices_view();
        self.player_options_view_key = Some(key);
        player_options::set_heart_rate_devices(state, &view);
        true
    }

    pub(crate) fn refresh_select_music(
        &mut self,
        state: &mut select_music::State,
        enabled: bool,
    ) -> bool {
        let key = select_music_view_key(enabled, heart_rate_view_generation);
        if self.select_music_view_key == Some(key) {
            return false;
        }
        let view = if enabled {
            readings_view()
        } else {
            gameplay::HeartRateView::default()
        };
        self.select_music_view_key = Some(key);
        select_music::set_heart_rate_view(state, view);
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

fn heart_rate_view_generation() -> (u64, u64) {
    (
        deadsync_heart_rate::player_readings_generation(),
        deadsync_profile::runtime_profile_generation(),
    )
}

fn select_music_view_key(
    enabled: bool,
    generation: impl FnOnce() -> (u64, u64),
) -> (bool, u64, u64) {
    let (readings, profiles) = if enabled { generation() } else { (0, 0) };
    (enabled, readings, profiles)
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

fn readings_view() -> gameplay::HeartRateView {
    let readings = deadsync_heart_rate::player_readings();
    let max_heart_rates = deadsync_profile::runtime_max_heart_rates();
    let players = std::array::from_fn(|idx| gameplay::HeartRatePlayerView {
        configured: readings[idx].configured,
        connected: readings[idx].connected,
        bpm: readings[idx].bpm,
        max_heart_rate: max_heart_rates[idx],
    });
    gameplay::HeartRateView { players }
}

pub(crate) fn refresh_gameplay(state: &mut gameplay::State) -> bool {
    let generation = heart_rate_view_generation();
    if gameplay::heart_rate_generation(state) == generation {
        return false;
    }
    gameplay::set_heart_rate_view(state, generation, readings_view());
    true
}

#[cfg(test)]
mod tests {
    use super::{runtime_config_changed, select_music_view_key};

    #[test]
    fn runtime_config_only_invalidates_on_input_changes() {
        let current = (true, false, 7);
        assert!(runtime_config_changed(false, current, current));
        assert!(!runtime_config_changed(true, current, current));
        assert!(runtime_config_changed(true, current, (false, false, 7)));
        assert!(runtime_config_changed(true, current, (true, true, 7)));
        assert!(runtime_config_changed(true, current, (true, false, 8)));
    }

    #[test]
    fn disabled_select_music_view_does_not_read_the_generation() {
        let key = select_music_view_key(false, || panic!("disabled view read generation"));
        assert_eq!(key, (false, 0, 0));
        assert_eq!(select_music_view_key(true, || (9, 4)), (true, 9, 4));
    }

    #[test]
    fn select_music_view_key_tracks_profile_changes() {
        let before = select_music_view_key(true, || (9, 4));
        let after = select_music_view_key(true, || (9, 5));

        assert_ne!(before, after);
    }
}
