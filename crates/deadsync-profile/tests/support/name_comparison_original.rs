use std::cmp::Ordering;

/// Compare names case-insensitively over ASCII bytes without allocating.
#[must_use]
pub fn ascii_case_insensitive_cmp(left: &str, right: &str) -> Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

/// Compare names using Rust's Unicode lowercase mapping without allocating.
pub fn unicode_case_insensitive_cmp(left: &str, right: &str) -> Ordering {
    let mut offset = 0;
    for (&a, &b) in left.as_bytes().iter().zip(right.as_bytes()) {
        if !a.is_ascii() || !b.is_ascii() {
            break;
        }
        match a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()) {
            Ordering::Equal => offset += 1,
            different => return different,
        }
    }
    if offset == left.len().min(right.len()) {
        return left.len().cmp(&right.len());
    }
    // Every skipped byte was ASCII, so offset is a character boundary on both
    // sides. Keep Unicode expansions and comparison of the remaining suffix.
    let (left, right) = (&left[offset..], &right[offset..]);
    left.chars()
        .flat_map(char::to_lowercase)
        .cmp(right.chars().flat_map(char::to_lowercase))
}

pub fn cmp_profile_ids_case_insensitive(a: &str, b: &str) -> core::cmp::Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
        .then_with(|| a.cmp(b))
}
