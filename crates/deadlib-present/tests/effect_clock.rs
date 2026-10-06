use deadlib_present::{anim::EffectClock, dsl::__dsl_parse_effect_clock};

#[path = "../../../tests/support/perf.rs"]
mod perf;

#[test]
fn effect_clock_parsing_has_no_allocation_churn() {
    perf::assert_no_churn(|| {
        for input in [
            "beat",
            "BGM",
            "timer",
            "\"'BeAtNoOffset'\"",
            "heartbeat",
            "éBeAt界",
            "",
        ] {
            std::hint::black_box(__dsl_parse_effect_clock(std::hint::black_box(input)));
        }
    });
}

#[test]
fn effect_clock_preserves_aliases_quotes_and_substring_matching() {
    for (text, expected) in [
        ("beat", EffectClock::Beat),
        ("BEATNOOFFSET", EffectClock::Beat),
        (" bgm ", EffectClock::Beat),
        ("\u{2003}\"'BeAt'\"\u{2003}", EffectClock::Beat),
        ("heartbeat", EffectClock::Beat),
        ("éBEATé", EffectClock::Beat),
        ("timer", EffectClock::Time),
        ("timerglobal", EffectClock::Time),
        ("music", EffectClock::Time),
        ("musicnooffset", EffectClock::Time),
        ("seconds", EffectClock::Time),
        ("time", EffectClock::Time),
        ("bgmusic", EffectClock::Time),
        ("be at", EffectClock::Time),
        ("βeat", EffectClock::Time),
        ("", EffectClock::Time),
    ] {
        assert_eq!(__dsl_parse_effect_clock(text), expected, "{text:?}");
    }
    for prefix in ["", "é", "'", "\"", " ", "\t", "timer"] {
        for suffix in ["", "界", "'", "\"", " ", "\n", "music"] {
            for clock in ["beat", "BeAt", "BGM", "timer", "music", "unknown"] {
                let input = format!("{prefix}{clock}{suffix}");
                let original = input
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_ascii_lowercase();
                let expected = if original == "bgm" || original.contains("beat") {
                    EffectClock::Beat
                } else {
                    EffectClock::Time
                };
                assert_eq!(__dsl_parse_effect_clock(&input), expected, "{input:?}");
            }
        }
    }
}
