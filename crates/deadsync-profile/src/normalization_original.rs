// Frozen from a6f3d013f for differential normalization and ordering checks.
use super::{ComboMode, TurnOption};
use std::cmp::Ordering;
const OPTION_KEY_STACK_BYTES: usize = 64;
#[inline]
pub fn with_compact_option_key<R>(value: &str, use_key: impl FnOnce(&str) -> R) -> R {
    let value = value.trim();
    let mut bytes = [0u8; OPTION_KEY_STACK_BYTES];
    let mut len = 0;
    for byte in value.bytes().filter(u8::is_ascii_alphanumeric) {
        if len == OPTION_KEY_STACK_BYTES {
            let mut key = String::with_capacity(value.len());
            key.extend(
                value
                    .bytes()
                    .filter(u8::is_ascii_alphanumeric)
                    .map(|byte| byte.to_ascii_lowercase() as char),
            );
            return use_key(&key);
        }
        bytes[len] = byte.to_ascii_lowercase();
        len += 1;
    }
    let key = std::str::from_utf8(&bytes[..len])
        .expect("ASCII profile option key must remain valid UTF-8");
    use_key(key)
}
pub fn ascii_case_insensitive_cmp(left: &str, right: &str) -> Ordering {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len().min(right.len()) < 20 {
        return left
            .iter()
            .map(u8::to_ascii_lowercase)
            .cmp(right.iter().map(u8::to_ascii_lowercase));
    }
    for (&a, &b) in left[..4].iter().zip(&right[..4]) {
        let order = a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase());
        if order != Ordering::Equal {
            return order;
        }
    }
    cmp_ascii_blocks(&left[4..], &right[4..])
}
#[inline(never)]
fn cmp_ascii_blocks(left: &[u8], right: &[u8]) -> Ordering {
    let mut offset = 0;
    for (&a, &b) in left
        .as_chunks::<16>()
        .0
        .iter()
        .zip(right.as_chunks::<16>().0)
    {
        let a = a.map(|byte| byte.to_ascii_lowercase());
        let b = b.map(|byte| byte.to_ascii_lowercase());
        let order = a.cmp(&b);
        if order != Ordering::Equal {
            return order;
        }
        offset += 16;
    }
    left[offset..]
        .iter()
        .map(u8::to_ascii_lowercase)
        .cmp(right[offset..].iter().map(u8::to_ascii_lowercase))
}
pub fn turn(s: &str) -> Result<TurnOption, String> {
    with_compact_option_key(s, |key| match key {
        "" | "none" | "noturn" | "noturning" | "noturns" => Ok(TurnOption::None),
        "mirror" => Ok(TurnOption::Mirror),
        "left" => Ok(TurnOption::Left),
        "right" => Ok(TurnOption::Right),
        "lrmirror" => Ok(TurnOption::LRMirror),
        "udmirror" => Ok(TurnOption::UDMirror),
        "shuffle" => Ok(TurnOption::Shuffle),
        "blender" | "supershuffle" => Ok(TurnOption::Blender),
        "random" | "hypershuffle" => Ok(TurnOption::Random),
        other => Err(format!("'{other}' is not a valid Turn setting")),
    })
}
pub fn combo(s: &str) -> Result<ComboMode, String> {
    with_compact_option_key(s, |key| match key {
        "fullcombo" => Ok(ComboMode::FullCombo),
        "currentcombo" => Ok(ComboMode::CurrentCombo),
        other => Err(format!("'{other}' is not a valid ComboMode setting")),
    })
}
