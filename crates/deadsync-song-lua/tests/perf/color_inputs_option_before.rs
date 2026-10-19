// Frozen option default function body from 552763528.
use crate::custom_option_default_text;
pub fn option_row_default_text(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "speedmodtype" => "X".to_string(),
        "speedmod" => "1".to_string(),
        "mini" => "0%".to_string(),
        "perspective" => "Overhead".to_string(),
        "noteskin" | "noteskinvariant" => "default".to_string(),
        "musicrate" => "1".to_string(),
        _ => custom_option_default_text(name).unwrap_or_default(),
    }
}
