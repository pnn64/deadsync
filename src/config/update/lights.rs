use super::*;

pub fn update_lights_driver(driver: LightsDriverKind) {
    {
        let mut cfg = lock_config();
        if cfg.lights_driver == driver {
            return;
        }
        cfg.lights_driver = driver;
    }
    save_without_keymaps();
}

pub fn update_lights_gameplay_pad_lights(mode: GameplayPadLightMode) {
    {
        let mut cfg = lock_config();
        if cfg.lights_gameplay_pad_lights == mode {
            return;
        }
        cfg.lights_gameplay_pad_lights = mode;
    }
    save_without_keymaps();
}
