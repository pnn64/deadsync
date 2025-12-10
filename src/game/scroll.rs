use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollSpeedSetting {
    CMod(f32),
    XMod(f32),
    MMod(f32),
}

impl Default for ScrollSpeedSetting {
    fn default() -> Self {
        ScrollSpeedSetting::CMod(600.0)
    }
}

impl fmt::Display for ScrollSpeedSetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScrollSpeedSetting::CMod(bpm) => {
                if (*bpm - bpm.round()).abs() < f32::EPSILON {
                    write!(f, "C{}", bpm.round() as i32)
                } else {
                    write!(f, "C{}", bpm)
                }
            }
            ScrollSpeedSetting::XMod(multiplier) => {
                if (*multiplier - multiplier.round()).abs() < f32::EPSILON {
                    write!(f, "X{}", multiplier.round() as i32)
                } else {
                    write!(f, "X{:.2}", multiplier)
                }
            }
            ScrollSpeedSetting::MMod(bpm) => {
                if (*bpm - bpm.round()).abs() < f32::EPSILON {
                    write!(f, "M{}", bpm.round() as i32)
                } else {
                    write!(f, "M{}", bpm)
                }
            }
        }
    }
}

impl FromStr for ScrollSpeedSetting {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err("ScrollSpeed value is empty".to_string());
        }

        let (variant, value_str) = if let Some(rest) = trimmed.strip_prefix('C') {
            ("C", rest)
        } else if let Some(rest) = trimmed.strip_prefix('c') {
            ("C", rest)
        } else if let Some(rest) = trimmed.strip_prefix('X') {
            ("X", rest)
        } else if let Some(rest) = trimmed.strip_prefix('x') {
            ("X", rest)
        } else if let Some(rest) = trimmed.strip_prefix('M') {
            ("M", rest)
        } else if let Some(rest) = trimmed.strip_prefix('m') {
            ("M", rest)
        } else {
            return Err(format!(
                "ScrollSpeed '{}' must start with 'C', 'X', or 'M'",
                trimmed
            ));
        };

        let value: f32 = value_str
            .trim()
            .parse()
            .map_err(|_| format!("ScrollSpeed '{}' is not a valid number", trimmed))?;

        if value <= 0.0 {
            return Err(format!(
                "ScrollSpeed '{}' must be greater than zero",
                trimmed
            ));
        }

        match variant {
            "C" => Ok(ScrollSpeedSetting::CMod(value)),
            "X" => Ok(ScrollSpeedSetting::XMod(value)),
            "M" => Ok(ScrollSpeedSetting::MMod(value)),
            _ => Err(format!(
                "ScrollSpeed '{}' has an unsupported modifier",
                trimmed
            )),
        }
    }
}

impl ScrollSpeedSetting {
    pub const ARROW_SPACING: f32 = 64.0;

    pub fn effective_bpm(
        self,
        current_chart_bpm: f32,
        reference_bpm: f32,
        _music_rate: f32,
    ) -> f32 {
        // Effective BPM is expressed per chart-second. Music rate scales beats per real second,
        // not this chart-time value. Keep X/M independent of rate here.
        match self {
            ScrollSpeedSetting::CMod(bpm) => bpm,
            ScrollSpeedSetting::XMod(multiplier) => current_chart_bpm * multiplier,
            ScrollSpeedSetting::MMod(target_bpm) => {
                if reference_bpm > 0.0 {
                    current_chart_bpm * (target_bpm / reference_bpm)
                } else {
                    current_chart_bpm
                }
            }
        }
    }

    pub fn beat_multiplier(self, reference_bpm: f32, music_rate: f32) -> f32 {
        // Pixel speed per real second for beat-based mods depends on d(beat)/dt.
        // Since d(beat)/dt scales with Music Rate, apply:
        // - XMod: multiplier only (rate is already implicit via d(beat)/dt)
        // - MMod: (target/reference) divided by rate to hold constant under rate
        match self {
            ScrollSpeedSetting::XMod(multiplier) => multiplier,
            ScrollSpeedSetting::MMod(target_bpm) => {
                let r = if music_rate.is_finite() && music_rate > 0.0 {
                    music_rate
                } else {
                    1.0
                };
                if reference_bpm > 0.0 {
                    (target_bpm / reference_bpm) / r
                } else {
                    1.0 / r
                }
            }
            _ => 1.0,
        }
    }

    pub fn pixels_per_second(
        self,
        current_chart_bpm: f32,
        reference_bpm: f32,
        music_rate: f32,
    ) -> f32 {
        let bpm = self.effective_bpm(current_chart_bpm, reference_bpm, music_rate);
        if !bpm.is_finite() || bpm <= 0.0 {
            0.0
        } else {
            (bpm / 60.0) * Self::ARROW_SPACING
        }
    }

    pub fn travel_time_seconds(
        self,
        draw_distance: f32,
        current_chart_bpm: f32,
        reference_bpm: f32,
        music_rate: f32,
    ) -> f32 {
        let speed = self.pixels_per_second(current_chart_bpm, reference_bpm, music_rate);
        if speed <= 0.0 {
            0.0
        } else {
            draw_distance / speed
        }
    }
}
