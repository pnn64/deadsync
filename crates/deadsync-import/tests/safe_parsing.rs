#[path = "../../../tests/support/perf.rs"]
pub mod perf;
#[path = "../src/xml.rs"]
mod xml;

#[test]
fn unicode_xml_spans() {
    for unit in ["a", "é", "東京", "🙂", "\0"] {
        for len in [0, 1, 15, 16, 31, 32, 127, 128, 1024] {
            let text = unit.repeat(len);
            let input = format!(
                "<?xml version=\"1.0\"?><!--{text}--><根 名前='{text}'>{text}&amp;<![CDATA[{text}]]><子/>{text}</根>"
            );
            let root = xml::parse(&input).expect("valid fixture");
            assert_eq!(root.tag, "根");
            assert_eq!(root.attrs, [("名前".to_owned(), text.clone())]);
            assert_eq!(root.text, format!("{text}&{text}{text}"));
            assert_eq!(
                root.children,
                [xml::XmlNode {
                    tag: "子".into(),
                    ..Default::default()
                }]
            );
        }
    }
    for input in [
        "<根",
        "<根 a='🙂",
        "<根>🙂",
        "<根><![CDATA[🙂",
        "<!--🙂",
        "<?🙂",
        "<根 /🙂>",
    ] {
        assert!(xml::parse(input).is_err(), "{input:?}");
    }
}

#[test]
#[ignore = "manual performance measurement"]
fn safe_bench() {
    use std::hint::black_box;
    for (name, text) in [
        ("ascii", "a".repeat(4096)),
        ("unicode", "東京é🙂".repeat(512)),
    ] {
        for (kind, input) in [
            ("text", format!("<Stats>{text}</Stats>")),
            ("cdata", format!("<Stats><![CDATA[{text}]]></Stats>")),
            ("attribute", format!("<Stats text='{text}'/>")),
            ("comment", format!("<!--{text}--><Stats/>")),
        ] {
            perf::measure(&format!("xml_{kind}_{name}"), input.len(), || {
                black_box(xml::parse(black_box(&input)).expect("valid fixture"));
            });
        }
    }
    let score = r#"<Song Dir="Songs/Pack/Café &amp; Tea/"><Steps StepsType="dance-single" Difficulty="Hard"><HighScoreList><HighScore><Name>東京</Name><Grade>Grade_Tier01</Grade><PercentDP>0.9912</PercentDP></HighScore></HighScoreList></Steps></Song>"#;
    let input = format!(
        "<Stats><SongScores>{}</SongScores></Stats>",
        score.repeat(64)
    );
    perf::measure("xml_stats_64", 64, || {
        black_box(xml::parse(black_box(&input)).expect("valid stats fixture"));
    });
}
