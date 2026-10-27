//! BCP47 core/extension syntax with extension contents kept opaque.
use super::chunks;
use deadlib_image_png::ChunkPosition;

pub const TRANSLATED: &str = "Titre \u{e9}";
pub const TEXT: &str = "Exact \u{e9} \u{1f980}\n";
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Typed,
    Opaque,
    Malformed,
}
pub struct Case {
    pub name: String,
    pub tag: Vec<u8>,
    pub outcome: Outcome,
}
impl Case {
    pub fn data(&self, compressed: bool) -> Vec<u8> {
        let mut data = b"Comment\0".to_vec();
        data.extend([u8::from(compressed), 0]);
        data.extend(&self.tag);
        data.push(0);
        data.extend(TRANSLATED.as_bytes());
        data.push(0);
        if compressed {
            data.extend(chunks::stored(TEXT.as_bytes()));
        } else {
            data.extend(TEXT.as_bytes());
        }
        data
    }
    pub fn bytes(&self, compressed: bool, position: ChunkPosition) -> Vec<u8> {
        chunks::fixture(*b"iTXt", &self.data(compressed), position)
    }
}
pub fn cases() -> Vec<Case> {
    use Outcome::*;
    let mut cases = Vec::new();
    for value in 0..=255u8 {
        let mut tag = b"en-".to_vec();
        tag.push(value);
        tag.extend(b"-aa");
        cases.push(Case {
            name: format!("singleton-{value:02x}"),
            tag,
            outcome: if !value.is_ascii_alphanumeric() {
                Malformed
            } else if b"tux".contains(&value.to_ascii_lowercase()) {
                Typed
            } else {
                Opaque
            },
        });
    }
    let singletons: Vec<_> = (b'0'..=b'9')
        .chain(b'a'..=b'z')
        .filter(|&v| v != b'x')
        .collect();
    assert_eq!(singletons.len(), 35);
    for first in singletons.iter().copied() {
        for second in singletons.iter().copied() {
            for upper in [false, true] {
                let last = if upper {
                    second.to_ascii_uppercase()
                } else {
                    second
                };
                let tag = vec![
                    b'e', b'n', b'-', first, b'-', b'a', b'a', b'-', last, b'-', b'b', b'b',
                ];
                cases.push(Case {
                    name: format!("pair-{first:02x}-{second:02x}-{upper}"),
                    tag,
                    outcome: if first == second {
                        Malformed
                    } else if b"tu".contains(&first) && b"tu".contains(&second) {
                        Typed
                    } else {
                        Opaque
                    },
                });
            }
        }
    }
    for extension in *b"tu" {
        for count in 0..=9 {
            let mut tag = vec![b'e', b'n', b'-', extension, b'-'];
            tag.extend(std::iter::repeat_n(b'a', count));
            cases.push(Case {
                name: format!("length-{extension:02x}-{count}"),
                tag,
                outcome: if (2..=8).contains(&count) {
                    Typed
                } else {
                    Malformed
                },
            });
        }
    }
    for prefix in ["x-", "en-x-"] {
        for count in 0..=9 {
            let mut tag = prefix.as_bytes().to_vec();
            tag.extend(std::iter::repeat_n(b'a', count));
            cases.push(Case {
                name: format!("private-{prefix}-{count}"),
                tag,
                outcome: if (1..=8).contains(&count) {
                    Typed
                } else {
                    Malformed
                },
            });
        }
    }
    for (index, (tag, outcome)) in [
        ("ar-AE-u-nu-latn", Typed),
        ("EN-U-NU-LATN", Typed),
        ("en-t-de-u-ca-gregory", Typed),
        ("en-u-aa-aa", Typed),
        ("en-t-h0", Typed),
        ("en-u-zz-future", Typed),
        ("en-u-aa-x-a-u-aa", Typed),
        ("en-x-a-u-aa-u-bb", Typed),
        ("en-a-future", Opaque),
        ("en-u-aa-0-future", Opaque),
        ("en-u-aa-U-bb", Malformed),
        ("en-u-aa-x", Malformed),
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(Case {
            name: format!("example-{index}"),
            tag: tag.as_bytes().to_vec(),
            outcome,
        });
    }
    assert_eq!(cases.len(), 2758);
    cases
}
pub fn seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds = Vec::new();
    for case in cases() {
        for compressed in [false, true] {
            if compressed && case.name.starts_with("pair-") {
                continue;
            }
            seeds.push((
                format!("language-extensions-{}-{compressed}", case.name),
                case.bytes(compressed, ChunkPosition::BeforeData),
            ));
        }
    }
    assert_eq!(seeds.len(), 3066);
    seeds
}
