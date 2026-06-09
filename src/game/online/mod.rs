pub mod arrowcloud;
pub mod downloads;
pub mod groovestats;
pub mod lobbies;

pub fn init() {
    groovestats::init();
    arrowcloud::init();
}
