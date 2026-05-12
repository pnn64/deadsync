use super::{ButtonLight, CabinetLight, Player, State};
use log::{debug, warn};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

const OUTPUT_COUNT: usize = 32;
const ON: &[u8] = b"255";
const OFF: &[u8] = b"0";

const PIUIO_LIGHTS: [MappedLight; 14] = [
    cabinet(23, CabinetLight::MarqueeUpperLeft),
    cabinet(26, CabinetLight::MarqueeUpperRight),
    cabinet(25, CabinetLight::MarqueeLowerLeft),
    cabinet(24, CabinetLight::MarqueeLowerRight),
    cabinet(10, CabinetLight::BassLeft),
    cabinet(10, CabinetLight::BassRight),
    button(20, Player::P1, ButtonLight::Left),
    button(21, Player::P1, ButtonLight::Right),
    button(18, Player::P1, ButtonLight::Up),
    button(19, Player::P1, ButtonLight::Down),
    button(4, Player::P2, ButtonLight::Left),
    button(5, Player::P2, ButtonLight::Right),
    button(2, Player::P2, ButtonLight::Up),
    button(3, Player::P2, ButtonLight::Down),
];

const ITGIO_LIGHTS: [MappedLight; 16] = [
    cabinet(8, CabinetLight::MarqueeUpperLeft),
    cabinet(10, CabinetLight::MarqueeUpperRight),
    cabinet(9, CabinetLight::MarqueeLowerLeft),
    cabinet(11, CabinetLight::MarqueeLowerRight),
    cabinet(15, CabinetLight::BassLeft),
    cabinet(15, CabinetLight::BassRight),
    button(13, Player::P1, ButtonLight::Start),
    button(1, Player::P1, ButtonLight::Left),
    button(0, Player::P1, ButtonLight::Right),
    button(3, Player::P1, ButtonLight::Up),
    button(2, Player::P1, ButtonLight::Down),
    button(12, Player::P2, ButtonLight::Start),
    button(5, Player::P2, ButtonLight::Left),
    button(4, Player::P2, ButtonLight::Right),
    button(7, Player::P2, ButtonLight::Up),
    button(6, Player::P2, ButtonLight::Down),
];

const PIUIO_MAP: BoardMap = BoardMap {
    label: "PIUIO_Leds",
    led_class: "piuio",
    lights: &PIUIO_LIGHTS,
};

const ITGIO_MAP: BoardMap = BoardMap {
    label: "ITGIO",
    led_class: "itgio",
    lights: &ITGIO_LIGHTS,
};

pub struct Driver {
    map: &'static BoardMap,
    last_outputs: [Option<bool>; OUTPUT_COUNT],
    warned_missing: bool,
    warned_write: bool,
}

impl Driver {
    pub fn piuio() -> Self {
        Self::new(&PIUIO_MAP)
    }

    pub fn itgio() -> Self {
        Self::new(&ITGIO_MAP)
    }

    fn new(map: &'static BoardMap) -> Self {
        Self {
            map,
            last_outputs: [None; OUTPUT_COUNT],
            warned_missing: false,
            warned_write: false,
        }
    }

    pub fn set(&mut self, state: &State) {
        let outputs = build_outputs(self.map, state);
        if outputs == self.last_outputs {
            return;
        }
        if !self.has_any_output() {
            if !self.warned_missing {
                debug!(
                    "No {} Linux LED outputs found at /sys/class/leds/{}::output*/brightness",
                    self.map.label, self.map.led_class
                );
                self.warned_missing = true;
            }
            return;
        }
        self.warned_missing = false;

        for (output, desired) in outputs.iter().enumerate() {
            let Some(desired) = desired else {
                continue;
            };
            if self.last_outputs[output] == Some(*desired) {
                continue;
            }
            if self.write_output(output, *desired) {
                self.last_outputs[output] = Some(*desired);
            }
        }
    }

    fn has_any_output(&self) -> bool {
        self.map
            .lights
            .iter()
            .any(|light| self.output_path(light.output).exists())
    }

    fn write_output(&mut self, output: usize, on: bool) -> bool {
        let path = self.output_path(output);
        let bytes = if on { ON } else { OFF };
        let result = OpenOptions::new()
            .write(true)
            .open(&path)
            .and_then(|mut file| file.write_all(bytes));
        match result {
            Ok(()) => {
                self.warned_write = false;
                true
            }
            Err(e) => {
                if !self.warned_write {
                    warn!(
                        "{} Linux LED write failed at {}: {e}. Check device permissions or udev rules.",
                        self.map.label,
                        path.display()
                    );
                    self.warned_write = true;
                }
                false
            }
        }
    }

    fn output_path(&self, output: usize) -> PathBuf {
        PathBuf::from(format!(
            "/sys/class/leds/{}::output{output}/brightness",
            self.map.led_class
        ))
    }
}

#[derive(Clone, Copy)]
struct BoardMap {
    label: &'static str,
    led_class: &'static str,
    lights: &'static [MappedLight],
}

#[derive(Clone, Copy)]
struct MappedLight {
    output: usize,
    source: Source,
}

#[derive(Clone, Copy)]
enum Source {
    Cabinet(CabinetLight),
    Button(Player, ButtonLight),
}

const fn cabinet(output: usize, light: CabinetLight) -> MappedLight {
    MappedLight {
        output,
        source: Source::Cabinet(light),
    }
}

const fn button(output: usize, player: Player, light: ButtonLight) -> MappedLight {
    MappedLight {
        output,
        source: Source::Button(player, light),
    }
}

fn build_outputs(map: &BoardMap, state: &State) -> [Option<bool>; OUTPUT_COUNT] {
    let mut outputs = [None; OUTPUT_COUNT];
    for light in map.lights {
        debug_assert!(light.output < OUTPUT_COUNT);
        set_output(&mut outputs, light.output, source_on(state, light.source));
    }
    outputs
}

fn set_output(outputs: &mut [Option<bool>; OUTPUT_COUNT], output: usize, on: bool) {
    let current = outputs[output].unwrap_or(false);
    outputs[output] = Some(current || on);
}

fn source_on(state: &State, source: Source) -> bool {
    match source {
        Source::Cabinet(light) => state.cabinet(light),
        Source::Button(player, button) => state.button(player, button),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piuio_maps_dance_outputs() {
        let mut state = State::default();
        state.set_cabinet(CabinetLight::MarqueeUpperLeft, true);
        state.set_cabinet(CabinetLight::BassRight, true);
        state.set_button(Player::P1, ButtonLight::Left, true);
        state.set_button(Player::P2, ButtonLight::Down, true);
        state.set_button(Player::P1, ButtonLight::Start, true);

        let outputs = build_outputs(&PIUIO_MAP, &state);
        assert_eq!(outputs[23], Some(true));
        assert_eq!(outputs[10], Some(true));
        assert_eq!(outputs[20], Some(true));
        assert_eq!(outputs[3], Some(true));
        assert_eq!(outputs[21], Some(false));
        assert_eq!(outputs[28], None);
    }

    #[test]
    fn itgio_maps_dance_outputs() {
        let mut state = State::default();
        state.set_cabinet(CabinetLight::BassLeft, true);
        state.set_button(Player::P1, ButtonLight::Start, true);
        state.set_button(Player::P1, ButtonLight::Right, true);
        state.set_button(Player::P2, ButtonLight::Up, true);

        let outputs = build_outputs(&ITGIO_MAP, &state);
        assert_eq!(outputs[15], Some(true));
        assert_eq!(outputs[13], Some(true));
        assert_eq!(outputs[12], Some(false));
        assert_eq!(outputs[0], Some(true));
        assert_eq!(outputs[7], Some(true));
        assert_eq!(outputs[4], Some(false));
    }
}
