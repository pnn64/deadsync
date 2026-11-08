use deadlib_image_png::{
    ChunkPosition, LANGUAGE_SUBTAG_REGISTRY_DATE, Metadata, zlib::Compression,
};
use std::collections::HashMap;

fn accepted(tag: &str) -> bool {
    Metadata::InternationalText {
        keyword: b"Comment".to_vec(),
        language: tag.into(),
        translated_keyword: String::new(),
        text: String::new(),
        compressed: false,
    }
    .to_chunk(ChunkPosition::BeforeData, Compression::Stored)
    .is_ok()
}

fn values(value: &str) -> Vec<String> {
    if let Some((start, end)) = value.split_once("..") {
        // Independent mixed-radix counter covers every reserved private code.
        let mut current = start.to_ascii_lowercase().into_bytes();
        let end = end.to_ascii_lowercase();
        let mut out = Vec::new();
        loop {
            out.push(String::from_utf8(current.clone()).unwrap());
            if current == end.as_bytes() {
                break;
            }
            for b in current.iter_mut().rev() {
                if *b == b'z' {
                    *b = b'a';
                } else {
                    *b += 1;
                    break;
                }
            }
        }
        out
    } else {
        vec![value.to_owned()]
    }
}

#[test]
fn every_pinned_iana_subtag_range_and_extlang_prefix_is_encodable() {
    let data = include_str!("fixtures/language/language-subtag-registry");
    assert!(data.starts_with(&format!("File-Date: {LANGUAGE_SUBTAG_REGISTRY_DATE}\n")));
    let mut tested = HashMap::<&str, usize>::new();
    for block in data.split("%%").skip(1) {
        let fields: HashMap<_, _> = block
            .lines()
            .filter_map(|line| line.split_once(": "))
            .collect();
        let Some(kind) = fields.get("Type").copied() else {
            continue;
        };
        if let Some(&subtag) = fields.get("Subtag") {
            for value in values(subtag) {
                let tag = match kind {
                    "language" => value,
                    "script" | "region" | "variant" => format!("en-{value}"),
                    "extlang" => format!("{}-{value}", fields["Prefix"]),
                    _ => panic!("unknown registry record type: {kind}"),
                };
                assert!(accepted(&tag), "{kind}: {tag}");
                assert!(accepted(&tag.to_ascii_uppercase()), "case: {tag}");
                if kind == "extlang" {
                    let wrong = format!("en-{subtag}");
                    assert!(!accepted(&wrong), "wrong extlang prefix: {wrong}");
                }
                *tested.entry(kind).or_default() += 1;
            }
        } else if let Some(tag) = fields.get("Tag") {
            assert!(accepted(tag), "{kind}: {tag}");
            *tested.entry(kind).or_default() += 1;
        }
    }
    assert!(tested["language"] > 8000);
    assert!(tested["extlang"] > 200);
    assert!(tested["script"] > 200);
    assert!(tested["region"] > 300);
    assert!(tested["variant"] > 100);
    assert_eq!(tested["grandfathered"], 26);
    println!("IANA {LANGUAGE_SUBTAG_REGISTRY_DATE}: {tested:?}");
}

#[test]
fn private_ranges_deprecated_codes_and_registered_extensions() {
    for tag in [
        "qaa",
        "qtz",
        "en-Qaaa",
        "en-Qabx",
        "en-AA",
        "en-QM",
        "en-QZ",
        "en-XA",
        "en-XZ",
        "en-ZZ",
        "en-BU",
        "bh",
        "i-klingon",
        "en-u-nu-latn",
        "en-t-de-u-nu-latn",
        "x-anything",
        "en-x-a-b-c",
        "zh-cmn-Hans-CN",
        "ar-arb",
    ] {
        assert!(accepted(tag), "{tag}");
    }
    for tag in [
        "zz", "zzzz", "en-Abcd", "en-AB", "en-abcde", "en-cmn", "zh-arb", "qaaa", "qzz", "en-Qaby",
        "en-QL", "en-a-aaa", "en-0-abc",
    ] {
        assert!(!accepted(tag), "{tag}");
    }
}
