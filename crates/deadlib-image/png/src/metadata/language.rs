use std::collections::HashSet;

// RFC 5646 2.1/2.2.9: grammar plus variant/singleton uniqueness. Registration
// and extension-specific semantics are separate from syntactic well-formedness.
pub(super) fn well_formed(value: &[u8]) -> bool {
    if value.is_empty() {
        return true; // PNG permits an unspecified language.
    }
    const GRANDFATHERED: &[&[u8]] = &[
        b"art-lojban",
        b"cel-gaulish",
        b"en-GB-oed",
        b"i-ami",
        b"i-bnn",
        b"i-default",
        b"i-enochian",
        b"i-hak",
        b"i-klingon",
        b"i-lux",
        b"i-mingo",
        b"i-navajo",
        b"i-pwn",
        b"i-tao",
        b"i-tay",
        b"i-tsu",
        b"no-bok",
        b"no-nyn",
        b"sgn-BE-FR",
        b"sgn-BE-NL",
        b"sgn-CH-DE",
        b"zh-guoyu",
        b"zh-hakka",
        b"zh-min",
        b"zh-min-nan",
        b"zh-xiang",
    ];
    if GRANDFATHERED
        .iter()
        .any(|tag| value.eq_ignore_ascii_case(tag))
    {
        return true;
    }
    let mut parts = value.split(|b| *b == b'-').peekable();
    let first = parts.next().unwrap();
    let private =
        |part: &[u8]| (1..=8).contains(&part.len()) && part.iter().all(u8::is_ascii_alphanumeric);
    if first.eq_ignore_ascii_case(b"x") {
        return parts.peek().is_some() && parts.all(private);
    }
    if !(2..=8).contains(&first.len()) || !first.iter().all(u8::is_ascii_alphabetic) {
        return false;
    }
    if first.len() <= 3 {
        for _ in 0..3 {
            if parts
                .peek()
                .is_some_and(|p| p.len() == 3 && p.iter().all(u8::is_ascii_alphabetic))
            {
                parts.next();
            } else {
                break;
            }
        }
    }
    if parts
        .peek()
        .is_some_and(|p| p.len() == 4 && p.iter().all(u8::is_ascii_alphabetic))
    {
        parts.next();
    }
    if parts.peek().is_some_and(|p| {
        (p.len() == 2 && p.iter().all(u8::is_ascii_alphabetic))
            || (p.len() == 3 && p.iter().all(u8::is_ascii_digit))
    }) {
        parts.next();
    }
    let mut variants = HashSet::new();
    while let Some(&part) = parts.peek() {
        if !((5..=8).contains(&part.len()) || (part.len() == 4 && part[0].is_ascii_digit()))
            || !part.iter().all(u8::is_ascii_alphanumeric)
        {
            break;
        }
        let mut key = [0; 8];
        for (a, b) in key.iter_mut().zip(part) {
            *a = b.to_ascii_lowercase();
        }
        if !variants.insert(key) {
            return false;
        }
        parts.next();
    }
    let mut singletons = 0u64;
    while let Some(part) = parts.next() {
        if part.eq_ignore_ascii_case(b"x") {
            return parts.peek().is_some() && parts.all(private);
        }
        if part.len() != 1 || !part[0].is_ascii_alphanumeric() {
            return false;
        }
        let b = part[0].to_ascii_lowercase();
        let bit = if b.is_ascii_digit() {
            b - b'0'
        } else {
            b - b'a' + 10
        };
        if singletons & (1 << bit) != 0 {
            return false;
        }
        singletons |= 1 << bit;
        let mut count = 0;
        while parts
            .peek()
            .is_some_and(|p| (2..=8).contains(&p.len()) && p.iter().all(u8::is_ascii_alphanumeric))
        {
            parts.next();
            count += 1;
        }
        if count == 0 {
            return false;
        }
    }
    true
}
