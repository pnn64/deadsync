//! Named ancillary fixtures independent of native serialization/compression.
use super::chunks as wire;

pub fn palette(
    name: &[u8],
    depth: u8,
    entries: &[[u16; 5]],
    position: ChunkPosition,
) -> AncillaryChunk {
    let mut data = name.to_vec();
    data.extend([0, depth]);
    for entry in entries {
        for (field, &value) in entry.iter().enumerate() {
            if depth == 8 && field < 4 {
                data.push(u8::try_from(value).unwrap());
            } else {
                data.extend(value.to_be_bytes());
            }
        }
    }
    AncillaryChunk {
        kind: *b"sPLT",
        data,
        position,
    }
}
use deadlib_image_png::{AncillaryChunk, ChunkPosition, Metadata};

pub const PROFILE: &[u8] = include_bytes!("../fixtures/icc/sRGB-v4.icc");
pub const LATIN_TEXT: &[u8] = b"Exact \xa1\xe9\xff\ntext";
pub const UNICODE_TEXT: &str = "Exact \u{e9}\n\u{1f980}";
const ENTRIES_8: [[u16; 5]; 2] = [[1, 2, 3, 255, 65535], [4, 5, 6, 0, 0]];
const ENTRIES_16: [[u16; 5]; 2] = [[1, 511, 257, 65535, 65535], [65535, 0, 32768, 0, 0]];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Form {
    Text,
    ZText,
    IText,
    IZText,
    Icc,
    Palette8,
    Palette16,
}
pub const FORMS: [Form; 7] = [
    Form::Text,
    Form::ZText,
    Form::IText,
    Form::IZText,
    Form::Icc,
    Form::Palette8,
    Form::Palette16,
];

impl Form {
    pub fn kind(self) -> [u8; 4] {
        match self {
            Self::Text => *b"tEXt",
            Self::ZText => *b"zTXt",
            Self::IText | Self::IZText => *b"iTXt",
            Self::Icc => *b"iCCP",
            Self::Palette8 | Self::Palette16 => *b"sPLT",
        }
    }
    pub fn value(self, name: &[u8]) -> Metadata {
        match self {
            Self::Text | Self::ZText => Metadata::Text {
                keyword: name.to_vec(),
                text: LATIN_TEXT.to_vec(),
                compressed: self == Self::ZText,
            },
            Self::IText | Self::IZText => Metadata::InternationalText {
                keyword: name.to_vec(),
                language: "en".into(),
                translated_keyword: "Title".into(),
                text: UNICODE_TEXT.into(),
                compressed: self == Self::IZText,
            },
            Self::Icc => Metadata::IccProfile {
                name: name.to_vec(),
                profile: PROFILE.to_vec(),
            },
            Self::Palette8 | Self::Palette16 => Metadata::SuggestedPalette {
                name: name.to_vec(),
                depth: if self == Self::Palette8 { 8 } else { 16 },
                entries: if self == Self::Palette8 {
                    ENTRIES_8.to_vec()
                } else {
                    ENTRIES_16.to_vec()
                },
            },
        }
    }
    pub fn raw(self, name: &[u8], position: ChunkPosition) -> AncillaryChunk {
        let mut data = name.to_vec();
        data.push(0);
        match self {
            Self::Text => data.extend(LATIN_TEXT),
            Self::ZText => {
                data.push(0);
                data.extend(wire::stored(LATIN_TEXT));
            }
            Self::IText | Self::IZText => {
                data.extend([u8::from(self == Self::IZText), 0]);
                data.extend(b"en\0Title\0");
                if self == Self::IZText {
                    data.extend(wire::stored(UNICODE_TEXT.as_bytes()));
                } else {
                    data.extend(UNICODE_TEXT.as_bytes());
                }
            }
            Self::Icc => {
                data.push(0);
                data.extend(wire::stored(PROFILE));
            }
            Self::Palette8 | Self::Palette16 => {
                data.push(if self == Self::Palette8 { 8 } else { 16 });
                for entry in if self == Self::Palette8 {
                    ENTRIES_8
                } else {
                    ENTRIES_16
                } {
                    for (field, value) in entry.into_iter().enumerate() {
                        if self == Self::Palette8 && field < 4 {
                            data.push(value as u8);
                        } else {
                            data.extend(value.to_be_bytes());
                        }
                    }
                }
            }
        }
        AncillaryChunk {
            kind: self.kind(),
            data,
            position,
        }
    }
    pub fn fixture(self, name: &[u8], position: ChunkPosition) -> Vec<u8> {
        let raw = self.raw(name, position);
        wire::fixture(raw.kind, &raw.data, position)
    }
    pub fn reference_value(self) -> Vec<u8> {
        match self {
            Self::Text | Self::ZText => LATIN_TEXT.to_vec(),
            Self::IText | Self::IZText => UNICODE_TEXT.as_bytes().to_vec(),
            Self::Icc => PROFILE.to_vec(),
            Self::Palette8 | Self::Palette16 => {
                let mut value = vec![if self == Self::Palette8 { 8 } else { 16 }];
                for entry in if self == Self::Palette8 {
                    ENTRIES_8
                } else {
                    ENTRIES_16
                } {
                    for field in entry {
                        value.extend(field.to_be_bytes());
                    }
                }
                value
            }
        }
    }
}

pub struct NameCase {
    pub label: String,
    pub bytes: Vec<u8>,
    pub valid: bool,
    pub c_reference: bool,
}

/// Equivalence classes from PNG 11.3.3.1; expected outcomes are specified here
/// independently of the native keyword validator and all wire constructors.
pub fn names() -> Vec<NameCase> {
    let mut cases = Vec::new();
    for byte in 0u8..=255 {
        let printable = matches!(byte,0x20..=0x7e | 0xa1..=0xff);
        for (label, bytes, valid) in [
            ("single", vec![byte], printable && byte != 32),
            ("first", vec![byte, b'A', b'B'], printable && byte != 32),
            ("middle", vec![b'A', byte, b'B'], printable),
            ("last", vec![b'A', b'B', byte], printable && byte != 32),
        ] {
            cases.push(NameCase {
                label: format!("byte-{label}-{byte}"),
                bytes,
                valid,
                c_reference: label == "single",
            });
        }
        for offset in [0, 1, 39, 77, 78] {
            let mut bytes = vec![b'A'; 79];
            bytes[offset] = byte;
            cases.push(NameCase {
                label: format!("long-{offset}-{byte}"),
                bytes,
                valid: printable && !(byte == 32 && (offset == 0 || offset == 78)),
                c_reference: offset == 39,
            });
        }
    }
    for len in 0..=256 {
        cases.push(NameCase {
            label: format!("length-{len}"),
            bytes: vec![b'A'; len],
            valid: (1..=79).contains(&len),
            c_reference: [0, 1, 2, 78, 79, 80, 81, 255, 256].contains(&len),
        });
    }
    for offset in 0..79 {
        let mut bytes = vec![b'A'; 79];
        bytes[offset] = 32;
        cases.push(NameCase {
            label: format!("space-{offset}"),
            bytes,
            valid: offset != 0 && offset != 78,
            c_reference: [0, 1, 39, 77, 78].contains(&offset),
        });
    }
    for offset in 0..78 {
        let mut bytes = vec![b'A'; 79];
        bytes[offset..offset + 2].fill(32);
        cases.push(NameCase {
            label: format!("double-space-{offset}"),
            bytes,
            valid: false,
            c_reference: [0, 1, 39, 77].contains(&offset),
        });
    }
    cases
}
