use super::*;

pub fn update_lights_driver(driver: LightsDriverKind) {
    update_config_value(driver, |cfg| &mut cfg.lights_driver);
}

pub fn update_lights_gameplay_pad_lights(mode: GameplayPadLightMode) {
    update_config_value(mode, |cfg| &mut cfg.lights_gameplay_pad_lights);
}

pub fn update_lights_simplify_bass(enabled: bool) {
    update_config_value(enabled, |cfg| &mut cfg.lights_simplify_bass);
}
