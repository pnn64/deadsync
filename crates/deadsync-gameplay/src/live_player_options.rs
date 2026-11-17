/// The retained song VM's current notefield options, after native option approach.
/// Missing override fields retain the player's profile defaults. Appearance is
/// seeded from that profile before the VM's explicit current values are applied.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SongLuaPlayerOptions {
    pub accel: AccelOverrides,
    pub visual: VisualOverrides,
    pub appearance: AppearanceEffects,
    pub visibility: VisibilityOverrides,
    pub scroll: ScrollOverrides,
    pub perspective: PerspectiveOverrides,
    pub scroll_speed: Option<ScrollSpeedSetting>,
    pub mini_percent: Option<f32>,
}

impl SongLuaPlayerOptions {
    /// Accept a normalized PlayerOptions name and its already approached value.
    /// This shares baked target conversion without parsing strings, allocating
    /// windows, or applying a second approach to Current. Explicit zero is kept.
    pub fn set_current(&mut self, key: &str, value: f32) -> bool {
        if !value.is_finite() {
            return false;
        }
        let raw = if matches!(key, "xmod" | "cmod" | "mmod") {
            value
        } else {
            value * 100.0
        };
        let mut applied = false;
        visit_song_lua_mod_targets(key, raw, raw, value, value, |target, _, value| {
            if song_lua_player_transform_target(target)
                || matches!(
                    target,
                    SongLuaEaseMaskTarget::NoAttack
                        | SongLuaEaseMaskTarget::RandAttack
                        | SongLuaEaseMaskTarget::FailSetting
                )
            {
                return;
            }
            song_lua_apply_eased_target(
                target,
                value,
                &mut self.accel,
                &mut AttackFlagOverrides::default(),
                &mut None,
                &mut self.visual,
                &mut self.appearance,
                &mut self.visibility,
                &mut self.scroll,
                &mut self.perspective,
                &mut self.scroll_speed,
                &mut self.mini_percent,
                &mut SongLuaPlayerTransformValues::default(),
            );
            applied = true;
        });
        applied
    }
}

#[cfg(test)]
mod live_player_options_tests {
    use super::*;

    #[test]
    fn current_mods_keep_native_float_bits_and_explicit_zero() {
        let amount = f32::from_bits(0x3d6f_1234);
        let mut current = SongLuaPlayerOptions::default();
        assert!(current.set_current("brake", amount));
        assert!(current.set_current("flip", -amount));
        assert_eq!(current.accel.brake.unwrap().to_bits(), amount.to_bits());
        assert_eq!(current.visual.flip.unwrap().to_bits(), (-amount).to_bits());

        assert!(current.set_current("flip", 0.0));
        let effects = merge_attack_visual_effects(
            VisualEffects {
                flip: 1.0,
                drunk: 0.5,
                ..Default::default()
            },
            current.visual,
        );
        assert_eq!(effects.flip, 0.0, "Current zero overrides the saved effect");
        assert_eq!(
            effects.drunk, 0.5,
            "unspecified options preserve the saved effect"
        );
    }

    #[test]
    fn current_mods_cover_columns_switches_perspective_and_speed_units() {
        let mut current = SongLuaPlayerOptions {
            appearance: AppearanceEffects {
                hidden: 0.75,
                ..Default::default()
            },
            ..Default::default()
        };
        for (key, value) in [
            ("stealth", 0.2),
            ("stealth4", 0.3),
            ("dark2", 0.4),
            ("movex1", -0.25),
            ("reverse", 0.6),
            ("incoming", 0.5),
            ("dizzyholds", 1.0),
            ("stealthtype", 1.0),
            ("mini", 0.6),
            ("cmod", 600.0),
        ] {
            assert!(current.set_current(key, value), "{key}");
        }
        assert_eq!(current.appearance.hidden, 0.75);
        assert_eq!(current.appearance.stealth, 0.2);
        assert_eq!(current.appearance.stealth_cols[3], 0.3);
        assert_eq!(current.visibility.dark_cols[1], Some(0.4));
        assert_eq!(current.visual.move_x_cols[0], Some(-0.25));
        assert_eq!(current.scroll.reverse, Some(0.6));
        assert_eq!(current.perspective.tilt, Some(-0.5));
        assert_eq!(current.perspective.skew, Some(0.5));
        assert_eq!(current.visual.dizzy_holds, Some(true));
        assert!(current.appearance.stealth_type);
        assert!((current.mini_percent.unwrap() - 60.0).abs() < 0.00001);
        assert_eq!(current.scroll_speed, Some(ScrollSpeedSetting::CMod(600.0)));
        assert!(current.set_current("xmod", 1.5));
        assert_eq!(current.scroll_speed, Some(ScrollSpeedSetting::XMod(1.5)));
    }

    #[test]
    fn invalid_current_values_do_not_publish_a_partial_change() {
        let mut current = SongLuaPlayerOptions::default();
        assert!(current.set_current("stealth", 0.25));
        let before = current;
        for (key, value) in [
            ("stealth", f32::NAN),
            ("brake", f32::INFINITY),
            ("unknown", 1.0),
            ("failsetting", 1.0),
            ("skewx", 1.0),
        ] {
            assert!(!current.set_current(key, value), "{key}");
            assert_eq!(current, before);
        }
    }
}
