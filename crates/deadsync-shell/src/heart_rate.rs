use deadsync_config::prelude as config;
use deadsync_theme_simply_love::screens::{gameplay, player_options};

pub(crate) fn sync_runtime(discover: bool) {
    let enabled = config::get().machine_enable_heart_rate_monitors;
    deadsync_profile::with_runtime_heart_rate_device_ids(|ids| {
        deadsync_heart_rate::configure(enabled, discover, ids);
    });
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
