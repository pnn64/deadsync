use deadsync_profile::NoCmodAlternative;
use deadsync_rules::scroll::ScrollSpeedSetting;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedModType {
    X,
    C,
    M,
}

impl SpeedModType {
    /// Index used by the Type-of-Speed-Mod row's x/c/m choices.
    #[inline(always)]
    pub const fn choice_index(self) -> usize {
        match self {
            Self::X => 0,
            Self::C => 1,
            Self::M => 2,
        }
    }

    #[inline(always)]
    pub const fn from_choice_index(idx: usize) -> Self {
        match idx {
            0 => Self::X,
            1 => Self::C,
            2 => Self::M,
            _ => Self::C,
        }
    }

    #[inline(always)]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::C => "C",
            Self::M => "M",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpeedMod {
    pub mod_type: SpeedModType,
    pub value: f32,
}

impl SpeedMod {
    pub fn display(&self) -> String {
        match self.mod_type {
            SpeedModType::X => format!("{:.2}x", self.value),
            SpeedModType::C => format!("C{}", self.value as i32),
            SpeedModType::M => format!("M{}", self.value as i32),
        }
    }
}

impl From<ScrollSpeedSetting> for SpeedMod {
    fn from(setting: ScrollSpeedSetting) -> Self {
        match setting {
            ScrollSpeedSetting::XMod(mult) => Self {
                mod_type: SpeedModType::X,
                value: mult,
            },
            ScrollSpeedSetting::CMod(bpm) => Self {
                mod_type: SpeedModType::C,
                value: bpm,
            },
            ScrollSpeedSetting::MMod(bpm) => Self {
                mod_type: SpeedModType::M,
                value: bpm,
            },
        }
    }
}

#[inline(always)]
pub const fn scroll_speed_for_mod(speed_mod: &SpeedMod) -> ScrollSpeedSetting {
    match speed_mod.mod_type {
        SpeedModType::C => ScrollSpeedSetting::CMod(speed_mod.value),
        SpeedModType::X => ScrollSpeedSetting::XMod(speed_mod.value),
        SpeedModType::M => ScrollSpeedSetting::MMod(speed_mod.value),
    }
}

#[inline(always)]
pub const fn no_cmod_alt_speed_mod_type(alt: NoCmodAlternative) -> Option<SpeedModType> {
    match alt {
        NoCmodAlternative::None => None,
        NoCmodAlternative::XMod => Some(SpeedModType::X),
        NoCmodAlternative::MMod => Some(SpeedModType::M),
    }
}

#[inline(always)]
fn round_to_step(x: f32, step: f32) -> f32 {
    (x / step).round() * step
}

pub fn convert_speed_mod_to_type(
    speed_mod: &SpeedMod,
    new_type: SpeedModType,
    reference_bpm: f32,
    rate: f32,
) -> SpeedMod {
    let target_bpm = match speed_mod.mod_type {
        SpeedModType::C | SpeedModType::M => speed_mod.value,
        SpeedModType::X => (reference_bpm * rate * speed_mod.value).round(),
    };
    let value = match new_type {
        SpeedModType::X => {
            let denom = reference_bpm * rate;
            let raw = if denom.is_finite() && denom > 0.0 {
                target_bpm / denom
            } else {
                1.0
            };
            round_to_step(raw, 0.05).clamp(0.05, 20.0)
        }
        SpeedModType::C | SpeedModType::M => round_to_step(target_bpm, 5.0).clamp(5.0, 2000.0),
    };
    SpeedMod {
        mod_type: new_type,
        value,
    }
}

pub fn effective_scroll_speed_with_alt(
    base: &SpeedMod,
    alt: NoCmodAlternative,
    is_no_cmod: bool,
    reference_bpm: f32,
    rate: f32,
) -> ScrollSpeedSetting {
    match no_cmod_alt_speed_mod_type(alt) {
        Some(new_type) if is_no_cmod && base.mod_type == SpeedModType::C => scroll_speed_for_mod(
            &convert_speed_mod_to_type(base, new_type, reference_bpm, rate),
        ),
        _ => scroll_speed_for_mod(base),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_mod_round_trips_persisted_settings() {
        for setting in [
            ScrollSpeedSetting::XMod(1.75),
            ScrollSpeedSetting::CMod(400.0),
            ScrollSpeedSetting::MMod(650.0),
        ] {
            assert_eq!(scroll_speed_for_mod(&SpeedMod::from(setting)), setting);
        }
    }

    #[test]
    fn no_cmod_alternative_preserves_target_scroll_speed() {
        let base = SpeedMod {
            mod_type: SpeedModType::C,
            value: 400.0,
        };
        assert_eq!(
            effective_scroll_speed_with_alt(&base, NoCmodAlternative::XMod, true, 200.0, 1.0,),
            ScrollSpeedSetting::XMod(2.0),
        );
        assert_eq!(
            effective_scroll_speed_with_alt(&base, NoCmodAlternative::MMod, false, 200.0, 1.0,),
            ScrollSpeedSetting::CMod(400.0),
        );
    }
}
