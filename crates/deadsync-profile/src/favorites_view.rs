use std::cmp::Ordering;

/// Compare names case-insensitively over ASCII bytes without allocating.
#[must_use]
pub fn ascii_case_insensitive_cmp(left: &str, right: &str) -> Ordering {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len().min(right.len()) < 20 {
        if left.first() != right.first() {
            return left
                .iter()
                .map(u8::to_ascii_lowercase)
                .cmp(right.iter().map(u8::to_ascii_lowercase));
        }
        for (&a, &b) in left.iter().zip(right) {
            if a != b {
                let order = a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
        return left.len().cmp(&right.len());
    }
    for (&a, &b) in left[..4].iter().zip(&right[..4]) {
        let order = a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase());
        if order != Ordering::Equal {
            return order;
        }
    }
    if left.len().min(right.len()) >= 68 {
        cmp_ascii_prefix_blocks(&left[4..], &right[4..])
    } else {
        cmp_ascii_blocks(&left[4..], &right[4..])
    }
}

// Keep block temporaries out of the short/first-byte return paths.
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

// Isolate long-prefix probing from the ordinary folded-block loop.
#[inline(never)]
fn cmp_ascii_prefix_blocks(left: &[u8], right: &[u8]) -> Ordering {
    let shared = left.len().min(right.len());
    let mut offset = 0;
    while shared - offset >= 16 {
        if left[offset..offset + 16] != right[offset..offset + 16] {
            break;
        }
        offset += 16;
    }
    cmp_ascii_blocks(&left[offset..], &right[offset..])
}

/// Compare names using Rust's Unicode lowercase mapping without allocating.
pub fn unicode_case_insensitive_cmp(left: &str, right: &str) -> Ordering {
    let mut offset = 0;
    // Resolve short prefixes before scanning either complete ASCII suffix.
    for (&a, &b) in left.as_bytes().iter().zip(right.as_bytes()).take(8) {
        if !a.is_ascii() || !b.is_ascii() {
            break;
        }
        match a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()) {
            Ordering::Equal => offset += 1,
            different => return different,
        }
    }
    if offset == 8 && left[offset..].is_ascii() && right[offset..].is_ascii() {
        return ascii_case_insensitive_cmp(&left[offset..], &right[offset..]);
    }
    for (&a, &b) in left.as_bytes()[offset..]
        .iter()
        .zip(&right.as_bytes()[offset..])
    {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparator_matches_lowercase_sort_keys_without_allocating() {
        let names = ["Alpha", "alpha", "BETA", "zebra", "Äther", "äther", ""];

        for left in names {
            for right in names {
                assert_eq!(
                    ascii_case_insensitive_cmp(left, right),
                    left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()),
                    "comparison changed for {left:?} and {right:?}"
                );
            }
        }
    }

    #[test]
    fn unicode_comparator_matches_lowercase_sort_keys_without_allocating() {
        let names = [
            "Alpha",
            "alpha",
            "İstanbul",
            "istanbul",
            "Σteps",
            "σteps",
            "ẞ",
            "ß",
            "",
        ];

        for left in names {
            for right in names {
                assert_eq!(
                    unicode_case_insensitive_cmp(left, right),
                    left.to_lowercase().cmp(&right.to_lowercase()),
                    "comparison changed for {left:?} and {right:?}"
                );
            }
        }
    }
}
