// Frozen from e105d114a for strict grammar and allocation comparisons.
use super::{GameFlag, MachinePreferredPlayMode, SelectMusicPatternInfoMode};
pub fn pattern(s: &str) -> Result<SelectMusicPatternInfoMode, ()> {
    match s.trim().to_ascii_lowercase().as_str() {
        "tech" => Ok(SelectMusicPatternInfoMode::Tech),
        "stamina" => Ok(SelectMusicPatternInfoMode::Stamina),
        "auto" => Ok(SelectMusicPatternInfoMode::Auto),
        _ => Err(()),
    }
}
pub fn play(s: &str) -> Result<MachinePreferredPlayMode, ()> {
    match s.trim().to_ascii_lowercase().as_str() {
        "regular" => Ok(MachinePreferredPlayMode::Regular),
        "marathon" => Ok(MachinePreferredPlayMode::Marathon),
        _ => Err(()),
    }
}
pub fn game(s: &str) -> Result<GameFlag, ()> {
    match s.trim().to_ascii_lowercase().as_str() {
        "dance" => Ok(GameFlag::Dance),
        "pump" => Ok(GameFlag::Pump),
        _ => Err(()),
    }
}
