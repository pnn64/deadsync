use super::support::{actor, assert_array_ulp, f32_array, f32_at, fixture, samples};
use deadlib_present::anim::{EffectClock, EffectMode};
use deadsync_song_lua::{SongLuaOverlayState, parse_overlay_effect_mode};
use deadsync_theme_simply_love::screens::gameplay::actor_conformance::{
    effect_sample, vibration_magnitude, vibration_sample,
};

struct Mt19937 {
    state: [u32; 624],
    index: usize,
}

impl Mt19937 {
    fn new(seed: u32) -> Self {
        let mut state = [0; 624];
        state[0] = seed;
        for index in 1..624 {
            state[index] = 1_812_433_253_u32
                .wrapping_mul(state[index - 1] ^ (state[index - 1] >> 30))
                .wrapping_add(index as u32);
        }
        Self { state, index: 624 }
    }

    fn next(&mut self) -> u32 {
        if self.index == 624 {
            for index in 0..624 {
                let value = (self.state[index] & 0x8000_0000)
                    | (self.state[(index + 1) % 624] & 0x7fff_ffff);
                let twisted = (value >> 1) ^ if value & 1 == 0 { 0 } else { 0x9908_b0df };
                self.state[index] = self.state[(index + 397) % 624] ^ twisted;
            }
            self.index = 0;
        }
        let mut value = self.state[self.index];
        self.index += 1;
        value ^= value >> 11;
        value ^= (value << 7) & 0x9d2c_5680;
        value ^= (value << 15) & 0xefc6_0000;
        value ^ (value >> 18)
    }

    fn uniform_signed(&mut self) -> f32 {
        const RANGE: f64 = 4_294_967_296.0;
        let canonical = (self.next() as f64 + self.next() as f64 * RANGE) / (RANGE * RANGE);
        canonical.mul_add(2.0, -1.0) as f32
    }
}

fn state_for(name: &str) -> SongLuaOverlayState {
    match name {
        "wag" => SongLuaOverlayState {
            x: 220.0,
            y: 120.0,
            effect_mode: EffectMode::Wag,
            effect_clock: EffectClock::Beat,
            effect_period: 1.0,
            effect_magnitude: [10.0, 20.0, 30.0],
            ..SongLuaOverlayState::default()
        },
        "pulse" => SongLuaOverlayState {
            x: 340.0,
            y: 120.0,
            effect_mode: EffectMode::Pulse,
            effect_period: 1.0,
            effect_magnitude: [0.75, 1.5, 1.0],
            effect_color1: [1.0, 0.8, 0.6, 1.0],
            effect_color2: [0.5, 1.0, 1.25, 1.0],
            ..SongLuaOverlayState::default()
        },
        "diffuse" => SongLuaOverlayState {
            x: 460.0,
            y: 120.0,
            effect_mode: EffectMode::DiffuseShift,
            effect_period: 1.0,
            effect_color1: [1.0, 0.0, 0.0, 1.0],
            effect_color2: [0.0, 0.25, 1.0, 0.5],
            effect_timing: Some([0.1, 0.2, 0.3, 0.15, 0.25]),
            ..SongLuaOverlayState::default()
        },
        "glow" => SongLuaOverlayState {
            x: 220.0,
            y: 280.0,
            effect_mode: EffectMode::GlowRamp,
            effect_period: 1.0,
            effect_offset: 0.125,
            effect_color1: [1.0, 1.0, 1.0, 0.8],
            effect_color2: [0.0, 0.0, 0.0, 0.0],
            ..SongLuaOverlayState::default()
        },
        "spin" => SongLuaOverlayState {
            x: 380.0,
            y: 280.0,
            effect_mode: EffectMode::Spin,
            effect_magnitude: [30.0, 60.0, 90.0],
            ..SongLuaOverlayState::default()
        },
        _ => panic!("unknown effect actor {name:?}"),
    }
}

#[test]
fn blink_ramp_and_shift_commands_remain_distinct_effect_modes() {
    assert_eq!(
        parse_overlay_effect_mode("diffuseblink"),
        Some(EffectMode::DiffuseBlink)
    );
    assert_eq!(
        parse_overlay_effect_mode("glowblink"),
        Some(EffectMode::GlowBlink)
    );
    assert_eq!(
        parse_overlay_effect_mode("glowramp"),
        Some(EffectMode::GlowRamp)
    );
    assert_eq!(
        parse_overlay_effect_mode("glowshift"),
        Some(EffectMode::GlowShift)
    );
}

fn assert_effect_actor(name: &str, field: &str) {
    let oracle = fixture("effects-vibration");
    for sample in samples(&oracle) {
        let time = f32_at(sample, "time");
        let beat = f32_at(sample, "beat");
        let actual = effect_sample(state_for(name), time, beat);
        let expected = &actor(sample, name)["effected"];
        let (actual, expected) = match field {
            "rotation" => (actual.rotation, f32_array(&expected["rotation"])),
            "zoom" => (actual.scale, f32_array(&expected["zoom"])),
            _ => panic!("unsupported vec3 field {field:?}"),
        };
        assert_array_ulp(actual, expected, 32, &format!("{name} {field} at {time}"));
    }
}

#[test]
fn wag_rotation_matches_itgmania_beat_clock() {
    assert_effect_actor("wag", "rotation");
}

#[test]
fn pulse_zoom_and_color_scaling_match_itgmania() {
    assert_effect_actor("pulse", "zoom");
}

#[test]
fn spin_rotation_matches_itgmania_timer_clock() {
    assert_effect_actor("spin", "rotation");
}

#[test]
fn diffuse_shift_timing_and_colors_match_itgmania() {
    let oracle = fixture("effects-vibration");
    for sample in samples(&oracle) {
        let time = f32_at(sample, "time");
        let actual = effect_sample(state_for("diffuse"), time, f32_at(sample, "beat"));
        let expected = f32_array(&actor(sample, "diffuse")["effected"]["diffuse"][0]);
        assert_array_ulp(
            actual.tint,
            expected,
            32,
            &format!("diffuse color at {time}"),
        );
    }
}

#[test]
fn glow_ramp_timing_and_colors_match_itgmania() {
    let oracle = fixture("effects-vibration");
    for sample in samples(&oracle) {
        let time = f32_at(sample, "time");
        let actual = effect_sample(state_for("glow"), time, f32_at(sample, "beat"));
        let expected = f32_array(&actor(sample, "glow")["effected"]["glow"]);
        assert_array_ulp(actual.glow, expected, 16, &format!("glow color at {time}"));
    }
}

#[test]
fn glow_shift_fades_with_native_actor_alpha() {
    let oracle = fixture("glow-alpha");
    let mut checked = 0;
    for sample in samples(&oracle) {
        for native in sample["actors"].as_array().expect("actors").iter().skip(1) {
            let mode = match native["effect"]["type"].as_str().expect("effect") {
                "glow_shift" => EffectMode::GlowShift,
                mode => panic!("unexpected effect {mode}"),
            };
            let state = SongLuaOverlayState {
                diffuse: f32_array(&native["current"]["diffuse"][0]),
                effect_mode: mode,
                effect_period: 0.05,
                effect_color1: [1.0, 1.0, 1.0, 0.0],
                effect_color2: [1.0, 1.0, 1.0, 0.5],
                ..Default::default()
            };
            let actual = effect_sample(state, f32_at(&native["effect"], "seconds"), 0.0);
            if native["drawn"] == false {
                assert_eq!(
                    actual.glow[3], 0.0,
                    "{} must stay dark at {}",
                    native["name"], sample["time"]
                );
            } else {
                assert_array_ulp(
                    actual.glow,
                    f32_array(&native["effected"]["glow"]),
                    32,
                    &format!("{} at {}", native["name"], sample["time"]),
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 40);
}

#[test]
fn vibration_magnitude_includes_actor_and_inherited_frame_values() {
    let state = SongLuaOverlayState {
        vibrate: true,
        effect_magnitude: [7.0, 11.0, 3.0],
        inherited_vibrate: [2.0, 4.0, 6.0],
        ..SongLuaOverlayState::default()
    };
    assert_eq!(vibration_magnitude(state), [9.0, 15.0, 9.0]);
}

#[test]
fn seeded_vibration_samples_match_itgmania_exactly() {
    let oracle = fixture("effects-vibration");
    let mut random = Mt19937::new(1337);
    for sample in samples(&oracle) {
        let expected = f32_array::<3>(&actor(sample, "vibrate")["effected"]["position"]);
        let jitter = std::array::from_fn(|_| random.uniform_signed());
        let actual = vibration_sample([100.0, 120.0, 0.0], [7.0, 11.0, 3.0], jitter);
        assert_array_ulp(
            actual,
            expected,
            4,
            &format!("vibration at {}", f32_at(sample, "time")),
        );
    }
}

#[test]
fn note_zoom_spline_matches_native_receptor_scale() {
    use deadsync_gameplay::{SongLuaNoteHideWindowRuntime, SongLuaNoteHideWindows};
    let mut hides = SongLuaNoteHideWindows::new(vec![SongLuaNoteHideWindowRuntime {
        column: 0,
        start_beat: 50.0 / 48.0,
        end_beat: 97.0 / 48.0,
    }]);
    hides.set_zoom_spline(0, 1.0 / 48.0, 99);
    let oracle = fixture("note-zoom-spline");
    for sample in samples(&oracle) {
        let beat = f32_at(sample, "beat");
        let zoom = 1.0 + hides.zoom_offset(0, beat);
        let expected: [f32; 3] = f32_array(&actor(sample, "receptor")["current"]["zoom"]);
        assert!(
            (zoom - expected[0]).abs() < 0.000_002,
            "native spline at beat {beat}: {zoom} != {}",
            expected[0]
        );
    }
}
