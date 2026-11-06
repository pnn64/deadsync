pub fn integer(out: &mut String, value: i16) {
    push_i16_decimal(out, value);
}
pub fn floating(out: &mut String, value: f32) {
    push_trimmed_float2(out, value);
}
pub fn transforms(out: &mut String, a: u8, b: u8, c: u8) {
    push_transform_parts(out, a, b, c);
}
pub fn demo(speed: f32, kind: usize) -> String {
    gameplay_mods_text(GameplayModsTextParams {
        speed: if kind == 1 {
            ScrollSpeedSetting::XMod(speed)
        } else {
            ScrollSpeedSetting::CMod(speed)
        },
        noteskin: "dance cel",
        insert_mask: if kind == 0 { 0 } else { 0xa5 },
        remove_mask: if kind == 0 { 0 } else { 0x36 },
        holds_mask: if kind == 0 { 0 } else { 0x12 },
        turn_bits: if kind == 0 { 0 } else { 0x83 },
        attack_mode: GameplayModsAttackMode::On,
        mini_percent: if kind == 0 { 0 } else { -35 },
        spacing_percent: 0,
        visual_delay_ms: if kind == 0 { 0 } else { 123 },
        average_error_bar_active: kind != 0,
        avg_error_bar_intensity_centi: 100,
        avg_error_bar_interval_ms: 500,
        accel: if kind == 0 {
            [0; 5]
        } else {
            [0, 50, 100, -75, 15]
        },
        visual: [0; 9],
        appearance: [0; 6],
        scroll: [0; 5],
        perspective_tilt: 0,
        perspective_skew: 0,
        dark: 0,
        blind: 0,
        cover: 0,
        disabled_timing_windows: if kind == 0 { 0 } else { 0x10 },
    })
}
