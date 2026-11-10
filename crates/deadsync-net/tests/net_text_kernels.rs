use deadsync_net::is_timeout_message;


#[test]
fn timeout_search_preserves_case_controls_boundaries_and_unicode() {
    for base in ["timeout", "timed out"] {
        for mask in 0..1usize << base.len() {
            let text: String = base
                .bytes()
                .enumerate()
                .map(|(i, b)| {
                    char::from(if mask & (1 << i) != 0 {
                        b.to_ascii_uppercase()
                    } else {
                        b
                    })
                })
                .collect();
            for text in [
                text.clone(),
                format!("\u{96ea}prefix{text}suffix"),
                format!("ttttt{text}"),
            ] {
                assert!(is_timeout_message(&text), "{text:?}");
            }
        }
        for i in 0..base.len() {
            for byte in 0..=127 {
                let mut bytes = base.as_bytes().to_vec();
                bytes[i] = byte;
                let text = String::from_utf8(bytes).unwrap();
                assert_eq!(
                    is_timeout_message(&text),
                    byte.to_ascii_lowercase() == base.as_bytes()[i],
                    "{text:?}"
                );
            }
            for ch in ["\u{130}", "\u{96ea}", "\u{2003}", "\u{e9}"] {
                let text = format!("{}{ch}{}", &base[..i], &base[i + 1..]);
                assert!(!is_timeout_message(&text), "{text:?}");
            }
        }
    }
    for text in [
        "",
        "t",
        "ti",
        "time",
        "timed",
        "time out",
        "t\0imeout",
    ] {
        assert!(!is_timeout_message(text), "{text:?}");
    }
}


#[test]
fn timeout_terms_are_matched_inside_longer_messages() {
    for text in ["timed outside", "ttimeout"] {
        assert!(is_timeout_message(text), "{text:?}");
    }
}
