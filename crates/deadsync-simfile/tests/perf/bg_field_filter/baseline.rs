// Frozen from 462b31ec5 (0.5.1705).

pub(super) fn bgchange_field_rejects_non_media(field: &str) -> bool {
    contains_ignore_ascii_case(field, ".ini") || contains_ignore_ascii_case(field, ".xml")
}

#[inline]
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}
