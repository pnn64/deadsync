pub(super) mod hid_blue_dot {
    use crate::{ButtonLight, CabinetLight, Player, State};
    use std::time::Duration;
    const VENDOR_ID: u16 = 0x04bd;
    const PRODUCT_ID: u16 = 0x00bd;
    const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);
    const REPORT_SIZE: usize = 3;
    const REPORT_ID: u8 = 0x00;
    const CABINET_REPORT: u8 = 0x01;
    const PAD_REPORT: u8 = 0x02;
    const CAB_MARQUEE_UPPER_LEFT: u8 = 0;
    const CAB_MARQUEE_UPPER_RIGHT: u8 = 1;
    const CAB_MARQUEE_LOWER_LEFT: u8 = 2;
    const CAB_MARQUEE_LOWER_RIGHT: u8 = 3;
    const CAB_P1_START: u8 = 4;
    const CAB_P2_START: u8 = 5;
    const CAB_BASS: u8 = 6;
    const PAD_P1_UP: u8 = 0;
    const PAD_P1_DOWN: u8 = 1;
    const PAD_P1_LEFT: u8 = 2;
    const PAD_P1_RIGHT: u8 = 3;
    const PAD_P2_UP: u8 = 4;
    const PAD_P2_DOWN: u8 = 5;
    const PAD_P2_LEFT: u8 = 6;
    const PAD_P2_RIGHT: u8 = 7;
    const fn build_cabinet_report(state: &State) -> [u8; REPORT_SIZE] {
        let mut bits = 0u8;
        set_bit(
            &mut bits,
            CAB_MARQUEE_UPPER_LEFT,
            state.cabinet(CabinetLight::MarqueeUpperLeft),
        );
        set_bit(
            &mut bits,
            CAB_MARQUEE_UPPER_RIGHT,
            state.cabinet(CabinetLight::MarqueeUpperRight),
        );
        set_bit(
            &mut bits,
            CAB_MARQUEE_LOWER_LEFT,
            state.cabinet(CabinetLight::MarqueeLowerLeft),
        );
        set_bit(
            &mut bits,
            CAB_MARQUEE_LOWER_RIGHT,
            state.cabinet(CabinetLight::MarqueeLowerRight),
        );
        set_bit(
            &mut bits,
            CAB_P1_START,
            state.menu_button(Player::P1, ButtonLight::Start),
        );
        set_bit(
            &mut bits,
            CAB_P2_START,
            state.menu_button(Player::P2, ButtonLight::Start),
        );
        set_bit(
            &mut bits,
            CAB_BASS,
            state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight),
        );
        [REPORT_ID, CABINET_REPORT, bits]
    }

    const fn build_pad_report(state: &State) -> [u8; REPORT_SIZE] {
        let mut bits = 0u8;
        set_bit(
            &mut bits,
            PAD_P1_UP,
            state.button(Player::P1, ButtonLight::Up),
        );
        set_bit(
            &mut bits,
            PAD_P1_DOWN,
            state.button(Player::P1, ButtonLight::Down),
        );
        set_bit(
            &mut bits,
            PAD_P1_LEFT,
            state.button(Player::P1, ButtonLight::Left),
        );
        set_bit(
            &mut bits,
            PAD_P1_RIGHT,
            state.button(Player::P1, ButtonLight::Right),
        );
        set_bit(
            &mut bits,
            PAD_P2_UP,
            state.button(Player::P2, ButtonLight::Up),
        );
        set_bit(
            &mut bits,
            PAD_P2_DOWN,
            state.button(Player::P2, ButtonLight::Down),
        );
        set_bit(
            &mut bits,
            PAD_P2_LEFT,
            state.button(Player::P2, ButtonLight::Left),
        );
        set_bit(
            &mut bits,
            PAD_P2_RIGHT,
            state.button(Player::P2, ButtonLight::Right),
        );
        [REPORT_ID, PAD_REPORT, bits]
    }

    const fn set_bit(bits: &mut u8, bit: u8, on: bool) {
        if on {
            *bits |= 1u8 << bit;
        }
    }

    pub(crate) const fn performance_reports(state: &State) -> [[u8; REPORT_SIZE]; 2] {
        [build_cabinet_report(state), build_pad_report(state)]
    }

    #[cfg(test)]
    pub(crate) const fn performance_cabinet_report(state: &State) -> [u8; REPORT_SIZE] {
        build_cabinet_report(state)
    }
    #[cfg(test)]
    pub(crate) const fn performance_pad_report(state: &State) -> [u8; REPORT_SIZE] {
        build_pad_report(state)
    }
}

pub(super) mod minimaid_hid {
    use crate::{ButtonLight, CabinetLight, Player, State};
    use std::time::Duration;
    const VENDOR_ID: u16 = 0xbeef;
    const PRODUCT_ID: u16 = 0x5730;
    const LIGHTING_INTERFACE: i32 = 1;
    const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);
    const REPORT_ID: u8 = 0;
    const REPORT_SIZE: usize = 9;
    const LIGHTS_OFFSET: usize = 2;
    const BLUE_LED_OFFSET: usize = 6;
    const KB_ENABLE_OFFSET: usize = 7;
    const ON: u8 = 0xff;
    const P1_MENU: u8 = 2;
    const P2_MENU: u8 = 3;
    const MARQUEE_P2_LOWER: u8 = 4;
    const MARQUEE_P2_UPPER: u8 = 5;
    const MARQUEE_P1_LOWER: u8 = 6;
    const MARQUEE_P1_UPPER: u8 = 7;
    const P1_UP: u8 = 8;
    const P1_DOWN: u8 = 9;
    const P1_LEFT: u8 = 10;
    const P1_RIGHT: u8 = 11;
    const P1_PAD_ENABLE: u8 = 12;
    const P2_UP: u8 = 16;
    const P2_DOWN: u8 = 17;
    const P2_LEFT: u8 = 18;
    const P2_RIGHT: u8 = 19;
    const P2_PAD_ENABLE: u8 = 20;
    const NEONS: u8 = 24;
    fn build_report(state: &State) -> [u8; REPORT_SIZE] {
        let mut report = [0u8; REPORT_SIZE];
        let mut lights = 0u32;
        set_bit(
            &mut lights,
            MARQUEE_P1_LOWER,
            state.cabinet(CabinetLight::MarqueeLowerLeft),
        );
        set_bit(
            &mut lights,
            MARQUEE_P2_LOWER,
            state.cabinet(CabinetLight::MarqueeLowerRight),
        );
        set_bit(
            &mut lights,
            MARQUEE_P1_UPPER,
            state.cabinet(CabinetLight::MarqueeUpperLeft),
        );
        set_bit(
            &mut lights,
            MARQUEE_P2_UPPER,
            state.cabinet(CabinetLight::MarqueeUpperRight),
        );
        let neons = state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight);
        set_bit(&mut lights, NEONS, neons);

        set_bit(
            &mut lights,
            P1_MENU,
            state.menu_button(Player::P1, ButtonLight::Start),
        );
        set_bit(
            &mut lights,
            P2_MENU,
            state.menu_button(Player::P2, ButtonLight::Start),
        );
        set_player(&mut lights, Player::P1, state);
        set_player(&mut lights, Player::P2, state);
        set_bit(&mut lights, P1_PAD_ENABLE, true);
        set_bit(&mut lights, P2_PAD_ENABLE, true);

        report[0] = REPORT_ID;
        report[LIGHTS_OFFSET..LIGHTS_OFFSET + 4].copy_from_slice(&lights.to_le_bytes());
        report[BLUE_LED_OFFSET] = if neons { ON } else { 0 };
        report[KB_ENABLE_OFFSET] = 1;
        report
    }

    const fn set_player(lights: &mut u32, player: Player, state: &State) {
        let (up, down, left, right) = match player {
            Player::P1 => (P1_UP, P1_DOWN, P1_LEFT, P1_RIGHT),
            Player::P2 => (P2_UP, P2_DOWN, P2_LEFT, P2_RIGHT),
        };
        set_bit(lights, up, state.button(player, ButtonLight::Up));
        set_bit(lights, down, state.button(player, ButtonLight::Down));
        set_bit(lights, left, state.button(player, ButtonLight::Left));
        set_bit(lights, right, state.button(player, ButtonLight::Right));
    }

    const fn set_bit(bits: &mut u32, bit: u8, on: bool) {
        if on {
            *bits |= 1u32 << bit;
        }
    }

    pub(crate) fn performance_report(state: &State) -> [u8; REPORT_SIZE] {
        build_report(state)
    }
}
pub(super) mod pac_drive {
    use crate::{ButtonLight, CabinetLight, PacDriveLightOrdering, Player, State};
    use std::time::Duration;
    const VENDOR_ID: u16 = 0xd209;
    const PRODUCT_ID_BASE: u16 = 0x1500;
    const PRODUCT_ID_COUNT: u16 = 8;
    const LIGHTING_INTERFACE: i32 = 0;
    const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);
    const REPORT_ID: u8 = 0x00;
    const REPORT_SIZE: usize = 5;
    const LED_LOW_BYTE: usize = 3;
    const LED_HIGH_BYTE: usize = 4;
    const LED01: u8 = 1;
    const LED02: u8 = 2;
    const LED03: u8 = 3;
    const LED04: u8 = 4;
    const LED05: u8 = 5;
    const LED06: u8 = 6;
    const LED07: u8 = 7;
    const LED08: u8 = 8;
    const LED09: u8 = 9;
    const LED10: u8 = 10;
    const LED11: u8 = 11;
    const LED12: u8 = 12;
    const LED13: u8 = 13;
    const LED14: u8 = 14;
    const LED15: u8 = 15;
    const LED16: u8 = 16;
    const fn build_report(state: &State, ordering: PacDriveLightOrdering) -> [u8; REPORT_SIZE] {
        let mut report = [0u8; REPORT_SIZE];
        report[0] = REPORT_ID;

        match ordering {
            PacDriveLightOrdering::OpenItg => build_openitg_report(state, &mut report),
            PacDriveLightOrdering::Sm5 => build_sm5_report(state, &mut report),
        }
        report
    }

    const fn build_openitg_report(state: &State, report: &mut [u8; REPORT_SIZE]) {
        set_led(report, LED01, state.button(Player::P1, ButtonLight::Left));
        set_led(report, LED02, state.button(Player::P1, ButtonLight::Right));
        set_led(report, LED03, state.button(Player::P1, ButtonLight::Up));
        set_led(report, LED04, state.button(Player::P1, ButtonLight::Down));
        set_led(report, LED05, state.button(Player::P2, ButtonLight::Left));
        set_led(report, LED06, state.button(Player::P2, ButtonLight::Right));
        set_led(report, LED07, state.button(Player::P2, ButtonLight::Up));
        set_led(report, LED08, state.button(Player::P2, ButtonLight::Down));
        set_led(report, LED09, state.cabinet(CabinetLight::MarqueeUpperLeft));
        set_led(
            report,
            LED10,
            state.cabinet(CabinetLight::MarqueeUpperRight),
        );
        set_led(report, LED11, state.cabinet(CabinetLight::MarqueeLowerLeft));
        set_led(
            report,
            LED12,
            state.cabinet(CabinetLight::MarqueeLowerRight),
        );
        set_led(
            report,
            LED13,
            state.menu_button(Player::P1, ButtonLight::Start),
        );
        set_led(
            report,
            LED14,
            state.menu_button(Player::P2, ButtonLight::Start),
        );
        let bass = state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight);
        set_led(report, LED15, bass);
        set_led(report, LED16, bass);
    }

    const fn build_sm5_report(state: &State, report: &mut [u8; REPORT_SIZE]) {
        set_led(report, LED01, state.cabinet(CabinetLight::MarqueeUpperLeft));
        set_led(
            report,
            LED02,
            state.cabinet(CabinetLight::MarqueeUpperRight),
        );
        set_led(report, LED03, state.cabinet(CabinetLight::MarqueeLowerLeft));
        set_led(
            report,
            LED04,
            state.cabinet(CabinetLight::MarqueeLowerRight),
        );
        let bass = state.cabinet(CabinetLight::BassLeft) || state.cabinet(CabinetLight::BassRight);
        set_led(report, LED05, bass);

        set_led(report, LED06, state.button(Player::P1, ButtonLight::Left));
        set_led(report, LED07, state.button(Player::P1, ButtonLight::Right));
        set_led(report, LED08, state.button(Player::P1, ButtonLight::Up));
        set_led(report, LED09, state.button(Player::P1, ButtonLight::Down));
        set_led(
            report,
            LED10,
            state.menu_button(Player::P1, ButtonLight::Start),
        );

        set_led(report, LED11, state.button(Player::P2, ButtonLight::Left));
        set_led(report, LED12, state.button(Player::P2, ButtonLight::Right));
        set_led(report, LED13, state.button(Player::P2, ButtonLight::Up));
        set_led(report, LED14, state.button(Player::P2, ButtonLight::Down));
        set_led(
            report,
            LED15,
            state.menu_button(Player::P2, ButtonLight::Start),
        );
    }

    const fn set_led(report: &mut [u8; REPORT_SIZE], led: u8, on: bool) {
        if !on {
            return;
        }
        if led >= LED09 {
            report[LED_LOW_BYTE] |= 1u8 << (led - LED09);
        } else {
            report[LED_HIGH_BYTE] |= 1u8 << (led - LED01);
        }
    }

    pub(crate) const fn performance_report(
        state: &State,
        ordering: PacDriveLightOrdering,
    ) -> [u8; REPORT_SIZE] {
        build_report(state, ordering)
    }
}
pub(super) mod gpb {
    use crate::{ButtonLight, Player, State};
    use std::time::Duration;
    const VENDOR_ID: u16 = 0x04d8;
    const PRODUCT_ID: u16 = 0xea6a;
    const LIGHTING_INTERFACE: i32 = 0x01;
    const REOPEN_INTERVAL: Duration = Duration::from_millis(1500);
    const REPORT_ID: u8 = 0x01;
    const REPORT_SIZE: usize = 17;
    const BTN01: usize = 1;
    const BTN02: usize = 2;
    const BTN03: usize = 3;
    const BTN04: usize = 4;
    const BTN05: usize = 5;
    const BTN06: usize = 6;
    const BTN07: usize = 7;
    const BTN08: usize = 8;
    const BTN09: usize = 9;
    const BTN10: usize = 10;
    const BTN11: usize = 11;
    const BTN12: usize = 12;
    const BTN13: usize = 13;
    const BTN14: usize = 14;
    const BTN15: usize = 15;
    const BTN16: usize = 16;
    fn build_report(state: &State) -> [u8; REPORT_SIZE] {
        let mut report = [0u8; REPORT_SIZE];
        report[0] = REPORT_ID;

        set(
            &mut report,
            BTN01,
            state.menu_button(Player::P1, ButtonLight::Select),
        );
        set(
            &mut report,
            BTN02,
            state.menu_button(Player::P1, ButtonLight::Left),
        );
        set(
            &mut report,
            BTN03,
            state.menu_button(Player::P1, ButtonLight::Right),
        );
        set(
            &mut report,
            BTN04,
            state.menu_button(Player::P1, ButtonLight::Start),
        );

        set(
            &mut report,
            BTN05,
            state.menu_button(Player::P2, ButtonLight::Select),
        );
        set(
            &mut report,
            BTN06,
            state.menu_button(Player::P2, ButtonLight::Left),
        );
        set(
            &mut report,
            BTN07,
            state.menu_button(Player::P2, ButtonLight::Right),
        );
        set(
            &mut report,
            BTN08,
            state.menu_button(Player::P2, ButtonLight::Start),
        );

        set(
            &mut report,
            BTN09,
            state.menu_button(Player::P1, ButtonLight::Up),
        );
        set(
            &mut report,
            BTN10,
            state.menu_button(Player::P1, ButtonLight::Down),
        );
        set(
            &mut report,
            BTN11,
            state.menu_button(Player::P2, ButtonLight::Up),
        );
        set(
            &mut report,
            BTN12,
            state.menu_button(Player::P2, ButtonLight::Down),
        );
        for light in [BTN13, BTN14, BTN15, BTN16] {
            set(&mut report, light, false);
        }
        report
    }

    fn set(report: &mut [u8; REPORT_SIZE], light_index: usize, on: bool) {
        report[light_index] = u8::from(on);
    }

    #[cfg(test)]
    pub(crate) fn performance_report(state: &State) -> [u8; REPORT_SIZE] {
        build_report(state)
    }
}

pub(super) mod linux_leds {
    use crate::{ButtonLight, CabinetLight, Player, State};
    const OUTPUT_COUNT: usize = 32;
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
        menu_button(13, Player::P1, ButtonLight::Start),
        button(1, Player::P1, ButtonLight::Left),
        button(0, Player::P1, ButtonLight::Right),
        button(3, Player::P1, ButtonLight::Up),
        button(2, Player::P1, ButtonLight::Down),
        menu_button(12, Player::P2, ButtonLight::Start),
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
        MenuButton(Player, ButtonLight),
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

    const fn menu_button(output: usize, player: Player, light: ButtonLight) -> MappedLight {
        MappedLight {
            output,
            source: Source::MenuButton(player, light),
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

    const fn source_on(state: &State, source: Source) -> bool {
        match source {
            Source::Cabinet(light) => state.cabinet(light),
            Source::Button(player, button) => state.button(player, button),
            Source::MenuButton(player, button) => state.menu_button(player, button),
        }
    }

    #[cfg(test)]
    pub(crate) fn performance_outputs(state: &State, itgio: bool) -> [Option<bool>; OUTPUT_COUNT] {
        build_outputs(if itgio { &ITGIO_MAP } else { &PIUIO_MAP }, state)
    }
    #[cfg(test)]
    pub(crate) fn performance_entry_size() -> usize {
        std::mem::size_of::<MappedLight>()
    }
}
