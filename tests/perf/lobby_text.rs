use super::*;
use std::hint::black_box;

// Frozen from committed main 466815487; only internal reference names change.
fn original_joined_song_info_text(joined: &lobby_data::JoinedLobby) -> Option<String> {
    let song_info = joined.song_info.as_ref()?;
    let title = song_info
        .title
        .as_deref()
        .unwrap_or(song_info.song_path.as_str());
    let title = original_truncate_text(title, 38);

    let mut detail = String::new();
    if let Some(chart_label) = song_info.chart_label.as_deref()
        && !chart_label.trim().is_empty()
    {
        detail.push_str(chart_label.trim());
    }
    if let Some(rate) = song_info
        .rate
        .filter(|rate| rate.is_finite() && *rate > 0.0)
    {
        if !detail.is_empty() {
            detail.push_str("  ");
        }
        detail.push_str(format!("{rate:.2}x").as_str());
    }

    if detail.is_empty() {
        Some(format!("Selected Song\n{title}"))
    } else {
        Some(format!("Selected Song\n{title}\n{detail}"))
    }
}

fn original_truncate_text(text: &str, max_chars: usize) -> String {
    // Byte length is an upper bound on the number of characters.
    if text.len() <= max_chars || text.chars().count() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    let end = text
        .char_indices()
        .nth(keep)
        .map_or(text.len(), |(end, _)| end);
    let mut out = String::with_capacity(end + 3);
    out.push_str(&text[..end]);
    out.push_str("...");
    out
}

fn fixture(title: Option<&str>, label: Option<&str>, rate: Option<f32>) -> lobby_data::JoinedLobby {
    lobby_data::JoinedLobby {
        code: "ABCD".into(),
        players: Vec::new(),
        song_info: Some(lobby_data::LobbySongInfo {
            song_path: "Pack/Fallback Title".into(),
            title: title.map(str::to_owned),
            chart_label: label.map(str::to_owned),
            rate,
            ..Default::default()
        }),
    }
}

#[test]
fn lobby_song_text_preserves_unicode_truncation_and_detail_rules() {
    for title in [
        None,
        Some(""),
        Some("Short"),
        Some("é界🎵abc"),
        Some("abcdefghijklmnopqrstuvwxyz0123456789--"),
        Some("abcdefghijklmnopqrstuvwxyz0123456789---"),
        Some(
            "🎵é界e\u{301}🎵é界e\u{301}🎵é界e\u{301}🎵é界e\u{301}🎵é界e\u{301}🎵é界e\u{301}🎵é界e\u{301}🎵é界e\u{301}",
        ),
    ] {
        for label in [
            None,
            Some(""),
            Some(" \t\n"),
            Some(" Challenge 12 "),
            Some("é界🎵\nDouble"),
        ] {
            for rate in [
                None,
                Some(1.0),
                Some(1.125),
                Some(0.0),
                Some(-1.0),
                Some(f32::NAN),
                Some(f32::INFINITY),
                Some(f32::MAX),
                Some(f32::MIN_POSITIVE),
                Some(f32::from_bits(1)),
            ] {
                let joined = fixture(title, label, rate);
                assert_eq!(
                    joined_song_info_text(&joined),
                    original_joined_song_info_text(&joined),
                    "{title:?} {label:?} {rate:?}"
                );
            }
        }
    }
    let mut joined = fixture(None, None, None);
    joined.song_info = None;
    assert_eq!(joined_song_info_text(&joined), None);
}

#[test]
fn lobby_song_text_uses_one_output_allocation() {
    for title in ["Short", &"é界🎵".repeat(100)] {
        let joined = fixture(Some(title), Some("Challenge 12"), Some(1.25));
        let (expected, before) = crate::perf::measure(|| original_joined_song_info_text(&joined));
        let (actual, after) = crate::perf::measure(|| joined_song_info_text(&joined));
        assert_eq!(actual, expected);
        assert_eq!(after.allocs, 1);
        assert_eq!(after.reallocs, 0);
        assert!(before.allocs >= 4);
        assert!(after.peak_added_bytes < before.peak_added_bytes);
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_runtime_traversal_lobby_text() {
    for (name, joined) in [
        ("title", fixture(Some("Short Title"), None, None)),
        (
            "chart",
            fixture(Some("Short Title"), Some("Challenge 12"), None),
        ),
        ("rate", fixture(None, None, Some(1.25))),
        (
            "full",
            fixture(Some("Short Title"), Some("Challenge 12"), Some(1.25)),
        ),
        (
            "unicode",
            fixture(Some(&"é界🎵".repeat(30)), Some("Challenge 12"), Some(1.25)),
        ),
        (
            "long_title",
            fixture(
                Some(&"é界🎵".repeat(4096)),
                Some("Challenge 12"),
                Some(1.25),
            ),
        ),
        (
            "large_rate",
            fixture(Some("Short Title"), Some("Challenge 12"), Some(f32::MAX)),
        ),
        (
            "long_label",
            fixture(Some("Short Title"), Some(&"Chart ".repeat(100)), Some(1.25)),
        ),
    ] {
        crate::traversal_perf::compare(
            &format!("text/{name}"),
            || {
                black_box(original_joined_song_info_text(black_box(&joined)));
            },
            || {
                black_box(joined_song_info_text(black_box(&joined)));
            },
        );
    }
}
