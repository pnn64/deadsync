// Frozen from 6c782aee5 for differential tests and paired measurements.
use super::MachineFont;
pub fn normalize_locale(raw: &str) -> String {
    let mut lower = raw
        .trim()
        .split('.')
        .next()
        .unwrap_or(raw)
        .split('@')
        .next()
        .unwrap_or(raw)
        .replace('_', "-");
    lower.make_ascii_lowercase();

    if lower.starts_with("zh") {
        if lower.contains("hant") || lower.contains("tw") || lower.contains("hk") {
            return "zh-Hant".to_string();
        }
        if lower.contains("hans") || lower.contains("cn") || lower.contains("sg") {
            return "zh-Hans".to_string();
        }
        return "zh-Hans".to_string();
    }

    lower
}
pub fn font(s: &str) -> Result<MachineFont, ()> {
    match s.trim().to_ascii_lowercase().as_str() {
        "common" | "wendy" => Ok(MachineFont::Wendy),
        "mega" => Ok(MachineFont::Mega),
        "unprofessional" => Ok(MachineFont::Unprofessional),
        _ => Err(()),
    }
}
