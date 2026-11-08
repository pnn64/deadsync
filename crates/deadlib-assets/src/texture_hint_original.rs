//! Frozen keyword scanner from 79bc83980.
use super::super::*;

pub(super) fn texture_hint_flags(bytes: &[u8]) -> u16 {
    let mut flags = 0u16;
    for index in 0..bytes.len() {
        let (first, second) = match bytes[index].to_ascii_lowercase() {
            b'1' => (b"16bpp".as_slice(), HINT_16BPP),
            b'3' => (b"32bpp".as_slice(), HINT_32BPP),
            b'a' => (b"alphamap".as_slice(), HINT_ALPHAMAP),
            b'c' => (b"clamp".as_slice(), HINT_CLAMP),
            b'g' => (b"grayscale".as_slice(), HINT_GRAYSCALE),
            b'l' => (b"linear".as_slice(), HINT_LINEAR),
            b'm' => (b"mipmaps".as_slice(), HINT_MIPMAPS),
            b'p' => (b"point".as_slice(), HINT_POINT),
            b'r' => (b"repeat".as_slice(), HINT_REPEAT),
            b's' => (b"stretch".as_slice(), HINT_STRETCH),
            b'w' => (b"wrap".as_slice(), HINT_WRAP),
            b'd' => {
                if hint_starts_with(bytes, index, b"dither") {
                    flags |= HINT_DITHER;
                }
                (b"doubleres".as_slice(), HINT_DOUBLERES)
            }
            b'n' => {
                if hint_starts_with(bytes, index, b"nearest") {
                    flags |= HINT_NEAREST;
                }
                (b"nomipmaps".as_slice(), HINT_NOMIPMAPS)
            }
            _ => continue,
        };
        if hint_starts_with(bytes, index, first) {
            flags |= second;
        }
    }
    flags
}
