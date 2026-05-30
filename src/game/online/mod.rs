pub mod arrowcloud;
pub mod downloads;
pub mod groovestats;
pub mod lobbies;

pub use arrowcloud::get_status as get_arrowcloud_status;
pub use groovestats::{
    active_service as groovestats_active_service, get_status, is_boogiestats_active,
};

pub fn init() {
    groovestats::init();
    arrowcloud::init();
    lobbies::init();
}
