// Frozen from 38890f14b70cc7aee1db6078a2ebd872e577195f; visibility only adapted.

pub(super) fn normalize_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        }
    }
    out
}
