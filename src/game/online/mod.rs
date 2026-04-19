pub mod arrowcloud;
pub mod downloads;
pub mod groovestats;
pub mod lobbies;

pub use arrowcloud::{
    ConnectionError as ArrowCloudError, ConnectionStatus as ArrowCloudConnectionStatus,
    api_base_url as arrowcloud_api_base_url, get_status as get_arrowcloud_status,
    leaderboards_url as arrowcloud_leaderboards_url,
    public_leaderboards_url as arrowcloud_public_leaderboards_url,
    submit_url as arrowcloud_submit_url,
};
pub use groovestats::{
    ConnectionError as GrooveStatsError, ConnectionStatus, Services,
    api_base_url as groovestats_api_base_url, boogiestats_api_base_url, get_status,
    is_boogiestats_active, player_leaderboards_url as groovestats_player_leaderboards_url,
    primary_api_base_url as groovestats_primary_api_base_url,
    qr_base_url as groovestats_qr_base_url, score_submit_url as groovestats_score_submit_url,
    service_name as groovestats_service_name,
};

pub fn init() {
    groovestats::init();
    arrowcloud::init();
    lobbies::init();
}
